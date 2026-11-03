//! The MIR dialect: operations, their fields, and every property the compiler reads from them.
//!
//! A MIR schema declares its vocabulary first (operand and attribute types, host enums, traits,
//! phase categories, metadata flags, and semantics forms), then classes and operations. An
//! operation inherits fields and properties from its class and overrides them; traits accumulate.
//! The generator checks every name against the vocabulary and emits the typed `InstKind` enum,
//! its `OpDef` table, operand traversal, operand type signatures, raw memory accesses, declared
//! semantics, the `Op` rewrite view, and builder methods.

use crate::{
    common::{
        EnumDecl, FieldTypes, Pattern, Pred, insert_enum, lower_pred, normalize, parse_enum,
        parse_pred, parse_variants,
    },
    syntax::{Cursor, Error, Node, Result, Span, Tok, Token, camel, doc_attrs},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

/// How many value operands a field holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arity {
    One,
    Optional,
    Many,
}

/// A field type: a Rust type holding value operands, or an attribute.
#[derive(Clone, Debug)]
struct FieldType {
    rust: String,
    /// `None` for attributes.
    arity: Option<Arity>,
    /// The ISLE type of the rewrite view; `None` elides the field from it.
    isle: Option<String>,
    /// The fields of a struct attribute that predicates may read, with their enum types.
    fields: Vec<(String, String)>,
}

/// A documented name in the vocabulary.
#[derive(Clone, Debug)]
struct Decl {
    name: String,
    docs: Vec<String>,
}

/// A phase category: its variant and diagnostic text.
#[derive(Clone, Debug)]
struct Category {
    name: String,
    text: String,
    docs: Vec<String>,
}

/// A semantics form and the field types of its arguments.
#[derive(Clone, Debug)]
struct SemanticsDecl {
    params: Vec<String>,
}

#[derive(Clone, Debug)]
struct FieldDecl {
    name: String,
    ty: String,
    docs: Vec<String>,
    span: Span,
}

/// A raw memory access.
#[derive(Clone, Debug)]
struct Access {
    kind: &'static str,
    address: String,
    size: Size,
    guard: Option<Pred>,
    span: Span,
}

#[derive(Clone, Debug)]
enum Size {
    Bytes(u64),
    Field(String),
}

/// A type in an operand signature.
#[derive(Clone, Debug)]
enum Ty {
    /// `i1`, `i160`, `i256`, `ptr<location>`, `slice<location>`.
    Concrete(Concrete),
    /// `pattern else default`: the operand's own type when it matches the pattern.
    Else(TyPattern, Box<Self>),
    /// `type(field)`: the signature of an earlier operand, or the type an attribute denotes.
    Of(String, Span),
    /// `if pred then a else b`.
    If(Pred, Box<Self>, Box<Self>),
    /// `from(field)`: types the field's Rust type computes for the operands.
    From(String, Span),
}

#[derive(Clone, Debug)]
enum Concrete {
    Int(u32),
    Ptr(Location),
    Slice(Location),
}

#[derive(Clone, Debug)]
enum Location {
    Named(String, Span),
    Field(String, Span),
}

#[derive(Clone, Debug)]
enum TyPattern {
    Int,
    Ptr(Option<Vec<Location>>),
    Slice(Option<Vec<Location>>),
}

#[derive(Clone, Debug)]
enum Types {
    Delegated,
    Fields(Vec<(String, Ty, Span)>),
}

#[derive(Clone, Debug)]
struct SemanticsUse {
    name: String,
    args: Vec<String>,
    span: Span,
}

/// Properties an operation, a class, or a `when` clause sets.
#[derive(Clone, Debug, Default)]
struct Props {
    mnemonic: Option<String>,
    isle: Option<String>,
    until: Option<(String, Span)>,
    effect: Option<(String, Span)>,
    traits: Vec<TraitUse>,
    side_effects: Option<bool>,
    category: Option<(String, Span)>,
    commutative: Option<(String, String, Span)>,
    builder: Option<(Option<String>, bool, Span)>,
    memory: Option<Vec<Access>>,
    types: Option<Types>,
    semantics: Option<SemanticsUse>,
}

/// A trait an operation has, always or when `guard` holds.
#[derive(Clone, Debug)]
struct TraitUse {
    name: String,
    span: Span,
    guard: Option<Pred>,
}

#[derive(Clone, Debug)]
struct When {
    pred: Pred,
    props: Props,
    span: Span,
}

/// A class or operation as written.
#[derive(Clone, Debug)]
struct OpDecl {
    name: String,
    docs: Vec<String>,
    span: Span,
    base: Option<(String, Span)>,
    /// Whether the fields form a tuple variant, and the fields.
    fields: Option<(bool, Vec<FieldDecl>)>,
    result: Option<(String, Span)>,
    props: Props,
    whens: Vec<When>,
    /// The namespace of the extension file that declares it, if not the core schema.
    namespace: Option<String>,
}

/// A MIR schema.
#[derive(Default)]
pub struct Schema {
    enums: BTreeMap<String, EnumDecl>,
    types: BTreeMap<String, FieldType>,
    traits: Vec<Decl>,
    categories: Vec<Category>,
    metadata: BTreeMap<String, Decl>,
    semantics: BTreeMap<String, SemanticsDecl>,
    classes: BTreeMap<String, OpDecl>,
    ops: Vec<OpDecl>,
}

impl Schema {
    /// Adds the declarations of one file.
    pub fn add_file(&mut self, nodes: &[Node]) -> Result<()> {
        let mut nodes = nodes.iter();
        let namespace = match nodes.next() {
            Some(node) => {
                let mut cur = node.cursor();
                cur.expect_word("dialect")?;
                let (name, span) = cur.ident()?;
                if name != "mir" {
                    return Err(Error::new(
                        span,
                        format!("expected the `mir` dialect, found `{name}`"),
                    ));
                }
                let namespace = if cur.eat(".") {
                    let (namespace, span) = cur.ident()?;
                    if namespace.is_empty()
                        || !namespace.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                    {
                        return Err(Error::new(
                            span,
                            "an extension namespace is lowercase letters and digits",
                        ));
                    }
                    Some(namespace.to_string())
                } else {
                    None
                };
                cur.finish()?;
                namespace
            }
            None => return Ok(()),
        };
        for node in nodes {
            let cur = node.cursor();
            match cur.peek_word() {
                Some("operand") => self.parse_operand(node)?,
                Some("attribute") => self.parse_attribute(node)?,
                Some("enum") => parse_enum(node, &mut self.enums)?,
                Some("trait") => {
                    let decl = parse_decl(node, "trait")?;
                    if self.traits.iter().any(|t| t.name == decl.name) {
                        return Err(Error::new(
                            node.span,
                            format!("trait `{}` is declared twice", decl.name),
                        ));
                    }
                    self.traits.push(decl);
                }
                Some("metadata") => {
                    let decl = parse_decl(node, "metadata")?;
                    self.metadata.insert(decl.name.clone(), decl);
                }
                Some("category") => self.parse_category(node)?,
                Some("semantics") => self.parse_semantics_decl(node)?,
                Some("class") => {
                    let mut decl = parse_op(node, "class")?;
                    decl.namespace.clone_from(&namespace);
                    if self.classes.insert(decl.name.clone(), decl).is_some() {
                        return Err(Error::new(node.span, "class is declared twice"));
                    }
                }
                Some("op") => {
                    let mut decl = parse_op(node, "op")?;
                    decl.namespace.clone_from(&namespace);
                    self.ops.push(decl);
                }
                _ => return Err(cur.unexpected("a declaration")),
            }
        }
        Ok(())
    }

    fn parse_operand(&mut self, node: &Node) -> Result<()> {
        let mut cur = node.cursor();
        cur.expect_word("operand")?;
        let (name, span) = parse_type_name(&mut cur)?;
        let arity = match cur.ident()? {
            ("one", _) => Arity::One,
            ("optional", _) => Arity::Optional,
            ("many", _) => Arity::Many,
            (other, span) => {
                return Err(Error::new(
                    span,
                    format!("expected `one`, `optional`, or `many`, found `{other}`"),
                ));
            }
        };
        cur.expect("=")?;
        let rust = cur.string()?.to_string();
        let isle = if cur.eat_word("isle") { Some(cur.ident()?.0.to_string()) } else { None };
        cur.finish()?;
        self.insert_type(
            name,
            span,
            FieldType { rust, arity: Some(arity), isle, fields: Vec::new() },
        )
    }

    fn parse_attribute(&mut self, node: &Node) -> Result<()> {
        let mut cur = node.cursor();
        cur.expect_word("attribute")?;
        let (name, span) = parse_type_name(&mut cur)?;
        let rust = if cur.eat("=") { cur.string()?.to_string() } else { name.clone() };
        let isle = if cur.eat_word("opaque") {
            None
        } else if cur.eat_word("isle") {
            Some(cur.ident()?.0.to_string())
        } else {
            Some(name.clone())
        };
        if cur.eat(":") {
            let (variants, exhaustive) = parse_variants(&mut cur)?;
            insert_enum(
                &mut self.enums,
                EnumDecl { rust: rust.clone(), variants, exhaustive, span },
            )?;
        }
        cur.finish()?;
        let mut fields = Vec::new();
        for child in node.children() {
            let mut cur = child.cursor();
            let (field, _) = cur.ident()?;
            cur.expect(":")?;
            let (ty, _) = cur.ident()?;
            cur.finish()?;
            fields.push((field.to_string(), ty.to_string()));
        }
        self.insert_type(name, span, FieldType { rust, arity: None, isle, fields })
    }

    fn insert_type(&mut self, name: String, span: Span, ty: FieldType) -> Result<()> {
        if self.types.insert(name.clone(), ty).is_some() {
            return Err(Error::new(span, format!("type `{name}` is declared twice")));
        }
        Ok(())
    }

    fn parse_category(&mut self, node: &Node) -> Result<()> {
        let mut cur = node.cursor();
        cur.expect_word("category")?;
        let (name, _) = cur.ident()?;
        cur.expect("=")?;
        let text = cur.string()?.to_string();
        cur.finish()?;
        self.categories.push(Category { name: name.to_string(), text, docs: node.docs.clone() });
        Ok(())
    }

    fn parse_semantics_decl(&mut self, node: &Node) -> Result<()> {
        let mut cur = node.cursor();
        cur.expect_word("semantics")?;
        let (name, _) = cur.ident()?;
        cur.expect("(")?;
        let params = cur.comma_list(Some(")"), |cur| Ok(parse_type_name(cur)?.0))?;
        cur.finish()?;
        self.semantics.insert(name.to_string(), SemanticsDecl { params });
        Ok(())
    }

    /// Resolves classes and checks every operation, then emits the generated Rust module.
    pub fn generate(&self) -> Result<String> {
        let mut ops = Vec::new();
        let mut mnemonics = BTreeMap::new();
        let mut names = BTreeSet::new();
        for decl in &self.ops {
            let op = self.resolve(decl)?;
            if !names.insert(op.name.clone()) {
                return Err(Error::new(
                    decl.span,
                    format!("operation `{}` is declared twice", op.name),
                ));
            }
            for mnemonic in std::iter::once(&op.mnemonic)
                .chain(op.whens.iter().filter_map(|when| when.props.mnemonic.as_ref()))
            {
                if let Some(other) = mnemonics.insert(mnemonic.clone(), op.name.clone())
                    && other != op.name
                {
                    return Err(Error::new(
                        decl.span,
                        format!("mnemonic `{mnemonic}` is already used by `{other}`"),
                    ));
                }
            }
            ops.push(op);
        }
        let mut out = String::from(
            "// @generated by solar-op-dsl from the MIR operation schema; do not edit.\n\n",
        );
        self.emit_vocabulary(&mut out);
        for decl in self.enums.values() {
            out.push_str(&decl.exhaustiveness_check());
        }
        emit_inst_kind(&mut out, &ops);
        self.emit_inst_kind_impl(&mut out, &ops)?;
        emit_builders(&mut out, &ops);
        emit_op_view(&mut out, &ops);
        Ok(out)
    }

    /// Merges an operation with its classes and checks its properties.
    fn resolve(&self, decl: &OpDecl) -> Result<Op> {
        let mut chain = vec![decl];
        let mut seen = BTreeSet::new();
        while let Some((base, span)) = &chain.last().unwrap().base {
            if !seen.insert(base.clone()) {
                return Err(Error::new(*span, format!("class `{base}` inherits from itself")));
            }
            let class = self
                .classes
                .get(base)
                .ok_or_else(|| Error::new(*span, format!("unknown class `{base}`")))?;
            chain.push(class);
        }
        let mut fields = None;
        let mut result = None;
        let mut props = Props::default();
        let mut whens = Vec::new();
        for decl in chain.iter().rev() {
            if decl.fields.is_some() {
                fields.clone_from(&decl.fields);
            }
            if decl.result.is_some() {
                result.clone_from(&decl.result);
            }
            merge(&mut props, &decl.props);
        }
        for decl in &chain {
            whens.extend(decl.whens.iter().cloned());
        }
        let (tuple, fields) = fields.unwrap_or((true, Vec::new()));
        let mut resolved_fields = Vec::new();
        for field in fields {
            let ty = self
                .types
                .get(&field.ty)
                .ok_or_else(|| {
                    Error::new(field.span, format!("unknown field type `{}`", field.ty))
                })?
                .clone();
            if resolved_fields.iter().any(|f: &Field| f.decl.name == field.name) {
                return Err(Error::new(
                    field.span,
                    format!("field `{}` is declared twice", field.name),
                ));
            }
            resolved_fields.push(Field { decl: field, ty });
        }
        let result = match result {
            Some((name, span)) => self.enum_variant("ResultKind", &name, span)?,
            None => "ResultKind::None".to_string(),
        };
        let mnemonic = match &decl.namespace {
            Some(namespace) => {
                let prefix = camel(namespace);
                let Some(rest) = decl
                    .name
                    .strip_prefix(&prefix)
                    .filter(|rest| rest.starts_with(|c: char| c.is_ascii_uppercase()))
                else {
                    return Err(Error::new(
                        decl.span,
                        format!("an operation in namespace `{namespace}` is named `{prefix}...`"),
                    ));
                };
                let dotted = format!("{namespace}.");
                for mnemonic in props
                    .mnemonic
                    .iter()
                    .chain(whens.iter().filter_map(|when| when.props.mnemonic.as_ref()))
                {
                    if !mnemonic.starts_with(&dotted) {
                        return Err(Error::new(
                            decl.span,
                            format!("mnemonic `{mnemonic}` must start with `{dotted}`"),
                        ));
                    }
                }
                // An unknown operation has every effect a core analysis could ask about; an
                // extension states the ones it has.
                if props.effect.is_none() {
                    return Err(Error::new(
                        decl.span,
                        "an extension operation must declare its `effect`",
                    ));
                }
                props
                    .mnemonic
                    .clone()
                    .unwrap_or_else(|| format!("{dotted}{}", rest.to_ascii_lowercase()))
            }
            None => props.mnemonic.clone().unwrap_or_else(|| decl.name.to_ascii_lowercase()),
        };
        let isle = props.isle.clone().unwrap_or_else(|| decl.name.to_ascii_lowercase());
        if matches!(
            isle.as_str(),
            "and" | "or" | "not" | "if" | "let" | "rule" | "type" | "decl" | "extern"
        ) {
            return Err(Error::new(
                decl.span,
                format!("`{isle}` is an ISLE keyword; name the term with `isle`"),
            ));
        }
        let mut op = Op {
            name: decl.name.clone(),
            docs: decl.docs.clone(),
            span: decl.span,
            tuple,
            fields: resolved_fields,
            result,
            mnemonic,
            isle,
            props,
            whens,
        };
        self.check(&mut op)?;
        Ok(op)
    }

    fn enum_variant(&self, ty: &str, name: &str, span: Span) -> Result<String> {
        self.enums
            .get(ty)
            .ok_or_else(|| Error::new(span, format!("the schema must declare `enum {ty}`")))?
            .resolve(name, span)
    }

    /// Checks every name an operation uses.
    fn check(&self, op: &mut Op) -> Result<()> {
        self.check_props(op, &op.props, false)?;
        for when in &op.whens {
            lower_pred(&when.pred, &OpFields { schema: self, op }, &self.enums)?;
            let props = &when.props;
            if when.pred.uses_metadata() {
                if props.mnemonic.is_some()
                    || props.effect.is_some()
                    || props.semantics.is_some()
                    || props.memory.is_some()
                {
                    return Err(Error::new(
                        when.span,
                        "a clause on instruction metadata may only set `until` and `category`",
                    ));
                }
            } else if !when.pred.is_const() {
                return Err(Error::new(
                    when.span,
                    "only clauses on instruction metadata may compare with constants",
                ));
            }
            if !props.traits.is_empty()
                || props.side_effects.is_some()
                || props.commutative.is_some()
                || props.builder.is_some()
                || props.types.is_some()
                || props.memory.is_some()
                || props.isle.is_some()
            {
                return Err(Error::new(
                    when.span,
                    "a `when` clause may only set `mnemonic`, `effect`, `until`, `category`, and `semantics`",
                ));
            }
            self.check_props(op, props, true)?;
        }
        if let Some((name, _, span)) = &op.props.builder {
            let name = name.as_deref().unwrap_or(&op.mnemonic);
            if !op.tuple || op.fields.iter().any(|f| f.ty.arity != Some(Arity::One)) {
                return Err(Error::new(
                    *span,
                    format!("builder `{name}` needs a tuple of value operands"),
                ));
            }
        }
        if op.props.types.is_none() && op.fields.iter().all(|f| f.ty.arity.is_none()) {
            op.props.types = Some(Types::Fields(Vec::new()));
        }
        match &op.props.types {
            None => return Err(Error::new(op.span, "operation declares no operand `types`")),
            Some(Types::Delegated) => {}
            Some(Types::Fields(entries)) => {
                for field in &op.fields {
                    let covered = entries.iter().any(|(name, ..)| *name == field.decl.name);
                    if field.ty.arity.is_some() && !covered {
                        return Err(Error::new(
                            op.span,
                            format!("`types` does not cover operand `{}`", field.decl.name),
                        ));
                    }
                }
                let mut earlier = Vec::new();
                for (name, ty, span) in entries {
                    let field = op.field(name, *span)?;
                    if field.ty.arity.is_none() {
                        return Err(Error::new(
                            *span,
                            format!("`{name}` is an attribute, not an operand"),
                        ));
                    }
                    self.check_ty(op, ty, &earlier)?;
                    earlier.push(name.clone());
                }
                let order = op.fields.iter().filter(|f| f.ty.arity.is_some()).map(|f| &f.decl.name);
                if !order.eq(entries.iter().map(|(name, ..)| name)) {
                    return Err(Error::new(
                        op.span,
                        "`types` must list the operands in field order",
                    ));
                }
            }
        }
        Ok(())
    }

    fn check_props(&self, op: &Op, props: &Props, in_when: bool) -> Result<()> {
        if let Some((name, span)) = &props.until {
            self.enum_variant("MirPhase", name, *span)?;
        }
        if let Some((name, span)) = &props.effect {
            self.enum_variant("EffectKind", name, *span)?;
        }
        if let Some((name, span)) = &props.category {
            self.category(name, *span)?;
        }
        for TraitUse { name, span, guard } in &props.traits {
            if name == "commutative" {
                return Err(Error::new(
                    *span,
                    "declare `commutative lhs, rhs` instead of the trait",
                ));
            }
            self.trait_const(name, *span)?;
            if let Some(guard) = guard {
                lower_pred(guard, &OpFields { schema: self, op }, &self.enums)?;
                if !guard.is_const() {
                    return Err(Error::new(
                        *span,
                        "a trait condition may only test attributes and operands",
                    ));
                }
            }
        }
        if let Some((lhs, rhs, span)) = &props.commutative {
            for name in [lhs, rhs] {
                if op.field(name, *span)?.ty.arity != Some(Arity::One) {
                    return Err(Error::new(
                        *span,
                        format!("commutative operand `{name}` must be one value"),
                    ));
                }
            }
        }
        for access in props.memory.iter().flatten() {
            if op.field(&access.address, access.span)?.ty.arity != Some(Arity::One) {
                return Err(Error::new(access.span, "a memory address must be one value operand"));
            }
            if let Size::Field(size) = &access.size
                && op.field(size, access.span)?.ty.arity != Some(Arity::One)
            {
                return Err(Error::new(access.span, "a memory size must be one value operand"));
            }
            if let Some(guard) = &access.guard {
                if !guard.is_const() {
                    return Err(Error::new(access.span, "a memory guard may only test attributes"));
                }
                lower_pred(guard, &OpFields { schema: self, op }, &self.enums)?;
            }
        }
        if let Some(semantics) = &props.semantics {
            self.check_semantics(op, semantics)?;
        }
        let _ = in_when;
        Ok(())
    }

    fn check_semantics(&self, op: &Op, semantics: &SemanticsUse) -> Result<()> {
        let decl = self.semantics.get(&semantics.name).ok_or_else(|| {
            Error::new(semantics.span, format!("unknown semantics `{}`", semantics.name))
        })?;
        if decl.params.len() != semantics.args.len() {
            return Err(Error::new(
                semantics.span,
                format!(
                    "`{}` takes {} arguments, found {}",
                    semantics.name,
                    decl.params.len(),
                    semantics.args.len()
                ),
            ));
        }
        for (param, arg) in decl.params.iter().zip(&semantics.args) {
            if param == "opcode" {
                continue;
            }
            let field = op.field(arg, semantics.span)?;
            if field.decl.ty != *param {
                return Err(Error::new(
                    semantics.span,
                    format!(
                        "`{}` expects a `{param}` argument, but `{arg}` is a `{}`",
                        semantics.name, field.decl.ty
                    ),
                ));
            }
        }
        Ok(())
    }

    fn check_ty(&self, op: &Op, ty: &Ty, earlier: &[String]) -> Result<()> {
        match ty {
            Ty::Concrete(concrete) => self.check_concrete(op, concrete),
            Ty::Else(pattern, default) => {
                match pattern {
                    TyPattern::Int => {}
                    TyPattern::Ptr(locations) | TyPattern::Slice(locations) => {
                        for location in locations.iter().flatten() {
                            self.location(op, location)?;
                        }
                    }
                }
                self.check_ty(op, default, earlier)
            }
            Ty::Of(name, span) => {
                let field = op.field(name, *span)?;
                match field.ty.arity {
                    None => Ok(()),
                    Some(Arity::One) if earlier.contains(name) => Ok(()),
                    _ => Err(Error::new(
                        *span,
                        format!("`type({name})` needs an earlier single operand or an attribute"),
                    )),
                }
            }
            Ty::If(pred, a, b) => {
                if !pred.is_const() {
                    return Err(Error::new(op.span, "a type condition may only test attributes"));
                }
                lower_pred(pred, &OpFields { schema: self, op }, &self.enums)?;
                self.check_ty(op, a, earlier)?;
                self.check_ty(op, b, earlier)
            }
            Ty::From(name, span) => {
                op.field(name, *span)?;
                Ok(())
            }
        }
    }

    fn check_concrete(&self, op: &Op, concrete: &Concrete) -> Result<()> {
        match concrete {
            Concrete::Int(_) => Ok(()),
            Concrete::Ptr(location) | Concrete::Slice(location) => {
                self.location(op, location).map(drop)
            }
        }
    }

    fn location(&self, op: &Op, location: &Location) -> Result<String> {
        match location {
            Location::Named(name, span) => self.enum_variant("PtrLocation", &camel(name), *span),
            Location::Field(name, span) => {
                let field = op.field(name, *span)?;
                if field.ty.rust != "PtrLocation" {
                    return Err(Error::new(
                        *span,
                        format!("`{name}` is not a `PtrLocation` attribute"),
                    ));
                }
                Ok(format!("*{name}"))
            }
        }
    }

    fn category(&self, name: &str, span: Span) -> Result<String> {
        self.categories
            .iter()
            .find(|c| normalize(&c.name) == normalize(name))
            .map(|c| format!("PhaseCategory::{}", c.name))
            .ok_or_else(|| Error::new(span, format!("unknown phase category `{name}`")))
    }

    fn trait_const(&self, name: &str, span: Span) -> Result<String> {
        self.traits
            .iter()
            .find(|t| t.name == name)
            .map(|t| format!("OpTraits::{}", t.name.to_ascii_uppercase()))
            .ok_or_else(|| Error::new(span, format!("unknown trait `{name}`")))
    }

    fn emit_vocabulary(&self, out: &mut String) {
        out.push_str(
            "/// The semantic operation family that a phase does not admit, named in phase-boundary\n\
             /// diagnostics. Each family has one required pass that removes it.\n\
             #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]\npub enum PhaseCategory {\n",
        );
        for category in &self.categories {
            out.push_str(&doc_attrs(&category.docs, "    "));
            writeln!(out, "    {},", category.name).unwrap();
        }
        out.push_str(
            "}\n\nimpl std::fmt::Display for PhaseCategory {\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n        f.write_str(match self {\n",
        );
        for category in &self.categories {
            writeln!(out, "            Self::{} => {:?},", category.name, category.text).unwrap();
        }
        out.push_str("        })\n    }\n}\n\nimpl OpTraits {\n");
        for (bit, decl) in self.traits.iter().enumerate() {
            out.push_str(&doc_attrs(&decl.docs, "    "));
            writeln!(
                out,
                "    pub const {}: Self = Self(1 << {bit});",
                decl.name.to_ascii_uppercase()
            )
            .unwrap();
        }
        out.push_str("}\n\n");
        for ty in self.types.values().filter(|ty| ty.arity.is_none()) {
            let (view, isle, view_expr, from_view) = match &ty.isle {
                Some(isle) => ("Self", isle.as_str(), "*self", "Some(view)"),
                None => ("()", "Unit", "", "None"),
            };
            let map_view = if ty.isle.is_some() { "view" } else { "" };
            let view_arg = if ty.isle.is_some() { "view" } else { "_view" };
            writeln!(
                out,
                "impl Operands for {rust} {{\n    type View = {view};\n    const ISLE_TYPE: &'static str = {isle:?};\n\n    #[inline]\n    fn collect<A: Array<Item = ValueId>>(&self, _out: &mut SmallVec<A>) {{}}\n\n    #[inline]\n    fn visit(&self, _f: &mut impl FnMut(ValueId)) {{}}\n\n    #[inline]\n    fn visit_mut(&mut self, _f: &mut impl FnMut(&mut ValueId)) {{}}\n\n    #[inline]\n    #[allow(clippy::unused_unit)]\n    fn view(&self) -> {view} {{\n        {view_expr}\n    }}\n\n    #[inline]\n    #[allow(clippy::unused_unit)]\n    fn map_view({view_arg}: {view}, _f: &mut impl FnMut(ValueId) -> ValueId) -> {view} {{\n        {map_view}\n    }}\n\n    #[inline]\n    fn from_view({view_arg}: {view}) -> Option<Self> {{\n        {from_view}\n    }}\n\n    const IS_OPERAND: bool = false;\n\n    fn from_operand(_: ValueId) -> Self {{\n        unreachable!(\"an attribute is not built from a value operand\")\n    }}\n}}\n",
                rust = ty.rust,
            )
            .unwrap();
        }
    }

    fn emit_inst_kind_impl(&self, out: &mut String, ops: &[Op]) -> Result<()> {
        out.push_str("impl InstKind {\n");

        // Operand type signatures.
        out.push_str(
            "    /// Returns the schema's exact operand signature, or `None` when result or module\n    /// information is required by the dedicated type checker.\n    #[allow(unused_variables, unused_mut, clippy::let_and_return)]\n    pub(crate) fn operand_types(&self, func: &Function) -> Option<SmallVec<[MirType; 8]>> {\n        match self {\n",
        );
        for op in ops {
            writeln!(out, "            {} => {{", op.binder("Self")).unwrap();
            match op.props.types.as_ref().unwrap() {
                Types::Delegated => out.push_str("                None\n"),
                Types::Fields(entries) => {
                    out.push_str("                let mut types = SmallVec::new();\n");
                    for (name, ty, _) in entries {
                        let field = op.field(name, op.span)?;
                        match (field.ty.arity.unwrap(), ty) {
                            (_, Ty::From(source, _)) => {
                                writeln!(out, "                let mut values = SmallVec::<[ValueId; 8]>::new();").unwrap();
                                if source != name {
                                    writeln!(
                                        out,
                                        "                Operands::collect({name}, &mut values);"
                                    )
                                    .unwrap();
                                }
                                writeln!(out, "                OperandTypes::operand_types({source}, func, &values, &mut types)?;").unwrap();
                            }
                            (Arity::One, ty) => {
                                let expr = self.lower_ty(op, ty, &format!("*{name}"))?;
                                writeln!(out, "                let ty_{name} = {expr};\n                types.push(ty_{name});").unwrap();
                            }
                            (_, ty) => {
                                let expr = self.lower_ty(op, ty, "value")?;
                                writeln!(out, "                Operands::visit({name}, &mut |value| types.push({expr}));").unwrap();
                            }
                        }
                    }
                    out.push_str("                Some(types)\n");
                }
            }
            out.push_str("            }\n");
        }
        out.push_str("        }\n    }\n\n");

        // The definition table.
        out.push_str(
            "    /// Returns the declarative definition for this operation.\n    #[inline]\n    #[must_use]\n    #[allow(unused_variables)]\n    pub const fn op_def(&self) -> &'static OpDef {\n        match self {\n",
        );
        for op in ops {
            writeln!(out, "            {} => {{", op.binder("Self")).unwrap();
            writeln!(
                out,
                "                const DEF: OpDef = {};",
                self.op_def(op, &op.props, None)?
            )
            .unwrap();
            let mut chain = String::new();
            for (index, when) in
                op.whens.iter().filter(|when| !when.pred.uses_metadata()).enumerate()
            {
                let pred = lower_pred(&when.pred, &OpFields { schema: self, op }, &self.enums)?;
                let def = if sets_op_def(&when.props) {
                    writeln!(
                        out,
                        "                const WHEN_{index}: OpDef = {};",
                        self.op_def(op, &when.props, Some("DEF"))?
                    )
                    .unwrap();
                    format!("&WHEN_{index}")
                } else {
                    "&DEF".to_string()
                };
                write!(chain, "if {pred} {{\n                    {def}\n                }} else ")
                    .unwrap();
            }
            writeln!(out, "                {chain}{{\n                    &DEF\n                }}\n            }}").unwrap();
        }
        out.push_str("        }\n    }\n\n");

        // Traits, with the conditional ones.
        out.push_str(
            "    /// Returns the traits of this operation: its definition's, and every conditional trait\n    /// whose condition holds.\n    #[inline]\n    #[must_use]\n    pub const fn traits(&self) -> OpTraits {\n        let traits = self.op_def().traits;\n        match self {\n",
        );
        for op in ops {
            let guarded = op.props.traits.iter().filter(|t| t.guard.is_some()).collect::<Vec<_>>();
            if guarded.is_empty() {
                continue;
            }
            let mut expr = String::from("traits");
            for TraitUse { name, span, guard } in guarded {
                let pred = lower_pred(
                    guard.as_ref().unwrap(),
                    &OpFields { schema: self, op },
                    &self.enums,
                )?;
                let bit = self.trait_const(name, *span)?;
                write!(expr, ".union(if {pred} {{ {bit} }} else {{ OpTraits::NONE }})").unwrap();
            }
            writeln!(
                out,
                "            #[allow(unused_variables)]\n            {} => {expr},",
                op.binder("Self")
            )
            .unwrap();
        }
        out.push_str("            _ => traits,\n        }\n    }\n\n");

        // Declared semantics.
        out.push_str(
            "    /// Returns what this operation computes, as its schema row declares it.\n    #[allow(unused_variables)]\n    #[must_use]\n    pub fn semantics(&self) -> Option<Semantics<'_>> {\n        use super::semantics::declare::*;\n        match self {\n",
        );
        for op in ops {
            let base = op.props.semantics.as_ref().map_or("None".to_string(), lower_semantics);
            let mut chain = String::new();
            for when in op.whens.iter().filter(|when| !when.pred.uses_metadata()) {
                let pred = lower_pred(&when.pred, &OpFields { schema: self, op }, &self.enums)?;
                let semantics =
                    when.props.semantics.as_ref().map_or_else(|| base.clone(), lower_semantics);
                write!(chain, "if {pred} {{\n                {semantics}\n            }} else ")
                    .unwrap();
            }
            if chain.is_empty() {
                writeln!(out, "            {} => {base},", op.binder("Self")).unwrap();
            } else {
                writeln!(
                    out,
                    "            {} => {chain}{{\n                {base}\n            }}",
                    op.binder("Self")
                )
                .unwrap();
            }
        }
        out.push_str("        }\n    }\n\n");
        out.push_str(
            "    /// Every operation's mnemonic with its semantics declaration as written.\n    pub(crate) const DECLARED_SEMANTICS: &[(&str, &str)] = &[\n",
        );
        for op in ops {
            writeln!(out, "        ({:?}, {:?}),", op.mnemonic, declared_semantics(op)).unwrap();
        }
        out.push_str("    ];\n\n");

        // Operand traversal.
        for (doc, sig, call) in [
            (
                "    /// Collects every value operand in canonical order.\n    ///\n    /// This is the canonical operand list for liveness and scheduling.\n",
                "pub(crate) fn collect_operands<A: Array<Item = ValueId>>(&self, out: &mut SmallVec<A>)",
                "Operands::collect({}, out);",
            ),
            (
                "    /// Shares one copy of the operand match between all visitors, instead of\n    /// instantiating it for every closure.\n",
                "fn visit_operands_dyn(&self, mut f: &mut dyn FnMut(ValueId))",
                "Operands::visit({}, &mut f);",
            ),
            (
                "    /// Visits every value operand mutably, in canonical order.\n",
                "pub fn visit_operands_mut(&mut self, mut f: impl FnMut(&mut ValueId))",
                "Operands::visit_mut({}, &mut f);",
            ),
        ] {
            write!(out, "{doc}    #[allow(unused_variables, unused_mut)]\n    {sig} {{\n        match self {{\n").unwrap();
            for op in ops {
                write!(out, "            {} => {{", op.binder("Self")).unwrap();
                for field in &op.fields {
                    write!(out, " {}", call.replace("{}", &field.decl.name)).unwrap();
                }
                out.push_str(" }\n");
            }
            out.push_str("        }\n    }\n\n");
        }
        out.push_str(
            "    /// Visits every value operand in canonical order, without collecting them.\n    #[inline]\n    pub fn visit_operands(&self, mut f: impl FnMut(ValueId)) {\n        self.visit_operands_dyn(&mut f);\n    }\n\n",
        );

        // Raw memory.
        for (doc, sig, mutable) in [
            (
                "    /// Visits each raw memory range the operation reads or writes, with its address and\n    /// size, reads before writes. Operations that reach memory only through\n    /// slices, frames, or the free memory pointer have none.\n",
                "pub fn visit_raw_memory(&self, mut f: impl FnMut(RawMemoryAccess, ValueId, RawMemorySize))",
                false,
            ),
            (
                "    /// Visits the offset and size operands of the raw memory ranges the operation\n    /// accesses, mutably.\n",
                "pub fn visit_raw_memory_operands_mut(&mut self, mut f: impl FnMut(&mut ValueId))",
                true,
            ),
        ] {
            write!(out, "{doc}    #[allow(unused_variables, unused_mut)]\n    {sig} {{\n        match self {{\n").unwrap();
            for op in ops {
                let Some(accesses) = &op.props.memory else { continue };
                writeln!(out, "            {} => {{", op.binder("Self")).unwrap();
                for access in accesses {
                    let call = if mutable {
                        match &access.size {
                            Size::Bytes(_) => format!("f({});", access.address),
                            Size::Field(size) => format!("f({}); f({size});", access.address),
                        }
                    } else {
                        let size = match &access.size {
                            Size::Bytes(bytes) => format!("RawMemorySize::Bytes({bytes})"),
                            Size::Field(size) => format!("RawMemorySize::Value(*{size})"),
                        };
                        format!(
                            "f(RawMemoryAccess::{}, *{}, {size});",
                            camel(access.kind),
                            access.address
                        )
                    };
                    match &access.guard {
                        Some(guard) => {
                            let pred =
                                lower_pred(guard, &OpFields { schema: self, op }, &self.enums)?;
                            writeln!(out, "                if {pred} {{ {call} }}").unwrap();
                        }
                        None => writeln!(out, "                {call}").unwrap(),
                    }
                }
                out.push_str("            }\n");
            }
            out.push_str("            _ => {}\n        }\n    }\n\n");
        }

        // Construction from operands.
        out.push_str(
            "    /// Returns the arity and constructor of an operation that is built\n    /// from value operands alone, by textual mnemonic.\n    #[must_use]\n    pub(crate) fn operand_only(mnemonic: &str) -> Option<(usize, fn(&[ValueId]) -> Self)> {\n        match mnemonic {\n",
        );
        for op in ops.iter().filter(|op| op.fields.iter().all(|f| f.ty.arity == Some(Arity::One))) {
            let args = (0..op.fields.len()).map(|i| format!("operands[{i}]")).collect::<Vec<_>>();
            let param = if args.is_empty() { "_" } else { "operands" };
            writeln!(
                out,
                "            {:?} => Some(({}, |{param}| {})),",
                op.mnemonic,
                op.fields.len(),
                op.construct("Self", &args)
            )
            .unwrap();
        }
        out.push_str("            _ => None,\n        }\n    }\n\n");

        // The rewrite view.
        out.push_str("    /// Returns the rewrite-rule view of this instruction.\n    #[must_use]\n    pub fn op(&self) -> Op {\n        match self {\n");
        for op in ops {
            let fields = op
                .fields
                .iter()
                .map(|f| format!("{0}: Operands::view({0})", f.decl.name))
                .collect::<Vec<_>>();
            writeln!(
                out,
                "            {} => Op::{}{},",
                op.binder("Self"),
                op.name,
                braces(&fields)
            )
            .unwrap();
        }
        out.push_str("        }\n    }\n\n");

        // Phase legality.
        out.push_str(
            "    /// Returns the diagnostic category of an operation that `phase` does not admit.\n    #[must_use]\n    #[allow(unused_variables)]\n    pub(crate) fn phase_violation(&self, phase: MirPhase, metadata: &InstructionMetadata) -> Option<PhaseCategory> {\n        let definition = self.op_def();\n        if !definition.phases.contains(phase) {\n            return definition.phase_category;\n        }\n        match self {\n",
        );
        for op in ops {
            let whens =
                op.whens.iter().filter(|when| when.pred.uses_metadata()).collect::<Vec<_>>();
            if whens.is_empty() {
                continue;
            }
            writeln!(out, "            {} => {{", op.binder("Self")).unwrap();
            for when in whens {
                let pred = lower_pred(&when.pred, &OpFields { schema: self, op }, &self.enums)?;
                let phases = match &when.props.until {
                    Some((name, span)) => format!(
                        "PhaseSet::through({})",
                        self.enum_variant("MirPhase", name, *span)?
                    ),
                    None => "definition.phases".to_string(),
                };
                let category = match &when.props.category {
                    Some((name, span)) => format!("Some({})", self.category(name, *span)?),
                    None => "definition.phase_category".to_string(),
                };
                writeln!(out, "                if {pred} && !{phases}.contains(phase) {{\n                    return {category};\n                }}").unwrap();
            }
            out.push_str("            }\n");
        }
        out.push_str("            _ => {}\n        }\n        None\n    }\n}\n\n");
        Ok(())
    }

    /// Builds an `OpDef` literal from properties, on top of `base` when given.
    fn op_def(&self, op: &Op, props: &Props, base: Option<&str>) -> Result<String> {
        let mut fields = Vec::new();
        if base.is_none() || props.mnemonic.is_some() {
            fields.push(format!(
                "mnemonic: {:?}",
                props.mnemonic.clone().unwrap_or_else(|| op.mnemonic.clone())
            ));
        }
        if base.is_none() {
            fields.push(format!("result: {}", op.result));
        }
        if base.is_none() || props.until.is_some() {
            let phases = match &props.until {
                Some((name, span)) => {
                    format!("PhaseSet::through({})", self.enum_variant("MirPhase", name, *span)?)
                }
                None => "PhaseSet::ALL".to_string(),
            };
            fields.push(format!("phases: {phases}"));
        }
        if base.is_none() || props.effect.is_some() {
            let effect = match &props.effect {
                Some((name, span)) => self.enum_variant("EffectKind", name, *span)?,
                None => "EffectKind::Pure".to_string(),
            };
            fields.push(format!("effect: {effect}"));
        }
        if base.is_none() {
            let mut traits = String::from("OpTraits::NONE");
            for TraitUse { name, span, .. } in props.traits.iter().filter(|t| t.guard.is_none()) {
                write!(traits, ".union({})", self.trait_const(name, *span)?).unwrap();
            }
            if props.commutative.is_some() {
                traits.push_str(".union(OpTraits::COMMUTATIVE)");
            }
            fields.push(format!("traits: {traits}"));
            fields.push(format!("has_side_effects: {}", props.side_effects.unwrap_or(false)));
        }
        if base.is_none() || props.category.is_some() {
            let category = match &props.category {
                Some((name, span)) => format!("Some({})", self.category(name, *span)?),
                None => "None".to_string(),
            };
            fields.push(format!("phase_category: {category}"));
        }
        if let Some(base) = base {
            fields.push(format!("..{base}"));
        }
        Ok(format!("OpDef {{ {} }}", fields.join(", ")))
    }

    /// Lowers a signature type; `value` is the expression of the operand it describes.
    fn lower_ty(&self, op: &Op, ty: &Ty, value: &str) -> Result<String> {
        Ok(match ty {
            Ty::Concrete(concrete) => self.lower_concrete(op, concrete)?,
            Ty::Else(pattern, default) => {
                let (pattern, guard) = match pattern {
                    TyPattern::Int => ("MirType::Int(_)".to_string(), String::new()),
                    TyPattern::Ptr(locations) | TyPattern::Slice(locations) => {
                        let ctor =
                            if matches!(pattern, TyPattern::Ptr(_)) { "Ptr" } else { "Slice" };
                        match locations.as_deref() {
                            None => (format!("MirType::{ctor}(_)"), String::new()),
                            Some([Location::Field(name, _)]) => (
                                format!("MirType::{ctor}(actual)"),
                                format!(" if actual == *{name}"),
                            ),
                            Some(locations) => {
                                let mut names = Vec::new();
                                for location in locations {
                                    let Location::Named(..) = location else {
                                        return Err(Error::new(
                                            op.span,
                                            "a location field cannot be an alternative",
                                        ));
                                    };
                                    names.push(self.location(op, location)?);
                                }
                                (format!("MirType::{ctor}({})", names.join(" | ")), String::new())
                            }
                        }
                    }
                };
                let default = self.lower_ty(op, default, value)?;
                format!(
                    "match func.value_ty({value}) {{ Some(ty @ {pattern}){guard} => ty, _ => {default} }}"
                )
            }
            Ty::Of(name, span) => {
                if op.field(name, *span)?.ty.arity.is_some() {
                    format!("ty_{name}")
                } else {
                    format!("AttributeType::mir_type({name})")
                }
            }
            Ty::If(pred, a, b) => format!(
                "if {} {{ {} }} else {{ {} }}",
                lower_pred(pred, &OpFields { schema: self, op }, &self.enums)?,
                self.lower_ty(op, a, value)?,
                self.lower_ty(op, b, value)?
            ),
            Ty::From(..) => {
                return Err(Error::new(op.span, "`from` must be a whole field signature"));
            }
        })
    }

    fn lower_concrete(&self, op: &Op, concrete: &Concrete) -> Result<String> {
        Ok(match concrete {
            Concrete::Int(bits) => format!("MirType::I{bits}"),
            Concrete::Ptr(location) => format!("MirType::Ptr({})", self.location(op, location)?),
            Concrete::Slice(location) => {
                format!("MirType::Slice({})", self.location(op, location)?)
            }
        })
    }
}

/// Whether a clause changes any `OpDef` field.
fn sets_op_def(props: &Props) -> bool {
    props.mnemonic.is_some()
        || props.until.is_some()
        || props.effect.is_some()
        || props.category.is_some()
}

fn lower_semantics(semantics: &SemanticsUse) -> String {
    if semantics.name == "opcode" {
        format!("Some(Semantics::Opcode({:?}, self.operands()))", semantics.args[0])
    } else {
        format!("Some({}({}))", semantics.name, semantics.args.join(", "))
    }
}

fn render_semantics(semantics: &SemanticsUse) -> String {
    format!("{}({})", semantics.name, semantics.args.join(", "))
}

/// Renders an operation's semantics declaration, folding per-location opcodes into one `match`.
fn declared_semantics(op: &Op) -> String {
    let clauses = op
        .whens
        .iter()
        .filter_map(|when| Some((&when.pred, when.props.semantics.as_ref()?)))
        .collect::<Vec<_>>();
    if clauses.is_empty() {
        return op.props.semantics.as_ref().map(render_semantics).unwrap_or_default();
    }
    let mut field = None;
    let mut arms = Vec::new();
    for (pred, semantics) in clauses {
        if let Pred::Is { field: name, alternatives, .. } = pred
            && let [Pattern::Variant { name: variant, args: None, .. }] = alternatives.as_slice()
            && semantics.name == "opcode"
            && field.get_or_insert(name) == &name
        {
            arms.push(format!("{variant} => {}", semantics.args[0]));
        } else {
            return format!(
                "{} (with clauses)",
                op.props.semantics.as_ref().map(render_semantics).unwrap_or_default()
            );
        }
    }
    format!("opcode(match {} {{ {} }})", field.unwrap(), arms.join(", "))
}

/// A resolved field.
#[derive(Clone, Debug)]
struct Field {
    decl: FieldDecl,
    ty: FieldType,
}

/// A resolved operation.
#[derive(Clone, Debug)]
struct Op {
    name: String,
    docs: Vec<String>,
    span: Span,
    tuple: bool,
    fields: Vec<Field>,
    result: String,
    mnemonic: String,
    isle: String,
    props: Props,
    whens: Vec<When>,
}

impl Op {
    fn field(&self, name: &str, span: Span) -> Result<&Field> {
        self.fields
            .iter()
            .find(|f| f.decl.name == name)
            .ok_or_else(|| Error::new(span, format!("`{}` has no field `{name}`", self.name)))
    }

    /// The pattern binding every field by name.
    fn binder(&self, path: &str) -> String {
        let names = self.fields.iter().map(|f| f.decl.name.clone()).collect::<Vec<_>>();
        if self.tuple {
            format!("{path}::{}{}", self.name, parens(&names))
        } else {
            format!("{path}::{}{}", self.name, braces(&names))
        }
    }

    /// The expression building the variant from field expressions in order.
    fn construct(&self, path: &str, args: &[String]) -> String {
        if self.tuple {
            format!("{path}::{}{}", self.name, parens(args))
        } else {
            let fields = self
                .fields
                .iter()
                .zip(args)
                .map(|(f, a)| format!("{}: {a}", f.decl.name))
                .collect::<Vec<_>>();
            format!("{path}::{}{}", self.name, braces(&fields))
        }
    }
}

fn parens(items: &[String]) -> String {
    if items.is_empty() { String::new() } else { format!("({})", items.join(", ")) }
}

fn braces(items: &[String]) -> String {
    if items.is_empty() { String::new() } else { format!(" {{ {} }}", items.join(", ")) }
}

/// Field lookups for predicates of one operation.
struct OpFields<'a> {
    schema: &'a Schema,
    op: &'a Op,
}

impl FieldTypes for OpFields<'_> {
    fn attribute_type(&self, field: &str, span: Span) -> Result<String> {
        let mut path = field.split('.');
        let field = self.op.field(path.next().unwrap(), span)?;
        if field.ty.arity.is_some() {
            return Err(Error::new(
                span,
                format!("`{}` is a value operand, not an attribute", field.decl.name),
            ));
        }
        let mut ty = &field.ty;
        let mut rust = ty.rust.clone();
        for name in path {
            let (_, inner) =
                ty.fields.iter().find(|(field, _)| field == name).ok_or_else(|| {
                    Error::new(span, format!("attribute `{rust}` declares no field `{name}`"))
                })?;
            rust.clone_from(inner);
            ty = self.schema.types.get(inner).unwrap_or(ty);
        }
        Ok(rust)
    }

    fn optional_operand(&self, field: &str, span: Span) -> Result<()> {
        if self.op.field(field, span)?.ty.arity != Some(Arity::Optional) {
            return Err(Error::new(span, format!("`{field}` is not an optional value operand")));
        }
        Ok(())
    }

    fn metadata(&self, name: &str, span: Span) -> Result<String> {
        if !self.schema.metadata.contains_key(name) {
            return Err(Error::new(span, format!("unknown metadata flag `{name}`")));
        }
        Ok(format!("metadata.{name}()"))
    }
}

fn emit_inst_kind(out: &mut String, ops: &[Op]) {
    out.push_str("/// The kind of a MIR instruction.\n#[derive(Clone, Debug, PartialEq, Eq, Hash)]\npub enum InstKind {\n");
    for op in ops {
        out.push_str(&doc_attrs(&op.docs, "    "));
        if op.fields.is_empty() {
            writeln!(out, "    {},", op.name).unwrap();
        } else if op.tuple {
            let tys = op.fields.iter().map(|f| f.ty.rust.clone()).collect::<Vec<_>>();
            writeln!(out, "    {}({}),", op.name, tys.join(", ")).unwrap();
        } else {
            writeln!(out, "    {} {{", op.name).unwrap();
            for field in &op.fields {
                out.push_str(&doc_attrs(&field.decl.docs, "        "));
                writeln!(out, "        {}: {},", field.decl.name, field.ty.rust).unwrap();
            }
            out.push_str("    },\n");
        }
    }
    out.push_str("}\n\n");
}

fn emit_builders(out: &mut String, ops: &[Op]) {
    out.push_str("impl crate::FunctionBuilder<'_> {\n");
    for op in ops {
        let Some((name, void, _)) = &op.props.builder else { continue };
        let name = name.as_deref().unwrap_or(&op.mnemonic);
        let params =
            op.fields.iter().map(|f| format!(", {}: ValueId", f.decl.name)).collect::<String>();
        let args = op.fields.iter().map(|f| f.decl.name.clone()).collect::<Vec<_>>();
        let kind = op.construct("InstKind", &args);
        writeln!(out, "    #[doc = \" Emits `{name}`.\"]").unwrap();
        if *void {
            writeln!(out, "    pub fn {name}(&mut self{params}) {{\n        // op(operands)\n        self.emit_void_inst({kind});\n    }}\n").unwrap();
        } else {
            writeln!(
                out,
                "    pub fn {name}(&mut self{params}) -> ValueId {{\n        let kind = {kind};\n        let ty = kind.op_def().result.default_type();\n        // %result = op(operands)\n        self.emit_inst(kind, ty)\n    }}\n"
            )
            .unwrap();
        }
    }
    out.push_str("}\n\n");
}

fn emit_op_view(out: &mut String, ops: &[Op]) {
    out.push_str(
        "/// Copyable view of an instruction for rewrite rules.\n///\n/// Value operands keep their identity, attributes are carried by value,\n/// and variable-length payloads are elided.\n#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]\npub enum Op {\n",
    );
    for op in ops {
        let fields = op
            .fields
            .iter()
            .map(|f| format!("{}: <{} as Operands>::View", f.decl.name, f.ty.rust))
            .collect::<Vec<_>>();
        writeln!(out, "    {}{},", op.name, braces(&fields)).unwrap();
    }
    out.push_str("}\n\nimpl Op {\n");
    let view_binder = |op: &Op| {
        let names = op.fields.iter().map(|f| f.decl.name.clone()).collect::<Vec<_>>();
        format!("Self::{}{}", op.name, braces(&names))
    };
    out.push_str("    /// Returns the declared result kind without rebuilding an instruction.\n    pub const fn result_kind(self) -> ResultKind {\n        match self {\n");
    for op in ops {
        let names = op.fields.iter().map(|f| format!("{}: _", f.decl.name)).collect::<Vec<_>>();
        writeln!(out, "            Self::{}{} => {},", op.name, braces(&names), op.result).unwrap();
    }
    out.push_str("        }\n    }\n\n");

    out.push_str("    /// Returns the declared commutative pair of operands, if the operation has one.\n    #[allow(unused_variables)]\n    pub fn commutative_pair(self) -> Option<(ValueId, ValueId)> {\n        match self {\n");
    for op in ops {
        let pair = op
            .props
            .commutative
            .as_ref()
            .map_or("None".to_string(), |(l, r, _)| format!("Some(({l}, {r}))"));
        writeln!(out, "            {} => {pair},", view_binder(op)).unwrap();
    }
    out.push_str("        }\n    }\n\n");

    out.push_str("    /// Returns the operation with its declared commutative pair exchanged,\n    /// when it has one and the two operands differ.\n    #[allow(unused_variables)]\n    pub fn commuted(self) -> Option<Self> {\n        match self {\n");
    for op in ops {
        match &op.props.commutative {
            Some((l, r, _)) => writeln!(
                out,
                "            {} => {{\n                // op(lhs, rhs, rest) -> op(rhs, lhs, rest)\n                let differs = {l} != {r};\n                let ({l}, {r}) = ({r}, {l});\n                differs.then_some({})\n            }}",
                view_binder(op),
                view_binder(op)
            )
            .unwrap(),
            None => writeln!(out, "            {} => None,", view_binder(op)).unwrap(),
        }
    }
    out.push_str("        }\n    }\n\n");

    out.push_str("    /// Orders only the declared commutative pair by the caller's key.\n    pub fn canonicalize_commutative_by_key<K: Ord>(self, mut key: impl FnMut(ValueId) -> K) -> Self {\n        match self {\n");
    for op in ops {
        let Some((l, r, _)) = &op.props.commutative else { continue };
        writeln!(
            out,
            "            {} => {{\n                // op(lhs, rhs, rest) -> op(lhs, rhs, rest), ordered by key\n                let ({l}, {r}) = if key({r}) < key({l}) {{ ({r}, {l}) }} else {{ ({l}, {r}) }};\n                {}\n            }}",
            view_binder(op),
            view_binder(op)
        )
        .unwrap();
    }
    out.push_str("            op => op,\n        }\n    }\n\n");

    out.push_str("    /// Every operation with its ISLE term, field names, and ISLE types, in declaration order.\n    const FIELDS: &'static [(&'static str, &'static str, &'static [(&'static str, &'static str)])] = &[\n");
    for op in ops {
        let fields = op
            .fields
            .iter()
            .map(|f| format!("({:?}, {:?})", f.decl.name, f.ty.isle.as_deref().unwrap_or("Unit")))
            .collect::<Vec<_>>();
        writeln!(out, "        ({:?}, {:?}, &[{}]),", op.name, op.isle, fields.join(", ")).unwrap();
    }
    out.push_str("    ];\n\n");

    out.push_str("    /// Rebuilds the instruction, unless a payload was elided from the view.\n    #[must_use]\n    pub fn into_kind(self) -> Option<InstKind> {\n        Some(match self {\n");
    for op in ops {
        let args = op
            .fields
            .iter()
            .map(|f| format!("<{} as Operands>::from_view({})?", f.ty.rust, f.decl.name))
            .collect::<Vec<_>>();
        writeln!(out, "            {} => {},", view_binder(op), op.construct("InstKind", &args))
            .unwrap();
    }
    out.push_str("        })\n    }\n\n");

    out.push_str("    /// Applies `f` to every value operand.\n    #[must_use]\n    #[allow(unused_mut)]\n    pub fn map_values(self, mut f: impl FnMut(ValueId) -> ValueId) -> Self {\n        match self {\n");
    for op in ops {
        let fields = op
            .fields
            .iter()
            .map(|field| {
                format!(
                    "{0}: <{1} as Operands>::map_view({0}, &mut f)",
                    field.decl.name, field.ty.rust
                )
            })
            .collect::<Vec<_>>();
        writeln!(out, "            {} => Self::{}{},", view_binder(op), op.name, braces(&fields))
            .unwrap();
    }
    out.push_str("        }\n    }\n}\n");
}

/// Overrides `props` with every property `child` sets; traits accumulate.
fn merge(props: &mut Props, child: &Props) {
    macro_rules! take {
        ($($field:ident),*) => {
            $( if child.$field.is_some() { props.$field.clone_from(&child.$field); } )*
        };
    }
    take!(
        mnemonic,
        isle,
        until,
        effect,
        side_effects,
        category,
        commutative,
        builder,
        memory,
        types,
        semantics
    );
    for item in &child.traits {
        if item.guard.is_some()
            || !props.traits.iter().any(|t| t.guard.is_none() && t.name == item.name)
        {
            props.traits.push(item.clone());
        }
    }
}

fn parse_decl(node: &Node, keyword: &str) -> Result<Decl> {
    let mut cur = node.cursor();
    cur.expect_word(keyword)?;
    let (name, _) = cur.ident()?;
    cur.finish()?;
    Ok(Decl { name: name.to_string(), docs: node.docs.clone() })
}

/// Parses a type name: an identifier with an optional `?` or `[]` suffix.
fn parse_type_name(cur: &mut Cursor<'_>) -> Result<(String, Span)> {
    let (name, span) = cur.ident()?;
    let mut name = name.to_string();
    if cur.eat("?") {
        name.push('?');
    } else if cur.eat("[") {
        cur.expect("]")?;
        name.push_str("[]");
    }
    Ok((name, span))
}

fn parse_field(cur: &mut Cursor<'_>, docs: Vec<String>) -> Result<FieldDecl> {
    let (name, span) = cur.ident()?;
    cur.expect(":")?;
    let (ty, _) = parse_type_name(cur)?;
    Ok(FieldDecl { name: name.to_string(), ty, docs, span })
}

fn parse_op(node: &Node, keyword: &str) -> Result<OpDecl> {
    let mut cur = node.cursor();
    cur.expect_word(keyword)?;
    let (name, _) = cur.ident()?;
    let base = if cur.eat(":") {
        let (base, span) = cur.ident()?;
        Some((base.to_string(), span))
    } else {
        None
    };
    let mut fields = if cur.eat("(") {
        Some((true, cur.comma_list(Some(")"), |cur| parse_field(cur, Vec::new()))?))
    } else {
        None
    };
    let result = if cur.eat("->") {
        let (result, span) = cur.ident()?;
        Some((result.to_string(), span))
    } else {
        None
    };
    cur.finish()?;
    let mut props = Props::default();
    let mut whens = Vec::new();
    for child in node.children() {
        let mut cur = child.cursor();
        if cur.eat_word("field") {
            let field = parse_field(&mut cur, child.docs.clone())?;
            cur.finish()?;
            match &mut fields {
                Some((false, list)) => list.push(field),
                Some((true, _)) => {
                    return Err(Error::new(child.span, "tuple operations cannot add `field`s"));
                }
                None => fields = Some((false, vec![field])),
            }
        } else if cur.eat_word("when") {
            let pred = parse_pred(&mut cur)?;
            cur.finish()?;
            let mut when_props = Props::default();
            for grandchild in child.children() {
                parse_prop(grandchild, &mut when_props)?;
            }
            whens.push(When { pred, props: when_props, span: child.span });
        } else {
            parse_prop(child, &mut props)?;
        }
    }
    Ok(OpDecl {
        name: name.to_string(),
        docs: node.docs.clone(),
        span: node.span,
        base,
        fields,
        result,
        props,
        whens,
        namespace: None,
    })
}

fn parse_prop(node: &Node, props: &mut Props) -> Result<()> {
    let mut cur = node.cursor();
    let (keyword, span) = cur.ident()?;
    let duplicate = || Error::new(span, format!("`{keyword}` is set twice"));
    match keyword {
        "mnemonic" => {
            if props.mnemonic.replace(cur.string()?.to_string()).is_some() {
                return Err(duplicate());
            }
        }
        "isle" => props.isle = Some(cur.ident()?.0.to_string()),
        "until" => props.until = Some(named(&mut cur)?),
        "effect" => props.effect = Some(named(&mut cur)?),
        "category" => props.category = Some(named(&mut cur)?),
        "traits" => {
            let traits = cur.comma_list(None, |cur| named(cur))?;
            let guard = if cur.eat_word("if") { Some(parse_pred(&mut cur)?) } else { None };
            props.traits.extend(traits.into_iter().map(|(name, span)| TraitUse {
                name,
                span,
                guard: guard.clone(),
            }));
        }
        "side_effects" => {
            props.side_effects =
                Some(if cur.eat_word("false") { false } else { cur.eat_word("true") || true });
        }
        "commutative" => {
            let (lhs, _) = cur.ident()?;
            cur.expect(",")?;
            let (rhs, _) = cur.ident()?;
            props.commutative = Some((lhs.to_string(), rhs.to_string(), span));
        }
        "builder" => {
            let name = match cur.peek_word() {
                Some("void") | None => None,
                Some(_) => Some(cur.ident()?.0.to_string()),
            };
            let void = cur.eat_word("void");
            props.builder = Some((name, void, span));
        }
        "memory" => props.memory = Some(cur.comma_list(None, parse_access)?),
        "types" => {
            props.types = Some(if cur.eat_word("delegated") {
                Types::Delegated
            } else {
                Types::Fields(cur.comma_list(None, |cur| {
                    let (name, span) = cur.ident()?;
                    cur.expect(":")?;
                    Ok((name.to_string(), parse_ty(cur)?, span))
                })?)
            });
        }
        "semantics" => {
            let (name, span) = cur.ident()?;
            cur.expect("(")?;
            let args = cur.comma_list(Some(")"), |cur| Ok(cur.ident()?.0.to_string()))?;
            props.semantics = Some(SemanticsUse { name: name.to_string(), args, span });
        }
        _ => return Err(Error::new(span, format!("unknown property `{keyword}`"))),
    }
    cur.finish()
}

fn named(cur: &mut Cursor<'_>) -> Result<(String, Span)> {
    let (name, span) = cur.ident()?;
    Ok((name.to_string(), span))
}

fn parse_access(cur: &mut Cursor<'_>) -> Result<Access> {
    let (kind, span) = cur.ident()?;
    let kind = match kind {
        "read" => "read",
        "write" => "write",
        "partial_write" => "partial_write",
        _ => {
            return Err(Error::new(
                span,
                format!("expected `read`, `write`, or `partial_write`, found `{kind}`"),
            ));
        }
    };
    cur.expect("(")?;
    let (address, _) = cur.ident()?;
    cur.expect(",")?;
    let size = match cur.peek() {
        Some(Token { tok: Tok::Int(bytes), .. }) => {
            let bytes = *bytes;
            cur.bump()?;
            Size::Bytes(bytes)
        }
        _ => Size::Field(cur.ident()?.0.to_string()),
    };
    cur.expect(")")?;
    let guard = if cur.eat_word("if") { Some(parse_pred(cur)?) } else { None };
    Ok(Access { kind, address: address.to_string(), size, guard, span })
}

fn parse_ty(cur: &mut Cursor<'_>) -> Result<Ty> {
    if cur.eat_word("if") {
        let pred = parse_pred(cur)?;
        cur.expect_word("then")?;
        let a = parse_ty(cur)?;
        cur.expect_word("else")?;
        let b = parse_ty(cur)?;
        return Ok(Ty::If(pred, Box::new(a), Box::new(b)));
    }
    let (word, span) = cur.ident()?;
    match word {
        "type" | "from" => {
            cur.expect("(")?;
            let (field, span) = cur.ident()?;
            cur.expect(")")?;
            return Ok(if word == "type" {
                Ty::Of(field.to_string(), span)
            } else {
                Ty::From(field.to_string(), span)
            });
        }
        "int" => {
            cur.expect_word("else")?;
            return Ok(Ty::Else(TyPattern::Int, Box::new(parse_ty(cur)?)));
        }
        "ptr" | "slice" => {
            let locations = if cur.eat("<") {
                let mut locations = vec![parse_location(cur)?];
                while cur.eat("|") {
                    locations.push(parse_location(cur)?);
                }
                cur.expect(">")?;
                Some(locations)
            } else {
                None
            };
            if cur.eat_word("else") {
                let pattern = if word == "ptr" {
                    TyPattern::Ptr(locations)
                } else {
                    TyPattern::Slice(locations)
                };
                return Ok(Ty::Else(pattern, Box::new(parse_ty(cur)?)));
            }
            let Some(mut locations) = locations.filter(|l| l.len() == 1) else {
                return Err(Error::new(span, format!("an exact `{word}` type names one location")));
            };
            let location = locations.pop().unwrap();
            return Ok(Ty::Concrete(if word == "ptr" {
                Concrete::Ptr(location)
            } else {
                Concrete::Slice(location)
            }));
        }
        _ => {}
    }
    if let Some(bits) = word.strip_prefix('i').and_then(|bits| bits.parse().ok())
        && matches!(bits, 1 | 160 | 256)
    {
        return Ok(Ty::Concrete(Concrete::Int(bits)));
    }
    Err(Error::new(span, format!("unknown type `{word}`")))
}

/// Parses a location: a `PtrLocation` in lower case, or `$field` for an attribute holding one.
fn parse_location(cur: &mut Cursor<'_>) -> Result<Location> {
    if cur.eat("#") {
        let (field, span) = cur.ident()?;
        return Ok(Location::Field(field.to_string(), span));
    }
    let (name, span) = cur.ident()?;
    Ok(Location::Named(name.to_string(), span))
}

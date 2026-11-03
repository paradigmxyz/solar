//! Schema pieces every dialect shares: host enum declarations, patterns, and predicates.

use crate::syntax::{Cursor, Error, Node, Result, Span, Tok, Token};
use std::{collections::BTreeMap, fmt::Write as _};

/// The shape of an enum variant, as patterns must spell it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    /// `Name`.
    Unit,
    /// `Name(..)`, with the payload enum of each position when the schema declares one.
    Tuple(Vec<Option<String>>),
    /// `Name { .. }`.
    Struct,
}

/// A declared variant of a host enum.
#[derive(Clone, Debug)]
pub struct Variant {
    pub name: String,
    pub shape: Shape,
}

/// A host Rust enum the schema refers to by variant name.
#[derive(Clone, Debug)]
pub struct EnumDecl {
    /// The Rust type, such as `PtrLocation`.
    pub rust: String,
    pub variants: Vec<Variant>,
    /// Whether the variants list every variant of the Rust enum, which generated code checks.
    pub exhaustive: bool,
    pub span: Span,
}

impl EnumDecl {
    /// Finds a variant by its Rust name, or by its `snake_case` spelling.
    pub fn variant(&self, name: &str) -> Option<&Variant> {
        let key = normalize(name);
        self.variants.iter().find(|variant| normalize(&variant.name) == key)
    }

    /// Resolves a variant name to its Rust path, or reports the variants the enum has.
    pub fn resolve(&self, name: &str, span: Span) -> Result<String> {
        let variant = self.variant(name).ok_or_else(|| {
            let names = self.variants.iter().map(|v| v.name.as_str()).collect::<Vec<_>>();
            Error::new(
                span,
                format!(
                    "`{}` has no variant `{name}`; it declares {}",
                    self.rust,
                    names.join(", ")
                ),
            )
        })?;
        Ok(format!("{}::{}", self.rust, variant.name))
    }

    /// Emits a compile-time check that an exhaustive declaration lists every Rust variant.
    pub fn exhaustiveness_check(&self) -> String {
        if !self.exhaustive {
            return String::new();
        }
        let arms = self
            .variants
            .iter()
            .map(|variant| {
                let suffix = match variant.shape {
                    Shape::Unit => "",
                    Shape::Tuple(_) => "(..)",
                    Shape::Struct => " { .. }",
                };
                format!("{}::{}{suffix}", self.rust, variant.name)
            })
            .collect::<Vec<_>>()
            .join(" | ");
        format!(
            "const _: () = {{\n    // The schema declares every variant of `{0}`.\n    #[allow(dead_code)]\n    fn exhaustive(value: &{0}) {{\n        match value {{\n            {arms} => {{}}\n        }}\n    }}\n}};\n",
            self.rust
        )
    }
}

/// Lower-cases a name and drops underscores, so `MemoryWrite` matches `memory_write`.
pub fn normalize(name: &str) -> String {
    name.chars().filter(|&c| c != '_').map(|c| c.to_ascii_lowercase()).collect()
}

/// Parses `Variant | Variant(Payload) | Variant {..} | ..`.
pub fn parse_variants(cur: &mut Cursor<'_>) -> Result<(Vec<Variant>, bool)> {
    let mut variants = Vec::new();
    let mut exhaustive = true;
    loop {
        if cur.eat("..") {
            exhaustive = false;
            break;
        }
        let (name, _) = cur.ident()?;
        let shape = if cur.eat("(") {
            Shape::Tuple(cur.comma_list(Some(")"), |cur| {
                if cur.eat_word("_") { Ok(None) } else { Ok(Some(cur.ident()?.0.to_string())) }
            })?)
        } else if cur.eat("{..}") {
            Shape::Struct
        } else {
            Shape::Unit
        };
        variants.push(Variant { name: name.to_string(), shape });
        if !cur.eat("|") {
            break;
        }
    }
    Ok((variants, exhaustive))
}

/// Parses an `enum Name: Variants;` declaration.
pub fn parse_enum(node: &Node, enums: &mut BTreeMap<String, EnumDecl>) -> Result<()> {
    let mut cur = node.cursor();
    cur.expect_word("enum")?;
    let (name, span) = cur.ident()?;
    cur.expect(":")?;
    let (variants, exhaustive) = parse_variants(&mut cur)?;
    cur.finish()?;
    insert_enum(enums, EnumDecl { rust: name.to_string(), variants, exhaustive, span })
}

/// Registers an enum declaration, rejecting duplicates.
pub fn insert_enum(enums: &mut BTreeMap<String, EnumDecl>, decl: EnumDecl) -> Result<()> {
    let span = decl.span;
    let name = decl.rust.clone();
    if enums.insert(name.clone(), decl).is_some() {
        return Err(Error::new(span, format!("enum `{name}` is declared twice")));
    }
    Ok(())
}

/// A pattern on a host enum value.
#[derive(Clone, Debug)]
pub enum Pattern {
    /// `_`.
    Wild,
    /// A variant, with optional payload patterns.
    Variant { name: String, args: Option<Vec<Vec<Self>>>, span: Span },
}

/// Parses `Pat (| Pat)*`.
pub fn parse_alternatives(cur: &mut Cursor<'_>) -> Result<Vec<Pattern>> {
    let mut alternatives = vec![parse_pattern(cur)?];
    while cur.eat("|") {
        alternatives.push(parse_pattern(cur)?);
    }
    Ok(alternatives)
}

fn parse_pattern(cur: &mut Cursor<'_>) -> Result<Pattern> {
    if cur.eat_word("_") {
        return Ok(Pattern::Wild);
    }
    let (name, span) = cur.ident()?;
    let args = if cur.eat("(") {
        Some(cur.comma_list(Some(")"), parse_alternatives)?)
    } else {
        cur.eat("{..}");
        None
    };
    Ok(Pattern::Variant { name: name.to_string(), args, span })
}

/// Lowers alternatives on a value of enum `ty` to a Rust pattern.
pub fn lower_alternatives(
    alternatives: &[Pattern],
    ty: &str,
    enums: &BTreeMap<String, EnumDecl>,
    span: Span,
) -> Result<String> {
    let decl = enums
        .get(ty)
        .ok_or_else(|| Error::new(span, format!("`{ty}` declares no variants to match")))?;
    let mut out = Vec::new();
    for pattern in alternatives {
        let Pattern::Variant { name, args, span } = pattern else {
            out.push("_".to_string());
            continue;
        };
        let path = decl.resolve(name, *span)?;
        let variant = decl.variant(name).unwrap();
        out.push(match (&variant.shape, args) {
            (Shape::Unit, None) => path,
            (Shape::Struct, None) => format!("{path} {{ .. }}"),
            (Shape::Tuple(_), None) => format!("{path}(..)"),
            (Shape::Tuple(payloads), Some(args)) if payloads.len() == args.len() => {
                let mut parts = Vec::new();
                for (payload, arg) in payloads.iter().zip(args) {
                    parts.push(match (payload, arg.as_slice()) {
                        (_, [Pattern::Wild]) => "_".to_string(),
                        (Some(payload), _) => lower_alternatives(arg, payload, enums, *span)?,
                        (None, _) => {
                            return Err(Error::new(
                                *span,
                                format!("the payload of `{name}` has no declared variants"),
                            ));
                        }
                    });
                }
                format!("{path}({})", parts.join(", "))
            }
            _ => {
                return Err(Error::new(
                    *span,
                    format!("pattern does not fit the shape of `{name}`"),
                ));
            }
        });
    }
    Ok(out.join(" | "))
}

/// A boolean condition on an instruction's attributes or metadata.
#[derive(Clone, Debug)]
pub enum Pred {
    /// `field is Pattern | Pattern`, where `field` may name a field of a struct attribute, as in
    /// `semantics.failure`.
    Is {
        field: String,
        alternatives: Vec<Pattern>,
        span: Span,
    },
    /// `field is absent`, for an optional value operand.
    Absent {
        field: String,
        span: Span,
    },
    /// `field`, a `bool` attribute.
    Flag {
        field: String,
        span: Span,
    },
    /// `field <op> integer`.
    Compare {
        field: String,
        op: &'static str,
        value: u64,
        span: Span,
    },
    /// `field == CONST` or `field != CONST`, an associated constant of the field's type.
    Const {
        field: String,
        negated: bool,
        name: String,
        span: Span,
    },
    /// `metadata.flag`.
    Metadata {
        name: String,
        span: Span,
    },
    Not(Box<Self>),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
}

impl Pred {
    /// Whether the predicate reads instruction metadata.
    pub fn uses_metadata(&self) -> bool {
        match self {
            Self::Metadata { .. } => true,
            Self::Not(inner) => inner.uses_metadata(),
            Self::And(a, b) | Self::Or(a, b) => a.uses_metadata() || b.uses_metadata(),
            _ => false,
        }
    }

    /// Whether the predicate only reads attribute fields with `is` and integer comparisons,
    /// which a `const fn` can evaluate.
    pub fn is_const(&self) -> bool {
        match self {
            Self::Is { .. } | Self::Absent { .. } | Self::Flag { .. } | Self::Compare { .. } => {
                true
            }
            Self::Const { .. } | Self::Metadata { .. } => false,
            Self::Not(inner) => inner.is_const(),
            Self::And(a, b) | Self::Or(a, b) => a.is_const() && b.is_const(),
        }
    }
}

/// Parses `a || b`, `a && b`, `!a`, `(a)`, and the atoms of [`Pred`].
pub fn parse_pred(cur: &mut Cursor<'_>) -> Result<Pred> {
    let mut lhs = parse_and(cur)?;
    while cur.eat("||") {
        lhs = Pred::Or(Box::new(lhs), Box::new(parse_and(cur)?));
    }
    Ok(lhs)
}

fn parse_and(cur: &mut Cursor<'_>) -> Result<Pred> {
    let mut lhs = parse_unary(cur)?;
    while cur.eat("&&") {
        lhs = Pred::And(Box::new(lhs), Box::new(parse_unary(cur)?));
    }
    Ok(lhs)
}

fn parse_unary(cur: &mut Cursor<'_>) -> Result<Pred> {
    if cur.eat("!") {
        return Ok(Pred::Not(Box::new(parse_unary(cur)?)));
    }
    if cur.eat("(") {
        let pred = parse_pred(cur)?;
        cur.expect(")")?;
        return Ok(pred);
    }
    let (field, span) = cur.ident()?;
    if field == "metadata" {
        cur.expect(".")?;
        let (name, span) = cur.ident()?;
        return Ok(Pred::Metadata { name: name.to_string(), span });
    }
    let mut field = field.to_string();
    while cur.eat(".") {
        write!(field, ".{}", cur.ident()?.0).unwrap();
    }
    if cur.eat_word("is") {
        if cur.eat_word("absent") {
            return Ok(Pred::Absent { field, span });
        }
        return Ok(Pred::Is { field, alternatives: parse_alternatives(cur)?, span });
    }
    let op = match cur.peek() {
        Some(Token { tok: Tok::Punct(op @ ("<" | "<=" | ">" | ">=" | "==" | "!=")), .. }) => *op,
        _ => return Ok(Pred::Flag { field, span }),
    };
    cur.bump()?;
    if let Some(Token { tok: Tok::Ident(name), .. }) = cur.peek()
        && matches!(op, "==" | "!=")
    {
        cur.bump()?;
        return Ok(Pred::Const { field, negated: op == "!=", name: name.clone(), span });
    }
    Ok(Pred::Compare { field, op, value: cur.int()?, span })
}

/// How generated code reaches the fields a predicate reads.
pub trait FieldTypes {
    /// The host type of an attribute field or of a field path into a struct attribute, or an
    /// error for unknown or value fields.
    fn attribute_type(&self, field: &str, span: Span) -> Result<String>;
    /// Checks that `field` is an optional value operand.
    fn optional_operand(&self, field: &str, span: Span) -> Result<()>;
    /// The Rust expression of a metadata flag.
    fn metadata(&self, name: &str, span: Span) -> Result<String>;
}

/// Lowers a predicate to a Rust boolean expression over fields bound by reference.
pub fn lower_pred(
    pred: &Pred,
    fields: &impl FieldTypes,
    enums: &BTreeMap<String, EnumDecl>,
) -> Result<String> {
    Ok(match pred {
        Pred::Is { field, alternatives, span } => {
            let ty = fields.attribute_type(field, *span)?;
            format!("matches!({field}, {})", lower_alternatives(alternatives, &ty, enums, *span)?)
        }
        Pred::Absent { field, span } => {
            fields.optional_operand(field, *span)?;
            format!("{field}.is_none()")
        }
        Pred::Flag { field, span } => {
            let ty = fields.attribute_type(field, *span)?;
            if ty != "bool" {
                return Err(Error::new(
                    *span,
                    format!(
                        "`{field}` is a `{ty}`, not a `bool`; test it with `is` or a comparison"
                    ),
                ));
            }
            place(field)
        }
        Pred::Compare { field, op, value, span } => {
            fields.attribute_type(field, *span)?;
            format!("{} {op} {value}", place(field))
        }
        Pred::Const { field, negated, name, span } => {
            let ty = fields.attribute_type(field, *span)?;
            format!("{} {} {ty}::{name}", place(field), if *negated { "!=" } else { "==" })
        }
        Pred::Metadata { name, span } => fields.metadata(name, *span)?,
        Pred::Not(inner) => format!("!({})", lower_pred(inner, fields, enums)?),
        Pred::And(a, b) => {
            format!("({} && {})", lower_pred(a, fields, enums)?, lower_pred(b, fields, enums)?)
        }
        Pred::Or(a, b) => {
            format!("({} || {})", lower_pred(a, fields, enums)?, lower_pred(b, fields, enums)?)
        }
    })
}

/// The value of a field bound by reference, or of a field path through it, which reads by value.
fn place(field: &str) -> String {
    if field.contains('.') { field.to_string() } else { format!("*{field}") }
}

/// Writes `/// docs` as doc attributes followed by `line`.
pub fn documented(out: &mut String, docs: &[String], indent: &str, line: &str) {
    out.push_str(&crate::syntax::doc_attrs(docs, indent));
    writeln!(out, "{indent}{line}").unwrap();
}

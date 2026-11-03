//! The EVM dialect: opcodes, logical stack operations, gas tiers, and word semantics.
//!
//! An opcode row names its byte, stack effect, traits, gas tier, availability, the bits its
//! operands and result use, the opcode that computes the same result with its operands swapped,
//! its immediate width, and, for pure opcodes, the word function it computes. A range row such as
//! `PUSH1..=PUSH32` declares a family whose properties are expressions in the suffix `n`. Gas
//! tiers declare their static price per fork, the warm price of an access, and the dynamic charge
//! per unit of sized work.

use crate::{
    common::{EnumDecl, normalize, parse_enum},
    syntax::{Cursor, Error, Node, Result, Span, Tok, Token, doc_attrs},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

/// An integer expression in constants, a family suffix, or tier parameters.
#[derive(Clone, Debug)]
enum Expr {
    Int(u64),
    Name(String, Span),
    Binary(Box<Self>, char, Box<Self>),
}

impl Expr {
    /// Evaluates with `n` bound, for opcode families.
    fn eval(&self, n: Option<u64>) -> Result<u64> {
        Ok(match self {
            Self::Int(value) => *value,
            Self::Name(name, span) => match (name.as_str(), n) {
                ("n", Some(n)) => n,
                _ => return Err(Error::new(*span, format!("`{name}` has no value here"))),
            },
            Self::Binary(a, op, b) => {
                let (a, b) = (a.eval(n)?, b.eval(n)?);
                match op {
                    '+' => a + b,
                    '-' => a.checked_sub(b).ok_or_else(|| {
                        Error::new(Span { file: 0, line: 0, col: 0 }, "negative value")
                    })?,
                    _ => a * b,
                }
            }
        })
    }

    /// Lowers to Rust with tier parameters, schema constants, and `static`.
    fn lower(&self, schema: &Schema, params: &[String]) -> Result<String> {
        Ok(match self {
            Self::Int(value) => value.to_string(),
            Self::Name(name, span) => {
                if params.contains(name) {
                    format!("{name} as u32")
                } else if name == "static" {
                    "self.gas(evm_version)".to_string()
                } else if schema.consts.iter().any(|c| c.name == *name) {
                    format!("Self::{name}")
                } else {
                    return Err(Error::new(*span, format!("unknown name `{name}`")));
                }
            }
            Self::Binary(a, op, b) => {
                format!("{} {op} {}", a.lower(schema, params)?, b.lower(schema, params)?)
            }
        })
    }
}

fn parse_expr(cur: &mut Cursor<'_>) -> Result<Expr> {
    let mut lhs = parse_term(cur)?;
    loop {
        let op = if cur.eat("+") {
            '+'
        } else if cur.eat("-") {
            '-'
        } else {
            return Ok(lhs);
        };
        lhs = Expr::Binary(Box::new(lhs), op, Box::new(parse_term(cur)?));
    }
}

fn parse_term(cur: &mut Cursor<'_>) -> Result<Expr> {
    let mut lhs = parse_atom(cur)?;
    while cur.eat("*") {
        lhs = Expr::Binary(Box::new(lhs), '*', Box::new(parse_atom(cur)?));
    }
    Ok(lhs)
}

fn parse_atom(cur: &mut Cursor<'_>) -> Result<Expr> {
    match cur.peek() {
        Some(Token { tok: Tok::Int(value), .. }) => {
            let value = *value;
            cur.bump()?;
            Ok(Expr::Int(value))
        }
        _ => {
            let (name, span) = cur.ident()?;
            Ok(Expr::Name(name.to_string(), span))
        }
    }
}

/// A word-semantics expression over operand names.
#[derive(Clone, Debug)]
enum Eval {
    Operand(String),
    Call(String, Vec<Self>, Span),
}

fn parse_eval(cur: &mut Cursor<'_>) -> Result<Eval> {
    let (name, span) = cur.ident()?;
    if cur.eat("(") {
        let args = cur.comma_list(Some(")"), parse_eval)?;
        Ok(Eval::Call(name.to_string(), args, span))
    } else {
        Ok(Eval::Operand(name.to_string()))
    }
}

#[derive(Clone, Debug)]
struct Decl {
    name: String,
    docs: Vec<String>,
}

#[derive(Clone, Debug)]
struct Const {
    name: String,
    docs: Vec<String>,
    value: u64,
}

#[derive(Clone, Debug)]
struct WordFn {
    params: usize,
    rust: String,
}

/// A value per fork: the first entry applies before every listed fork.
#[derive(Clone, Debug)]
struct Schedule {
    base: Expr,
    forks: Vec<(String, Expr, Span)>,
}

#[derive(Clone, Debug)]
struct Tier {
    name: String,
    docs: Vec<String>,
    params: Vec<(String, String)>,
    static_gas: Schedule,
    warm: Option<Expr>,
    dynamic: Option<(Schedule, String, Sizing)>,
    span: Span,
}

#[derive(Clone, Debug)]
enum Sizing {
    Arg(u64),
    Last,
}

#[derive(Clone, Debug)]
struct StackOpDecl {
    pattern: String,
    mnemonic: String,
    opcode: (String, Span),
    gas: (String, Span),
}

/// The stack signature of an opcode.
#[derive(Clone, Debug)]
enum Stack {
    /// An immediate decides the stack effect.
    Unknown,
    /// Named inputs, in pop order, with optional observed bit widths, and outputs.
    Named { inputs: Vec<(String, Option<u64>)>, outputs: Vec<(String, Option<u64>)> },
    /// Counts, as expressions in `n`.
    Counts(Expr, Expr),
}

#[derive(Clone, Debug)]
struct Opcode {
    docs: Vec<String>,
    /// The constant name, or the prefix of a family.
    name: String,
    /// The family's suffix range.
    range: Option<(u64, u64)>,
    byte: u64,
    span: Span,
    stack: Option<Stack>,
    mnemonic: Option<String>,
    traits: Vec<(String, Span)>,
    gas: Option<(String, Option<Expr>, Span)>,
    available: Option<(String, Option<String>, Span)>,
    swapped: Option<(String, Span)>,
    immediate: Option<Expr>,
    eval: Option<Eval>,
}

/// An EVM schema.
#[derive(Default)]
pub struct Schema {
    enums: BTreeMap<String, EnumDecl>,
    traits: Vec<Decl>,
    consts: Vec<Const>,
    tiers: Vec<Tier>,
    word_fns: BTreeMap<String, WordFn>,
    stack_ops: Vec<StackOpDecl>,
    opcodes: Vec<Opcode>,
}

/// One concrete opcode after expanding families.
struct Row {
    docs: Vec<String>,
    constant: String,
    byte: u8,
    n: Option<u64>,
    op: Opcode,
}

impl Schema {
    /// Adds the declarations of one file.
    pub fn add_file(&mut self, nodes: &[Node]) -> Result<()> {
        let mut nodes = nodes.iter();
        match nodes.next() {
            Some(node) => {
                let mut cur = node.cursor();
                cur.expect_word("dialect")?;
                let (name, span) = cur.ident()?;
                if name != "evm" {
                    return Err(Error::new(
                        span,
                        format!("expected the `evm` dialect, found `{name}`"),
                    ));
                }
                cur.finish()?;
            }
            None => return Ok(()),
        }
        for node in nodes {
            let mut cur = node.cursor();
            match cur.peek_word() {
                Some("enum") => parse_enum(node, &mut self.enums)?,
                Some("trait") => {
                    cur.bump()?;
                    let (name, _) = cur.ident()?;
                    cur.finish()?;
                    self.traits.push(Decl { name: name.to_string(), docs: node.docs.clone() });
                }
                Some("const") => {
                    cur.bump()?;
                    let (name, _) = cur.ident()?;
                    cur.expect("=")?;
                    let value = cur.int()?;
                    cur.finish()?;
                    self.consts.push(Const {
                        name: name.to_string(),
                        docs: node.docs.clone(),
                        value,
                    });
                }
                Some("word_fn") => {
                    cur.bump()?;
                    let (name, _) = cur.ident()?;
                    cur.expect("(")?;
                    let params = cur.comma_list(Some(")"), |cur| cur.ident().map(drop))?.len();
                    let rust = if cur.eat("=") {
                        cur.string()?.to_string()
                    } else {
                        format!("word::{name}")
                    };
                    cur.finish()?;
                    self.word_fns.insert(name.to_string(), WordFn { params, rust });
                }
                Some("gas_tier") => self.tiers.push(parse_tier(node)?),
                Some("stack_op") => self.stack_ops.push(parse_stack_op(node)?),
                Some("opcode") => self.opcodes.push(parse_opcode(node)?),
                _ => return Err(cur.unexpected("a declaration")),
            }
        }
        Ok(())
    }

    fn enum_variant(&self, ty: &str, name: &str, span: Span) -> Result<String> {
        self.enums
            .get(ty)
            .ok_or_else(|| Error::new(span, format!("the schema must declare `enum {ty}`")))?
            .resolve(name, span)
    }

    fn trait_const(&self, name: &str, span: Span) -> Result<String> {
        self.traits
            .iter()
            .find(|t| t.name == name)
            .map(|t| format!("OpcodeTraits::{}", t.name.to_ascii_uppercase()))
            .ok_or_else(|| Error::new(span, format!("unknown trait `{name}`")))
    }

    fn tier(&self, name: &str, arg: Option<&Expr>, n: Option<u64>, span: Span) -> Result<String> {
        let tier = self
            .tiers
            .iter()
            .find(|t| t.name == name)
            .ok_or_else(|| Error::new(span, format!("unknown gas tier `{name}`")))?;
        match (tier.params.len(), arg) {
            (0, None) => Ok(format!("GasTier::{name}")),
            (1, Some(arg)) => Ok(format!("GasTier::{name}({})", arg.eval(n)?)),
            _ => Err(Error::new(
                span,
                format!("gas tier `{name}` takes {} parameters", tier.params.len()),
            )),
        }
    }

    /// Checks the schema and emits the generated Rust modules: opcodes, and gas tiers.
    pub fn generate(&self) -> Result<(String, String)> {
        let rows = self.rows()?;
        let mut out = String::from(
            "// @generated by solar-op-dsl from the EVM opcode schema; do not edit.\n\n",
        );
        for decl in self.enums.values() {
            out.push_str(&decl.exhaustiveness_check());
        }
        out.push_str("impl OpcodeTraits {\n");
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
        for row in &rows {
            writeln!(out, "#[doc = \" Opcode byte for `{0}`.\"]\n#[allow(dead_code)]\npub const {0}: u8 = {1:#04x};", row.constant, row.byte).unwrap();
        }
        let constants = rows.iter().map(|row| row.constant.as_str()).collect::<BTreeSet<_>>();

        out.push_str("\n/// Maps each opcode byte to its generated schema definition.\nstatic OPCODE_DEFS: [Option<OpDef>; 256] = {\n    let mut map = [None; 256];\n");
        for row in &rows {
            let op = &row.op;
            let (stack_io, input_bits, result_bits) = match op
                .stack
                .as_ref()
                .ok_or_else(|| Error::new(op.span, "opcode declares no stack effect"))?
            {
                Stack::Unknown => ("None".to_string(), Vec::new(), 256),
                Stack::Counts(inputs, outputs) => (
                    format!("Some(({}, {}))", inputs.eval(row.n)?, outputs.eval(row.n)?),
                    Vec::new(),
                    256,
                ),
                Stack::Named { inputs, outputs } => {
                    let last =
                        inputs.iter().rposition(|(_, bits)| bits.is_some()).map_or(0, |i| i + 1);
                    let input_bits =
                        inputs[..last].iter().map(|(_, bits)| bits.unwrap_or(256)).collect();
                    let result_bits = match outputs.as_slice() {
                        [] => 256,
                        [(_, bits)] => bits.unwrap_or(256),
                        _ => {
                            return Err(Error::new(
                                op.span,
                                "an opcode produces at most one named result",
                            ));
                        }
                    };
                    (
                        format!("Some(({}, {}))", inputs.len(), outputs.len()),
                        input_bits,
                        result_bits,
                    )
                }
            };
            let mut traits = String::from("OpcodeTraits::NONE");
            for (name, span) in &op.traits {
                write!(traits, ".union({})", self.trait_const(name, *span)?).unwrap();
            }
            let gas = match &op.gas {
                Some((name, arg, span)) => self.tier(name, arg.as_ref(), row.n, *span)?,
                None => return Err(Error::new(op.span, "opcode declares no gas tier")),
            };
            let availability = match &op.available {
                None => "Availability::Legacy".to_string(),
                Some((name, Some(version), span)) => {
                    format!(
                        "{}({})",
                        self.enum_variant("Availability", name, *span)?,
                        self.enum_variant("EvmVersion", version, *span)?
                    )
                }
                Some((name, None, span)) => self.enum_variant("Availability", name, *span)?,
            };
            let commutative = op.traits.iter().any(|(name, _)| name == "commutative");
            let swapped = match &op.swapped {
                Some((name, span)) => {
                    if !constants.contains(name.as_str()) {
                        return Err(Error::new(*span, format!("unknown opcode `{name}`")));
                    }
                    if commutative {
                        return Err(Error::new(*span, "a commutative opcode swaps to itself"));
                    }
                    format!("Some({name})")
                }
                None if commutative => format!("Some({})", row.constant),
                None => "None".to_string(),
            };
            let immediate = op.immediate.as_ref().map_or(Ok(0), |expr| expr.eval(row.n))?;
            let mnemonic = op.mnemonic.clone().unwrap_or_else(|| row.constant.to_ascii_lowercase());
            writeln!(
                out,
                "    map[{byte} as usize] = Some(OpDef {{ opcode: {byte}, mnemonic: {mnemonic:?}, stack_io: {stack_io}, traits: {traits}, result_bits: {result_bits}, input_bits: &{input_bits:?}, gas: {gas}, availability: {availability}, swapped: {swapped}, immediate: {immediate} }});",
                byte = row.constant,
            )
            .unwrap();
        }
        out.push_str("    map\n};\n\n");

        out.push_str("/// Returns the opcode for a canonical mnemonic.\n#[must_use]\npub fn from_mnemonic(mnemonic: &str) -> Option<u8> {\n    match mnemonic {\n");
        for row in &rows {
            let mnemonic =
                row.op.mnemonic.clone().unwrap_or_else(|| row.constant.to_ascii_lowercase());
            writeln!(out, "        {mnemonic:?} => Some({}),", row.constant).unwrap();
        }
        out.push_str("        _ => None,\n    }\n}\n\n");

        out.push_str("/// Evaluates a pure EVM opcode with concrete operands in pop order.\npub fn eval_opcode(opcode: u8, operands: &[U256]) -> Option<U256> {\n    Some(match (opcode, operands) {\n");
        for row in &rows {
            let Some(eval) = &row.op.eval else { continue };
            let Some(Stack::Named { inputs, .. }) = &row.op.stack else {
                return Err(Error::new(row.op.span, "`eval` needs named stack inputs"));
            };
            if !row.op.traits.iter().any(|(name, _)| name == "pure") {
                return Err(Error::new(row.op.span, "only `pure` opcodes have word semantics"));
            }
            let names = inputs.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>();
            writeln!(
                out,
                "        ({}, &[{}]) => {},",
                row.constant,
                names.join(", "),
                self.lower_eval(eval, &names)?
            )
            .unwrap();
        }
        out.push_str("        _ => return None,\n    })\n}\n\n");

        self.emit_stack_ops(&mut out, &constants)?;
        let mut gas = String::from(
            "// @generated by solar-op-dsl from the EVM opcode schema; do not edit.\n\n",
        );
        self.emit_tiers(&mut gas)?;
        Ok((out, gas))
    }

    fn lower_eval(&self, eval: &Eval, names: &[String]) -> Result<String> {
        Ok(match eval {
            Eval::Operand(name) if names.contains(name) => name.clone(),
            Eval::Operand(name) => {
                return Err(Error::new(
                    Span { file: 0, line: 0, col: 0 },
                    format!("unknown operand `{name}`"),
                ));
            }
            Eval::Call(name, args, span) => {
                let word_fn = self
                    .word_fns
                    .get(name)
                    .ok_or_else(|| Error::new(*span, format!("unknown word function `{name}`")))?;
                if word_fn.params != args.len() {
                    return Err(Error::new(
                        *span,
                        format!("`{name}` takes {} operands", word_fn.params),
                    ));
                }
                let args = args
                    .iter()
                    .map(|arg| self.lower_eval(arg, names))
                    .collect::<Result<Vec<_>>>()?;
                format!("{}({})", word_fn.rust, args.join(", "))
            }
        })
    }

    /// Expands families and checks byte order and uniqueness.
    fn rows(&self) -> Result<Vec<Row>> {
        let mut rows = Vec::new();
        for op in &self.opcodes {
            match op.range {
                None => rows.push(Row {
                    docs: op.docs.clone(),
                    constant: op.name.clone(),
                    byte: byte(op.byte, op.span)?,
                    n: None,
                    op: op.clone(),
                }),
                Some((first, last)) => {
                    for n in first..=last {
                        rows.push(Row {
                            docs: op.docs.clone(),
                            constant: format!("{}{n}", op.name),
                            byte: byte(op.byte + n - first, op.span)?,
                            n: Some(n),
                            op: op.clone(),
                        });
                    }
                }
            }
        }
        let mut prev = None;
        for row in &rows {
            if prev.is_some_and(|prev| row.byte <= prev) {
                return Err(Error::new(
                    row.op.span,
                    format!("opcode `{}` is not in ascending byte order", row.constant),
                ));
            }
            prev = Some(row.byte);
            let _ = &row.docs;
        }
        Ok(rows)
    }

    fn emit_stack_ops(&self, out: &mut String, constants: &BTreeSet<&str>) -> Result<()> {
        let decl = self.enums.get("StackOp").ok_or_else(|| {
            Error::new(Span { file: 0, line: 1, col: 1 }, "the schema must declare `enum StackOp`")
        })?;
        out.push_str("impl StackOp {\n    /// Returns the generated definition for this logical operation.\n    #[must_use]\n    pub const fn definition(self) -> StackOpDef {\n        match self {\n");
        for op in &self.stack_ops {
            let path = decl.resolve(&op.pattern, op.opcode.1)?;
            let suffix = match &decl.variant(&op.pattern).unwrap().shape {
                crate::common::Shape::Unit => "",
                crate::common::Shape::Tuple(_) => "(..)",
                crate::common::Shape::Struct => " { .. }",
            };
            if !constants.contains(op.opcode.0.as_str()) {
                return Err(Error::new(op.opcode.1, format!("unknown opcode `{}`", op.opcode.0)));
            }
            let gas = self.tier(&op.gas.0, None, None, op.gas.1)?;
            writeln!(
                out,
                "            {path}{suffix} => StackOpDef {{ mnemonic: {:?}, ir_opcode: {}, gas: {gas} }},",
                op.mnemonic, op.opcode.0
            )
            .unwrap();
        }
        out.push_str("        }\n    }\n}\n\n");
        Ok(())
    }

    fn emit_tiers(&self, out: &mut String) -> Result<()> {
        out.push_str("/// The gas class of an opcode in the fork schedule.\n#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]\npub enum GasTier {\n");
        for tier in &self.tiers {
            out.push_str(&doc_attrs(&tier.docs, "    "));
            let params = tier.params.iter().map(|(_, ty)| ty.clone()).collect::<Vec<_>>();
            if params.is_empty() {
                writeln!(out, "    {},", tier.name).unwrap();
            } else {
                writeln!(out, "    {}({}),", tier.name, params.join(", ")).unwrap();
            }
        }
        out.push_str("}\n\nimpl GasTier {\n");
        for c in &self.consts {
            out.push_str(&doc_attrs(&c.docs, "    "));
            writeln!(out, "    pub const {}: u32 = {};", c.name, c.value).unwrap();
        }
        let pattern = |tier: &Tier| {
            if tier.params.is_empty() {
                format!("Self::{}", tier.name)
            } else {
                let names = tier.params.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>();
                format!("Self::{}({})", tier.name, names.join(", "))
            }
        };
        let wild = |tier: &Tier| {
            if tier.params.is_empty() {
                format!("Self::{}", tier.name)
            } else {
                format!("Self::{}(_)", tier.name)
            }
        };
        let params =
            |tier: &Tier| tier.params.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>();

        out.push_str("\n    /// Gas charged on `evm_version` before dynamic components.\n    pub const fn gas(self, evm_version: EvmVersion) -> u32 {\n        match self {\n");
        for tier in &self.tiers {
            writeln!(
                out,
                "            {} => {},",
                pattern(tier),
                self.schedule(&tier.static_gas, &params(tier))?
            )
            .unwrap();
        }
        out.push_str("        }\n    }\n\n");

        out.push_str("    /// Gas charged on `evm_version` before dynamic components for an access of\n    /// the given warmth. Warmth only reprices account and storage accesses,\n    /// and only since Berlin.\n    pub const fn gas_at(self, evm_version: EvmVersion, warmth: Warmth) -> u32 {\n        if !matches!(warmth, Warmth::Warm) || !since(evm_version, EvmVersion::Berlin) {\n            return self.gas(evm_version);\n        }\n        match self {\n");
        for tier in self.tiers.iter().filter(|tier| tier.warm.is_some()) {
            writeln!(
                out,
                "            {} => {},",
                wild(tier),
                tier.warm.as_ref().unwrap().lower(self, &[])?
            )
            .unwrap();
        }
        out.push_str("            _ => self.gas(evm_version),\n        }\n    }\n\n");

        out.push_str("    /// Gas charged per unit of runtime-sized work: exponent bytes, hashed or\n    /// copied words, and logged bytes.\n    pub const fn dynamic_gas(self, evm_version: EvmVersion) -> u32 {\n        match self {\n");
        for tier in &self.tiers {
            if let Some((schedule, ..)) = &tier.dynamic {
                writeln!(out, "            {} => {},", wild(tier), self.schedule(schedule, &[])?)
                    .unwrap();
            }
        }
        out.push_str("            _ => 0,\n        }\n    }\n\n");

        out.push_str("    /// The stack argument that sizes the dynamic work: the exponent of\n    /// `EXP`, the byte length of a hash or a log, the last argument of a copy.\n    pub const fn sizing_argument(self, arguments: usize) -> Option<usize> {\n        match self {\n");
        for tier in &self.tiers {
            if let Some((_, _, sizing)) = &tier.dynamic {
                let index = match sizing {
                    Sizing::Arg(index) => format!("Some({index})"),
                    Sizing::Last => "arguments.checked_sub(1)".to_string(),
                };
                writeln!(out, "            {} => {index},", wild(tier)).unwrap();
            }
        }
        out.push_str("            _ => None,\n        }\n    }\n\n");

        out.push_str("    /// Units of dynamic work for `arguments`, in stack order with immediates\n    /// resolved: exponent bytes, hashed or copied words, or logged bytes. One\n    /// unit when the size is not known statically.\n    pub fn dynamic_units(self, arguments: &[Option<U256>]) -> u32 {\n        let Some(size) = self.sizing_argument(arguments.len()).and_then(|index| arguments[index])\n        else {\n            return u32::from(self.sizing_argument(arguments.len()).is_some());\n        };\n        let units = match self {\n");
        for tier in &self.tiers {
            if let Some((_, unit, _)) = &tier.dynamic {
                let units = match unit.as_str() {
                    "byte" => "size",
                    "word" => "size.div_ceil(U256::from(32))",
                    "byte_len" => "U256::from(size.bit_len().div_ceil(8))",
                    _ => {
                        return Err(Error::new(
                            tier.span,
                            format!(
                                "unknown unit `{unit}`; expected `byte`, `word`, or `byte_len`"
                            ),
                        ));
                    }
                };
                writeln!(out, "            {} => {units},", wild(tier)).unwrap();
            }
        }
        out.push_str("            _ => U256::ZERO,\n        };\n        units.saturating_to::<u32>()\n    }\n\n");

        out.push_str("    /// Gas of a tier priced the same on every EVM version, usable in constants.\n    pub const fn fixed_gas(self) -> u32 {\n        match self {\n");
        let forked = self
            .tiers
            .iter()
            .filter(|tier| !tier.static_gas.forks.is_empty())
            .map(wild)
            .collect::<Vec<_>>();
        if !forked.is_empty() {
            writeln!(
                out,
                "            {} => panic!(\"tier is priced per EVM version\"),",
                forked.join("\n            | ")
            )
            .unwrap();
        }
        out.push_str("            _ => self.gas(EvmVersion::Homestead),\n        }\n    }\n}\n");
        Ok(())
    }

    fn schedule(&self, schedule: &Schedule, params: &[String]) -> Result<String> {
        let mut out = String::new();
        for (fork, value, span) in schedule.forks.iter().rev() {
            let fork = self.enum_variant("EvmVersion", fork, *span)?;
            write!(out, "if since(evm_version, {fork}) {{ {} }} else ", value.lower(self, params)?)
                .unwrap();
        }
        if out.is_empty() {
            schedule.base.lower(self, params)
        } else {
            Ok(format!("{out}{{ {} }}", schedule.base.lower(self, params)?))
        }
    }
}

fn byte(value: u64, span: Span) -> Result<u8> {
    u8::try_from(value)
        .map_err(|_| Error::new(span, format!("opcode byte {value:#x} does not fit in a byte")))
}

/// Parses `base, Fork: value, Fork: value`.
fn parse_schedule(cur: &mut Cursor<'_>) -> Result<Schedule> {
    let base = parse_expr(cur)?;
    let mut forks = Vec::new();
    while cur.eat(",") {
        let (fork, span) = cur.ident()?;
        cur.expect(":")?;
        forks.push((fork.to_string(), parse_expr(cur)?, span));
    }
    Ok(Schedule { base, forks })
}

fn parse_tier(node: &Node) -> Result<Tier> {
    let mut cur = node.cursor();
    cur.expect_word("gas_tier")?;
    let (name, span) = cur.ident()?;
    let params = if cur.eat("(") {
        cur.comma_list(Some(")"), |cur| {
            let (name, _) = cur.ident()?;
            cur.expect(":")?;
            let (ty, _) = cur.ident()?;
            Ok((name.to_string(), ty.to_string()))
        })?
    } else {
        Vec::new()
    };
    cur.finish()?;
    let mut static_gas = None;
    let mut warm = None;
    let mut dynamic = None;
    for child in node.children() {
        let mut cur = child.cursor();
        let (keyword, span) = cur.ident()?;
        match keyword {
            "static" => static_gas = Some(parse_schedule(&mut cur)?),
            "warm" => warm = Some(parse_expr(&mut cur)?),
            "dynamic" => {
                let schedule = parse_schedule(&mut cur)?;
                cur.expect_word("per")?;
                let (unit, _) = cur.ident()?;
                cur.expect_word("of")?;
                let sizing =
                    if cur.eat_word("last") { Sizing::Last } else { Sizing::Arg(cur.int()?) };
                dynamic = Some((schedule, unit.to_string(), sizing));
            }
            _ => return Err(Error::new(span, format!("unknown gas tier property `{keyword}`"))),
        }
        cur.finish()?;
    }
    let static_gas =
        static_gas.ok_or_else(|| Error::new(span, "gas tier declares no `static` price"))?;
    Ok(Tier {
        name: name.to_string(),
        docs: node.docs.clone(),
        params,
        static_gas,
        warm,
        dynamic,
        span,
    })
}

fn parse_stack_op(node: &Node) -> Result<StackOpDecl> {
    let mut cur = node.cursor();
    cur.expect_word("stack_op")?;
    let (pattern, _) = cur.ident()?;
    cur.finish()?;
    let (mut mnemonic, mut opcode, mut gas) = (None, None, None);
    for child in node.children() {
        let mut cur = child.cursor();
        match cur.ident()? {
            ("mnemonic", _) => mnemonic = Some(cur.string()?.to_string()),
            ("opcode", _) => opcode = Some(cur.ident().map(|(n, s)| (n.to_string(), s))?),
            ("gas", _) => gas = Some(cur.ident().map(|(n, s)| (n.to_string(), s))?),
            (other, span) => {
                return Err(Error::new(
                    span,
                    format!("unknown stack operation property `{other}`"),
                ));
            }
        }
        cur.finish()?;
    }
    let missing =
        |what: &str| Error::new(node.span, format!("stack operation declares no `{what}`"));
    Ok(StackOpDecl {
        pattern: pattern.to_string(),
        mnemonic: mnemonic.ok_or_else(|| missing("mnemonic"))?,
        opcode: opcode.ok_or_else(|| missing("opcode"))?,
        gas: gas.ok_or_else(|| missing("gas"))?,
    })
}

/// Splits `PUSH32` into `PUSH` and 32.
fn split_suffix(name: &str, span: Span) -> Result<(String, u64)> {
    let digits = name.len() - name.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 {
        return Err(Error::new(span, format!("family bound `{name}` has no numeric suffix")));
    }
    let (prefix, suffix) = name.split_at(name.len() - digits);
    Ok((prefix.to_string(), suffix.parse().unwrap()))
}

/// Named stack slots with the bits each one holds, or `None` for `(..)`.
type StackList = Option<Vec<(String, Option<u64>)>>;

fn parse_stack_list(cur: &mut Cursor<'_>) -> Result<StackList> {
    cur.expect("(")?;
    if cur.eat("..") {
        cur.expect(")")?;
        return Ok(None);
    }
    Ok(Some(cur.comma_list(Some(")"), |cur| {
        let (name, _) = cur.ident()?;
        let bits = if cur.eat(":") { Some(cur.int()?) } else { None };
        Ok((name.to_string(), bits))
    })?))
}

fn parse_opcode(node: &Node) -> Result<Opcode> {
    let mut cur = node.cursor();
    cur.expect_word("opcode")?;
    let (name, span) = cur.ident()?;
    let (name, range) = if cur.eat("..=") {
        let (last, last_span) = cur.ident()?;
        let (prefix, first) = split_suffix(name, span)?;
        let (last_prefix, last) = split_suffix(last, last_span)?;
        if prefix != last_prefix || last < first {
            return Err(Error::new(
                last_span,
                "a family range needs one prefix and ascending suffixes",
            ));
        }
        (prefix, Some((first, last)))
    } else {
        (name.to_string(), None)
    };
    cur.expect("=")?;
    let byte = cur.int()?;
    let stack = if matches!(cur.peek(), Some(Token { tok: Tok::Punct("("), .. })) {
        let inputs = parse_stack_list(&mut cur)?;
        cur.expect("->")?;
        let outputs = parse_stack_list(&mut cur)?;
        Some(match (inputs, outputs) {
            (Some(inputs), Some(outputs)) => Stack::Named { inputs, outputs },
            (None, None) => Stack::Unknown,
            _ => return Err(Error::new(span, "an unknown stack effect is `(..) -> (..)`")),
        })
    } else {
        None
    };
    cur.finish()?;
    let mut op = Opcode {
        docs: node.docs.clone(),
        name,
        range,
        byte,
        span,
        stack,
        mnemonic: None,
        traits: Vec::new(),
        gas: None,
        available: None,
        swapped: None,
        immediate: None,
        eval: None,
    };
    for child in node.children() {
        let mut cur = child.cursor();
        let (keyword, span) = cur.ident()?;
        match keyword {
            "stack" => {
                let inputs = parse_expr(&mut cur)?;
                cur.expect("->")?;
                op.stack = Some(Stack::Counts(inputs, parse_expr(&mut cur)?));
            }
            "mnemonic" => op.mnemonic = Some(cur.string()?.to_string()),
            "traits" => {
                op.traits =
                    cur.comma_list(None, |cur| cur.ident().map(|(n, s)| (n.to_string(), s)))?
            }
            "gas" => {
                let (tier, span) = cur.ident()?;
                let arg = if cur.eat("(") {
                    let arg = parse_expr(&mut cur)?;
                    cur.expect(")")?;
                    Some(arg)
                } else {
                    None
                };
                op.gas = Some((tier.to_string(), arg, span));
            }
            "available" => {
                let (gate, span) = cur.ident()?;
                let version = if normalize(gate) == "since" {
                    Some(cur.ident()?.0.to_string())
                } else {
                    None
                };
                op.available = Some((gate.to_string(), version, span));
            }
            "swapped" => op.swapped = Some(cur.ident().map(|(n, s)| (n.to_string(), s))?),
            "immediate" => op.immediate = Some(parse_expr(&mut cur)?),
            "eval" => op.eval = Some(parse_eval(&mut cur)?),
            _ => return Err(Error::new(span, format!("unknown opcode property `{keyword}`"))),
        }
        cur.finish()?;
    }
    Ok(op)
}

//! A grammar-based generator of Solidity contracts in the subset every leg of the chain runs, with
//! random call sequences.
//!
//! Programs are kept as a typed AST so that a divergence can be reduced by rewriting the tree:
//! dropping functions and statements, and replacing expressions by literals of their type, which
//! always yields a program that type-checks.

use alloy_primitives::U256;
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};
use std::fmt::Write;

/// A small deterministic random number generator (SplitMix64).
#[derive(Clone, Debug)]
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed ^ 0x9e37_79b9_7f4a_7c15)
    }

    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    pub(crate) fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }

    pub(crate) fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }

    pub(crate) fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    pub(crate) fn word(&mut self) -> U256 {
        U256::from_limbs([self.next(), self.next(), self.next(), self.next()])
    }
}

/// A value type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum Ty {
    U(u16),
    I(u16),
    Bool,
    Addr,
}

impl Ty {
    pub(crate) fn sol(self) -> String {
        match self {
            Self::U(n) => format!("uint{n}"),
            Self::I(n) => format!("int{n}"),
            Self::Bool => "bool".into(),
            Self::Addr => "address".into(),
        }
    }

    pub(crate) fn is_int(self) -> bool {
        matches!(self, Self::U(_) | Self::I(_))
    }

    pub(crate) fn bits(self) -> u16 {
        match self {
            Self::U(n) | Self::I(n) => n,
            Self::Bool => 8,
            Self::Addr => 160,
        }
    }

    /// The type's range as math integers.
    pub(crate) fn range(self) -> (BigInt, BigInt) {
        let one = BigInt::from(1);
        match self {
            Self::U(n) => (BigInt::from(0), (&one << n as usize) - 1),
            Self::I(n) => (-(&one << (n as usize - 1)), (&one << (n as usize - 1)) - 1),
            Self::Bool => (BigInt::from(0), one),
            Self::Addr => (BigInt::from(0), (&one << 160usize) - 1),
        }
    }
}

const INT_TYPES: &[Ty] = &[
    Ty::U(8),
    Ty::U(16),
    Ty::U(32),
    Ty::U(64),
    Ty::U(128),
    Ty::U(256),
    Ty::U(256),
    Ty::U(256),
    Ty::I(8),
    Ty::I(32),
    Ty::I(128),
    Ty::I(256),
    Ty::I(256),
];

/// A literal: a math integer of the type (0 or 1 for `bool`; an address as a number).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Lit {
    pub(crate) ty: Ty,
    /// Decimal, possibly negative.
    pub(crate) value: String,
}

impl Lit {
    pub(crate) fn zero(ty: Ty) -> Self {
        Self { ty, value: "0".into() }
    }

    pub(crate) fn sol(&self) -> String {
        match self.ty {
            Ty::Bool => {
                if self.value == "0" {
                    "false".into()
                } else {
                    "true".into()
                }
            }
            Ty::Addr => {
                let v: U256 = self.value.parse().unwrap();
                let bytes = v.to_be_bytes::<32>();
                let a = alloy_primitives::Address::from_slice(&bytes[12..]);
                format!("address({})", a.to_checksum(None))
            }
            // `int8(-128)` is a valid explicit conversion of a literal that fits.
            ty => format!("{}({})", ty.sol(), self.value),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Exp,
    And,
    Or,
    Xor,
    Shl,
    Shr,
}

impl BinOp {
    fn sol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::Mod => "%",
            Self::Exp => "**",
            Self::And => "&",
            Self::Or => "|",
            Self::Xor => "^",
            Self::Shl => "<<",
            Self::Shr => ">>",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum CmpOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

impl CmpOp {
    fn sol(self) -> &'static str {
        match self {
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Eq => "==",
            Self::Ne => "!=",
        }
    }
}

/// A storage or local location.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Place {
    Var(String),
    Map(String, Box<Expr>),
    Map2(String, Box<Expr>, Box<Expr>),
    Arr(String, Box<Expr>),
    Field(String, String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Expr {
    Lit(Lit),
    Read(Place, Ty),
    Bin(BinOp, Box<Self>, Box<Self>, Ty),
    Neg(Box<Self>, Ty),
    BitNot(Box<Self>, Ty),
    Cmp(CmpOp, Box<Self>, Box<Self>),
    Not(Box<Self>),
    AndAnd(Box<Self>, Box<Self>),
    OrOr(Box<Self>, Box<Self>),
    Cast(Ty, Box<Self>),
    Tern(Box<Self>, Box<Self>, Box<Self>, Ty),
    Call(String, Vec<Self>, Ty),
    Len(String),
    Sender,
    Max(Ty),
    Min(Ty),
}

impl Expr {
    pub(crate) fn ty(&self) -> Ty {
        match self {
            Self::Lit(l) => l.ty,
            Self::Read(_, t) | Self::Bin(_, _, _, t) | Self::Neg(_, t) | Self::BitNot(_, t) => *t,
            Self::Tern(_, _, _, t) | Self::Call(_, _, t) => *t,
            Self::Cmp(..) | Self::Not(_) | Self::AndAnd(..) | Self::OrOr(..) => Ty::Bool,
            Self::Cast(t, _) | Self::Max(t) | Self::Min(t) => *t,
            Self::Len(_) => Ty::U(256),
            Self::Sender => Ty::Addr,
        }
    }

    pub(crate) fn sol(&self) -> String {
        match self {
            Self::Lit(l) => l.sol(),
            Self::Read(p, _) => p.sol(),
            Self::Bin(op, a, b, _) => format!("({} {} {})", a.sol(), op.sol(), b.sol()),
            Self::Neg(a, _) => format!("(-{})", a.sol()),
            Self::BitNot(a, _) => format!("(~{})", a.sol()),
            Self::Cmp(op, a, b) => format!("({} {} {})", a.sol(), op.sol(), b.sol()),
            Self::Not(a) => format!("(!{})", a.sol()),
            Self::AndAnd(a, b) => format!("({} && {})", a.sol(), b.sol()),
            Self::OrOr(a, b) => format!("({} || {})", a.sol(), b.sol()),
            Self::Cast(t, a) => format!("{}({})", t.sol(), a.sol()),
            Self::Tern(c, a, b, _) => format!("({} ? {} : {})", c.sol(), a.sol(), b.sol()),
            Self::Call(f, args, _) => {
                let args: Vec<_> = args.iter().map(Self::sol).collect();
                format!("{f}({})", args.join(", "))
            }
            Self::Len(a) => format!("{a}.length"),
            Self::Sender => "msg.sender".into(),
            Self::Max(t) => format!("type({}).max", t.sol()),
            Self::Min(t) => format!("type({}).min", t.sol()),
        }
    }

    /// Direct subexpressions.
    pub(crate) fn children(&self) -> Vec<&Self> {
        match self {
            Self::Bin(_, a, b, _) | Self::Cmp(_, a, b) | Self::AndAnd(a, b) | Self::OrOr(a, b) => {
                vec![a, b]
            }
            Self::Neg(a, _) | Self::BitNot(a, _) | Self::Not(a) | Self::Cast(_, a) => vec![a],
            Self::Tern(c, a, b, _) => vec![c, a, b],
            Self::Call(_, args, _) => args.iter().collect(),
            Self::Read(p, _) => p.exprs(),
            _ => vec![],
        }
    }

    /// Direct subexpressions, for reduction.
    pub(crate) fn children_mut(&mut self) -> Vec<&mut Self> {
        match self {
            Self::Bin(_, a, b, _) | Self::Cmp(_, a, b) | Self::AndAnd(a, b) | Self::OrOr(a, b) => {
                vec![a, b]
            }
            Self::Neg(a, _) | Self::BitNot(a, _) | Self::Not(a) | Self::Cast(_, a) => vec![a],
            Self::Tern(c, a, b, _) => vec![c, a, b],
            Self::Call(_, args, _) => args.iter_mut().collect(),
            Self::Read(p, _) => p.exprs_mut(),
            _ => vec![],
        }
    }

    fn reads_storage(&self, c: &Contract) -> bool {
        match self {
            Self::Read(p, _) if p.is_storage(c) => true,
            Self::Len(_) => true,
            Self::Call(f, args, _) => {
                args.iter().any(|e| e.reads_storage(c))
                    || c.helpers.iter().any(|h| &h.name == f && h.reads_storage(c))
            }
            e => e.children().iter().any(|e| e.reads_storage(c)),
        }
    }

    fn reads_env(&self) -> bool {
        match self {
            Self::Sender => true,
            e => e.children().iter().any(|e| e.reads_env()),
        }
    }
}

impl Place {
    pub(crate) fn sol(&self) -> String {
        match self {
            Self::Var(v) => v.clone(),
            Self::Map(m, k) => format!("{m}[{}]", k.sol()),
            Self::Map2(m, a, b) => format!("{m}[{}][{}]", a.sol(), b.sol()),
            Self::Arr(a, i) => format!("{a}[{}]", i.sol()),
            Self::Field(s, f) => format!("{s}.{f}"),
        }
    }

    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Var(v) | Self::Map(v, _) | Self::Map2(v, _, _) | Self::Arr(v, _) => v,
            Self::Field(s, _) => s,
        }
    }

    pub(crate) fn is_storage(&self, c: &Contract) -> bool {
        c.state.iter().any(|s| s.name == self.name())
    }

    pub(crate) fn exprs(&self) -> Vec<&Expr> {
        match self {
            Self::Map(_, k) | Self::Arr(_, k) => vec![k],
            Self::Map2(_, a, b) => vec![a, b],
            _ => vec![],
        }
    }

    pub(crate) fn exprs_mut(&mut self) -> Vec<&mut Expr> {
        match self {
            Self::Map(_, k) | Self::Arr(_, k) => vec![k],
            Self::Map2(_, a, b) => vec![a, b],
            _ => vec![],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Stmt {
    Decl(Ty, String, Expr),
    Assign(Place, Option<BinOp>, Expr),
    If(Expr, Vec<Self>, Vec<Self>),
    /// `for (uint256 i = 0; i < bound; i++)`.
    For(String, u8, Vec<Self>),
    Require(Expr, Option<String>),
    Revert(usize, Vec<Expr>),
    Emit(usize, Vec<Expr>),
    Return(Vec<Expr>),
    Push(String, Expr),
    Pop(String),
    Delete(Place),
    Unchecked(Vec<Self>),
    Assert(Expr),
}

impl Stmt {
    /// Nested statement lists, for reduction.
    pub(crate) fn blocks_mut(&mut self) -> Vec<&mut Vec<Self>> {
        match self {
            Self::If(_, a, b) => vec![a, b],
            Self::For(_, _, b) | Self::Unchecked(b) => vec![b],
            _ => vec![],
        }
    }

    /// Expressions directly in the statement, for reduction.
    pub(crate) fn exprs_mut(&mut self) -> Vec<&mut Expr> {
        match self {
            Self::Decl(_, _, e) | Self::Require(e, _) | Self::Assert(e) | Self::Push(_, e) => {
                vec![e]
            }
            Self::Assign(p, _, e) => {
                let mut v = p.exprs_mut();
                v.push(e);
                v
            }
            Self::Delete(p) => p.exprs_mut(),
            Self::If(c, _, _) => vec![c],
            Self::Revert(_, es) | Self::Emit(_, es) | Self::Return(es) => es.iter_mut().collect(),
            _ => vec![],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum StateKind {
    Scalar(Ty),
    Map(Ty, Ty),
    Map2(Ty, Ty, Ty),
    /// A fixed-size array.
    Arr(Ty, usize),
    DynArr(Ty),
    /// A struct of value-type fields.
    Struct(Vec<(String, Ty)>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StateVar {
    pub(crate) name: String,
    pub(crate) kind: StateKind,
    pub(crate) public: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Function {
    pub(crate) name: String,
    pub(crate) params: Vec<(Ty, String)>,
    pub(crate) returns: Vec<Ty>,
    pub(crate) body: Vec<Stmt>,
    /// Specs to attach as NatSpec, one per line, such as `ensures result >= a0`.
    pub(crate) specs: Vec<String>,
    /// Whether the function is an internal pure helper.
    pub(crate) helper: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Contract {
    pub(crate) name: String,
    pub(crate) state: Vec<StateVar>,
    pub(crate) events: Vec<Vec<(Ty, bool)>>,
    pub(crate) errors: Vec<Vec<Ty>>,
    pub(crate) ctor: Vec<Stmt>,
    pub(crate) helpers: Vec<Function>,
    pub(crate) funcs: Vec<Function>,
    pub(crate) invariants: Vec<String>,
}

/// A call of the sequence: the function and its arguments.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Call {
    pub(crate) func: String,
    pub(crate) args: Vec<Lit>,
}

impl Function {
    fn reads_storage(&self, c: &Contract) -> bool {
        let mut any = false;
        visit_exprs(&self.body, &mut |e| any |= e.reads_storage(c));
        any || writes(&self.body, c)
    }

    /// Solidity's mutability for the body: `pure`, `view`, or nonpayable.
    fn mutability(&self, c: &Contract) -> &'static str {
        let mut env = false;
        visit_exprs(&self.body, &mut |e| env |= e.reads_env());
        if writes(&self.body, c) {
            ""
        } else if self.reads_storage(c) || env {
            " view"
        } else {
            " pure"
        }
    }
}

fn writes(body: &[Stmt], c: &Contract) -> bool {
    body.iter().any(|s| match s {
        Stmt::Assign(p, _, _) | Stmt::Delete(p) => p.is_storage(c),
        Stmt::Emit(..) | Stmt::Push(..) | Stmt::Pop(_) => true,
        Stmt::If(_, a, b) => writes(a, c) || writes(b, c),
        Stmt::For(_, _, b) | Stmt::Unchecked(b) => writes(b, c),
        _ => false,
    })
}

/// Calls `f` on every expression of `body`, nested ones included.
pub(crate) fn visit_exprs(body: &[Stmt], f: &mut dyn FnMut(&Expr)) {
    fn expr(e: &Expr, f: &mut dyn FnMut(&Expr)) {
        f(e);
        for c in e.children() {
            expr(c, f);
        }
    }
    for s in body {
        match s {
            Stmt::Decl(_, _, e) | Stmt::Require(e, _) | Stmt::Assert(e) | Stmt::Push(_, e) => {
                expr(e, f)
            }
            Stmt::Assign(p, _, e) => {
                p.exprs().into_iter().for_each(|e| expr(e, f));
                expr(e, f)
            }
            Stmt::Delete(p) => p.exprs().into_iter().for_each(|e| expr(e, f)),
            Stmt::If(c, a, b) => {
                expr(c, f);
                visit_exprs(a, f);
                visit_exprs(b, f);
            }
            Stmt::For(_, _, b) | Stmt::Unchecked(b) => visit_exprs(b, f),
            Stmt::Revert(_, es) | Stmt::Emit(_, es) | Stmt::Return(es) => {
                es.iter().for_each(|e| expr(e, f))
            }
            Stmt::Pop(_) => {}
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Printing

impl Contract {
    pub(crate) fn sol(&self) -> String {
        let mut out = String::new();
        out.push_str("// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\n\n");
        for inv in &self.invariants {
            writeln!(out, "/// @custom:invariant {inv}").unwrap();
        }
        writeln!(out, "contract {} {{", self.name).unwrap();
        for (i, s) in self.structs().iter().enumerate() {
            let fields: Vec<_> = s.iter().map(|(n, t)| format!("{} {n};", t.sol())).collect();
            writeln!(out, "    struct S{i} {{ {} }}", fields.join(" ")).unwrap();
        }
        let mut struct_index = 0;
        for v in &self.state {
            let vis = if v.public { " public" } else { "" };
            let ty = match &v.kind {
                StateKind::Scalar(t) => t.sol(),
                StateKind::Map(k, t) => format!("mapping({} => {})", k.sol(), t.sol()),
                StateKind::Map2(a, b, t) => {
                    format!("mapping({} => mapping({} => {}))", a.sol(), b.sol(), t.sol())
                }
                StateKind::Arr(t, n) => format!("{}[{n}]", t.sol()),
                StateKind::DynArr(t) => format!("{}[]", t.sol()),
                StateKind::Struct(_) => {
                    struct_index += 1;
                    format!("S{}", struct_index - 1)
                }
            };
            writeln!(out, "    {ty}{vis} {};", v.name).unwrap();
        }
        for (i, e) in self.events.iter().enumerate() {
            let ps: Vec<_> = e
                .iter()
                .enumerate()
                .map(|(j, (t, ix))| {
                    format!("{}{} p{j}", t.sol(), if *ix { " indexed" } else { "" })
                })
                .collect();
            writeln!(out, "    event Ev{i}({});", ps.join(", ")).unwrap();
        }
        for (i, e) in self.errors.iter().enumerate() {
            let ps: Vec<_> =
                e.iter().enumerate().map(|(j, t)| format!("{} p{j}", t.sol())).collect();
            writeln!(out, "    error Er{i}({});", ps.join(", ")).unwrap();
        }
        out.push_str("\n    constructor() {\n");
        block(&mut out, &self.ctor, 2);
        out.push_str("    }\n");
        for f in self.helpers.iter().chain(&self.funcs) {
            out.push('\n');
            for s in &f.specs {
                writeln!(out, "    /// @custom:{s}").unwrap();
            }
            let ps: Vec<_> = f.params.iter().map(|(t, n)| format!("{} {n}", t.sol())).collect();
            let rets = if f.returns.is_empty() {
                String::new()
            } else {
                let rs: Vec<_> = f.returns.iter().map(|t| t.sol()).collect();
                format!(" returns ({})", rs.join(", "))
            };
            let vis = if f.helper { " internal" } else { " external" };
            let m = f.mutability(self);
            writeln!(out, "    function {}({}){vis}{m}{rets} {{", f.name, ps.join(", ")).unwrap();
            block(&mut out, &f.body, 2);
            out.push_str("    }\n");
        }
        out.push_str("}\n");
        out
    }

    fn structs(&self) -> Vec<&Vec<(String, Ty)>> {
        self.state
            .iter()
            .filter_map(|v| match &v.kind {
                StateKind::Struct(f) => Some(f),
                _ => None,
            })
            .collect()
    }
}

fn block(out: &mut String, body: &[Stmt], depth: usize) {
    let pad = "    ".repeat(depth);
    for s in body {
        match s {
            Stmt::Decl(t, n, e) => writeln!(out, "{pad}{} {n} = {};", t.sol(), e.sol()).unwrap(),
            Stmt::Assign(p, op, e) => {
                let op = op.map(BinOp::sol).unwrap_or("");
                writeln!(out, "{pad}{} {op}= {};", p.sol(), e.sol()).unwrap()
            }
            Stmt::If(c, a, b) => {
                writeln!(out, "{pad}if ({}) {{", c.sol()).unwrap();
                block(out, a, depth + 1);
                if b.is_empty() {
                    writeln!(out, "{pad}}}").unwrap();
                } else {
                    writeln!(out, "{pad}}} else {{").unwrap();
                    block(out, b, depth + 1);
                    writeln!(out, "{pad}}}").unwrap();
                }
            }
            Stmt::For(i, n, b) => {
                writeln!(out, "{pad}for (uint256 {i} = 0; {i} < {n}; {i}++) {{").unwrap();
                block(out, b, depth + 1);
                writeln!(out, "{pad}}}").unwrap();
            }
            Stmt::Require(c, None) => writeln!(out, "{pad}require({});", c.sol()).unwrap(),
            Stmt::Require(c, Some(m)) => {
                writeln!(out, "{pad}require({}, \"{m}\");", c.sol()).unwrap()
            }
            Stmt::Revert(i, es) => {
                let es: Vec<_> = es.iter().map(Expr::sol).collect();
                writeln!(out, "{pad}revert Er{i}({});", es.join(", ")).unwrap()
            }
            Stmt::Emit(i, es) => {
                let es: Vec<_> = es.iter().map(Expr::sol).collect();
                writeln!(out, "{pad}emit Ev{i}({});", es.join(", ")).unwrap()
            }
            Stmt::Return(es) if es.is_empty() => writeln!(out, "{pad}return;").unwrap(),
            Stmt::Return(es) if es.len() == 1 => {
                writeln!(out, "{pad}return {};", es[0].sol()).unwrap()
            }
            Stmt::Return(es) => {
                let es: Vec<_> = es.iter().map(Expr::sol).collect();
                writeln!(out, "{pad}return ({});", es.join(", ")).unwrap()
            }
            Stmt::Push(a, e) => writeln!(out, "{pad}{a}.push({});", e.sol()).unwrap(),
            Stmt::Pop(a) => writeln!(out, "{pad}{a}.pop();").unwrap(),
            Stmt::Delete(p) => writeln!(out, "{pad}delete {};", p.sol()).unwrap(),
            Stmt::Unchecked(b) => {
                writeln!(out, "{pad}unchecked {{").unwrap();
                block(out, b, depth + 1);
                writeln!(out, "{pad}}}").unwrap();
            }
            Stmt::Assert(c) => writeln!(out, "{pad}assert({});", c.sol()).unwrap(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Generation

/// Generation knobs.
#[derive(Clone, Debug)]
pub(crate) struct Config {
    pub(crate) max_funcs: usize,
    pub(crate) max_stmts: usize,
    pub(crate) max_depth: usize,
    pub(crate) calls: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self { max_funcs: 5, max_stmts: 6, max_depth: 3, calls: 12 }
    }
}

#[derive(Clone)]
struct Local {
    name: String,
    ty: Ty,
    mutable: bool,
}

struct Gen<'a> {
    rng: &'a mut Rng,
    cfg: &'a Config,
    c: Contract,
    locals: Vec<Local>,
    fresh: usize,
    /// Inside an `unchecked` block, where Solidity forbids a nested one.
    in_unchecked: bool,
    /// Generating pure helper `i`: no storage, no context, only earlier helpers.
    helper: Option<usize>,
    /// Generating a storage key or index: keys read no further storage.
    in_key: bool,
}

/// Generates a contract and a call sequence from `seed`.
pub(crate) fn generate(seed: u64, cfg: &Config) -> (Contract, Vec<Call>) {
    let mut rng = Rng::new(seed);
    let contract = {
        let mut g = Gen {
            rng: &mut rng,
            cfg,
            c: Contract {
                name: "C".into(),
                state: vec![],
                events: vec![],
                errors: vec![],
                ctor: vec![],
                helpers: vec![],
                funcs: vec![],
                invariants: vec![],
            },
            locals: vec![],
            fresh: 0,
            in_unchecked: false,
            helper: None,
            in_key: false,
        };
        g.state();
        for _ in 0..1 + g.rng.below(2) {
            let n = 1 + g.rng.below(3);
            let e = (0..n).map(|_| (g.value_ty(), g.rng.chance(40))).collect();
            g.c.events.push(e);
        }
        for _ in 0..1 + g.rng.below(2) {
            let n = g.rng.below(3);
            let e = (0..n).map(|_| g.value_ty()).collect();
            g.c.errors.push(e);
        }
        g.ctor();
        for i in 0..g.rng.below(3) {
            g.helper_fn(i);
        }
        let nf = 1 + g.rng.below(cfg.max_funcs);
        for i in 0..nf {
            g.function(i);
        }
        g.c
    };
    let calls = calls(&contract, &mut rng, cfg.calls);
    (contract, calls)
}

/// A random call sequence over the contract's external functions.
pub(crate) fn calls(c: &Contract, rng: &mut Rng, n: usize) -> Vec<Call> {
    (0..n)
        .map(|_| {
            let f = rng.pick(&c.funcs);
            Call {
                func: f.name.clone(),
                args: f.params.iter().map(|(t, _)| arg(rng, *t)).collect(),
            }
        })
        .collect()
}

/// A literal of `ty`, drawn mostly from boundary values.
pub(crate) fn arg(rng: &mut Rng, ty: Ty) -> Lit {
    let (lo, hi) = ty.range();
    let v: BigInt = match ty {
        Ty::Bool => BigInt::from(rng.below(2)),
        Ty::Addr => match rng.below(4) {
            0 => BigInt::from(0x2000),
            1 => BigInt::from(rng.below(4)),
            2 => BigInt::from(0x1000),
            _ => (rng.word() >> 96usize).to_string().parse::<BigInt>().unwrap(),
        },
        _ => match rng.below(10) {
            0 => lo.clone(),
            1 => hi.clone(),
            2 => &hi - 1,
            3 => &lo + 1,
            4 | 5 => BigInt::from(rng.below(5) as i64) - if matches!(ty, Ty::I(_)) { 2 } else { 0 },
            6 => {
                let k = rng.below(ty.bits() as usize);
                let p = BigInt::from(1) << k;
                if p > hi { hi.clone() } else { p + rng.below(3) as i64 - 1 }
            }
            7 => BigInt::from(rng.below(300)),
            _ => {
                let span = &hi - &lo + 1;
                let w: BigInt = rng.word().to_string().parse().unwrap();
                &lo + w % span
            }
        },
    };
    let v = v.clamp(lo, hi);
    Lit { ty, value: v.to_string() }
}

impl Gen<'_> {
    fn name(&mut self, prefix: &str) -> String {
        self.fresh += 1;
        format!("{prefix}{}", self.fresh)
    }

    fn value_ty(&mut self) -> Ty {
        match self.rng.below(10) {
            0 => Ty::Bool,
            1 => Ty::Addr,
            _ => *self.rng.pick(INT_TYPES),
        }
    }

    fn int_ty(&mut self) -> Ty {
        *self.rng.pick(INT_TYPES)
    }

    fn key_ty(&mut self) -> Ty {
        *self.rng.pick(&[Ty::U(256), Ty::U(256), Ty::Addr, Ty::U(8), Ty::I(32), Ty::Bool])
    }

    fn state(&mut self) {
        let n = 2 + self.rng.below(5);
        for i in 0..n {
            let kind = match self.rng.below(12) {
                0..=4 => StateKind::Scalar(self.value_ty()),
                5 | 6 => StateKind::Map(self.key_ty(), self.value_ty()),
                7 => StateKind::Map2(self.key_ty(), self.key_ty(), self.int_ty()),
                8 => StateKind::Arr(self.value_ty(), 1 + self.rng.below(4)),
                9 => StateKind::DynArr(self.value_ty()),
                _ => {
                    let nf = 1 + self.rng.below(3);
                    StateKind::Struct((0..nf).map(|j| (format!("f{j}"), self.value_ty())).collect())
                }
            };
            let public = self.rng.chance(50);
            self.c.state.push(StateVar { name: format!("s{i}"), kind, public });
        }
    }

    fn ctor(&mut self) {
        let mut body = vec![];
        for v in self.c.state.clone() {
            if let StateKind::Scalar(t) = v.kind
                && self.rng.chance(50)
            {
                let lit = arg(self.rng, t);
                body.push(Stmt::Assign(Place::Var(v.name.clone()), None, Expr::Lit(lit)));
            }
            if let StateKind::DynArr(t) = v.kind {
                for _ in 0..self.rng.below(3) {
                    body.push(Stmt::Push(v.name.clone(), Expr::Lit(arg(self.rng, t))));
                }
            }
        }
        self.c.ctor = body;
    }

    fn params(&mut self, max: usize) -> Vec<(Ty, String)> {
        (0..self.rng.below(max + 1)).map(|i| (self.value_ty(), format!("a{i}"))).collect()
    }

    fn helper_fn(&mut self, index: usize) {
        let mut params = self.params(3);
        if params.is_empty() {
            params.push((self.int_ty(), "a0".into()));
        }
        let ret = self.int_ty();
        self.locals =
            params.iter().map(|(t, n)| Local { name: n.clone(), ty: *t, mutable: true }).collect();
        self.helper = Some(index);
        let mut body = self.stmts(self.cfg.max_stmts / 2, 1, &[ret]);
        if !matches!(body.last(), Some(Stmt::Return(_))) {
            body.push(Stmt::Return(vec![self.expr(ret, 2)]));
        }
        self.helper = None;
        self.c.helpers.push(Function {
            name: format!("h{index}"),
            params,
            returns: vec![ret],
            body,
            specs: vec![],
            helper: true,
        });
    }

    fn function(&mut self, index: usize) {
        let params = self.params(3);
        let returns: Vec<Ty> = (0..self.rng.below(3)).map(|_| self.value_ty()).collect();
        self.locals =
            params.iter().map(|(t, n)| Local { name: n.clone(), ty: *t, mutable: true }).collect();
        let n = 1 + self.rng.below(self.cfg.max_stmts);
        let mut body = self.stmts(n, 0, &returns);
        if !matches!(body.last(), Some(Stmt::Return(_) | Stmt::Revert(..)))
            && (!returns.is_empty() || self.rng.chance(20))
        {
            let es = returns.iter().map(|t| self.expr(*t, 2)).collect();
            body.push(Stmt::Return(es));
        }
        self.c.funcs.push(Function {
            name: format!("f{index}"),
            params,
            returns,
            body,
            specs: vec![],
            helper: false,
        });
    }

    fn stmts(&mut self, n: usize, depth: usize, returns: &[Ty]) -> Vec<Stmt> {
        let scope = self.locals.len();
        let mut out = vec![];
        for _ in 0..n {
            if let Some(s) = self.stmt(depth, returns) {
                let terminal = matches!(s, Stmt::Return(_) | Stmt::Revert(..));
                out.push(s);
                if terminal {
                    break;
                }
            }
        }
        self.locals.truncate(scope);
        out
    }

    fn storage_places(&mut self, ty_filter: Option<Ty>) -> Vec<(Place, Ty)> {
        if self.helper.is_some() || self.in_key {
            return vec![];
        }
        self.in_key = true;
        let out = self.storage_places_inner(ty_filter);
        self.in_key = false;
        out
    }

    fn storage_places_inner(&mut self, ty_filter: Option<Ty>) -> Vec<(Place, Ty)> {
        let mut out = vec![];
        for v in self.c.state.clone() {
            let n = v.name.clone();
            let kind_ty = match &v.kind {
                StateKind::Scalar(t)
                | StateKind::Map(_, t)
                | StateKind::Map2(_, _, t)
                | StateKind::Arr(t, _)
                | StateKind::DynArr(t) => Some(*t),
                StateKind::Struct(_) => None,
            };
            if let (Some(want), Some(t)) = (ty_filter, kind_ty)
                && want != t
            {
                continue;
            }
            match v.kind {
                StateKind::Scalar(t) => out.push((Place::Var(n), t)),
                StateKind::Map(k, t) => {
                    let k = self.expr(k, 1);
                    out.push((Place::Map(n, Box::new(k)), t))
                }
                StateKind::Map2(a, b, t) => {
                    let a = self.expr(a, 1);
                    let b = self.expr(b, 1);
                    out.push((Place::Map2(n, Box::new(a), Box::new(b)), t))
                }
                StateKind::Arr(t, len) => {
                    let i = self.index(len);
                    out.push((Place::Arr(n, Box::new(i)), t))
                }
                StateKind::DynArr(t) => {
                    let i = self.index(3);
                    out.push((Place::Arr(n, Box::new(i)), t))
                }
                StateKind::Struct(fs) => {
                    for (f, t) in fs {
                        if ty_filter.is_none_or(|want| want == t) {
                            out.push((Place::Field(n.clone(), f), t))
                        }
                    }
                }
            }
        }
        out
    }

    /// An index expression, usually in bounds.
    fn index(&mut self, len: usize) -> Expr {
        if self.rng.chance(70) {
            Expr::Lit(Lit { ty: Ty::U(256), value: self.rng.below(len + 1).to_string() })
        } else {
            self.leaf(Ty::U(256))
        }
    }

    fn stmt(&mut self, depth: usize, returns: &[Ty]) -> Option<Stmt> {
        let deep = depth >= self.cfg.max_depth;
        let helper = self.helper.is_some();
        Some(match self.rng.below(20) {
            0..=2 => {
                let t = self.value_ty();
                let e = self.expr(t, 3);
                let name = self.name("l");
                self.locals.push(Local { name: name.clone(), ty: t, mutable: true });
                Stmt::Decl(t, name, e)
            }
            3..=6 => {
                let mut places: Vec<(Place, Ty)> = self
                    .locals
                    .iter()
                    .filter(|l| l.mutable)
                    .map(|l| (Place::Var(l.name.clone()), l.ty))
                    .collect();
                places.extend(self.storage_places(None));
                if places.is_empty() {
                    return None;
                }
                let (p, t) = self.rng.pick(&places).clone();
                let op = if t.is_int() && self.rng.chance(40) {
                    Some(*self.rng.pick(&[
                        BinOp::Add,
                        BinOp::Sub,
                        BinOp::Mul,
                        BinOp::Or,
                        BinOp::Xor,
                    ]))
                } else {
                    None
                };
                Stmt::Assign(p, op, self.expr(t, 3))
            }
            7 | 8 if !deep => {
                let c = self.expr(Ty::Bool, 3);
                let n = 1 + self.rng.below(3);
                let a = self.stmts(n, depth + 1, returns);
                let b = if self.rng.chance(50) {
                    let n = 1 + self.rng.below(3);
                    self.stmts(n, depth + 1, returns)
                } else {
                    vec![]
                };
                Stmt::If(c, a, b)
            }
            9 if !deep => {
                let i = self.name("i");
                self.locals.push(Local { name: i.clone(), ty: Ty::U(256), mutable: false });
                let n = 1 + self.rng.below(3);
                let body = self.stmts(n, depth + 1, returns);
                self.locals.retain(|l| l.name != i);
                Stmt::For(i, 1 + self.rng.below(4) as u8, body)
            }
            10 | 11 => {
                let msg = self.rng.chance(30).then(|| format!("m{}", self.rng.below(100)));
                Stmt::Require(self.expr(Ty::Bool, 3), msg)
            }
            12 if !helper && depth > 0 => {
                let i = self.rng.below(self.c.errors.len());
                let tys = self.c.errors[i].clone();
                Stmt::Revert(i, tys.iter().map(|t| self.expr(*t, 2)).collect())
            }
            13 if !helper => {
                let i = self.rng.below(self.c.events.len());
                let tys = self.c.events[i].clone();
                Stmt::Emit(i, tys.iter().map(|(t, _)| self.expr(*t, 2)).collect())
            }
            14 if depth > 0 && !helper => {
                Stmt::Return(returns.iter().map(|t| self.expr(*t, 2)).collect())
            }
            15 if !helper => {
                let arrays: Vec<_> = self
                    .c
                    .state
                    .iter()
                    .filter_map(|v| match v.kind {
                        StateKind::DynArr(t) => Some((v.name.clone(), t)),
                        _ => None,
                    })
                    .collect();
                if arrays.is_empty() {
                    return None;
                }
                let (a, t) = self.rng.pick(&arrays).clone();
                if self.rng.chance(70) { Stmt::Push(a, self.expr(t, 2)) } else { Stmt::Pop(a) }
            }
            16 if !helper => {
                let places = self.storage_places(None);
                if places.is_empty() {
                    return None;
                }
                Stmt::Delete(self.rng.pick(&places).0.clone())
            }
            17 if !deep && !self.in_unchecked => {
                self.in_unchecked = true;
                let n = 1 + self.rng.below(3);
                let body = self.stmts(n, depth + 1, returns);
                self.in_unchecked = false;
                Stmt::Unchecked(body)
            }
            18 => Stmt::Assert(self.expr(Ty::Bool, 2)),
            _ => return None,
        })
    }

    fn leaf(&mut self, ty: Ty) -> Expr {
        let mut options: Vec<Expr> = self
            .locals
            .iter()
            .filter(|l| l.ty == ty)
            .map(|l| Expr::Read(Place::Var(l.name.clone()), ty))
            .collect();
        if self.rng.chance(60) {
            for (p, t) in self.storage_places(Some(ty)) {
                if !matches!(p, Place::Map(..) | Place::Map2(..) | Place::Arr(..))
                    || self.rng.chance(50)
                {
                    options.push(Expr::Read(p, t));
                }
            }
        }
        if ty == Ty::Addr && self.helper.is_none() {
            options.push(Expr::Sender);
        }
        if ty == Ty::U(256) && self.helper.is_none() {
            for v in &self.c.state {
                if matches!(v.kind, StateKind::DynArr(_)) {
                    options.push(Expr::Len(v.name.clone()));
                }
            }
        }
        if options.is_empty() || self.rng.chance(30) {
            if ty.is_int() && self.rng.chance(5) {
                return if self.rng.chance(50) { Expr::Max(ty) } else { Expr::Min(ty) };
            }
            // Literals in code are mostly small, so that more paths run to the end.
            if ty.is_int() && self.rng.chance(75) {
                let v = self.rng.below(11) as i64 - if matches!(ty, Ty::I(_)) { 3 } else { 0 };
                return Expr::Lit(Lit { ty, value: v.to_string() });
            }
            return Expr::Lit(arg(self.rng, ty));
        }
        self.rng.pick(&options).clone()
    }

    fn expr(&mut self, ty: Ty, depth: usize) -> Expr {
        if depth == 0 || self.rng.chance(30) {
            return self.leaf(ty);
        }
        let d = depth - 1;
        match ty {
            Ty::Bool => match self.rng.below(6) {
                0..=2 => {
                    let t = if self.rng.chance(85) { self.int_ty() } else { Ty::Addr };
                    let ops: &[CmpOp] = if t == Ty::Addr {
                        &[CmpOp::Eq, CmpOp::Ne]
                    } else {
                        &[CmpOp::Lt, CmpOp::Le, CmpOp::Gt, CmpOp::Ge, CmpOp::Eq, CmpOp::Ne]
                    };
                    let op = *self.rng.pick(ops);
                    Expr::Cmp(op, Box::new(self.expr(t, d)), Box::new(self.expr(t, d)))
                }
                3 => Expr::Not(Box::new(self.expr(Ty::Bool, d))),
                4 => {
                    Expr::AndAnd(Box::new(self.expr(Ty::Bool, d)), Box::new(self.expr(Ty::Bool, d)))
                }
                _ => Expr::OrOr(Box::new(self.expr(Ty::Bool, d)), Box::new(self.expr(Ty::Bool, d))),
            },
            Ty::Addr => {
                if self.rng.chance(30) {
                    let x = self.expr(Ty::U(160), d);
                    Expr::Cast(Ty::Addr, Box::new(x))
                } else {
                    self.leaf(ty)
                }
            }
            Ty::U(_) | Ty::I(_) => match self.rng.below(16) {
                0..=7 => {
                    let op = *self.rng.pick(&[
                        BinOp::Add,
                        BinOp::Add,
                        BinOp::Sub,
                        BinOp::Sub,
                        BinOp::Mul,
                        BinOp::Div,
                        BinOp::Mod,
                        BinOp::And,
                        BinOp::Or,
                        BinOp::Xor,
                    ]);
                    Expr::Bin(op, Box::new(self.expr(ty, d)), Box::new(self.expr(ty, d)), ty)
                }
                8 => {
                    // Shifts take an unsigned amount of any width.
                    let op = *self.rng.pick(&[BinOp::Shl, BinOp::Shr]);
                    let amount = if self.rng.chance(60) {
                        let v = self.rng.below(260).min(255);
                        Expr::Lit(Lit { ty: Ty::U(8), value: v.to_string() })
                    } else {
                        self.expr(Ty::U(8), d)
                    };
                    Expr::Bin(op, Box::new(self.expr(ty, d)), Box::new(amount), ty)
                }
                9 => {
                    let e = Expr::Lit(Lit { ty: Ty::U(8), value: self.rng.below(5).to_string() });
                    Expr::Bin(BinOp::Exp, Box::new(self.expr(ty, d)), Box::new(e), ty)
                }
                10 if matches!(ty, Ty::I(_)) => Expr::Neg(Box::new(self.expr(ty, d)), ty),
                11 => Expr::BitNot(Box::new(self.expr(ty, d)), ty),
                12 => {
                    // Explicit conversions change width or signedness, not both.
                    let from = match ty {
                        Ty::U(n) if self.rng.chance(50) && n != 16 && n != 64 => Ty::I(n),
                        Ty::U(_) => *self.rng.pick(&[
                            Ty::U(8),
                            Ty::U(32),
                            Ty::U(64),
                            Ty::U(128),
                            Ty::U(256),
                        ]),
                        Ty::I(n) if self.rng.chance(50) => Ty::U(n),
                        Ty::I(_) => *self.rng.pick(&[Ty::I(8), Ty::I(32), Ty::I(128), Ty::I(256)]),
                        _ => unreachable!(),
                    };
                    Expr::Cast(ty, Box::new(self.expr(from, d)))
                }
                13 => {
                    let c = self.expr(Ty::Bool, d);
                    let a = self.expr(ty, d);
                    let b = self.expr(ty, d);
                    Expr::Tern(Box::new(c), Box::new(a), Box::new(b), ty)
                }
                14 => {
                    let limit = self.helper.unwrap_or(self.c.helpers.len());
                    let hs: Vec<_> = self.c.helpers[..limit]
                        .iter()
                        .filter(|h| h.returns[0] == ty)
                        .map(|h| (h.name.clone(), h.params.clone()))
                        .collect();
                    if hs.is_empty() {
                        return self.leaf(ty);
                    }
                    let (name, params) = self.rng.pick(&hs).clone();
                    let args = params.iter().map(|(t, _)| self.expr(*t, d)).collect();
                    Expr::Call(name, args, ty)
                }
                _ => self.leaf(ty),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let cfg = Config::default();
        for seed in 0..50 {
            let (a, ca) = generate(seed, &cfg);
            let (b, cb) = generate(seed, &cfg);
            assert_eq!(a, b);
            assert_eq!(ca, cb);
            assert!(a.sol().contains("contract C {"));
            assert!(ca.iter().all(|c| a.funcs.iter().any(|f| f.name == c.func)));
        }
    }

    #[test]
    fn literals_fit() {
        let mut rng = Rng::new(7);
        for ty in [Ty::U(8), Ty::I(8), Ty::U(256), Ty::I(256), Ty::Addr, Ty::Bool] {
            let (lo, hi) = ty.range();
            for _ in 0..200 {
                let v: BigInt = arg(&mut rng, ty).value.parse().unwrap();
                assert!(lo <= v && v <= hi, "{ty:?}: {v}");
            }
        }
    }
}

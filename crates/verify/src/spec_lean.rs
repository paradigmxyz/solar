//! Lean export of specs (`--emit-lean-spec OUT.lean`).
//!
//! Writes each contract's specs as a Lean module over the spec language of solidity-lean's
//! `SolidCore.Spec` (`Syntax.lean`), whose `Semantics.lean` and `Contract.lean` give a spec its
//! meaning over the calls and observables SolidCore, solar's IRs and EthLib share: a function's
//! `Pre` and `Post` over the pre- and post-storage, the return data and the success flag. The
//! module binds the specs to a manifest: the source path and its sha256, the compiler and options
//! the bytecode was built with, and the bytecode's sha256.
//!
//! The export resolves every state variable a spec reads to its storage slot and byte offset,
//! with Solidity's layout rules (declaration order, bases first, value types packed into a slot),
//! so a spec's meaning is over the raw slots the bytecode reads and writes. Mapping entries,
//! dynamic array elements and struct fields become `Place`s whose slots Lean computes with
//! keccak256, as Solidity does.
//!
//! Every construct the export does not translate is an error and nothing is written: a spec is
//! never dropped or weakened. Not exported: casts, member calls and builtins other than
//! `msg.sender`, `msg.value`, `block.timestamp` and `this`, shifts by a computed amount
//! (solar-verify encodes them through a table that differs from `2 ^ k` past 255), `~`, strings and
//! dynamic bytes, mappings with `bytesN` keys (left-aligned in the hashed word, which
//! solar-verify's `uintN` model of `bytesN` does not show), arrays of structs, `@custom:check`
//! (enforced only by `--instrument`, which changes the bytecode), spec functions without a
//! single-return body, and ensures that read a parameter the body assigns (the spec would read the
//! final local value, which no observable shows).
//!
//! Loop invariants, ghost updates, asserts, assumes and lemma uses are statement-level proof
//! steps: they are not part of a function's external spec and are not exported; the trusted ones
//! (`@custom:assume`, `@custom:trusted`) are listed in the module's `trusted`.
use crate::{
    prog::{Contract, Func, Program, Spec},
    ty::{Ty, parse_ty_text},
    walk,
};
use solar_parse::{
    ast::{
        self, BinOpKind, ElementaryType, Expr, ExprKind, IndexKind, ItemKind, LitKind, SourceUnit,
        StmtKind, TypeKind, UnOpKind,
    },
    interface::{Span, SpannedOption, data_structures::map::FxHashMap},
};
use std::fmt::Write;

/// What the manifest records besides the source.
pub(crate) struct Opts {
    /// The Lean namespace of the module.
    pub module: String,
    /// The source path, as given.
    pub source: String,
    pub source_sha256: String,
    /// The compiler that built the bytecode, with its version and commit.
    pub compiler: String,
    pub options: Vec<String>,
    pub bytecode_sha256: String,
}

type R<T> = Result<T, (Span, String)>;

/// Where a state variable lives.
#[derive(Clone, Debug)]
struct Slot {
    slot: u64,
    /// Byte offset in the slot, from the least significant byte.
    offset: u32,
    ty: Ty,
}

/// Bytes a value type takes in storage, or `None` for a type that takes whole slots.
fn value_bytes(ty: &Ty) -> Option<u32> {
    match ty {
        Ty::Bool => Some(1),
        Ty::Uint(n) | Ty::Int(n) => Some(u32::from(*n) / 8),
        Ty::Addr | Ty::Contract(_) => Some(20),
        Ty::Bytes(n) => Some(u32::from(*n) / 8),
        Ty::Enum(_, k) if *k <= 256 => Some(1),
        _ => None,
    }
}

/// Lays out a list of types from slot 0 with Solidity's rules: a value type packs into the
/// current slot when it fits, anything else starts a new slot and so does what follows it.
/// Returns each item's slot and offset, and the number of slots taken.
fn pack(p: &Program<'_>, tys: &[Ty], sp: Span) -> R<(Vec<(u64, u32)>, u64)> {
    let (mut slot, mut off) = (0u64, 0u32);
    let mut out = vec![];
    for ty in tys {
        if let Some(n) = value_bytes(ty) {
            if off + n > 32 {
                slot += 1;
                off = 0;
            }
            out.push((slot, off));
            off += n;
        } else {
            if off > 0 {
                slot += 1;
                off = 0;
            }
            out.push((slot, 0));
            slot += slots_of(p, ty, sp)?;
        }
    }
    Ok((out, if off > 0 { slot + 1 } else { slot }))
}

/// Slots a type takes.
fn slots_of(p: &Program<'_>, ty: &Ty, sp: Span) -> R<u64> {
    Ok(match ty {
        Ty::Map(..) | Ty::Arr(_, None) | Ty::Opaque => 1,
        Ty::Struct(n) => {
            let fields = p.struct_fields(n).ok_or((sp, format!("unknown struct `{n}`")))?;
            let tys: Vec<Ty> = fields.iter().map(|f| f.1.clone()).collect();
            pack(p, &tys, sp)?.1
        }
        Ty::Arr(e, Some(n)) => match value_bytes(e) {
            Some(b) if b <= 16 => n.div_ceil(u64::from(32 / b)),
            _ => n * slots_of(p, e, sp)?,
        },
        _ if value_bytes(ty).is_some() => 1,
        _ => return Err((sp, format!("no storage layout for `{}`", ty.name()))),
    })
}

/// A Lean `Ty`.
fn lean_ty(ty: &Ty, sp: Span) -> R<String> {
    Ok(match ty {
        Ty::Bool => ".bool".into(),
        Ty::Uint(n) => format!("(.uint {n})"),
        Ty::Int(n) => format!("(.int {n})"),
        Ty::Addr | Ty::Contract(_) => ".address".into(),
        // A `bytesN` value as storage holds it: its N bytes read as an unsigned number.
        Ty::Bytes(n) => format!("(.fixedBytes {})", n / 8),
        Ty::Enum(_, k) => format!("(.enum {k})"),
        Ty::Lit => ".math".into(),
        _ => return Err((sp, format!("values of type `{}` are not exported", ty.name()))),
    })
}

/// The ABI type of a parameter or return value: `bytesN` is left-aligned, unlike `uintN`.
fn abi_ty<'a>(p: &Program<'a>, v: &'a ast::VariableDefinition<'a>, sp: Span) -> R<String> {
    if let TypeKind::Elementary(ElementaryType::FixedBytes(n)) = &v.ty.kind {
        return Ok(format!("(.fixedBytes {})", n.bytes()));
    }
    let ty = crate::vc::ty_of(p, &v.ty);
    if let Ty::Int(n) | Ty::Uint(n) = ty
        && n % 8 != 0
    {
        return Err((sp, format!("integer width {n} is not exported")));
    }
    lean_ty(&ty, sp)
}

/// Whether a type has a `bytesN` mapping key anywhere.
fn bytes_key(t: &ast::Type<'_>) -> bool {
    match &t.kind {
        TypeKind::Mapping(m) => {
            matches!(m.key.kind, TypeKind::Elementary(ElementaryType::FixedBytes(_)))
                || bytes_key(&m.value)
        }
        TypeKind::Array(a) => bytes_key(&a.element),
        _ => false,
    }
}

/// A translated spec expression: a value, a storage location not yet read, or a ghost variable
/// with the keys applied so far.
enum Val {
    Term(String),
    Place { place: String, offset: String, ty: Ty },
    Ghost { name: String, keys: Vec<String>, ty: Ty },
}

/// One spec function the specs call.
struct SpecFnOut {
    name: String,
    params: Vec<(String, String)>,
    ret: String,
    body: String,
    opaque: bool,
}

struct Cx<'p, 'a> {
    p: &'p Program<'a>,
    c: &'p Contract<'a>,
    layout: &'p FxHashMap<String, Slot>,
    bytes_keys: &'p [String],
    /// The function whose spec this is, if any.
    f: Option<&'p Func<'a>>,
    /// Names bound by quantifiers and spec function parameters.
    bound: Vec<String>,
    /// Inside a spec function's body: its parameters are bound names, and the function's own
    /// parameters and returns are out of scope.
    in_fn: bool,
    allow_old: bool,
    allow_result: bool,
    fns: Vec<SpecFnOut>,
    visiting: Vec<String>,
}

fn paren(s: &str) -> String {
    format!("({s})")
}

fn bin(op: &str, a: &str, b: &str) -> String {
    format!(".bin .{op} {} {}", paren(a), paren(b))
}

impl<'p, 'a> Cx<'p, 'a> {
    fn value(&self, v: Val, sp: Span) -> R<String> {
        match v {
            Val::Term(t) => Ok(t),
            Val::Place { place, offset, ty } => {
                let n = value_bytes(&ty).ok_or((
                    sp,
                    format!(
                        "a spec reads `{}` as a whole; only value types are exported",
                        ty.name()
                    ),
                ))?;
                Ok(format!(".load {} {} {n} {}", paren(&place), paren(&offset), lean_ty(&ty, sp)?))
            }
            Val::Ghost { name, keys, ty } => {
                if matches!(ty, Ty::Map(..) | Ty::Arr(..) | Ty::Struct(_)) {
                    return Err((sp, format!("ghost `{name}` read as a whole")));
                }
                let keys: Vec<String> = keys.iter().map(|k| paren(k)).collect();
                Ok(format!(".ghost {name:?} [{}]", keys.join(", ")))
            }
        }
    }

    fn term(&mut self, e: &'a Expr<'a>) -> R<String> {
        let v = self.expr(e)?;
        self.value(v, e.span)
    }

    fn lit_u64(e: &Expr<'_>) -> Option<u64> {
        match &e.kind {
            ExprKind::Lit(l, _) => match &l.kind {
                LitKind::Number(n) => u64::try_from(*n).ok(),
                _ => None,
            },
            ExprKind::Tuple(xs) if xs.len() == 1 => match &xs[0] {
                SpannedOption::Some(x) => Self::lit_u64(x),
                SpannedOption::None(_) => None,
            },
            _ => None,
        }
    }

    fn ident(&mut self, n: &str, sp: Span) -> R<Val> {
        if self.bound.iter().rev().any(|b| b == n) {
            return Ok(Val::Term(format!(".var {n:?}")));
        }
        if !self.in_fn
            && let Some(f) = self.f
        {
            if let Some(i) = f.params.iter().position(|p| p.0 == n) {
                return Ok(Val::Term(format!(".arg {i}")));
            }
            if let Some(i) = f.rets.iter().position(|r| r.0 == n) {
                if !self.allow_result {
                    return Err((sp, format!("return value `{n}` outside a postcondition")));
                }
                return Ok(Val::Term(format!(".ret {i}")));
            }
            if n == "result" {
                if !self.allow_result || f.rets.len() != 1 {
                    return Err((
                        sp,
                        "`result` needs a postcondition of a function with one return value".into(),
                    ));
                }
                return Ok(Val::Term(".ret 0".into()));
            }
        }
        match n {
            "__time" => return Ok(Val::Term(".env .time".into())),
            "this" => return Ok(Val::Term(".env .self".into())),
            _ => {}
        }
        if let Some(sv) = self.c.state.iter().find(|s| s.name == n) {
            if sv.ghost_decl.is_some() {
                return Ok(Val::Ghost { name: n.into(), keys: vec![], ty: sv.ty.clone() });
            }
            if sv.ghost {
                return Err((sp, format!("model variable `{n}` is not exported")));
            }
            if sv.constant {
                if let Some(v) = sv.init.and_then(Self::lit_u64) {
                    return Ok(Val::Term(format!(".lit {v}")));
                }
                return Err((sp, format!("constant `{n}` without a literal value")));
            }
            if sv.immutable || sv.transient {
                return Err((sp, format!("`{n}` is immutable or transient: not in storage")));
            }
            if self.bytes_keys.iter().any(|b| b == n) {
                return Err((sp, format!("`{n}` has a `bytesN` mapping key")));
            }
            let s = self.layout.get(n).ok_or((sp, format!("no storage slot for `{n}`")))?;
            return Ok(Val::Place {
                place: format!(".slot {}", s.slot),
                offset: format!(".lit {}", s.offset),
                ty: s.ty.clone(),
            });
        }
        Err((sp, format!("unknown name `{n}` in a spec")))
    }

    fn index(&mut self, b: Val, key: String, sp: Span) -> R<Val> {
        Ok(match b {
            Val::Ghost { name, mut keys, ty: Ty::Map(_, v) } => {
                keys.push(key);
                Val::Ghost { name, keys, ty: *v }
            }
            Val::Place { place, ty: Ty::Map(kt, v), .. } => {
                if value_bytes(&kt).is_none() {
                    return Err((sp, format!("mapping key of type `{}` not exported", kt.name())));
                }
                Val::Place {
                    place: format!(".entry {} {}", paren(&place), paren(&key)),
                    offset: ".lit 0".into(),
                    ty: *v,
                }
            }
            Val::Place { place, ty: Ty::Arr(e, len), .. } => {
                if matches!(*e, Ty::Struct(_)) {
                    return Err((sp, "arrays of structs are not exported".into()));
                }
                match value_bytes(&e) {
                    Some(n) if n <= 16 => {
                        let per = 32 / n;
                        let lit = |v: u32| format!(".lit {v}");
                        let offset = bin("mul", &bin("mod", &key, &lit(per)), &lit(n));
                        let place = match len {
                            None => format!(".packedElem {} {} {per}", paren(&place), paren(&key)),
                            Some(_) => format!(
                                ".offset {} {}",
                                paren(&place),
                                paren(&bin("div", &key, &lit(per)))
                            ),
                        };
                        Val::Place { place, offset, ty: *e }
                    }
                    _ => {
                        let s = slots_of(self.p, &e, sp)?;
                        let place = match len {
                            None => format!(".elem {} {} {s}", paren(&place), paren(&key)),
                            Some(_) => format!(
                                ".offset {} {}",
                                paren(&place),
                                paren(&bin("mul", &key, &format!(".lit {s}")))
                            ),
                        };
                        Val::Place { place, offset: ".lit 0".into(), ty: *e }
                    }
                }
            }
            _ => return Err((sp, "indexing not exported".into())),
        })
    }

    fn expr(&mut self, e: &'a Expr<'a>) -> R<Val> {
        let sp = e.span;
        Ok(match &e.kind {
            ExprKind::Lit(l, _) => match &l.kind {
                LitKind::Number(n) => Val::Term(format!(".lit {n}")),
                LitKind::Bool(b) => Val::Term(format!(".lit {}", u8::from(*b))),
                _ => return Err((sp, "literal not exported".into())),
            },
            ExprKind::Ident(i) => self.ident(i.as_str(), sp)?,
            ExprKind::Tuple(xs) if let [SpannedOption::Some(x)] = &xs[..] => self.expr(x)?,
            ExprKind::Ternary(c, a, b) => {
                let (c, a, b) = (self.term(c)?, self.term(a)?, self.term(b)?);
                Val::Term(format!(".ite {} {} {}", paren(&c), paren(&a), paren(&b)))
            }
            ExprKind::Unary(op, x) => {
                let t = self.term(x)?;
                match op.kind {
                    UnOpKind::Not => Val::Term(format!(".un .not {}", paren(&t))),
                    UnOpKind::Neg => Val::Term(format!(".un .neg {}", paren(&t))),
                    k => return Err((sp, format!("operator `{}` not exported", k.to_str()))),
                }
            }
            ExprKind::Binary(l, op, r) => {
                use BinOpKind::*;
                if matches!(op.kind, Shl | Shr) {
                    let k = Self::lit_u64(r)
                        .ok_or((sp, "shift by a computed amount is not exported".to_string()))?;
                    let t = self.term(l)?;
                    let f = if op.kind == Shl { "shl" } else { "shr" };
                    return Ok(Val::Term(format!(".{f} {} {k}", paren(&t))));
                }
                let name = match op.kind {
                    Add => "add",
                    Sub => "sub",
                    Mul => "mul",
                    Div => "div",
                    Rem => "mod",
                    Pow => "pow",
                    BitAnd => "band",
                    BitOr => "bor",
                    BitXor => "bxor",
                    Eq => "eq",
                    Ne => "ne",
                    Lt => "lt",
                    Le => "le",
                    Gt => "gt",
                    Ge => "ge",
                    And => "and",
                    Or => "or",
                    k => return Err((sp, format!("operator `{}` not exported", k.to_str()))),
                };
                let (a, b) = (self.term(l)?, self.term(r)?);
                Val::Term(bin(name, &a, &b))
            }
            ExprKind::Member(base, m) => {
                let m = m.as_str();
                if let ExprKind::TypeCall(t) = &base.kind
                    && let TypeKind::Elementary(ElementaryType::UInt(n)) = &t.kind
                {
                    // `type(uintN).max` and `.min`: the type's bounds.
                    return match m {
                        "max" => Ok(Val::Term(format!(
                            ".lit {}",
                            crate::ty::pow2(u32::from(n.bits())) - 1
                        ))),
                        "min" => Ok(Val::Term(".lit 0".into())),
                        _ => Err((sp, format!("member `.{m}` of a type not exported"))),
                    };
                }
                if let ExprKind::Ident(b) = &base.kind {
                    match (b.as_str(), m) {
                        ("msg", "sender") => return Ok(Val::Term(".env .sender".into())),
                        ("msg", "value") => return Ok(Val::Term(".env .value".into())),
                        ("block", "timestamp") => return Ok(Val::Term(".env .timestamp".into())),
                        _ => {}
                    }
                }
                match self.expr(base)? {
                    Val::Place { place, ty: Ty::Arr(_, None), .. } if m == "length" => {
                        Val::Term(format!(".len {}", paren(&place)))
                    }
                    Val::Place { place, ty: Ty::Struct(sn), .. } => {
                        let fields = self
                            .p
                            .struct_fields(&sn)
                            .ok_or((sp, format!("unknown struct `{sn}`")))?
                            .clone();
                        let tys: Vec<Ty> = fields.iter().map(|f| f.1.clone()).collect();
                        let (pos, _) = pack(self.p, &tys, sp)?;
                        let i = fields
                            .iter()
                            .position(|f| f.0 == m)
                            .ok_or((sp, format!("no field `{m}` in `{sn}`")))?;
                        let (slot, off) = pos[i];
                        Val::Place {
                            place: format!(".offset {} (.lit {slot})", paren(&place)),
                            offset: format!(".lit {off}"),
                            ty: tys[i].clone(),
                        }
                    }
                    _ => return Err((sp, format!("member `.{m}` not exported"))),
                }
            }
            ExprKind::Index(base, IndexKind::Index(Some(k))) => {
                let b = self.expr(base)?;
                let key = self.term(k)?;
                self.index(b, key, sp)?
            }
            ExprKind::Call(callee, args) => {
                if let ExprKind::Type(t) = &callee.kind {
                    // A conversion of a literal that fits the target type, such as
                    // `address(0)`, is the literal's value; any other conversion is refused.
                    let bits = match &t.kind {
                        TypeKind::Elementary(ElementaryType::Address(_)) => 160,
                        TypeKind::Elementary(ElementaryType::UInt(n)) => u32::from(n.bits()),
                        _ => return Err((sp, "conversion not exported".into())),
                    };
                    let argv: Vec<&'a Expr<'a>> = args.exprs().collect();
                    return match argv[..] {
                        [a] => match Self::lit_u64(a) {
                            Some(v) if bits >= 64 || v < (1u64 << bits) => {
                                Ok(Val::Term(format!(".lit {v}")))
                            }
                            _ => Err((sp, "conversion of a non-literal not exported".into())),
                        },
                        _ => Err((sp, "conversion not exported".into())),
                    };
                }
                let ExprKind::Ident(fname) = &callee.kind else {
                    return Err((sp, "call not exported".into()));
                };
                let n = fname.as_str();
                let argv: Vec<&'a Expr<'a>> = args.exprs().collect();
                if n == "old" && argv.len() == 1 {
                    if !self.allow_old {
                        return Err((sp, "`old` outside a postcondition".into()));
                    }
                    let t = self.term(argv[0])?;
                    return Ok(Val::Term(format!(".old {}", paren(&t))));
                }
                if n == "__implies" && argv.len() == 2 {
                    let (a, b) = (self.term(argv[0])?, self.term(argv[1])?);
                    return Ok(Val::Term(bin("implies", &a, &b)));
                }
                if let Some(idx) = n.strip_prefix("__q").and_then(|i| i.parse::<usize>().ok()) {
                    let q = self.p.quants[idx].clone();
                    let mut tys = vec![];
                    for (tytxt, name) in &q.binders {
                        let ty =
                            parse_ty_text(tytxt).ok_or((sp, format!("binder type `{tytxt}`")))?;
                        tys.push((name.clone(), lean_ty(&ty, sp)?));
                        self.bound.push(name.clone());
                    }
                    let body = self.term(argv[0]);
                    self.bound.truncate(self.bound.len() - tys.len());
                    let mut t = body?;
                    let k = if q.forall { "all" } else { "ex" };
                    for (name, ty) in tys.iter().rev() {
                        t = format!(".{k} {name:?} {ty} {}", paren(&t));
                    }
                    return Ok(Val::Term(t));
                }
                let c = self.c;
                let f = c
                    .funcs
                    .iter()
                    .find(|f| f.name == n && f.params.len() == argv.len())
                    .ok_or((sp, format!("call of `{n}` not exported")))?;
                let mut ts = vec![];
                for a in &argv {
                    ts.push(paren(&self.term(a)?));
                }
                self.spec_fn(f, sp)?;
                Val::Term(format!(".call {n:?} [{}]", ts.join(", ")))
            }
            _ => return Err((sp, "expression not exported".into())),
        })
    }

    /// Exports a spec function the specs call, once.
    fn spec_fn(&mut self, f: &'p Func<'a>, sp: Span) -> R<()> {
        if self.fns.iter().any(|g| g.name == f.name) {
            return Ok(());
        }
        if self.visiting.contains(&f.name) {
            return Err((sp, format!("spec function `{}` is recursive", f.name)));
        }
        if self.c.funcs.iter().filter(|g| g.name == f.name).count() > 1 {
            return Err((sp, format!("overloaded spec function `{}`", f.name)));
        }
        let body = f.spec_body.ok_or((sp, format!("`{}` has no single-return body", f.name)))?;
        let [(_, ret)] = &f.rets[..] else {
            return Err((sp, format!("`{}` needs one return value", f.name)));
        };
        let mut params = vec![];
        for (n, t) in &f.params {
            params.push((n.clone(), lean_ty(t, sp)?));
        }
        let ret = lean_ty(ret, sp)?;
        self.visiting.push(f.name.clone());
        let saved = (std::mem::take(&mut self.bound), self.in_fn, self.allow_result);
        self.bound = f.params.iter().map(|p| p.0.clone()).collect();
        self.in_fn = true;
        self.allow_result = false;
        let b = self.term(body);
        (self.bound, self.in_fn, self.allow_result) = saved;
        self.visiting.pop();
        let body = b?;
        self.fns.push(SpecFnOut { name: f.name.clone(), params, ret, body, opaque: f.opaque });
        Ok(())
    }
}

/// The depth of nested spec function calls, with the functions in the order `spec_fn` exports
/// them (callees first): each one more than its deepest callee.
fn fuel(fns: &[SpecFnOut]) -> usize {
    let mut depth: FxHashMap<&str, usize> = FxHashMap::default();
    for f in fns {
        let d = fns
            .iter()
            .filter(|g| f.body.contains(&format!(".call {:?}", g.name)))
            .map(|g| depth.get(g.name.as_str()).copied().unwrap_or(0))
            .max()
            .unwrap_or(0);
        depth.insert(&f.name, d + 1);
    }
    depth.values().copied().max().unwrap_or(0)
}

/// Lays out a contract's state variables.
fn layout(p: &Program<'_>, c: &Contract<'_>, sp: Span) -> R<FxHashMap<String, Slot>> {
    let vars: Vec<_> = c
        .state
        .iter()
        .filter(|s| !s.ghost && !s.constant && !s.immutable && !s.transient)
        .collect();
    let tys: Vec<Ty> = vars.iter().map(|s| s.ty.clone()).collect();
    let (pos, _) = pack(p, &tys, sp)?;
    Ok(vars
        .iter()
        .zip(pos)
        .map(|(s, (slot, offset))| (s.name.clone(), Slot { slot, offset, ty: s.ty.clone() }))
        .collect())
}

/// Whether `f`'s body may assign `name`: an assignment, increment or `delete` of it anywhere in
/// the body, or any inline assembly (whose Yul assignments are not graded here).
fn assigns(f: &Func<'_>, name: &str) -> bool {
    let Some(body) = &f.item.body else { return false };
    let mut hit = false;
    for s in body.stmts.iter() {
        walk::all_stmts(s, &mut |st| {
            if matches!(st.kind, StmtKind::Assembly(..)) {
                hit = true;
            }
            walk::stmt_exprs(st, &mut |e| {
                walk::sub_exprs(e, &mut |x| {
                    let target = match &x.kind {
                        ExprKind::Assign(t, ..) | ExprKind::Delete(t) => Some(t),
                        ExprKind::Unary(op, t)
                            if matches!(
                                op.kind,
                                UnOpKind::PreInc
                                    | UnOpKind::PreDec
                                    | UnOpKind::PostInc
                                    | UnOpKind::PostDec
                            ) =>
                        {
                            Some(t)
                        }
                        _ => None,
                    };
                    if let Some(t) = target
                        && matches!(&t.kind, ExprKind::Ident(i) if i.as_str() == name)
                    {
                        hit = true;
                    }
                });
            });
        });
    }
    hit
}

fn q(s: &str) -> String {
    format!("{s:?}")
}

fn list(ts: &[String]) -> String {
    if ts.is_empty() {
        "[]".to_string()
    } else {
        let ts: Vec<String> = ts.iter().map(|t| paren(t)).collect();
        format!("[\n        {}]", ts.join(",\n        "))
    }
}

/// Exports `program`'s specs, or the first construct that cannot be exported.
pub(crate) fn export(program: &Program<'_>, unit: &SourceUnit<'_>, opts: &Opts) -> R<String> {
    let mut bytes_keys = vec![];
    for item in unit.items.iter() {
        if let ItemKind::Contract(c) = &item.kind {
            for it in c.body.iter() {
                if let ItemKind::Variable(v) = &it.kind
                    && bytes_key(&v.ty)
                    && let Some(n) = v.name
                {
                    bytes_keys.push(n.as_str().to_string());
                }
            }
        }
    }
    let mut out = String::new();
    let _ = writeln!(
        out,
        "import Verify.Spec.Contract\n\n/-!\nSpecs of `{}`, exported by solar-verify {} with `--emit-lean-spec`.\nGenerated: do not edit.\n-/\n\nnamespace {}\n\nopen SolidCore.Spec\n",
        opts.source,
        env!("CARGO_PKG_VERSION"),
        opts.module
    );
    let opts_txt: Vec<String> = opts.options.iter().map(|o| q(o)).collect();
    let _ = writeln!(
        out,
        "/-- What the specs were exported from and are about. -/\ndef manifest : Manifest :=\n  {{ source := {},\n    sourceSha256 := {},\n    compiler := {},\n    options := [{}],\n    bytecodeSha256 := {} }}\n",
        q(&opts.source),
        q(&opts.source_sha256),
        q(&opts.compiler),
        opts_txt.join(", "),
        q(&opts.bytecode_sha256)
    );
    let mut trusted: Vec<String> = vec![];
    for c in program.contracts.iter().filter(|c| !c.kind.is_interface() && !c.kind.is_library()) {
        let csp = c.funcs.first().map(|f| f.item.header.span).unwrap_or(Span::DUMMY);
        let lay = layout(program, c, csp)?;
        let mut cx = Cx {
            p: program,
            c,
            layout: &lay,
            bytes_keys: &bytes_keys,
            f: None,
            bound: vec![],
            in_fn: false,
            allow_old: false,
            allow_result: false,
            fns: vec![],
            visiting: vec![],
        };
        // Contract invariants, then struct invariants over every stored value of the struct.
        let mut invs = vec![];
        for s in &c.invariants {
            invs.push(cx.term(s.expr)?);
        }
        for sv in c.state.iter().filter(|s| !s.ghost && !s.constant && !s.immutable) {
            let (sn, key) = match &sv.ty {
                Ty::Struct(sn) => (sn.clone(), None),
                Ty::Map(k, v) if matches!(&**v, Ty::Struct(_)) => {
                    let Ty::Struct(sn) = &**v else { unreachable!() };
                    (sn.clone(), Some((**k).clone()))
                }
                ty => {
                    if let Some(sn) = struct_inside(ty)
                        && program.struct_invs.contains_key(&sn)
                    {
                        return Err((
                            csp,
                            format!(
                                "invariant of struct `{sn}` stored in `{}`: only struct and mapping-of-struct variables are exported",
                                sv.name
                            ),
                        ));
                    }
                    continue;
                }
            };
            let Some(sinvs) = program.struct_invs.get(&sn) else { continue };
            for s in sinvs {
                invs.push(struct_inv(&mut cx, &sv.name, &sn, key.as_ref(), s)?);
            }
        }
        // Function specs, for every external entry point.
        let mut specs = vec![];
        for f in c
            .funcs
            .iter()
            .filter(|f| f.is_entry() && !f.item.kind.is_constructor() && f.item.body.is_some())
        {
            let sp = f.item.header.span;
            if let Some(ck) = f.checks.first() {
                return Err((
                    ck.span,
                    "`@custom:check` is enforced only by --instrument: not exported".into(),
                ));
            }
            if f.trusted {
                trusted.push(format!("trusted function {}.{}", c.name, f.name));
            }
            if f.requires.is_empty()
                && f.ensures.is_empty()
                && f.succeeds.is_empty()
                && invs.is_empty()
            {
                continue;
            }
            let mut params = vec![];
            for v in f.item.header.parameters.vars.iter() {
                params.push(abi_ty(program, v, v.span)?);
            }
            let mut rets = vec![];
            if let Some(rs) = &f.item.header.returns {
                for v in rs.vars.iter() {
                    rets.push(abi_ty(program, v, v.span)?);
                }
            }
            for (pn, _) in &f.params {
                if assigns(f, pn) && f.ensures.iter().any(|s| mentions(s, pn)) {
                    return Err((
                        sp,
                        format!("an ensures reads parameter `{pn}`, which the body assigns"),
                    ));
                }
            }
            cx.f = Some(f);
            let requires = specs_of(&mut cx, &f.requires, false)?;
            let ensures = specs_of(&mut cx, &f.ensures, true)?;
            let succeeds = specs_of(&mut cx, &f.succeeds, false)?;
            cx.f = None;
            let sig = f
                .item
                .header
                .parameters
                .vars
                .iter()
                .map(|v| abi_name(program, v))
                .collect::<Option<Vec<_>>>()
                .ok_or((sp, "parameter type without an ABI name".to_string()))?;
            let sel = selector(&format!("{}({})", f.name, sig.join(",")));
            let payable = f.item.header.state_mutability() == ast::StateMutability::Payable;
            specs.push(format!(
                "{{ name := {},\n      selector := 0x{sel:08x},\n      params := [{}],\n      rets := [{}],\n      payable := {payable},\n      requires := {},\n      ensures := {},\n      succeedsIf := {} }}",
                q(&format!("{}.{}", c.name, f.name)),
                params.join(", "),
                rets.join(", "),
                list(&requires),
                list(&ensures),
                list(&succeeds)
            ));
        }
        let ghosts: Vec<String> =
            c.state.iter().filter(|s| s.ghost_decl.is_some()).map(|s| q(&s.name)).collect();
        let fuel = fuel(&cx.fns);
        let fns_txt: Vec<String> = cx
            .fns
            .iter()
            .map(|f| {
                let ps: Vec<String> = f.params.iter().map(|(n, t)| format!("({}, {t})", q(n))).collect();
                format!(
                    "{{ name := {},\n      params := [{}],\n      ret := {},\n      body := {},\n      isOpaque := {} }}",
                    q(&f.name),
                    ps.join(", "),
                    f.ret,
                    f.body,
                    f.opaque
                )
            })
            .collect();
        let invs: Vec<String> = invs.iter().map(|t| paren(t)).collect();
        let _ = writeln!(
            out,
            "/-- The specs of `{name}`. -/\ndef {name} : Bundle where\n  fuel := {fuel}\n  spec :=\n  {{ name := {},\n    fns := [\n    {}],\n    invariants := [\n    {}],\n    ghosts := [{}],\n    specs := [\n    {}] }}\n\n/-- Every spec function call resolves and nests within the fuel. -/\ntheorem {name}_wf : {name}.WF = true := by decide +kernel\n",
            q(&c.name),
            fns_txt.join(",\n    "),
            invs.join(",\n    "),
            ghosts.join(", "),
            specs.join(",\n    "),
            name = c.name,
        );
    }
    let mut assumes: Vec<String> = program
        .stmt_specs
        .values()
        .flatten()
        .filter(|(tag, _)| tag == "assume")
        .map(|(_, s)| format!("@custom:assume {}", s.text))
        .collect();
    assumes.sort();
    trusted.extend(assumes);
    let trusted: Vec<String> = trusted.iter().map(|t| q(t)).collect();
    let _ = writeln!(
        out,
        "/-- Trusted-base items these specs' proofs rely on beyond the verdict\n(`SolidCore.Spec.Assumptions`). -/\ndef trusted : List String := [{}]\n\nend {}",
        trusted.join(", "),
        opts.module
    );
    Ok(out)
}

fn specs_of<'p, 'a>(cx: &mut Cx<'p, 'a>, list: &[Spec<'a>], post: bool) -> R<Vec<String>> {
    cx.allow_old = post;
    cx.allow_result = post;
    let r = list.iter().map(|s| cx.term(s.expr)).collect::<R<Vec<_>>>();
    cx.allow_old = false;
    cx.allow_result = false;
    r
}

fn struct_inside(ty: &Ty) -> Option<String> {
    match ty {
        Ty::Struct(n) => Some(n.clone()),
        Ty::Map(_, v) | Ty::Arr(v, _) => struct_inside(v),
        _ => None,
    }
}

/// A struct invariant over the struct stored in `var`, or in every entry of the mapping `var`
/// (keyed by `key`): its field names read the stored fields.
fn struct_inv<'p, 'a>(
    cx: &mut Cx<'p, 'a>,
    var: &str,
    sn: &str,
    key: Option<&Ty>,
    s: &Spec<'a>,
) -> R<String> {
    let sp = s.span;
    let base = cx.layout.get(var).ok_or((sp, format!("no slot for `{var}`")))?;
    let place = match key {
        None => format!(".slot {}", base.slot),
        Some(_) => format!(".entry (.slot {}) (.var \"__key\")", base.slot),
    };
    let fields = cx.p.struct_fields(sn).ok_or((sp, format!("unknown struct `{sn}`")))?.clone();
    let tys: Vec<Ty> = fields.iter().map(|f| f.1.clone()).collect();
    let (pos, _) = pack(cx.p, &tys, sp)?;
    let mut fld = vec![];
    for (i, (n, ty)) in fields.iter().enumerate() {
        let (slot, off) = pos[i];
        let v = Val::Place {
            place: format!(".offset ({place}) (.lit {slot})"),
            offset: format!(".lit {off}"),
            ty: ty.clone(),
        };
        fld.push((n.clone(), cx.value(v, sp)?));
    }
    // Bind the field names as variables, then replace each by the field's stored value.
    cx.bound.extend(fld.iter().map(|f| f.0.clone()));
    let r = cx.term(s.expr);
    cx.bound.truncate(cx.bound.len() - fld.len());
    let mut t = r?;
    for (n, v) in &fld {
        t = t.replace(&format!(".var {n:?}"), &paren(v));
    }
    Ok(match key {
        None => t,
        Some(k) => format!(".all \"__key\" {} {}", lean_ty(k, sp)?, paren(&t)),
    })
}

/// Whether a spec mentions an identifier.
fn mentions(s: &Spec<'_>, name: &str) -> bool {
    let mut hit = false;
    walk::sub_exprs(s.expr, &mut |x| {
        if matches!(&x.kind, ExprKind::Ident(i) if i.as_str() == name) {
            hit = true;
        }
    });
    hit
}

/// The canonical ABI name of a parameter's type.
fn abi_name<'a>(p: &Program<'a>, v: &'a ast::VariableDefinition<'a>) -> Option<String> {
    Some(match &v.ty.kind {
        TypeKind::Elementary(ElementaryType::FixedBytes(n)) => format!("bytes{}", n.bytes()),
        _ => match crate::vc::ty_of(p, &v.ty) {
            Ty::Bool => "bool".into(),
            Ty::Uint(n) => format!("uint{n}"),
            Ty::Int(n) => format!("int{n}"),
            Ty::Addr | Ty::Contract(_) => "address".into(),
            Ty::Enum(..) => "uint8".into(),
            _ => return None,
        },
    })
}

/// The 4-byte selector of a signature.
fn selector(sig: &str) -> u32 {
    let h = alloy_primitives::keccak256(sig.as_bytes());
    u32::from_be_bytes([h[0], h[1], h[2], h[3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selectors() {
        assert_eq!(selector("transfer(address,uint256)"), 0xa9059cbb);
    }
}

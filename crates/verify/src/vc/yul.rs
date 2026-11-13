//! Inline assembly (Yul).
//!
//! Yul values are uint256 words in the encoder's Int encoding. Arithmetic follows the EVM
//! (wrapping mod 2^256, division by zero gives 0); comparisons give 0/1 flags; bitwise operations
//! use the encoder's bitvector view. Words are kept as linear forms over atoms where possible
//! (constant + sum of k * atom), so the usual slot idioms (shl/or/mstore of a seed and an address)
//! normalise to one canonical term and keccak inputs assembled from byte slices of memory come out
//! as plain arithmetic, e.g. keccak256 over owner || seed is keccak(32, owner * 2^96 + seed).
//!
//! Memory is modelled per assembly block with concrete offsets only: byte -> (word, byte index).
//! Reading memory the block did not write gives a fresh word (sound: unknown). A Solidity memory
//! struct referenced in assembly is a pointer to a region holding its fields as ABI words. Raw
//! storage (sload/sstore) is the ghost state variable __raw : mapping(uint256 => uint256), assumed
//! disjoint from the declared state variables.
use super::{Env, R, Var, Vc};
use crate::{
    prog::{Contract, Func, Program},
    ty::{Ty, num, pow2},
    walk,
};
use num_bigint::BigInt;
use num_traits::{One, Signed, Zero};
use solar_parse::{
    ast::{self, Expr, ExprKind, LitKind, StmtKind, yul},
    interface::{Span, data_structures::map::FxHashMap, sym},
};
use std::collections::{BTreeMap, BTreeSet};

type Iv = (BigInt, BigInt);

fn w256() -> BigInt {
    pow2(256)
}

/// Linear form: c + sum k * atom (atoms are SMT terms the form does not look into).
#[derive(Clone, Debug, PartialEq)]
struct Lin {
    c: BigInt,
    ts: BTreeMap<String, BigInt>,
}

impl Lin {
    fn k(c: BigInt) -> Self {
        Self { c, ts: BTreeMap::new() }
    }
    fn atom(t: &str) -> Self {
        let mut ts = BTreeMap::new();
        ts.insert(t.to_string(), BigInt::one());
        Self { c: BigInt::zero(), ts }
    }
    fn konst(&self) -> Option<BigInt> {
        if self.ts.is_empty() { Some(self.c.clone()) } else { None }
    }
    fn add(&self, o: &Self, sign: i32) -> Self {
        let mut r = self.clone();
        let s = BigInt::from(sign);
        r.c += &o.c * &s;
        for (t, k) in &o.ts {
            let e = r.ts.entry(t.clone()).or_insert_with(BigInt::zero);
            *e += k * &s;
        }
        r.ts.retain(|_, k| !k.is_zero());
        r
    }
    fn scale(&self, k: &BigInt) -> Self {
        if k.is_zero() {
            return Self::k(BigInt::zero());
        }
        Self { c: &self.c * k, ts: self.ts.iter().map(|(t, x)| (t.clone(), x * k)).collect() }
    }
    fn render(&self) -> String {
        let mut parts: Vec<String> = self
            .ts
            .iter()
            .map(|(t, k)| if k.is_one() { t.clone() } else { format!("(* {} {t})", num(k)) })
            .collect();
        if !self.c.is_zero() || parts.is_empty() {
            parts.push(num(&self.c));
        }
        if parts.len() == 1 { parts.pop().unwrap() } else { format!("(+ {})", parts.join(" ")) }
    }
}

// ------------------------------------------------------------------ tiny s-expressions
enum S {
    A(String),
    L(Vec<Self>),
}

fn tokenize(s: &str) -> Vec<String> {
    let b: Vec<char> = s.chars().collect();
    let (mut i, mut out) = (0, vec![]);
    while i < b.len() {
        let c = b[i];
        if c == '(' || c == ')' {
            out.push(c.to_string());
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '|' {
            let mut j = i + 1;
            while j < b.len() && b[j] != '|' {
                j += 1;
            }
            out.push(b[i..=j.min(b.len() - 1)].iter().collect());
            i = j + 1;
        } else {
            let mut j = i;
            while j < b.len() && !b[j].is_whitespace() && b[j] != '(' && b[j] != ')' {
                j += 1;
            }
            out.push(b[i..j].iter().collect());
            i = j;
        }
    }
    out
}

fn parse_at(t: &[String], i: &mut usize) -> Option<S> {
    let tok = t.get(*i)?;
    *i += 1;
    if tok == "(" {
        let mut v = vec![];
        while t.get(*i)? != ")" {
            v.push(parse_at(t, i)?);
        }
        *i += 1;
        Some(S::L(v))
    } else {
        Some(S::A(tok.clone()))
    }
}

fn parse(s: &str) -> Option<S> {
    if s.len() > 100_000 {
        return None;
    }
    let t = tokenize(s);
    let mut i = 0;
    let r = parse_at(&t, &mut i)?;
    if i == t.len() { Some(r) } else { None }
}

fn pr(s: &S) -> String {
    match s {
        S::A(a) => a.clone(),
        S::L(v) => format!("({})", v.iter().map(pr).collect::<Vec<_>>().join(" ")),
    }
}

fn numeral(s: &S) -> Option<BigInt> {
    match s {
        S::A(a) => a.parse().ok(),
        S::L(v) => match v.as_slice() {
            [S::A(m), x] if m == "-" => numeral(x).map(|n| -n),
            _ => None,
        },
    }
}

/// Per-block context of the Yul executor.
struct Yc<'a> {
    mem: BTreeMap<u64, (usize, u8)>,
    words: Vec<String>,
    /// Solidity memory structs used as pointers: name, base, byte length
    regions: Vec<(String, u64, u64)>,
    funcs: Vec<FxHashMap<String, &'a yul::Function<'a>>>,
    scopes: Vec<Vec<String>>,
    leaves: Vec<Vec<(String, Env)>>,
    /// ranges of terms known from Solidity types (binder symbols in quantified specs)
    extra: FxHashMap<String, Iv>,
    /// xor results and their operands, to cancel x ^ (x ^ y)
    xors: FxHashMap<String, (String, String)>,
    depth: usize,
    /// the free memory pointer read at the start of the block (`mload(0x40)`), if any: memory at
    /// `fmp + k` is free (memory-safe assembly may use it) and modelled at FREE_BASE + k
    fmp: Option<String>,
    /// Solidity memory/calldata pointers (bytes, strings, arrays) read in this block
    ptrs: FxHashMap<String, String>,
}

const SCRATCH_END: u64 = 0x80;
const REGION_BASE: u64 = 1 << 32;
/// model address of the free memory pointer's target (see Yc::fmp)
const FREE_BASE: u64 = 1 << 36;
const FREE_LEN: u64 = 1 << 20;

fn unsupported<T>(span: Span, what: impl Into<String>) -> R<T> {
    Err((span, what.into()))
}

impl<'p, 'a> Vc<'p, 'a> {
    // ------------------------------------------------------------------ term analysis
    fn y_lin(&self, t: &str) -> Lin {
        match parse(t) {
            Some(s) => self.y_lin_s(&s, 0),
            None => Lin::atom(t),
        }
    }
    fn y_lin_s(&self, s: &S, depth: usize) -> Lin {
        if let Some(n) = numeral(s) {
            return Lin::k(n);
        }
        match s {
            S::A(a) => {
                if depth < 8
                    && let Some(d) = self.defs.get(a)
                    && (d.starts_with("(+ ") || d.starts_with("(* ") || d.starts_with("(- "))
                    && let Some(ds) = parse(d)
                {
                    let l = self.y_lin_s(&ds, depth + 1);
                    if l.ts.keys().all(|k| !k.contains(' ')) {
                        return l;
                    }
                }
                Lin::atom(a)
            }
            S::L(v) => match v.as_slice() {
                [S::A(op), rest @ ..] if op == "+" && !rest.is_empty() => {
                    let mut acc = Lin::k(BigInt::zero());
                    for x in rest {
                        acc = acc.add(&self.y_lin_s(x, depth), 1);
                    }
                    acc
                }
                [S::A(op), x] if op == "-" => self.y_lin_s(x, depth).scale(&BigInt::from(-1)),
                [S::A(op), x, rest @ ..] if op == "-" && !rest.is_empty() => {
                    let mut acc = self.y_lin_s(x, depth);
                    for y in rest {
                        acc = acc.add(&self.y_lin_s(y, depth), -1);
                    }
                    acc
                }
                [S::A(op), rest @ ..] if op == "*" && !rest.is_empty() => {
                    let consts: Vec<BigInt> = rest.iter().filter_map(numeral).collect();
                    if consts.len() == rest.len() {
                        return Lin::k(consts.iter().fold(BigInt::one(), |a, b| a * b));
                    }
                    if consts.len() + 1 == rest.len() {
                        let k = consts.iter().fold(BigInt::one(), |a, b| a * b);
                        let other = rest.iter().find(|x| numeral(x).is_none()).unwrap();
                        return self.y_lin_s(other, depth).scale(&k);
                    }
                    Lin::atom(&pr(s))
                }
                _ => Lin::atom(&pr(s)),
            },
        }
    }
    /// A 0/1 flag term (ite b 1 0) as its condition b.
    fn y_flag(&self, t: &str) -> Option<String> {
        let s = parse(t)?;
        match &s {
            S::A(a) if a == "0" => Some("false".into()),
            S::A(a) if a == "1" => Some("true".into()),
            S::A(a) => {
                let d = self.defs.get(a)?;
                if d.starts_with("(ite ") { self.y_flag(d) } else { None }
            }
            S::L(v) => match v.as_slice() {
                [S::A(i), c, S::A(one), S::A(zero)] if i == "ite" && one == "1" && zero == "0" => {
                    Some(pr(c))
                }
                _ => None,
            },
        }
    }
    fn y_konst(&self, t: &str) -> Option<BigInt> {
        if let Ok(n) = t.parse::<BigInt>() {
            return Some(n);
        }
        let l = self.y_lin(t);
        if let Some(k) = l.konst() {
            return Some(k);
        }
        match self.iv(t) {
            Some((lo, hi)) if lo == hi => Some(lo),
            _ => None,
        }
    }
    fn y_iv_atom(&self, cx: &Yc<'a>, t: &str) -> Option<Iv> {
        if let Some(r) = cx.extra.get(t) {
            return Some(r.clone());
        }
        self.iv(t)
    }
    fn y_lin_iv(&self, cx: &Yc<'a>, l: &Lin) -> Option<Iv> {
        let (mut lo, mut hi) = (l.c.clone(), l.c.clone());
        for (t, k) in &l.ts {
            let (a, b) = self.y_iv_atom(cx, t)?;
            let (x, y) = (k * &a, k * &b);
            if k.is_negative() {
                lo += y;
                hi += x;
            } else {
                lo += x;
                hi += y;
            }
        }
        Some((lo, hi))
    }
    fn y_fits(&self, cx: &Yc<'a>, l: &Lin, m: &BigInt) -> bool {
        matches!(self.y_lin_iv(cx, l), Some((lo, hi)) if !lo.is_negative() && &hi < m)
    }
    /// x mod m for a power of two m, as a linear form when the ranges allow it.
    fn y_modp(&mut self, cx: &Yc<'a>, l: Lin, m: &BigInt) -> Lin {
        if self.y_fits(cx, &l, m) {
            return l;
        }
        let mut r = Lin { c: ((&l.c % m) + m) % m, ts: BTreeMap::new() };
        for (t, k) in &l.ts {
            let k2 = ((k % m) + m) % m;
            if !k2.is_zero() {
                r.ts.insert(t.clone(), k2);
            }
        }
        if self.y_fits(cx, &r, m) {
            return r;
        }
        Lin::atom(&format!("(mod {} {})", r.render(), num(m)))
    }
    fn y_wrap(&mut self, cx: &Yc<'a>, l: Lin) -> String {
        self.y_modp(cx, l, &w256()).render()
    }
    /// floor(w / 2^s) mod 2^n of a word w in [0, 2^256).
    fn y_slice(&mut self, cx: &Yc<'a>, w: &Lin, s: u32, n: u32) -> Lin {
        if s >= 256 || n == 0 {
            return Lin::k(BigInt::zero());
        }
        let p = pow2(s);
        let q = if s == 0 {
            w.clone()
        } else {
            let cl = ((&w.c % &p) + &p) % &p;
            let ch = (&w.c - &cl) / &p;
            let mut hi = Lin::k(ch);
            let mut lo = Lin::k(cl);
            for (t, k) in &w.ts {
                if (k % &p).is_zero() {
                    hi.ts.insert(t.clone(), k / &p);
                } else {
                    lo.ts.insert(t.clone(), k.clone());
                }
            }
            if self.y_fits(cx, &lo, &p) {
                hi
            } else {
                Lin::atom(&format!("(div {} {})", w.render(), num(&p)))
            }
        };
        self.y_modp(cx, q, &pow2(n))
    }
    fn y_flagt(b: &str) -> String {
        match b {
            "true" => "1".into(),
            "false" => "0".into(),
            _ => format!("(ite {b} 1 0)"),
        }
    }
    fn y_cond(&self, t: &str) -> String {
        if let Some(b) = self.y_flag(t) {
            return b;
        }
        match self.y_konst(t) {
            Some(k) => (if k.is_zero() { "false" } else { "true" }).into(),
            None => format!("(not (= {t} 0))"),
        }
    }
    fn y_signed(&self, cx: &Yc<'a>, t: &str) -> String {
        let l = self.y_lin(t);
        if self.y_fits(cx, &l, &pow2(255)) {
            return t.to_string();
        }
        format!("(ite (< {t} {}) {t} (- {t} {}))", num(&pow2(255)), num(&w256()))
    }
    fn y_bv(&mut self, op: &str, a: &str, b: &str) -> String {
        let (x, y) = (self.bv_term(a, 256), self.bv_term(b, 256));
        format!("(bv2nat ({op} {x} {y}))")
    }
    /// Binder symbols of the quantifiers being evaluated: terms mentioning them cannot be named.
    /// (binder symbols are the only ones spelled `|name!N|`, see `quant`).
    fn y_closed(&self, t: &str) -> bool {
        self.bound.is_empty() || !t.contains('!')
    }
    /// The env key of raw (or transient) storage: its own name in contract code, `@host.` in
    /// library code inlined into a contract.
    fn raw_key(&self, transient: bool) -> Option<String> {
        let base = if transient { "__traw" } else { "__raw" };
        if self.env.contains_key(base) {
            return Some(base.to_string());
        }
        let h = format!("@host.{base}");
        self.env.contains_key(&h).then_some(h)
    }

    fn y_fresh(&mut self, span: Span, what: &str) -> R<String> {
        if self.spec {
            return unsupported(span, format!("`{what}` in assembly evaluated inside a spec"));
        }
        Ok(self.decl(what, &Ty::Uint(256)))
    }

    // ------------------------------------------------------------------ keccak
    /// keccak256 of `len` bytes whose big-endian value is `val`: one uninterpreted function shared
    /// by assembly and Solidity's keccak256(abi.encode(..)), injective through inverses and with
    /// results in [2^128, 2^256) (hashes never collide with small constant slots).
    fn keccak_term(&mut self, len: &BigInt, val: &str) -> String {
        if self.declared_funs.insert("sv_keccak/2".into()) {
            self.smt.send("(declare-fun sv_keccak (Int Int) Int)\n(declare-fun sv_keccak_len (Int) Int)\n(declare-fun sv_keccak_val (Int) Int)\n");
            self.ret_bound("sv_keccak", &Ty::Uint(256));
        }
        let t = format!("(sv_keccak {} {val})", num(len));
        // Injectivity and range as ground facts of each hash term (a quantified axiom makes the
        // solver's satisfiability checks time out). A hash inside a quantified spec mentions its
        // binders and gets no facts; its instances share the facts of equal terms in the body.
        if self.bound.is_empty() || self.y_closed(&t) {
            let x = self.fresh("keccak");
            self.smt.send(&format!(
                "(declare-const {x} Int)\n(assert (= {x} {t}))\n(assert (and (= (sv_keccak_len {x}) {}) (= (sv_keccak_val {x}) {val}) (<= {} {x}) (<= {x} {})))\n",
                num(len),
                num(&pow2(128)),
                num(&(w256() - 1))
            ));
            self.ranges.insert(x.clone(), (pow2(128), w256() - 1));
            self.keccak_in.insert(x.clone(), (len.clone(), val.to_string()));
            return x;
        }
        t
    }

    // ------------------------------------------------------------------ .slot
    /// The slot of state variable `n`, as assembly reads it with `n.slot`: a constant of its own,
    /// below 2^128 and distinct from every other such slot. Only variables that fill whole slots
    /// take one (so no two share a slot), and the slots are symbolic, so a proof holds for any
    /// storage layout, the compiler's included.
    fn slot_const(&mut self, n: &str) -> String {
        let t = format!("|slot.{n}|");
        if !self.slot_consts.contains(&t) {
            let hi = pow2(128);
            let mut d = format!(
                "(declare-const {t} Int)\n(assert (and (<= 0 {t}) (< {t} {})))\n",
                num(&hi)
            );
            for o in &self.slot_consts {
                d.push_str(&format!("(assert (not (= {t} {o})))\n"));
            }
            self.smt.send(&d);
            self.ranges.insert(t.clone(), (BigInt::zero(), hi - 1));
            self.slot_consts.push(t.clone());
        }
        t
    }

    /// `n.slot` / `n.offset` of a state variable in assembly, or None when `n` is not one.
    fn y_state_slot(&mut self, cx: &Yc<'a>, span: Span, n: &str, f: &str) -> R<Option<String>> {
        if Self::is_local(cx, n) || self.aliases.contains_key(n) || self.slocals.contains(n) {
            return Ok(None);
        }
        let Some(sv) = self.c.state.iter().find(|s| s.name == n) else { return Ok(None) };
        if sv.constant || sv.immutable || sv.ghost {
            return Ok(None);
        }
        if sv.transient {
            return unsupported(span, format!("assembly `{n}.{f}` of a transient variable"));
        }
        let whole = matches!(sv.ty, Ty::Map(..) | Ty::Arr(_, None) | Ty::Uint(256) | Ty::Int(256));
        if !whole {
            return unsupported(
                span,
                format!(
                    "assembly `{n}.{f}`: `{n}` ({}) may share its slot or span several",
                    sv.ty.name()
                ),
            );
        }
        if f == "offset" {
            return Ok(Some("0".into()));
        }
        if self.spec {
            return unsupported(span, format!("assembly `{n}.slot` inside a spec evaluation"));
        }
        Ok(Some(self.slot_const(n)))
    }

    /// Where raw storage address `p` lies when assembly built it from a `.slot`: the state
    /// variable and the mapping keys, one per `keccak256(key . slot)` level, innermost first.
    fn slot_path(&self, p: &str, depth: usize) -> Option<(String, Vec<String>)> {
        if depth > 8 {
            return None;
        }
        if let Some(n) = p.strip_prefix("|slot.").and_then(|s| s.strip_suffix('|')) {
            return Some((n.to_string(), vec![]));
        }
        let (len, val) = self.keccak_in.get(p)?;
        if *len != BigInt::from(64) {
            return None;
        }
        let l = self.y_lin(val);
        let w = w256();
        // the low word is one slot term, the high word the key
        for (atom, k) in &l.ts {
            if !k.is_one() {
                continue;
            }
            let Some((n, mut keys)) = self.slot_path(atom, depth + 1) else { continue };
            let mut rest = l.clone();
            rest.ts.remove(atom);
            if !(&rest.c % &w).is_zero() || rest.ts.values().any(|k| !(k % &w).is_zero()) {
                return None;
            }
            let key = Lin {
                c: &rest.c / &w,
                ts: rest.ts.iter().map(|(t, k)| (t.clone(), k / &w)).collect(),
            };
            keys.push(key.render());
            return Some((n, keys));
        }
        None
    }

    /// Whether `p` mentions a `.slot`, directly or through the input of a keccak it holds.
    fn slot_mention(&self, p: &str, depth: usize) -> bool {
        depth <= 8
            && tokenize(p).iter().any(|t| {
                t.starts_with("|slot.")
                    || self.keccak_in.get(t).is_some_and(|(_, v)| self.slot_mention(v, depth + 1))
            })
    }

    /// `sload(p)` (`store` None) or `sstore(p, v)` where assembly built `p` from a state
    /// variable's `.slot`: the declared variable's own storage, a mapping entry when `p` is
    /// `keccak256(key . slot)` (nested for nested mappings). A key outside the key type's range
    /// names no entry Solidity can reach, so that access stays raw storage. None when `p`
    /// comes from no `.slot`; an error when it does in a way not modelled.
    fn y_slot_access(&mut self, span: Span, addr: &str, store: Option<&str>) -> R<Option<String>> {
        let Some((n, keys)) = self.slot_path(addr, 0) else {
            if self.slot_mention(addr, 0) {
                return unsupported(
                    span,
                    "assembly storage access at an address derived from a `.slot` other than the variable itself or `keccak256(key . slot)` of a mapping",
                );
            }
            return Ok(None);
        };
        if self.spec {
            return unsupported(
                span,
                "assembly storage access through `.slot` inside a spec evaluation",
            );
        }
        let vkey = if self.env.contains_key(&n) { n.clone() } else { format!("@host.{n}") };
        let Some(var) = self.env.get(&vkey).cloned() else {
            return unsupported(span, format!("assembly storage access to `{n}` out of scope"));
        };
        let mut ty = var.ty.clone();
        let mut conds = vec![];
        let mut sel = vec![var.t.clone()];
        for k in &keys {
            let Ty::Map(kt, vt) = ty else {
                return unsupported(
                    span,
                    format!("assembly hashes `{n}.slot`, which is no mapping at that depth"),
                );
            };
            match *kt {
                Ty::Uint(256) => {}
                Ty::Addr | Ty::Contract(_) => conds.push(format!("(< {k} {})", num(&pow2(160)))),
                other => {
                    return unsupported(
                        span,
                        format!(
                            "assembly access to `{n}` through `.slot` with a key of type {}",
                            other.name()
                        ),
                    );
                }
            }
            let cur = sel.last().unwrap().clone();
            sel.push(format!("(select {cur} {k})"));
            ty = *vt;
        }
        let signed = match ty {
            Ty::Uint(256) => false,
            Ty::Int(256) => true,
            t => {
                return unsupported(
                    span,
                    format!(
                        "assembly access to `{n}` through `.slot`: a value of type {} is not one whole word",
                        t.name()
                    ),
                );
            }
        };
        let inr = if conds.is_empty() {
            "true".to_string()
        } else {
            format!("(and {})", conds.join(" "))
        };
        let rkey = self.raw_key(false).ok_or((
            span,
            "raw storage (sload/sstore) is only modelled in contracts".to_string(),
        ))?;
        let raw = self.env[&rkey].clone();
        let w = num(&w256());
        match store {
            None => {
                let leaf = sel.last().unwrap();
                let word = if signed {
                    format!("(ite (< {leaf} 0) (+ {leaf} {w}) {leaf})")
                } else {
                    leaf.clone()
                };
                let x = self.fresh("sload");
                self.smt.send(&format!(
                    "(declare-const {x} Int)\n(assert (= {x} (ite {inr} {word} (select {} {addr}))))\n(assert {})\n",
                    raw.t,
                    Ty::Uint(256).range(&x).unwrap()
                ));
                self.ranges.insert(x.clone(), (BigInt::zero(), w256() - 1));
                Ok(Some(x))
            }
            Some(v) => {
                let half = num(&pow2(255));
                let mut acc = if signed {
                    format!("(ite (< {v} {half}) {v} (- {v} {w}))")
                } else {
                    v.to_string()
                };
                for (i, k) in keys.iter().enumerate().rev() {
                    acc = format!("(store {} {k} {acc})", sel[i]);
                }
                let nv = Var {
                    ty: var.ty.clone(),
                    t: format!("(ite {inr} {acc} {})", var.t),
                    len: var.len,
                };
                self.bind(&vkey, var.ty, nv);
                let nr = format!("(ite {inr} {} (store {} {addr} {v}))", raw.t, raw.t);
                self.bind(&rkey, raw.ty.clone(), Var { ty: raw.ty, t: nr, len: None });
                Ok(Some(String::new()))
            }
        }
    }

    /// The ABI words of a value of a static type (structs expanded into their fields).
    fn abi_words(&self, ty: &Ty, t: &str, out: &mut Vec<String>) -> bool {
        match ty {
            Ty::Bool => out.push(format!("(ite {t} 1 0)")),
            Ty::Uint(_) | Ty::Addr | Ty::Contract(_) | Ty::Enum(..) => out.push(t.to_string()),
            Ty::Lit => match t.parse::<BigInt>() {
                Ok(n) if !n.is_negative() && n < w256() => out.push(t.to_string()),
                _ => return false,
            },
            Ty::Int(_) => {
                let pos = self.iv(t).is_some_and(|(l, _)| !l.is_negative());
                out.push(if pos { t.to_string() } else { format!("(mod {t} {})", num(&w256())) })
            }
            Ty::Struct(sn) => {
                let Some(fs) = self.prog.struct_fields(sn) else {
                    return false;
                };
                for (f, fty) in fs {
                    if !self.abi_words(fty, &format!("(|{sn}.{f}| {t})"), out) {
                        return false;
                    }
                }
            }
            _ => return false,
        }
        true
    }

    fn keccak_words(&mut self, words: &[String], tys: &[Ty]) -> String {
        let k = words.len();
        let mut acc = Lin::k(BigInt::zero());
        let mut cx = Yc::new();
        for (w, ty) in words.iter().zip(tys) {
            if let Some(b) = ty.bounds()
                && !w.contains(' ')
            {
                cx.extra.insert(w.clone(), b);
            }
        }
        let mut fits = vec![];
        for (i, w) in words.iter().enumerate() {
            let l = self.y_lin(w);
            let f = self.y_fits(&cx, &l, &w256());
            fits.push(f);
            let l = if f { l } else { Lin::atom(w) };
            acc = acc.add(&l.scale(&pow2(256 * (k - 1 - i) as u32)), 1);
        }
        let v = acc.render();
        let x = self.keccak_term(&BigInt::from(32 * k), &v);
        // Word projections: for a k-word preimage whose words are each below 2^256, word i is
        // a function of the preimage value, hence of the hash. Stating it as an uninterpreted
        // projection lets the solver get field equality from hash equality by congruence,
        // instead of reasoning about sums of 2^(256*k)-sized terms.
        if k > 1 && !x.contains(' ') {
            for (i, w) in words.iter().enumerate() {
                if !fits[i] {
                    continue;
                }
                let f = format!("sv_keccak_w{k}_{i}");
                if self.declared_funs.insert(format!("{f}/1")) {
                    self.smt.send(&format!(
                        "(declare-fun {f} (Int) Int)
"
                    ));
                }
                self.smt.send(&format!(
                    "(assert (= ({f} {x}) {w}))
"
                ));
            }
        }
        x
    }

    /// Solidity keccak256(..): abi.encode of static values goes through the shared keccak;
    /// abi.encodePacked has its own uninterpreted function; anything else (strings, bytes,
    /// dynamic types) an injective-per-arity uninterpreted function of the flattened arguments.
    /// A hash argument as an Int term: a bool as 0/1; an array or struct (whose contents we do not
    /// encode) as a fresh value, so its hash is an arbitrary value (equal contents are not known to
    /// hash equally: an over-approximation).
    fn hash_arg(&mut self, v: &Var) -> String {
        match v.ty.sort().as_str() {
            "Int" => v.t.clone(),
            "Bool" => format!("(ite {} 1 0)", v.t),
            _ => self.decl("hashed", &Ty::Opaque),
        }
    }

    pub(super) fn keccak_solidity(&mut self, argv: &[&'a Expr<'a>]) -> R<Var> {
        let abi_call = |a: &'a Expr<'a>| -> Option<(&'a str, &'a ast::CallArgs<'a>)> {
            if let ExprKind::Call(c2, inner) = &a.kind
                && let ExprKind::Member(b, m) = &c2.kind
                && matches!(&b.kind, ExprKind::Ident(i) if i.name == sym::abi)
            {
                return Some((m.as_str(), inner));
            }
            None
        };
        let mut flat = vec![];
        let mut packed = false;
        let mut words: Option<(Vec<String>, Vec<Ty>)> = None;
        for a in argv {
            match abi_call(a) {
                Some((m, inner)) => {
                    let mut ws = Some((vec![], vec![]));
                    for x in inner.exprs() {
                        let v = self.eval(x)?;
                        if let Some((w, tys)) = ws.as_mut() {
                            let n0 = w.len();
                            if !self.abi_words(&v.ty, &v.t, w) {
                                ws = None;
                            } else {
                                let ty = if matches!(
                                    v.ty,
                                    Ty::Uint(_) | Ty::Addr | Ty::Contract(_) | Ty::Enum(..)
                                ) && w.len() == n0 + 1
                                {
                                    v.ty.clone()
                                } else {
                                    Ty::Opaque
                                };
                                while tys.len() < w.len() {
                                    tys.push(ty.clone());
                                }
                            }
                        }
                        let t = self.hash_arg(&v);
                        flat.push(t);
                    }
                    if m == "encode" && argv.len() == 1 {
                        words = ws;
                    }
                    packed |= m == "encodePacked";
                }
                None => {
                    let v = self.eval(a)?;
                    let t = self.hash_arg(&v);
                    flat.push(t)
                }
            }
        }
        let ty = Ty::Uint(256);
        if let Some((ws, tys)) = words
            && !ws.is_empty()
        {
            let t = self.keccak_words(&ws, &tys);
            return Ok(Var { ty, t, len: None });
        }
        let f = if packed { "sv_keccak256_packed" } else { "sv_keccak256" };
        self.declare_fun(f, flat.len(), "Int");
        self.ret_bound(f, &ty);
        self.injective(f, flat.len());
        let t = if flat.is_empty() { f.to_string() } else { format!("({f} {})", flat.join(" ")) };
        if self.bound.is_empty() {
            self.smt.send(&format!("(assert {})\n", ty.range(&t).unwrap()));
        }
        Ok(Var { ty, t, len: None })
    }

    // ------------------------------------------------------------------ memory
    fn mem_write(
        &mut self,
        cx: &mut Yc<'a>,
        span: Span,
        off: u64,
        val: String,
        from: u8,
        internal: bool,
    ) -> R<()> {
        let n = 32 - from as u64;
        if !internal && !Self::y_scratch(off, n) {
            if cx.regions.iter().any(|(_, b, l)| off < b + l && off + n > *b) {
                return unsupported(span, "assembly writes into a Solidity memory variable");
            }
            return unsupported(
                span,
                format!(
                    "assembly writes memory at 0x{off:x} outside the scratch space (0x00-0x7f); only memory-safe scratch use is modelled"
                ),
            );
        }
        cx.words.push(val);
        let id = cx.words.len() - 1;
        for i in 0..n {
            cx.mem.insert(off + i, (id, from + i as u8));
        }
        Ok(())
    }
    /// The big-endian value of memory bytes [off, off + len).
    fn mem_read(&mut self, cx: &mut Yc<'a>, span: Span, off: u64, len: u64) -> R<Lin> {
        if len > 4096 {
            return unsupported(span, "assembly reads a memory range longer than 4096 bytes");
        }
        // unwritten bytes: fresh words, kept so later reads agree
        let mut i = off;
        while i < off + len {
            if cx.mem.contains_key(&i) {
                i += 1;
                continue;
            }
            let mut j = i;
            while j < off + len && j - i < 32 && !cx.mem.contains_key(&j) {
                j += 1;
            }
            let w = self.y_fresh(span, "mload")?;
            cx.words.push(w);
            let id = cx.words.len() - 1;
            let k = j - i;
            for b in 0..k {
                cx.mem.insert(i + b, (id, (32 - k + b) as u8));
            }
            i = j;
        }
        let mut acc = Lin::k(BigInt::zero());
        let mut i = off;
        while i < off + len {
            let (id, from) = cx.mem[&i];
            let mut j = i + 1;
            while j < off + len && (j - i) < 32 && cx.mem[&j] == (id, from + (j - i) as u8) {
                j += 1;
            }
            let to = from as u64 + (j - i);
            let wl = self.y_lin(&cx.words[id].clone());
            let piece = if from == 0 && to == 32 {
                wl
            } else {
                self.y_slice(cx, &wl, 8 * (32 - to as u32), 8 * (to - from as u64) as u32)
            };
            let after = off + len - j;
            acc = acc.add(&piece.scale(&pow2(8 * after as u32)), 1);
            i = j;
        }
        Ok(acc)
    }
    /// A memory address: a constant, or the free memory pointer plus a small constant.
    fn y_addr(&self, cx: &Yc<'a>, t: &str) -> Option<u64> {
        let l = self.y_lin(t);
        if let Some(k) = l.konst() {
            return (!k.is_negative() && k < BigInt::from(1u64 << 40))
                .then(|| u64::try_from(k).unwrap());
        }
        let p = cx.fmp.as_ref()?;
        if l.ts.len() == 1
            && l.ts.get(p).is_some_and(|k| k.is_one())
            && !l.c.is_negative()
            && l.c < BigInt::from(FREE_LEN / 2)
        {
            return Some(FREE_BASE + u64::try_from(&l.c).unwrap());
        }
        None
    }

    /// Memory the block may overwrite without touching Solidity data: the scratch space and
    /// the free memory past the free memory pointer.
    fn y_scratch(off: u64, len: u64) -> bool {
        off.saturating_add(len) <= SCRATCH_END
            || (off >= FREE_BASE && off.saturating_add(len) <= FREE_BASE + FREE_LEN)
    }

    /// An instruction that writes `len` unknown bytes at `off` (call output, returndatacopy..).
    fn y_clobber(&mut self, cx: &mut Yc<'a>, span: Span, off: &str, len: &str) -> R<()> {
        let n = self.y_konst(len);
        if n.as_ref().is_some_and(|n| n.is_zero()) {
            return Ok(());
        }
        match (self.y_addr(cx, off), n) {
            (Some(o), Some(n)) if !n.is_negative() && n <= BigInt::from(4096) => {
                let n = u64::try_from(n).unwrap();
                if !Self::y_scratch(o, n) {
                    return unsupported(
                        span,
                        "assembly copies data into memory outside the scratch space or free memory",
                    );
                }
                for i in o..o + n {
                    cx.mem.remove(&i);
                }
                Ok(())
            }
            (Some(o), _) if (FREE_BASE..FREE_BASE + FREE_LEN).contains(&o) => {
                // into free memory, of unknown length: all of free memory from o is unknown
                cx.mem.retain(|k, _| *k < o || *k >= FREE_BASE + FREE_LEN);
                Ok(())
            }
            _ => unsupported(
                span,
                "assembly copies data into memory at a symbolic location or of symbolic length",
            ),
        }
    }

    fn y_offset(&self, cx: &Yc<'a>, span: Span, t: &str, what: &str) -> R<u64> {
        if let Some(o) = self.y_addr(cx, t) {
            return Ok(o);
        }
        match self.y_konst(t) {
            Some(k) if !k.is_negative() && k < BigInt::from(1u64 << 40) => {
                Ok(u64::try_from(k).unwrap())
            }
            Some(_) => unsupported(span, format!("assembly {what} is out of the modelled range")),
            None => unsupported(
                span,
                format!("assembly {what} is symbolic; only constant memory offsets are supported"),
            ),
        }
    }

    // ------------------------------------------------------------------ statements
    pub(super) fn exec_asm(&mut self, a: &'a ast::StmtAssembly<'a>, span: Span) -> R<()> {
        if let Some(d) = &a.dialect
            && d.value.as_str() != "evmasm"
        {
            return unsupported(
                span,
                format!("assembly dialect {:?} is not supported", d.value.as_str()),
            );
        }
        let mut cx = Yc::new();
        self.y_block(&mut cx, &a.block)
    }

    fn y_block(&mut self, cx: &mut Yc<'a>, b: &'a yul::Block<'a>) -> R<()> {
        let mut fs = FxHashMap::default();
        for s in b.stmts.iter() {
            if let yul::StmtKind::FunctionDef(f) = &s.kind {
                fs.insert(f.name.as_str().to_string(), f);
            }
        }
        cx.funcs.push(fs);
        cx.scopes.push(vec![]);
        let mut r = Ok(());
        for s in b.stmts.iter() {
            if self.guard == "false" && !self.spec {
                break;
            }
            r = self.y_stmt(cx, s);
            if r.is_err() {
                break;
            }
        }
        cx.funcs.pop();
        for n in cx.scopes.pop().unwrap_or_default() {
            self.env.remove(&n);
        }
        r
    }

    fn is_local(cx: &Yc<'a>, n: &str) -> bool {
        cx.scopes.iter().any(|s| s.iter().any(|x| x == n))
    }

    fn y_set_local(&mut self, n: &str, t: String) {
        if self.spec {
            self.env.insert(n.to_string(), Var { ty: Ty::Uint(256), t, len: None });
        } else {
            self.bind(n, Ty::Uint(256), Var { ty: Ty::Uint(256), t, len: None });
        }
    }

    fn y_stmt(&mut self, cx: &mut Yc<'a>, s: &'a yul::Stmt<'a>) -> R<()> {
        use yul::StmtKind as K;
        match &s.kind {
            K::Block(b) => self.y_block(cx, b),
            K::FunctionDef(_) => Ok(()),
            K::VarDecl(names, init) => {
                let vals = match init {
                    Some(e) => self.y_multi(cx, e)?,
                    None => vec!["0".to_string(); names.len()],
                };
                if vals.len() != names.len() {
                    return unsupported(s.span, "assembly declaration arity mismatch");
                }
                for (n, v) in names.iter().zip(vals) {
                    let n = n.as_str().to_string();
                    self.y_set_local(&n, v);
                    cx.scopes.last_mut().unwrap().push(n);
                }
                Ok(())
            }
            K::AssignSingle(p, e) => {
                let v = self.y_expr(cx, e)?;
                self.y_assign(cx, s.span, p, v)
            }
            K::AssignMulti(ps, e) => {
                let vals = self.y_multi(cx, e)?;
                if vals.len() != ps.len() {
                    return unsupported(s.span, "assembly assignment arity mismatch");
                }
                for (p, v) in ps.iter().zip(vals) {
                    self.y_assign(cx, s.span, p, v)?;
                }
                Ok(())
            }
            K::Expr(e) => {
                let vs = self.y_multi(cx, e)?;
                if !vs.is_empty() {
                    return unsupported(s.span, "assembly expression statement returns values");
                }
                Ok(())
            }
            K::If(c, b) => {
                if self.spec {
                    return unsupported(
                        s.span,
                        "assembly `if` (branching) inside a spec evaluation",
                    );
                }
                let cv = self.y_expr(cx, c)?;
                let cond = self.y_cond(&cv);
                if cond == "false" {
                    return Ok(());
                }
                let (g0, env0) = (self.guard.clone(), self.env.clone());
                self.assume(&cond);
                self.y_block(cx, b)?;
                let s1 = (
                    std::mem::replace(&mut self.guard, g0),
                    std::mem::replace(&mut self.env, env0),
                );
                self.assume(&format!("(not {cond})"));
                let s2 = (self.guard.clone(), self.env.clone());
                let (g, e) = self.merge(vec![s1, s2]);
                self.guard = g;
                self.env = e;
                Ok(())
            }
            K::Switch(sw) => {
                if self.spec {
                    return unsupported(
                        s.span,
                        "assembly `switch` (branching) inside a spec evaluation",
                    );
                }
                let sel = self.y_expr(cx, &sw.selector)?;
                let (g0, env0) = (self.guard.clone(), self.env.clone());
                let mut states = vec![];
                let mut nots = vec![];
                let mut has_default = false;
                for case in sw.cases.iter() {
                    self.guard = g0.clone();
                    self.env = env0.clone();
                    match &case.constant {
                        Some(l) => {
                            let k = self.y_lit(s.span, l)?;
                            let c = format!("(= {sel} {k})");
                            self.assume(&c);
                            nots.push(format!("(not {c})"));
                        }
                        None => {
                            has_default = true;
                            for n in nots.clone() {
                                self.assume(&n);
                            }
                        }
                    }
                    self.y_block(cx, &case.body)?;
                    states.push((self.guard.clone(), self.env.clone()));
                }
                if !has_default {
                    self.guard = g0;
                    self.env = env0;
                    for n in nots {
                        self.assume(&n);
                    }
                    states.push((self.guard.clone(), self.env.clone()));
                }
                let (g, e) = self.merge(states);
                self.guard = g;
                self.env = e;
                Ok(())
            }
            K::For(_) => unsupported(s.span, "assembly `for` loops are not supported"),
            K::Break | K::Continue => {
                unsupported(s.span, "assembly break/continue are not supported")
            }
            K::Leave => {
                if self.spec {
                    return unsupported(s.span, "assembly `leave` inside a spec evaluation");
                }
                let st = (self.guard.clone(), self.env.clone());
                cx.leaves
                    .last_mut()
                    .ok_or((s.span, "`leave` outside a function".to_string()))?
                    .push(st);
                self.guard = "false".into();
                Ok(())
            }
        }
    }

    fn y_path(&self, span: Span, p: &ast::AstPath<'_>) -> R<String> {
        let segs = p.segments();
        if segs.len() != 1 {
            return unsupported(
                span,
                format!(
                    "assembly access `{}` (.slot/.offset/.length) is not supported",
                    segs.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(".")
                ),
            );
        }
        Ok(segs[0].as_str().to_string())
    }

    fn y_assign(
        &mut self,
        cx: &mut Yc<'a>,
        span: Span,
        p: &'a ast::AstPath<'a>,
        v: String,
    ) -> R<()> {
        let n = self.y_path(span, p)?;
        if Self::is_local(cx, &n) {
            self.y_set_local(&n, v);
            return Ok(());
        }
        if self.aliases.contains_key(&n) {
            return unsupported(span, format!("assembly assignment to storage pointer `{n}`"));
        }
        let Some(var) = self.env.get(&n).cloned() else {
            return unsupported(span, format!("unknown assembly identifier `{n}`"));
        };
        let l = self.y_lin(&v);
        let t = match &var.ty {
            Ty::Opaque => {
                // a bytes/string pointer set from assembly: its contents become unknown
                cx.ptrs.insert(n.clone(), v);
                let t = self.decl("asmbytes", &Ty::Opaque);
                self.bind(&n, Ty::Opaque, Var { ty: Ty::Opaque, t, len: None });
                return Ok(());
            }
            Ty::Bool => self.y_cond(&v),
            Ty::Uint(256) => v,
            // any word: a call through it must show it holds a function (`call_ptr`)
            Ty::Fn(_) => v,
            Ty::Enum(en, _) => {
                // the word must name a variant, as the enum's range is assumed of it everywhere
                if self.spec {
                    return unsupported(
                        span,
                        format!("assembly assignment to enum `{n}` in a spec"),
                    );
                }
                let r = var.ty.range(&v).unwrap();
                let why = format!(
                    "assembly may assign `{n}` a word outside enum `{en}`, which the model cannot hold"
                );
                self.must_hold(&r, &why, span)?;
                v
            }
            Ty::Uint(k) => self.y_modp(cx, l, &pow2(*k as u32)).render(),
            Ty::Addr | Ty::Contract(_) => self.y_modp(cx, l, &pow2(160)).render(),
            Ty::Int(k) => {
                let k = *k as u32;
                let low = self.y_modp(cx, l, &pow2(k)).render();
                format!("(ite (< {low} {}) {low} (- {low} {}))", num(&pow2(k - 1)), num(&pow2(k)))
            }
            t => {
                return unsupported(
                    span,
                    format!("assembly assignment to `{n}` of type {}", t.name()),
                );
            }
        };
        let ty = var.ty;
        self.bind(&n, ty.clone(), Var { ty, t, len: None });
        Ok(())
    }

    fn y_from_solidity(&mut self, cx: &mut Yc<'a>, span: Span, n: &str, v: &Var) -> R<String> {
        match &v.ty {
            Ty::Bool => Ok(Self::y_flagt(&v.t)),
            Ty::Uint(_) | Ty::Addr | Ty::Contract(_) | Ty::Enum(..) => {
                if let Some(b) = v.ty.bounds()
                    && !v.t.contains(' ')
                {
                    cx.extra.insert(v.t.clone(), b);
                }
                Ok(v.t.clone())
            }
            Ty::Fn(_) => Ok(v.t.clone()),
            Ty::Lit => match self.y_konst(&v.t) {
                Some(k) => Ok(num(&(((k % w256()) + w256()) % w256()))),
                None => Ok(format!("(mod {} {})", v.t, num(&w256()))),
            },
            Ty::Int(_) => {
                let pos = self.iv(&v.t).is_some_and(|(l, _)| !l.is_negative());
                Ok(if pos { v.t.clone() } else { format!("(mod {} {})", v.t, num(&w256())) })
            }
            Ty::Struct(sn) => {
                if let Some((_, b, _)) = cx.regions.iter().find(|(r, _, _)| r == n) {
                    return Ok(b.to_string());
                }
                let fields = self.prog.struct_fields(sn).cloned().unwrap_or_default();
                let base = REGION_BASE + (cx.regions.len() as u64) * 0x10000;
                for (i, (f, fty)) in fields.iter().enumerate() {
                    let mut ws = vec![];
                    if !self.abi_words(fty, &format!("(|{sn}.{f}| {})", v.t), &mut ws)
                        || ws.len() != 1
                    {
                        return unsupported(
                            span,
                            format!(
                                "assembly use of memory struct `{n}` with a non-value field `{f}`"
                            ),
                        );
                    }
                    self.mem_write(cx, span, base + 32 * i as u64, ws.pop().unwrap(), 0, true)?;
                }
                cx.regions.push((n.to_string(), base, 32 * fields.len() as u64));
                Ok(base.to_string())
            }
            Ty::Opaque | Ty::Arr(..) => {
                // a memory pointer: an arbitrary word, the same within the block; memory behind
                // it is not tracked (reads give arbitrary words, writes are unsupported)
                if let Some(p) = cx.ptrs.get(n) {
                    return Ok(p.clone());
                }
                let p = self.y_fresh(span, "ptr")?;
                let hi = BigInt::from(1u64 << 32);
                self.smt.send(&format!(
                    "(assert (< {p} {hi}))
"
                ));
                self.ranges.insert(p.clone(), (BigInt::zero(), hi - 1));
                cx.ptrs.insert(n.to_string(), p.clone());
                Ok(p)
            }
            t => unsupported(
                span,
                format!(
                    "assembly use of `{n}` of type {} (memory/calldata pointers other than structs are not supported)",
                    t.name()
                ),
            ),
        }
    }

    fn y_read(&mut self, cx: &mut Yc<'a>, span: Span, n: &str) -> R<String> {
        if Self::is_local(cx, n) {
            return self
                .env
                .get(n)
                .map(|v| v.t.clone())
                .ok_or((span, format!("assembly variable `{n}` read out of scope")));
        }
        if self.aliases.contains_key(n) {
            return unsupported(span, format!("assembly use of storage pointer `{n}`"));
        }
        if let Some(v) = self.env.get(n).cloned() {
            return self.y_from_solidity(cx, span, n, &v);
        }
        let c = self.c;
        if let Some(sv) = c.state.iter().find(|s| s.name == n && s.constant) {
            let init = sv.init.ok_or((span, "constant without initializer".to_string()))?;
            let v = self.eval(init)?;
            let v = Var { ty: if v.ty == Ty::Lit { Ty::Lit } else { sv.ty.clone() }, ..v };
            return self.y_from_solidity(cx, span, n, &v);
        }
        unsupported(span, format!("unknown assembly identifier `{n}`"))
    }

    fn y_lit(&self, span: Span, l: &ast::Lit<'_>) -> R<String> {
        match &l.kind {
            LitKind::Number(n) => Ok(n.to_string()),
            LitKind::Address(a) => {
                let s = a.to_string();
                let b = BigInt::parse_bytes(s.trim_start_matches("0x").as_bytes(), 16)
                    .unwrap_or_default();
                Ok(num(&b))
            }
            LitKind::Bool(b) => Ok(if *b { "1" } else { "0" }.into()),
            LitKind::Str(_, bytes, _) => {
                let b = bytes.as_byte_str();
                if b.len() > 32 {
                    return unsupported(span, "assembly string literal longer than 32 bytes");
                }
                let v = BigInt::from_bytes_be(num_bigint::Sign::Plus, b) << (8 * (32 - b.len()));
                Ok(num(&v))
            }
            _ => unsupported(span, "unsupported assembly literal"),
        }
    }

    fn y_expr(&mut self, cx: &mut Yc<'a>, e: &'a yul::Expr<'a>) -> R<String> {
        let mut v = self.y_multi(cx, e)?;
        if v.len() != 1 {
            return unsupported(e.span, "assembly expression must have exactly one value");
        }
        Ok(v.pop().unwrap())
    }

    fn y_multi(&mut self, cx: &mut Yc<'a>, e: &'a yul::Expr<'a>) -> R<Vec<String>> {
        match &e.kind {
            yul::ExprKind::Lit(l) => Ok(vec![self.y_lit(e.span, l)?]),
            yul::ExprKind::Path(p) => {
                let segs = p.segments();
                if let [v, f] = segs {
                    let (v, f) = (v.as_str(), f.as_str());
                    if (f == "slot" || f == "offset")
                        && let Some(t) = self.y_state_slot(cx, e.span, v, f)?
                    {
                        return Ok(vec![t]);
                    }
                    if (f == "offset" || f == "length")
                        && !Self::is_local(cx, v)
                        && let Some(var) = self.env.get(v).cloned()
                        && matches!(var.ty, Ty::Opaque | Ty::Arr(..))
                    {
                        // a calldata slice: where it starts is unknown; its length is
                        // the Solidity length when we track it
                        if f == "length"
                            && let Some(l) = var.len
                        {
                            return Ok(vec![l]);
                        }
                        let key = format!("{v}.{f}");
                        if let Some(p) = cx.ptrs.get(&key) {
                            return Ok(vec![p.clone()]);
                        }
                        let p = self.y_fresh(e.span, "cdptr")?;
                        cx.ptrs.insert(key, p.clone());
                        return Ok(vec![p]);
                    }
                }
                let n = self.y_path(e.span, p)?;
                Ok(vec![self.y_read(cx, e.span, &n)?])
            }
            yul::ExprKind::Call(c) => self.y_call(cx, e.span, c),
        }
    }

    fn y_user_call(
        &mut self,
        cx: &mut Yc<'a>,
        span: Span,
        f: &'a yul::Function<'a>,
        args: Vec<String>,
    ) -> R<Vec<String>> {
        if cx.depth > 16 {
            return unsupported(span, format!("recursive assembly function `{}`", f.name.as_str()));
        }
        if args.len() != f.parameters.len() {
            return unsupported(span, "assembly function arity mismatch");
        }
        // the function body sees only its parameters and returns: hide the caller's locals
        let hidden: Vec<String> = cx.scopes.iter().flatten().cloned().collect();
        let mut stash = vec![];
        let names: Vec<String> =
            f.parameters.iter().chain(f.returns.iter()).map(|i| i.as_str().to_string()).collect();
        for n in hidden.iter().chain(names.iter()) {
            if let Some(v) = self.env.remove(n) {
                stash.push((n.clone(), v));
            }
        }
        let saved_scopes = std::mem::replace(&mut cx.scopes, vec![names.clone()]);
        cx.leaves.push(vec![]);
        cx.depth += 1;
        for (p, a) in f.parameters.iter().zip(args) {
            self.y_set_local(p.as_str(), a);
        }
        for r in f.returns.iter() {
            self.y_set_local(r.as_str(), "0".into());
        }
        let r = self.y_block(cx, &f.body);
        cx.depth -= 1;
        let mut exits = cx.leaves.pop().unwrap_or_default();
        let out = r.map(|_| {
            exits.push((self.guard.clone(), self.env.clone()));
            let (g, e) = self.merge(exits);
            self.guard = g;
            self.env = e;
            f.returns
                .iter()
                .map(|r| self.env.get(r.as_str()).map(|v| v.t.clone()).unwrap_or("0".into()))
                .collect::<Vec<_>>()
        });
        cx.scopes = saved_scopes;
        for n in &names {
            self.env.remove(n);
        }
        for (n, v) in stash {
            self.env.insert(n, v);
        }
        out
    }

    /// xor with peepholes: flags, constants, x ^ x = 0, x ^ (x ^ y) = y, and pushed through
    /// ite(c, p, q) so the branchless select idioms (min/max) become plain ite terms.
    fn y_xor(&mut self, cx: &mut Yc<'a>, a: &str, b: &str, depth: usize) -> String {
        if let (Some(x), Some(y)) = (self.y_flag(a), self.y_flag(b)) {
            return Self::y_flagt(&format!("(xor {x} {y})"));
        }
        let (k0, k1) = (self.y_konst(a), self.y_konst(b));
        if let (Some(x), Some(y)) = (&k0, &k1) {
            return num(&(x ^ y));
        }
        if k0.as_ref().is_some_and(|k| k.is_zero()) {
            return b.to_string();
        }
        if k1.as_ref().is_some_and(|k| k.is_zero()) {
            return a.to_string();
        }
        if a == b {
            return "0".into();
        }
        let ops = |t: &str| -> Option<(String, String)> {
            cx.xors
                .get(t)
                .cloned()
                .or_else(|| self.defs.get(t).and_then(|d| cx.xors.get(d).cloned()))
        };
        for (x, y) in [(a, b), (b, a)] {
            if let Some((p, q)) = ops(y) {
                if p == x {
                    return q;
                }
                if q == x {
                    return p;
                }
            }
        }
        if depth < 4 {
            for (x, y) in [(a, b), (b, a)] {
                let d = self.defs.get(y).cloned().unwrap_or_else(|| y.to_string());
                if let Some(S::L(v)) = parse(&d)
                    && let [S::A(i), c, p, q] = v.as_slice()
                    && i == "ite"
                {
                    let (c, p, q) = (pr(c), pr(p), pr(q));
                    let l = self.y_xor(cx, x, &p, depth + 1);
                    let r = self.y_xor(cx, x, &q, depth + 1);
                    return if l == r { l } else { format!("(ite {c} {l} {r})") };
                }
            }
        }
        let t = self.y_bv("bvxor", a, b);
        cx.xors.insert(t.clone(), (a.to_string(), b.to_string()));
        t
    }

    /// `sload(p.slot)` / `sstore(p.slot, v)` on a local storage pointer `p` to a struct that
    /// fits one slot: the word is the struct's fields packed as Solidity lays them out (first
    /// field in the lowest bytes), so a store writes every field and a load packs them.
    fn y_struct_slot(
        &mut self,
        cx: &mut Yc<'a>,
        span: Span,
        name: &str,
        c: &'a yul::ExprCall<'a>,
    ) -> R<Option<Vec<String>>> {
        if name != "sload" && name != "sstore" {
            return Ok(None);
        }
        let Some(first) = c.arguments.first() else {
            return Ok(None);
        };
        let yul::ExprKind::Path(p) = &first.kind else {
            return Ok(None);
        };
        let segs = p.segments();
        if segs.len() != 2 || segs[1].name != sym::slot {
            return Ok(None);
        }
        let ptr = segs[0].as_str().to_string();
        if !self.aliases.contains_key(&ptr) {
            return Ok(None);
        }
        let cur = self.with_alias(&ptr, |s, t| s.eval(t))?;
        let Ty::Struct(sn) = cur.ty.clone() else {
            return Ok(None);
        };
        let fields = self.prog.struct_fields(&sn).cloned().unwrap_or_default();
        // byte width of each field, packed from offset 0; the whole struct must fit one slot
        let mut layout = vec![];
        let mut off = 0u32;
        for (f, ty) in &fields {
            let bytes = match ty {
                Ty::Bool => 1,
                Ty::Uint(n) | Ty::Int(n) => (*n as u32) / 8,
                Ty::Addr | Ty::Contract(_) => 20,
                Ty::Enum(_, k) if *k <= 256 => 1,
                _ => {
                    return unsupported(
                        span,
                        format!(
                            "assembly `{name}({ptr}.slot)`: field `{f}` of `{sn}` is not a packed value type"
                        ),
                    );
                }
            };
            if off + bytes > 32 {
                return unsupported(
                    span,
                    format!(
                        "assembly `{name}({ptr}.slot)`: struct `{sn}` spans more than one slot"
                    ),
                );
            }
            layout.push((f.clone(), ty.clone(), off * 8, bytes * 8));
            off += bytes;
        }
        if name == "sload" {
            if self.spec && c.arguments.len() != 1 {
                return Ok(None);
            }
            let mut terms = vec![];
            for (f, ty, shift, bits) in &layout {
                let v = format!("(|{sn}.{f}| {})", cur.t);
                let word = match ty {
                    Ty::Bool => format!("(ite {v} 1 0)"),
                    Ty::Int(_) => format!("(ite (< {v} 0) (+ {v} {}) {v})", num(&pow2(*bits))),
                    _ => v,
                };
                terms.push(format!("(* {word} {})", num(&pow2(*shift))));
            }
            let w =
                if terms.is_empty() { "0".to_string() } else { format!("(+ {})", terms.join(" ")) };
            let x = self.fresh("sload");
            self.smt.send(&format!("(declare-const {x} Int)\n(assert (= {x} {w}))\n"));
            return Ok(Some(vec![x]));
        }
        if self.spec {
            return unsupported(span, "assembly `sstore` inside a spec evaluation");
        }
        let Some(val) = c.arguments.get(1) else {
            return Ok(None);
        };
        let v = self.y_expr(cx, val)?;
        let mut args = vec![];
        for (_, ty, shift, bits) in &layout {
            let raw = format!("(mod (div {v} {}) {})", num(&pow2(*shift)), num(&pow2(*bits)));
            args.push(match ty {
                Ty::Bool => format!("(not (= {raw} 0))"),
                Ty::Int(_) => format!(
                    "(ite (< {raw} {}) {raw} (- {raw} {}))",
                    num(&pow2(bits - 1)),
                    num(&pow2(*bits))
                ),
                _ => raw,
            });
        }
        let nb = Var { ty: cur.ty, t: format!("(|mk.{sn}| {})", args.join(" ")), len: None };
        self.with_alias(&ptr, |s, t| s.assign(t, nb))?;
        Ok(Some(vec![]))
    }

    fn y_call(&mut self, cx: &mut Yc<'a>, span: Span, c: &'a yul::ExprCall<'a>) -> R<Vec<String>> {
        let name = c.name.as_str();
        if let Some(r) = self.y_struct_slot(cx, span, name, c)? {
            return Ok(r);
        }
        // arguments are evaluated right to left, as the EVM dialect does
        let mut a = vec![];
        for x in c.arguments.iter().rev() {
            a.push(self.y_expr(cx, x)?);
        }
        a.reverse();
        if let Some(f) = cx.funcs.iter().rev().find_map(|m| m.get(name).copied()) {
            return self.y_user_call(cx, span, f, a);
        }
        let nargs = a.len();
        let want = |k: usize| -> R<()> {
            if nargs == k {
                Ok(())
            } else {
                unsupported(span, format!("assembly `{name}` expects {k} arguments"))
            }
        };
        let w = w256();
        let one = |t: String| -> R<Vec<String>> { Ok(vec![t]) };
        match name {
            "add" | "sub" => {
                want(2)?;
                let l =
                    self.y_lin(&a[0]).add(&self.y_lin(&a[1]), if name == "add" { 1 } else { -1 });
                one(self.y_wrap(cx, l))
            }
            "mul" => {
                want(2)?;
                for (f, o) in [(0, 1), (1, 0)] {
                    if let Some(b) = self.y_flag(&a[f]) {
                        return one(if b == "true" {
                            a[o].clone()
                        } else if b == "false" {
                            "0".into()
                        } else {
                            format!("(ite {b} {} 0)", a[o])
                        });
                    }
                }
                let (x, y) = (self.y_lin(&a[0]), self.y_lin(&a[1]));
                if let Some(k) = x.konst() {
                    return one(self.y_wrap(cx, y.scale(&k)));
                }
                if let Some(k) = y.konst() {
                    return one(self.y_wrap(cx, x.scale(&k)));
                }
                let t = format!("(* {} {})", a[0], a[1]);
                let fits =
                    matches!(self.y_iv_atom(cx, &t), Some((lo, hi)) if !lo.is_negative() && hi < w);
                one(if fits { t } else { format!("(mod {t} {})", num(&w)) })
            }
            "div" | "mod" => {
                want(2)?;
                let x = self.y_lin(&a[0]);
                match self.y_konst(&a[1]) {
                    Some(k) if k.is_zero() => one("0".into()),
                    Some(k) if k.is_positive() && (&k & (&k - 1u32)).is_zero() => {
                        let s = (k.bits() - 1) as u32;
                        let r = if name == "div" {
                            if s == 0 { x } else { self.y_slice(cx, &x, s, 256 - s) }
                        } else {
                            self.y_slice(cx, &x, 0, s)
                        };
                        one(r.render())
                    }
                    Some(k) => one(format!("({name} {} {})", a[0], num(&k))),
                    None => one(format!("(ite (= {1} 0) 0 ({name} {0} {1}))", a[0], a[1])),
                }
            }
            "sdiv" | "smod" => {
                want(2)?;
                let (x, y) = (self.y_signed(cx, &a[0]), self.y_signed(cx, &a[1]));
                let q = format!(
                    "(* (ite (< {y} 0) (- 1) 1) (ite (>= {x} 0) (div {x} (abs {y})) (- (div (- {x}) (abs {y})))))"
                );
                let r = if name == "sdiv" { q } else { format!("(- {x} (* {y} {q}))") };
                one(format!("(ite (= {} 0) 0 (mod {r} {}))", a[1], num(&w)))
            }
            "exp" => {
                want(2)?;
                match (self.y_konst(&a[0]), self.y_konst(&a[1])) {
                    (Some(b), Some(e)) if !b.is_negative() && !e.is_negative() => {
                        one(num(&b.modpow(&e, &w)))
                    }
                    (Some(b), None) if b == BigInt::from(2) => {
                        self.ensure_pow2();
                        one(format!("(mod (|pow2| {}) {})", a[1], num(&w)))
                    }
                    (_, Some(e)) if e <= BigInt::from(8) && !e.is_negative() => {
                        let k: usize = e.try_into().unwrap_or(0);
                        if k == 0 {
                            return one("1".into());
                        }
                        if k == 1 {
                            return one(a[0].clone());
                        }
                        one(format!("(mod (* {}) {})", vec![a[0].as_str(); k].join(" "), num(&w)))
                    }
                    _ => unsupported(span, "assembly `exp` with a symbolic exponent"),
                }
            }
            "addmod" | "mulmod" => {
                want(3)?;
                let o = if name == "addmod" { "+" } else { "*" };
                one(format!("(ite (= {2} 0) 0 (mod ({o} {0} {1}) {2}))", a[0], a[1], a[2]))
            }
            "signextend" => {
                want(2)?;
                match self.y_konst(&a[0]) {
                    Some(k) if k >= BigInt::from(31) => one(a[1].clone()),
                    Some(k) if !k.is_negative() => {
                        let n = 8 * (u32::try_from(k).unwrap_or(0) + 1);
                        let l = self.y_lin(&a[1]);
                        let low = self.y_slice(cx, &l, 0, n).render();
                        one(format!(
                            "(ite (< {low} {}) {low} (+ {low} {}))",
                            num(&pow2(n - 1)),
                            num(&(w - pow2(n)))
                        ))
                    }
                    _ => unsupported(span, "assembly `signextend` with a symbolic width"),
                }
            }
            "lt" | "gt" | "eq" => {
                want(2)?;
                let op = match name {
                    "lt" => "<",
                    "gt" => ">",
                    _ => "=",
                };
                one(Self::y_flagt(&format!("({op} {} {})", a[0], a[1])))
            }
            "slt" | "sgt" => {
                want(2)?;
                let (x, y) = (self.y_signed(cx, &a[0]), self.y_signed(cx, &a[1]));
                let op = if name == "slt" { "<" } else { ">" };
                one(Self::y_flagt(&format!("({op} {x} {y})")))
            }
            "iszero" => {
                want(1)?;
                match self.y_flag(&a[0]) {
                    Some(b) if b == "true" => one("0".into()),
                    Some(b) if b == "false" => one("1".into()),
                    Some(b) => one(Self::y_flagt(&format!("(not {b})"))),
                    None => one(Self::y_flagt(&format!("(= {} 0)", a[0]))),
                }
            }
            "not" => {
                want(1)?;
                let l = Lin::k(w - 1).add(&self.y_lin(&a[0]), -1);
                one(l.render())
            }
            "xor" => {
                want(2)?;
                one(self.y_xor(cx, &a[0], &a[1], 0))
            }
            "and" | "or" => {
                want(2)?;
                if let (Some(x), Some(y)) = (self.y_flag(&a[0]), self.y_flag(&a[1])) {
                    let t = match name {
                        "and" => format!("(and {x} {y})"),
                        "or" => format!("(or {x} {y})"),
                        _ => format!("(xor {x} {y})"),
                    };
                    return one(Self::y_flagt(&t));
                }
                let (k0, k1) = (self.y_konst(&a[0]), self.y_konst(&a[1]));
                if let (Some(x), Some(y)) = (&k0, &k1) {
                    let r = match name {
                        "and" => x & y,
                        "or" => x | y,
                        _ => x ^ y,
                    };
                    return one(num(&r));
                }
                for (k, other) in [(&k0, &a[1]), (&k1, &a[0])] {
                    let Some(k) = k else { continue };
                    if k.is_zero() {
                        return one(if name == "and" { "0".into() } else { other.clone() });
                    }
                    if name == "and" && (k & (k + 1u32)).is_zero() {
                        // a low mask: and(x, 2^n - 1) = x mod 2^n
                        let n = k.bits() as u32;
                        let l = self.y_lin(other);
                        return one(self.y_slice(cx, &l, 0, n).render());
                    }
                }
                if a[0] == a[1] {
                    return one(if name == "xor" { "0".into() } else { a[0].clone() });
                }
                if name == "or" {
                    // disjoint bits: or = add
                    let (x, y) = (self.y_lin(&a[0]), self.y_lin(&a[1]));
                    for (p, q) in [(&x, &y), (&y, &x)] {
                        let tz =
                            p.ts.values()
                                .chain(std::iter::once(&p.c))
                                .filter(|k| !k.is_zero())
                                .map(|k| k.trailing_zeros().unwrap_or(0))
                                .min()
                                .unwrap_or(256);
                        if tz > 0
                            && tz <= 256
                            && self.y_fits(cx, q, &pow2(tz as u32))
                            && self.y_fits(cx, p, &w)
                        {
                            return one(p.add(q, 1).render());
                        }
                    }
                }
                let op = match name {
                    "and" => "bvand",
                    "or" => "bvor",
                    _ => "bvxor",
                };
                one(self.y_bv(op, &a[0], &a[1]))
            }
            "shl" | "shr" | "sar" => {
                want(2)?;
                let (s, x) = (&a[0], &a[1]);
                match self.y_konst(s) {
                    Some(k) if k >= BigInt::from(256) => {
                        if name == "sar" {
                            one(format!("(ite (< {x} {}) 0 {})", num(&pow2(255)), num(&(w - 1))))
                        } else {
                            one("0".into())
                        }
                    }
                    Some(k) if !k.is_negative() => {
                        let k = u32::try_from(k).unwrap();
                        let l = self.y_lin(x);
                        match name {
                            "shl" => one(self.y_wrap(cx, l.scale(&pow2(k)))),
                            "shr" => one(self.y_slice(cx, &l, k, 256 - k).render()),
                            _ => {
                                let sx = self.y_signed(cx, x);
                                one(format!("(mod (div {sx} {}) {})", num(&pow2(k)), num(&w)))
                            }
                        }
                    }
                    _ => {
                        self.ensure_pow2();
                        match name {
                            "shl" => one(format!("(mod (* {x} (|pow2| {s})) {})", num(&w))),
                            "shr" => one(format!("(div {x} (|pow2| {s}))")),
                            _ => {
                                let sx = self.y_signed(cx, x);
                                one(format!("(mod (div {sx} (|pow2| {s})) {})", num(&w)))
                            }
                        }
                    }
                }
            }
            "byte" => {
                want(2)?;
                match self.y_konst(&a[0]) {
                    Some(k) if k >= BigInt::from(32) => one("0".into()),
                    Some(k) if !k.is_negative() => {
                        let i = u32::try_from(k).unwrap();
                        let l = self.y_lin(&a[1]);
                        one(self.y_slice(cx, &l, 8 * (31 - i), 8).render())
                    }
                    _ => {
                        // one case per index the word can have (from its known range), else 0
                        let (lo, hi) = self
                            .iv(&a[0])
                            .map(|(l, h)| (l.max(BigInt::zero()), h))
                            .unwrap_or((BigInt::zero(), w256() - 1));
                        let l = self.y_lin(&a[1]);
                        let mut acc = "0".to_string();
                        let top = hi.min(BigInt::from(31));
                        let mut k = top;
                        while k >= lo {
                            let i = u32::try_from(&k).unwrap();
                            let b = self.y_slice(cx, &l, 8 * (31 - i), 8).render();
                            acc = format!("(ite (= {} {i}) {b} {acc})", a[0]);
                            k -= 1;
                        }
                        let x = self.y_fresh(span, "byte")?;
                        self.smt.send(&format!("(assert (= {x} {acc}))\n"));
                        self.ranges.insert(x.clone(), (BigInt::zero(), BigInt::from(255)));
                        one(x)
                    }
                }
            }
            "caller" | "address" | "callvalue" | "timestamp" | "number" | "chainid" | "origin" => {
                want(0)?;
                let k = match name {
                    "caller" => "msg.sender",
                    "address" => "this",
                    "callvalue" => "msg.value",
                    "timestamp" => "block.timestamp",
                    "number" => "block.number",
                    "chainid" => "block.chainid",
                    _ => "tx.origin",
                };
                let v = self
                    .env
                    .get(k)
                    .cloned()
                    .ok_or((span, format!("`{name}()` is not available here")))?;
                one(v.t)
            }
            "gas" | "gasprice" | "returndatasize" | "coinbase" | "basefee" | "blobbasefee"
            | "prevrandao" | "difficulty" | "gaslimit" | "msize" | "codesize" | "calldatasize"
            | "selfbalance" | "balance" | "extcodesize" | "extcodehash" | "blockhash"
            | "blobhash" => one(self.y_fresh(span, name)?),
            "pop" | "log0" | "log1" | "log2" | "log3" | "log4" => Ok(vec![]),
            "revert" | "invalid" => {
                if self.spec {
                    return unsupported(span, "assembly that may revert inside a spec evaluation");
                }
                self.reverts_if("true");
                self.guard = "false".into();
                Ok(vec![])
            }
            "sload" | "tload" => {
                want(1)?;
                if name == "sload"
                    && let Some(x) = self.y_slot_access(span, &a[0], None)?
                {
                    return one(x);
                }
                let key = self
                    .raw_key(name == "tload")
                    .ok_or((span, format!("raw storage ({name}) is only modelled in contracts")))?;
                let raw = self.env[&key].clone();
                let t = format!("(select {} {})", raw.t, a[0]);
                if self.bound.is_empty() || self.y_closed(&t) {
                    let x = self.fresh(name);
                    self.smt.send(&format!(
                        "(declare-const {x} Int)\n(assert (= {x} {t}))\n(assert {})\n",
                        Ty::Uint(256).range(&x).unwrap()
                    ));
                    self.ranges.insert(x.clone(), (BigInt::zero(), w - 1));
                    return one(x);
                }
                one(t)
            }
            "sstore" | "tstore" => {
                want(2)?;
                if self.spec {
                    return unsupported(
                        span,
                        format!("assembly `{name}` inside a spec evaluation"),
                    );
                }
                if name == "sstore" && self.y_slot_access(span, &a[0], Some(&a[1]))?.is_some() {
                    return Ok(vec![]);
                }
                let key = self
                    .raw_key(name == "tstore")
                    .ok_or((span, format!("raw storage ({name}) is only modelled in contracts")))?;
                let raw = self.env[&key].clone();
                let t = format!("(store {} {} {})", raw.t, a[0], a[1]);
                self.bind(&key, raw.ty.clone(), Var { ty: raw.ty, t, len: None });
                Ok(vec![])
            }
            "mstore" | "mstore8" => {
                want(2)?;
                let off = self.y_offset(cx, span, &a[0], "memory offset")?;
                if name == "mstore" {
                    self.mem_write(cx, span, off, a[1].clone(), 0, false)?;
                } else {
                    let l = self.y_lin(&a[1]);
                    let b = self.y_slice(cx, &l, 0, 8).render();
                    self.mem_write(cx, span, off, b, 31, false)?;
                }
                Ok(vec![])
            }
            "mload" => {
                want(1)?;
                let Some(off) = self.y_addr(cx, &a[0]) else {
                    // memory we do not track (behind a Solidity pointer): an arbitrary word
                    return one(self.y_fresh(span, "mload")?);
                };
                if off == 0x40 && cx.fmp.is_none() && !(0x40..0x60).any(|i| cx.mem.contains_key(&i))
                {
                    // the free memory pointer: below 2^32 (memory that large costs more gas than
                    // a block has), and the memory past it is free
                    let p = self.y_fresh(span, "fmp")?;
                    let hi = BigInt::from(1u64 << 32);
                    self.smt.send(&format!(
                        "(assert (and (<= 128 {p}) (< {p} {hi})))
"
                    ));
                    self.ranges.insert(p.clone(), (BigInt::from(128), hi - 1));
                    self.mem_write(cx, span, 0x40, p.clone(), 0, true)?;
                    cx.fmp = Some(p);
                }
                let l = self.mem_read(cx, span, off, 32)?;
                one(l.render())
            }
            "keccak256" => {
                want(2)?;
                let (Some(off), Some(len)) = (self.y_addr(cx, &a[0]), self.y_konst(&a[1])) else {
                    // hash of memory we do not track: an arbitrary word
                    return one(self.y_fresh(span, "keccak")?);
                };
                if len.is_negative() || len > BigInt::from(4096) {
                    return one(self.y_fresh(span, "keccak")?);
                }
                let len = u64::try_from(len).unwrap();
                let v = self.mem_read(cx, span, off, len)?.render();
                one(self.keccak_term(&BigInt::from(len), &v))
            }
            "calldataload" => {
                want(1)?;
                one(self.y_fresh(span, "calldataload")?)
            }
            "returndatacopy" | "calldatacopy" | "codecopy" => {
                want(3)?;
                if self.spec {
                    return unsupported(
                        span,
                        format!("assembly `{name}` inside a spec evaluation"),
                    );
                }
                self.y_clobber(cx, span, &a[0], &a[2])?;
                Ok(vec![])
            }
            "extcodecopy" => {
                want(4)?;
                if self.spec {
                    return unsupported(
                        span,
                        format!("assembly `{name}` inside a spec evaluation"),
                    );
                }
                self.y_clobber(cx, span, &a[1], &a[3])?;
                Ok(vec![])
            }
            "call" => {
                want(7)?;
                if self.spec {
                    return unsupported(span, "assembly `call` inside a spec evaluation");
                }
                // an external call: outside code may reenter (see external_call); value leaves
                // this contract on success; the output buffer becomes unknown
                self.y_clobber(cx, span, &a[5], &a[6])?;
                let ok = self.decl("call_ok", &Ty::Bool);
                if self.y_konst(&a[2]).is_none_or(|v| !v.is_zero()) {
                    let v = a[2].clone();
                    self.send_value(&v, Some(&ok));
                }
                self.external_call(vec![], false, span)?;
                one(Self::y_flagt(&ok))
            }
            "create" | "create2" => {
                want(if name == "create" { 3 } else { 4 })?;
                if self.spec {
                    return unsupported(
                        span,
                        format!("assembly `{name}` inside a spec evaluation"),
                    );
                }
                // the new contract's constructor is outside code; the result is an arbitrary
                // address (0 on failure)
                if self.y_konst(&a[0]).is_none_or(|v| !v.is_zero()) {
                    let ok = self.decl("create_ok", &Ty::Bool);
                    let v = a[0].clone();
                    self.send_value(&v, Some(&ok));
                }
                self.external_call(vec![], false, span)?;
                let r = self.decl("created", &Ty::Addr);
                one(r)
            }
            "staticcall" => {
                want(6)?;
                if self.spec {
                    return unsupported(span, "assembly `staticcall` inside a spec evaluation");
                }
                // the output buffer becomes unknown; the result is an arbitrary success flag
                self.y_clobber(cx, span, &a[4], &a[5])?;
                let ok = self.decl("staticcall", &Ty::Bool);
                one(Self::y_flagt(&ok))
            }
            _ => unsupported(span, format!("assembly builtin `{name}` is not supported")),
        }
    }

    // ------------------------------------------------------------------ specs
    /// A view/pure function called in a spec without a single-`return` body: evaluate its
    /// straight-line body (declarations, assignments, inline assembly, a final return).
    pub(super) fn spec_inline(
        &mut self,
        c: &'p Contract<'a>,
        f: &'p Func<'a>,
        args: &[Var],
        span: Span,
    ) -> R<Vec<Var>> {
        let not_spec = |why: &str| -> R<Vec<Var>> {
            Err((
                span,
                format!(
                    "`{}` is not a spec function (pure/view with a single `return`, or a straight-line body): {why}",
                    f.name
                ),
            ))
        };
        if !f.is_static() {
            return not_spec("it is not view/pure");
        }
        let Some(body) = &f.item.body else {
            return not_spec("it has no body");
        };
        if f.item.header.modifiers.iter().any(|m| c.modifiers.contains_key(m.name.last().as_str()))
        {
            return not_spec("it has modifiers");
        }
        if self.depth > 8 {
            return not_spec("recursion");
        }
        let saved_env = self.env.clone();
        let saved_c = std::mem::replace(&mut self.c, c);
        for ((n, ty), a) in f.params.iter().zip(args) {
            self.env.insert(n.clone(), Var { ty: ty.clone(), t: a.t.clone(), len: a.len.clone() });
        }
        for (n, ty) in &f.rets {
            let z = self.zero_var(ty);
            self.env.insert(n.clone(), z);
        }
        self.depth += 1;
        let fname = f.name.clone();
        let r = (|| -> R<Vec<Var>> {
            let last = body.stmts.len().saturating_sub(1);
            for (i, s) in body.stmts.iter().enumerate() {
                let bad = |what: &str| -> R<Vec<Var>> {
                    Err((
                        s.span,
                        format!(
                            "`{fname}` cannot be evaluated in a spec ({what}); only straight-line bodies (declarations, assignments, inline assembly, a final return) are supported"
                        ),
                    ))
                };
                match &s.kind {
                    StmtKind::DeclSingle(v)
                        if v.data_location != Some(ast::DataLocation::Storage) =>
                    {
                        let ty = self.ty_of_decl(&v.ty);
                        let val = match &v.initializer {
                            Some(i) => {
                                if reverts(i) {
                                    return bad("it may revert");
                                }
                                self.eval(i)?
                            }
                            None => self.zero_var(&ty),
                        };
                        let name = v.name.map(|n| n.as_str().to_string()).unwrap_or_default();
                        self.bind(&name, ty, val);
                    }
                    StmtKind::Expr(e) => {
                        if reverts(e) {
                            return bad("it may revert");
                        }
                        self.eval_multi(e)?;
                    }
                    StmtKind::Assembly(a) => self.exec_asm(a, s.span)?,
                    StmtKind::Return(e) if i == last => {
                        return match e {
                            Some(e) => {
                                if reverts(e) {
                                    return bad("it may revert");
                                }
                                self.eval_multi(e)
                            }
                            None => Ok(f.rets.iter().map(|(n, _)| self.env[n].clone()).collect()),
                        };
                    }
                    StmtKind::If(..)
                    | StmtKind::While(..)
                    | StmtKind::For { .. }
                    | StmtKind::DoWhile(..) => return bad("it branches"),
                    StmtKind::Revert(..) => return bad("it reverts"),
                    _ => return bad("unsupported statement"),
                }
            }
            Ok(f.rets.iter().map(|(n, _)| self.env[n].clone()).collect())
        })();
        self.depth -= 1;
        self.env = saved_env;
        self.c = saved_c;
        let mut out = r?;
        for (v, (_, ty)) in out.iter_mut().zip(&f.rets) {
            v.ty = ty.clone();
        }
        Ok(out)
    }
}

impl<'a> Yc<'a> {
    fn new() -> Self {
        Yc {
            mem: BTreeMap::new(),
            words: vec![],
            regions: vec![],
            funcs: vec![],
            scopes: vec![],
            leaves: vec![],
            extra: FxHashMap::default(),
            xors: FxHashMap::default(),
            depth: 0,
            fmp: None,
            ptrs: FxHashMap::default(),
        }
    }
}

/// Does this expression call require/assert/revert (a spec evaluation cannot model reverts)?
fn reverts<'a>(e: &'a Expr<'a>) -> bool {
    let mut found = false;
    walk::sub_exprs(e, &mut |x| {
        if let ExprKind::Call(c, _) = &x.kind
            && let ExprKind::Ident(i) = &c.kind
            && ["require", "assert", "revert"].contains(&i.as_str())
        {
            found = true;
        }
    });
    found
}

/// Write effects of an assembly block: Solidity variables it assigns, `__raw` when it may
/// sstore; `all` when it may call out (call, delegatecall, create...).
pub(super) fn asm_effects(
    b: &yul::Block<'_>,
    out: &mut BTreeSet<String>,
    all: &mut bool,
    lock: bool,
) {
    fn ex(e: &yul::Expr<'_>, out: &mut BTreeSet<String>, all: &mut bool, lock: bool) {
        if let yul::ExprKind::Call(c) = &e.kind {
            match c.name.as_str() {
                "sstore" => {
                    out.insert("__raw".into());
                }
                "tstore" => {
                    out.insert("__traw".into());
                }
                // under a held reentrancy lock a call or creation reaches only the entry points
                // still open, whose writes the caller adds (modset_under_lock)
                "call" | "create" | "create2" if lock => {}
                "call" | "callcode" | "delegatecall" | "create" | "create2" | "selfdestruct" => {
                    *all = true
                }
                _ => {}
            }
            for a in c.arguments.iter() {
                ex(a, out, all, lock);
            }
        }
    }
    for s in b.stmts.iter() {
        use yul::StmtKind as K;
        match &s.kind {
            K::Block(b) => asm_effects(b, out, all, lock),
            K::AssignSingle(p, e) => {
                out.insert(p.segments()[0].as_str().to_string());
                ex(e, out, all, lock);
            }
            K::AssignMulti(ps, e) => {
                for p in ps.iter() {
                    out.insert(p.segments()[0].as_str().to_string());
                }
                ex(e, out, all, lock);
            }
            K::Expr(e) => ex(e, out, all, lock),
            K::VarDecl(_, Some(e)) => ex(e, out, all, lock),
            K::If(c, b) => {
                ex(c, out, all, lock);
                asm_effects(b, out, all, lock);
            }
            K::Switch(sw) => {
                ex(&sw.selector, out, all, lock);
                for c in sw.cases.iter() {
                    asm_effects(&c.body, out, all, lock);
                }
            }
            K::For(f) => {
                asm_effects(&f.init, out, all, lock);
                ex(&f.cond, out, all, lock);
                asm_effects(&f.step, out, all, lock);
                asm_effects(&f.body, out, all, lock);
            }
            K::FunctionDef(f) => asm_effects(&f.body, out, all, lock),
            _ => {}
        }
    }
}

/// State variables whose `.slot` some assembly in the program reads.
pub(super) fn slot_taken(prog: &Program<'_>) -> BTreeSet<String> {
    fn ex(e: &yul::Expr<'_>, out: &mut BTreeSet<String>) {
        match &e.kind {
            yul::ExprKind::Path(p) => {
                if let [v, f] = p.segments()
                    && f.name == sym::slot
                {
                    out.insert(v.as_str().to_string());
                }
            }
            yul::ExprKind::Call(c) => {
                for a in c.arguments.iter() {
                    ex(a, out);
                }
            }
            _ => {}
        }
    }
    fn blk(b: &yul::Block<'_>, out: &mut BTreeSet<String>) {
        use yul::StmtKind as K;
        for s in b.stmts.iter() {
            match &s.kind {
                K::Block(b) => blk(b, out),
                K::AssignSingle(_, e)
                | K::AssignMulti(_, e)
                | K::Expr(e)
                | K::VarDecl(_, Some(e)) => ex(e, out),
                K::If(c, b) => {
                    ex(c, out);
                    blk(b, out);
                }
                K::Switch(sw) => {
                    ex(&sw.selector, out);
                    for c in sw.cases.iter() {
                        blk(&c.body, out);
                    }
                }
                K::For(f) => {
                    blk(&f.init, out);
                    ex(&f.cond, out);
                    blk(&f.step, out);
                    blk(&f.body, out);
                }
                K::FunctionDef(f) => blk(&f.body, out),
                _ => {}
            }
        }
    }
    let mut out = BTreeSet::new();
    for c in &prog.contracts {
        let bodies = c
            .funcs
            .iter()
            .chain(&c.overridden)
            .map(|f| f.item)
            .chain(c.modifiers.values().copied())
            .filter_map(|f| f.body.as_ref());
        for b in bodies {
            for s in b.stmts.iter() {
                walk::all_stmts(s, &mut |s| {
                    if let StmtKind::Assembly(a) = &s.kind {
                        blk(&a.block, &mut out);
                    }
                });
            }
        }
    }
    out
}

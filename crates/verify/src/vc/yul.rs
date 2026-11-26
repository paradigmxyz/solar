//! Inline assembly (Yul).
//!
//! Yul values are uint256 words in the encoder's Int encoding. Arithmetic follows the EVM
//! (wrapping mod 2^256, division by zero gives 0); comparisons give 0/1 flags; bitwise operations
//! use the encoder's bitvector view. Words are kept as linear forms over atoms where possible
//! (constant + sum of k * atom), so the usual slot idioms (shl/or/mstore of a seed and an address)
//! normalise to one canonical term and keccak inputs assembled from byte slices of memory come out
//! as plain arithmetic, e.g. keccak256 over owner || seed is keccak(32, owner * 2^96 + seed).
//!
//! Memory is modelled per assembly block with concrete offsets only. Each byte is a binding of the
//! path state (env key `__ym<hex offset>`, see `mkey`), so branches, `leave` and loops merge memory
//! as they merge variables. A byte written as part of a word is bound to its canonical byte term,
//! which `Yc::canon` maps back to (word, byte index) so whole words read back as the word.
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
    interface::{
        Span,
        data_structures::map::{FxHashMap, FxHashSet},
        sym,
    },
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
    /// canonical byte terms of the words written to memory: term -> (word, byte index)
    canon: FxHashMap<String, (usize, u8)>,
    words: Vec<String>,
    /// Solidity memory structs used as pointers: name, base, byte length
    regions: Vec<(String, u64, u64, String)>,
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

/// The env key of memory byte `off` (model address) in an assembly block.
fn mkey(off: u64) -> String {
    format!("{MEM_KEY}{off:x}")
}

/// The model address of a memory byte's env key.
fn mkey_off(k: &str) -> Option<u64> {
    u64::from_str_radix(k.strip_prefix(MEM_KEY)?, 16).ok()
}

const MEM_KEY: &str = "__ym";

/// A memory byte as the block bound it.
enum Byte {
    /// byte `from` of word `id`
    Of(usize, u8),
    /// any other term (merged bytes, ite of the branches' bytes), a value in [0, 256)
    Term(String),
}

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
        // Inside a quantified spec the same facts guard the body (see `Vc::quant`).
        if let Some(top) = self.quant_facts.last_mut() {
            top.push(format!(
                "(and (= (sv_keccak_len {t}) {}) (= (sv_keccak_val {t}) {val}) (<= {} {t}) (<= {t} {}))",
                num(len),
                num(&pow2(128)),
                num(&(w256() - 1))
            ));
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
            // A `bytesN` word holds its bytes first: the value shifted to the top.
            Ty::Bytes(n) => out.push(format!("(* {t} {})", num(&pow2(256 - *n as u32)))),
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
            // A small fixed-size array of static elements: its elements in place, in order.
            Ty::Arr(e, Some(n)) if *n <= 32 => {
                for i in 0..*n {
                    // Nested arrays multiply: past 32 words in all, keep the hash opaque.
                    if !self.abi_words(e, &format!("(select {t} {i})"), out) || out.len() > 32 {
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
        let ground = !x.contains(' ');
        if k > 1 && (ground || !self.quant_facts.is_empty()) {
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
                if ground {
                    self.smt.send(&format!(
                        "(assert (= ({f} {x}) {w}))
"
                    ));
                } else if let Some(top) = self.quant_facts.last_mut() {
                    top.push(format!("(= ({f} {x}) {w})"));
                }
            }
        }
        x
    }

    /// A hash argument as an Int term, and the tag of its encoding: a bool as 0/1; an array or
    /// struct (whose contents we do not encode) as a fresh value, so its hash is an arbitrary
    /// value (equal contents are not known to hash equally: an over-approximation). The value is
    /// the one its encoding holds (a signed integer in two's complement over its width when
    /// packed, else over a word), and the tag names that encoding: a fixed-width value's type,
    /// or `dyn` for anything whose encoding the term does not pin down.
    fn hash_arg(&mut self, v: &Var, packed: bool) -> (String, String) {
        let word = |n: u32| num(&pow2(n));
        match &v.ty {
            Ty::Bool => (format!("(ite {} 1 0)", v.t), "bool".into()),
            Ty::Uint(n) => (v.t.clone(), format!("uint{n}")),
            Ty::Bytes(n) => (v.t.clone(), format!("bytes{n}")),
            Ty::Addr | Ty::Contract(_) => (v.t.clone(), "address".into()),
            Ty::Enum(..) => (v.t.clone(), "uint8".into()),
            Ty::Int(n) => {
                let w = if packed { *n as u32 } else { 256 };
                (format!("(mod {} {})", v.t, word(w)), format!("int{n}"))
            }
            Ty::Lit if !packed => (format!("(mod {} {})", v.t, word(256)), "uint256".into()),
            _ if v.ty.sort() == "Int" => (v.t.clone(), "dyn".into()),
            _ => (self.decl("hashed", &Ty::Opaque), "dyn".into()),
        }
    }

    /// Solidity keccak256, sha256 and ripemd160 of their argument, as an uninterpreted function
    /// of the flattened values: one function per encoding (`abi.encode`, `encodePacked`, the
    /// other `abi.*` forms, or raw bytes) and per list of argument tags, so two calls share a
    /// function only when equal values give equal preimages. It is injective unless a packed
    /// encoding holds a value of no fixed width, which can shift the bytes of its neighbours.
    /// The argument of a hash builtin, evaluated once: the `abi.*` form that encodes it (`raw` for
    /// bytes passed as they are) and its values.
    pub(super) fn hash_args(&mut self, argv: &[&'a Expr<'a>]) -> R<(String, Vec<Var>)> {
        let mut kind = "raw".to_string();
        let mut vals = vec![];
        for a in argv {
            if let ExprKind::Call(c2, inner) = &a.kind
                && let ExprKind::Member(b, m) = &c2.kind
                && matches!(&b.kind, ExprKind::Ident(i) if i.name == sym::abi)
            {
                kind = m.as_str().to_string();
                for (i, x) in inner.exprs().enumerate() {
                    let v = self.eval(x)?;
                    // `encodeWithSignature` encodes only the signature's 4-byte selector, which
                    // distinct signatures can share: hash that, a function of the signature that
                    // is not injective.
                    let v =
                        if i == 0 && kind == "encodeWithSignature" { self.selector(v) } else { v };
                    vals.push(v);
                }
            } else {
                vals.push(self.eval(a)?);
            }
        }
        Ok((kind, vals))
    }

    /// The `bytes4` selector of a function signature: an uninterpreted function of the signature,
    /// with no injectivity, since distinct signatures can share a selector.
    fn selector(&mut self, sig: Var) -> Var {
        let ty = Ty::Bytes(32);
        let f = "sv_selector";
        self.declare_fun(f, 1, "Int");
        let t = format!("({f} {})", sig.t);
        if self.bound.is_empty() {
            self.smt.send(&format!(
                "(assert {})
",
                ty.range(&t).unwrap()
            ));
        }
        Var { ty, t, len: None }
    }

    /// Solidity keccak256, sha256 and ripemd160 of their argument, as an uninterpreted function
    /// of the flattened values: one function per encoding (`abi.encode`, `encodePacked`, the
    /// other `abi.*` forms, or raw bytes) and per list of argument tags, so two calls share a
    /// function only when equal values give equal preimages. It is injective unless a packed
    /// encoding holds a value of no fixed width, which can shift the bytes of its neighbours.
    ///
    /// The packed encoding of anything with bytes in it hashes as those bytes: the hash of
    /// `packed_bytes`, a function of the byte sequence, so equal arguments hash equally however
    /// the hash was reached (through a variable holding the bytes or not). The raw hash is
    /// injective over byte sequences, and the packed bytes are not claimed injective in their
    /// arguments (different splits can give the same bytes), so two such hashes differ only
    /// where their bytes are known to differ.
    pub(super) fn hash_flat(&mut self, h: &str, kind: &str, vals: &[Var]) -> String {
        if kind == "encodePacked"
            && vals.iter().any(|v| v.ty == Ty::Opaque)
            && let Some(p) = self.packed_bytes(vals)
        {
            return self.hash_flat(h, "raw", &[Var { ty: Ty::Opaque, t: p, len: None }]);
        }
        let mut flat = vec![];
        let mut tags = vec![];
        for v in vals {
            let (t, tag) = self.hash_arg(v, kind == "encodePacked");
            flat.push(t);
            tags.push(tag);
        }
        let f = format!("sv_{h}.{kind}.{}", tags.join("."));
        self.declare_fun(&f, flat.len(), "Int");
        self.ret_bound(&f, &Ty::Uint(256));
        if !(kind == "encodePacked" && tags.iter().any(|t| t == "dyn")) {
            self.injective(&f, flat.len());
        }
        if flat.is_empty() { f } else { format!("({f} {})", flat.join(" ")) }
    }

    /// The bytes `abi.encodePacked` builds from `vals`, as a function of the values: `None`
    /// unless every value is of a fixed width or bytes (a string or bytes value). Its length is
    /// the sum of the parts' lengths, and each byte of a fixed-width part or of a literal is
    /// known at its offset (a constant, plus the lengths of the bytes values before it). Nothing
    /// says it is injective: bytes values of unknown length can split the same bytes apart in
    /// more than one way. Bytes values are terms of their contents: equal bytes are one term
    /// (memory bytes written in place get a new term, and are never shared while written; see
    /// `memalias`).
    pub(super) fn packed_bytes(&mut self, vals: &[Var]) -> Option<String> {
        // The packed encoding of one bytes value is that value.
        if let [v] = vals
            && v.ty == Ty::Opaque
        {
            return Some(v.t.clone());
        }
        let mut flat = vec![];
        let mut tags = vec![];
        // Each part: its term, and its width in bytes when it is of a fixed width.
        let mut parts = vec![];
        for v in vals {
            let w = match &v.ty {
                Ty::Opaque => None,
                Ty::Bool | Ty::Enum(..) => Some(1),
                Ty::Uint(n) | Ty::Int(n) | Ty::Bytes(n) if n % 8 == 0 && *n <= 256 => {
                    Some(*n as u64 / 8)
                }
                Ty::Addr | Ty::Contract(_) => Some(20),
                _ => return None,
            };
            let (t, tag) =
                if w.is_some() { self.hash_arg(v, true) } else { (v.t.clone(), "dyn".to_string()) };
            flat.push(t.clone());
            tags.push(tag);
            parts.push((t, w));
        }
        let f = format!("sv_packed.{}", tags.join("."));
        self.declare_fun(&f, flat.len(), "Int");
        let p = if flat.is_empty() { f } else { format!("({f} {})", flat.join(" ")) };
        if self.bound.is_empty() {
            let byte = self.bytes_byte_fn();
            let mut facts = vec![];
            let mut k = 0u64;
            let mut lens: Vec<String> = vec![];
            // An offset: a constant plus the lengths of the bytes values before it.
            let off = |k: u64, lens: &[String]| {
                if lens.is_empty() { k.to_string() } else { format!("(+ {k} {})", lens.join(" ")) }
            };
            for (t, w) in &parts {
                match *w {
                    Some(w) => {
                        for j in 0..w {
                            let sh = num(&pow2(8 * (w - 1 - j) as u32));
                            let b = match (w, j) {
                                (1, _) => t.clone(),
                                (_, 0) => format!("(div {t} {sh})"),
                                _ if j == w - 1 => format!("(mod {t} 256)"),
                                _ => format!("(mod (div {t} {sh}) 256)"),
                            };
                            facts.push(format!("(= ({byte} {p} {}) {b})", off(k + j, &lens)));
                        }
                        k += w;
                    }
                    None => match Self::lit_bytes(t) {
                        Some(bs) => {
                            for (j, b) in bs.iter().enumerate() {
                                facts.push(format!(
                                    "(= ({byte} {p} {}) {b})",
                                    off(k + j as u64, &lens)
                                ));
                            }
                            k += bs.len() as u64;
                        }
                        None => lens.push(self.opaque_len(t)),
                    },
                }
            }
            let len = self.opaque_len(&p);
            facts.push(format!("(= {len} {})", off(k, &lens)));
            for x in facts {
                self.smt.send(&format!("(assert {x})\n"));
            }
        }
        // Bytes of fixed-width values alone: their keccak256 is the hash of their packed
        // encoding (the functions `hash_flat` names), so hashing a variable that holds them
        // agrees with hashing the encoding in place. Only the functions are declared here: their
        // injectivity comes with the first hash that uses them.
        if !parts.is_empty() && parts.iter().all(|(_, w)| w.is_some()) && self.bound.is_empty() {
            let direct = format!("sv_keccak256.encodePacked.{}", tags.join("."));
            let raw = "sv_keccak256.raw.dyn";
            self.declare_fun(&direct, flat.len(), "Int");
            self.ret_bound(&direct, &Ty::Uint(256));
            self.declare_fun(raw, 1, "Int");
            self.ret_bound(raw, &Ty::Uint(256));
            self.smt.send(&format!("(assert (= ({raw} {p}) ({direct} {})))\n", flat.join(" ")));
        }
        Some(p)
    }

    /// The function giving byte `i` of bytes value `t`, `(sv_bbyte t i)`: a function of the
    /// value, so the same bytes read twice (in the code and in a spec) agree.
    pub(super) fn bytes_byte_fn(&mut self) -> &'static str {
        let f = "sv_bbyte";
        self.declare_fun(f, 2, "Int");
        self.ret_bound(f, &Ty::Uint(8));
        f
    }

    /// The bytes of the term of a string or hex literal that fits 32 bytes (`|str.<hex>|`).
    pub(super) fn lit_bytes(t: &str) -> Option<Vec<u8>> {
        let hex = t.strip_prefix("|str.")?.strip_suffix('|')?;
        (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok()).collect()
    }

    pub(super) fn keccak_solidity(&mut self, argv: &[&'a Expr<'a>]) -> R<Var> {
        let ty = Ty::Uint(256);
        // The hash of bytes known here (a string or hex literal, or their packed encoding) is
        // a constant: the real keccak256, so every way of computing it agrees.
        if let [a] = argv
            && let Some(bytes) = Self::known_bytes(a)
        {
            let h = alloy_primitives::keccak256(&bytes);
            let t = num(&BigInt::from_bytes_be(num_bigint::Sign::Plus, h.as_slice()));
            return Ok(Var { ty, t, len: None });
        }
        let (kind, vals) = self.hash_args(argv)?;
        // abi.encode of static values goes through the keccak that assembly shares. Its words
        // are read from each value when it is evaluated, while the compiled code encodes memory
        // only after every argument ran: an argument that writes anything (`abi.encode(a, a[0] =
        // 1)`, a call that writes a memory array) could change an earlier one's words, so such a
        // call hashes opaquely instead.
        if kind == "encode" && argv.len() == 1 && !vals.is_empty() && !self.encode_writes(argv[0]) {
            let mut ws = vec![];
            let mut tys = vec![];
            let mut ok = true;
            for v in &vals {
                let n0 = ws.len();
                if !self.abi_words(&v.ty, &v.t, &mut ws) {
                    ok = false;
                    break;
                }
                let wty = if matches!(v.ty, Ty::Uint(_) | Ty::Addr | Ty::Contract(_) | Ty::Enum(..))
                    && ws.len() == n0 + 1
                {
                    v.ty.clone()
                } else {
                    Ty::Opaque
                };
                while tys.len() < ws.len() {
                    tys.push(wty.clone());
                }
            }
            if ok && !ws.is_empty() {
                let t = self.keccak_words(&ws, &tys);
                return Ok(Var { ty, t, len: None });
            }
        }
        // A reference argument (bytes, string, an array or struct) is hashed by the value it had
        // when evaluated, but encoded from memory after every argument ran: when a later one may
        // write, the hash is of nothing known.
        if kind != "raw"
            && argv.len() == 1
            && vals.iter().any(|v| matches!(v.ty, Ty::Opaque | Ty::Arr(..) | Ty::Struct(_)))
            && self.encode_writes(argv[0])
        {
            let t = self.decl("keccak", &ty);
            return Ok(Var { ty, t, len: None });
        }
        let t = self.hash_flat("keccak256", &kind, &vals);
        if self.bound.is_empty() {
            self.smt.send(&format!("(assert {})\n", ty.range(&t).unwrap()));
        }
        Ok(Var { ty, t, len: None })
    }

    /// The bytes `e` stands for when the source gives them: a string or hex literal, its
    /// conversion to `bytes` or `string`, or `abi.encodePacked` of such literals.
    fn known_bytes(e: &Expr<'_>) -> Option<Vec<u8>> {
        match &e.peel_parens().kind {
            ExprKind::Lit(lit, _) => match &lit.kind {
                ast::LitKind::Str(_, b, _) => Some(b.as_byte_str().to_vec()),
                _ => None,
            },
            ExprKind::Call(c, args) => match &c.peel_parens().kind {
                ExprKind::Type(t)
                    if matches!(
                        t.kind,
                        ast::TypeKind::Elementary(
                            ast::ElementaryType::Bytes | ast::ElementaryType::String
                        )
                    ) =>
                {
                    let xs: Vec<_> = args.exprs().collect();
                    if xs.len() == 1 { Self::known_bytes(xs[0]) } else { None }
                }
                ExprKind::Member(b, m)
                    if matches!(&b.kind, ExprKind::Ident(i) if i.name == sym::abi)
                        && m.as_str() == "encodePacked" =>
                {
                    let mut out = vec![];
                    for x in args.exprs() {
                        out.extend(Self::known_bytes(x)?);
                    }
                    Some(out)
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Whether an argument of this `abi.*` call may write anything.
    pub(super) fn encode_writes(&self, e: &'a Expr<'a>) -> bool {
        let ExprKind::Call(_, inner) = &e.kind else {
            return true;
        };
        let (mut out, mut all) = (BTreeSet::new(), false);
        // Every argument, the first too: one argument (a struct constructor, a tuple) can name
        // an object and then write it, before the compiled code encodes any of it.
        for x in inner.exprs() {
            walk::sub_exprs(x, &mut |y| {
                self.effects(y, &mut out, &mut all, &mut FxHashSet::default())
            });
        }
        all || !out.is_empty()
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
            if cx.regions.iter().any(|(_, b, l, _)| off < b + l && off + n > *b) {
                return unsupported(span, "assembly writes into a Solidity memory variable");
            }
            return unsupported(
                span,
                format!(
                    "assembly writes memory at 0x{off:x} outside the scratch space (0x00-0x7f); only memory-safe scratch use is modelled"
                ),
            );
        }
        // Free memory at the free memory pointer may be what a constant offset past the scratch
        // space reads (the pointer is 0x80 while nothing is allocated): those reads are stale.
        if off >= FREE_BASE {
            self.mem_forget(SCRATCH_END, REGION_BASE);
        }
        cx.words.push(val);
        let id = cx.words.len() - 1;
        for i in 0..n {
            let t = self.byte_term(cx, id, from + i as u8);
            self.env.insert(mkey(off + i), Var { ty: Ty::Uint(8), t, len: None });
        }
        Ok(())
    }

    /// The canonical term of byte `b` (0 is the most significant) of word `id`.
    fn byte_term(&mut self, cx: &mut Yc<'a>, id: usize, b: u8) -> String {
        let w = cx.words[id].clone();
        let sh = pow2(8 * (31 - b as u32));
        let t = match self.y_konst(&w) {
            Some(k) => num(&((k / &sh) % BigInt::from(256))),
            None if b == 31 => format!("(mod {w} 256)"),
            None if b == 0 => format!("(div {w} {})", num(&sh)),
            None => format!("(mod (div {w} {}) 256)", num(&sh)),
        };
        cx.canon.entry(t.clone()).or_insert((id, b));
        t
    }

    fn mem_byte(&self, cx: &Yc<'a>, off: u64) -> Option<Byte> {
        let v = self.env.get(&mkey(off))?;
        Some(match cx.canon.get(&v.t) {
            Some(&(id, b)) => Byte::Of(id, b),
            None => Byte::Term(v.t.clone()),
        })
    }

    /// Forget memory bytes [lo, hi): later reads give fresh words.
    fn mem_forget(&mut self, lo: u64, hi: u64) {
        self.env.retain(|k, _| mkey_off(k).is_none_or(|o| o < lo || o >= hi));
    }

    /// Drop the block's memory bindings when it ends.
    fn mem_drop(&mut self) {
        self.env.retain(|k, _| mkey_off(k).is_none());
    }
    /// The big-endian value of memory bytes [off, off + len).
    fn mem_read(&mut self, cx: &mut Yc<'a>, span: Span, off: u64, len: u64) -> R<Lin> {
        if len > 4096 {
            return unsupported(span, "assembly reads a memory range longer than 4096 bytes");
        }
        // unwritten bytes: fresh words, kept so later reads agree
        let mut i = off;
        while i < off + len {
            if self.env.contains_key(&mkey(i)) {
                i += 1;
                continue;
            }
            let mut j = i;
            while j < off + len && j - i < 32 && !self.env.contains_key(&mkey(j)) {
                j += 1;
            }
            let w = self.y_fresh(span, "mload")?;
            cx.words.push(w);
            let id = cx.words.len() - 1;
            let k = j - i;
            for b in 0..k {
                let t = self.byte_term(cx, id, (32 - k + b) as u8);
                self.env.insert(mkey(i + b), Var { ty: Ty::Uint(8), t, len: None });
            }
            i = j;
        }
        let mut acc = Lin::k(BigInt::zero());
        let mut i = off;
        while i < off + len {
            let (id, from) = match self.mem_byte(cx, i).unwrap() {
                Byte::Of(id, from) => (id, from),
                Byte::Term(t) => {
                    let after = off + len - i - 1;
                    acc = acc.add(&self.y_lin(&t).scale(&pow2(8 * after as u32)), 1);
                    i += 1;
                    continue;
                }
            };
            let mut j = i + 1;
            while j < off + len
                && (j - i) < 32
                && matches!(self.mem_byte(cx, j), Some(Byte::Of(id2, f2)) if id2 == id && f2 == from + (j - i) as u8)
            {
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
        if let Some(a) = Self::y_region_addr(cx, &l) {
            return Some(a);
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

    /// A memory struct's pointer plus a constant within it: the model address of that field.
    fn y_region_addr(cx: &Yc<'a>, l: &Lin) -> Option<u64> {
        let [(t, k)] = &l.ts.iter().collect::<Vec<_>>()[..] else { return None };
        if !k.is_one() || l.c.is_negative() {
            return None;
        }
        let (_, b, len, _) = cx.regions.iter().find(|r| &r.3 == *t)?;
        let c = u64::try_from(&l.c).ok()?;
        (c < (*len).max(32)).then_some(b + c)
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
                self.mem_forget(o, o + n);
                if o >= FREE_BASE {
                    self.mem_forget(SCRATCH_END, REGION_BASE);
                }
                Ok(())
            }
            (Some(o), _) if (FREE_BASE..FREE_BASE + FREE_LEN).contains(&o) => {
                self.mem_forget(SCRATCH_END, REGION_BASE);
                // into free memory, of unknown length: all of free memory from o is unknown
                self.mem_forget(o, FREE_BASE + FREE_LEN);
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

    /// The output buffer of a call, when the frame's latest call is recorded (see `rd_note`)
    /// and the buffer is short and at a constant offset in the scratch space: what the call
    /// returns is then written there byte by byte.
    fn y_rd_out(&self, off: &str, len: &str) -> Option<(u64, u64)> {
        if !self.env.contains_key(super::RD_N) {
            return None;
        }
        let o = u64::try_from(self.y_konst(off)?).ok()?;
        let n = u64::try_from(self.y_konst(len)?).ok()?;
        (n <= 64 && o.checked_add(n)? <= SCRATCH_END).then_some((o, n))
    }

    /// Records a call of the block as the frame's latest (`count` when no `external_call`
    /// counted it): its flag `ok`, and return data of unknown length and contents, the first
    /// bytes of which the call copies into its output buffer `out` (`y_rd_out`). The EVM copies
    /// min(buffer size, return data size) bytes: a byte past the end of the return data keeps
    /// what memory held, an arbitrary byte where the block never wrote it.
    fn y_rd_record(&mut self, ok: &str, out: Option<(u64, u64)>, count: bool) {
        if !self.env.contains_key(super::RD_N) {
            return;
        }
        let rd = self.decl("returndata", &Ty::Opaque);
        let size = self.opaque_len(&rd);
        let word = self.rd_word(&rd);
        let rec = Some((ok.to_string(), size.clone(), word));
        if count {
            self.rd_note(rec)
        } else {
            self.rd_set(rec)
        }
        let Some((o, n)) = out else { return };
        let byte = self.bytes_byte_fn();
        for i in 0..n {
            let k = mkey(o + i);
            // what memory held before: an arbitrary byte where the block never wrote it
            let old = match self.env.get(&k) {
                Some(v) => v.t.clone(),
                None => self.decl("mbyte", &Ty::Uint(8)),
            };
            let b = format!("({byte} {rd} {i})");
            self.smt.send(&format!("(assert {})\n", Ty::Uint(8).range(&b).unwrap()));
            let t = self.def("rdbyte", "Int", &format!("(ite (< {i} {size}) {b} {old})"));
            self.ranges.insert(t.clone(), (BigInt::zero(), BigInt::from(255)));
            self.env.insert(k, Var { ty: Ty::Uint(8), t, len: None });
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
        let r = self.y_block(&mut cx, &a.block);
        self.mem_drop();
        r
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
            Ty::Uint(256) | Ty::Bytes(256) => v,
            // any word: a call through it must show it holds a function (`call_ptr`)
            Ty::Fn(_) => v,
            Ty::Bytes(_) => {
                // The word holds the bytes first; what code reads of the low bytes it leaves
                // is not modelled.
                return unsupported(
                    span,
                    format!("assembly assignment to `{n}` of type {}", var.ty.name()),
                );
            }
            Ty::Enum(..) => {
                // the raw word: its range is owed where Solidity reads it (`check_raw_enum`)
                if self.spec {
                    return unsupported(
                        span,
                        format!("assembly assignment to enum `{n}` in a spec"),
                    );
                }
                self.raw_enums.insert(n.clone());
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
            // On the stack a `bytesN` holds its bytes first: the value shifted to the top.
            Ty::Bytes(256) => Ok(v.t.clone()),
            Ty::Bytes(k) => Ok(format!("(* {} {})", v.t, num(&pow2(256 - *k as u32)))),
            Ty::Uint(_) | Ty::Addr | Ty::Contract(_) | Ty::Enum(..) => {
                // an enum assembly set holds a raw word: no range is known of it
                let raw = matches!(v.ty, Ty::Enum(..)) && self.raw_enums.contains(n);
                if !raw
                    && let Some(b) = v.ty.bounds()
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
                if let Some((_, _, _, p)) = cx.regions.iter().find(|r| r.0 == n) {
                    return Ok(p.clone());
                }
                let fields = self.prog.struct_fields(sn).cloned().unwrap_or_default();
                let base = REGION_BASE + (cx.regions.len() as u64) * 0x10000;
                for (i, (f, fty)) in fields.iter().enumerate() {
                    // A struct or array field is a pointer in memory, whatever its ABI words.
                    let mut ws = vec![];
                    if matches!(fty, Ty::Struct(_) | Ty::Arr(..))
                        || !self.abi_words(fty, &format!("(|{sn}.{f}| {})", v.t), &mut ws)
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
                // Its value is where Solidity allocated it: some address past the scratch space,
                // unknown here (and unrelated across blocks). Fields read through it resolve to
                // the region modelled above (see `y_region_addr`).
                let len = 32 * fields.len() as u64;
                let p = self.y_fresh(span, "sptr")?;
                self.smt.send(&format!(
                    "(assert (and (>= {p} {SCRATCH_END}) (< (+ {p} {len}) {})))
",
                    1u64 << 32
                ));
                self.ranges.insert(
                    p.clone(),
                    (BigInt::from(SCRATCH_END), BigInt::from((1u64 << 32) - len - 1)),
                );
                cx.regions.push((n.to_string(), base, len, p.clone()));
                Ok(p)
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
                Ty::Uint(n) | Ty::Int(n) | Ty::Bytes(n) => (*n as u32) / 8,
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
        for (f, ty, shift, bits) in &layout {
            let raw = format!("(mod (div {v} {}) {})", num(&pow2(*shift)), num(&pow2(*bits)));
            // The word must hold a valid bool and enum value: the struct value stands for its
            // fields, and an invalid byte would be read back as a different word.
            let valid = match ty {
                Ty::Bool => Some("bool".to_string()),
                Ty::Enum(e, _) => Some(format!("enum {e}")),
                _ => None,
            };
            if let (Some(what), Some((_, hi))) =
                (valid, ty.bounds().or(Some((BigInt::zero(), BigInt::one()))))
            {
                self.prove(
                    &format!("(<= {raw} {})", num(&hi)),
                    &format!("assembly `sstore` through `{ptr}.slot` may write an invalid {what} to field `{f}`"),
                    span,
                    None,
                );
            }
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

    /// The contract whose `type(C).creationCode` a `create`/`create2` deploys, when its code
    /// arguments are `add(p, 32)` and `mload(p)` of a Solidity variable `p` holding that value.
    fn y_creation_code(&self, cx: &Yc<'a>, c: &'a yul::ExprCall<'a>) -> Option<String> {
        let k = c.arguments.len();
        let (off, len) = (&c.arguments[k - 3], &c.arguments[k - 2]);
        let path = |e: &yul::Expr<'_>| match &e.kind {
            yul::ExprKind::Path(p) if p.segments().len() == 1 => {
                Some(p.segments()[0].as_str().to_string())
            }
            _ => None,
        };
        let lit32 = |e: &yul::Expr<'_>| matches!(&e.kind, yul::ExprKind::Lit(l) if matches!(&l.kind, LitKind::Number(n) if *n == 32u64));
        let yul::ExprKind::Call(add) = &off.kind else { return None };
        let yul::ExprKind::Call(ml) = &len.kind else { return None };
        if add.name.as_str() != "add" || ml.name.as_str() != "mload" || ml.arguments.len() != 1 {
            return None;
        }
        let [x, y] = &add.arguments[..] else { return None };
        let p = match (path(x), path(y)) {
            (Some(p), None) if lit32(y) => p,
            (None, Some(p)) if lit32(x) => p,
            _ => return None,
        };
        if path(&ml.arguments[0]).as_deref() != Some(p.as_str()) || Self::is_local(cx, &p) {
            return None;
        }
        let v = self.env.get(&p)?;
        self.creation_code.get(&v.t).cloned()
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
            // the frame's latest call's, once every path made one (see `rd_note`)
            "returndatasize"
                if !self.spec
                    && matches!(
                        self.env.get(super::RD_N).map(|v| v.t.as_str()),
                        Some("1" | "2")
                    ) =>
            {
                want(0)?;
                one(self.env[super::RD_SIZE].t.clone())
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
                self.raw_slot_clear(&a[0], name == "tload", span)?;
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
                self.raw_slot_clear(&a[0], name == "tstore", span)?;
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
                if off == 0x40
                    && cx.fmp.is_none()
                    && !(0x40..0x60).any(|i| self.env.contains_key(&mkey(i)))
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
                // calldata is fixed for the whole call: the same offset reads the same word
                // (Solidity calldata parameters are not tied to it)
                self.declare_fun("sv_calldataload", 1, "Int");
                let t = format!("(sv_calldataload {})", a[0]);
                if self.bound.is_empty() || self.y_closed(&t) {
                    let x = self.fresh("calldataload");
                    self.smt.send(&format!(
                        "(declare-const {x} Int)\n(assert (= {x} {t}))\n(assert {})\n",
                        Ty::Uint(256).range(&x).unwrap()
                    ));
                    self.ranges.insert(x.clone(), (BigInt::zero(), w256() - 1));
                    return one(x);
                }
                one(t)
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
                // this contract on success; the output buffer becomes unknown, or, while the
                // frame's latest call is recorded, what the call returned
                let out = self.y_rd_out(&a[5], &a[6]);
                if out.is_none() {
                    self.y_clobber(cx, span, &a[5], &a[6])?;
                }
                let ok = self.decl("call_ok", &Ty::Bool);
                if self.y_konst(&a[2]).is_none_or(|v| !v.is_zero()) {
                    let v = a[2].clone();
                    self.send_value(&v, Some(&ok));
                }
                self.external_call(vec![], false, span)?;
                self.y_rd_record(&ok, out, false);
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
                // the new contract's constructor is outside code, unless the code deployed is
                // `type(C).creationCode` of a contract C of the program whose deployment runs
                // only its own code; the result is an arbitrary address (0 on failure)
                if self.y_konst(&a[0]).is_none_or(|v| !v.is_zero()) {
                    let ok = self.decl("create_ok", &Ty::Bool);
                    let v = a[0].clone();
                    self.send_value(&v, Some(&ok));
                }
                let known = self.y_creation_code(cx, c);
                match &known {
                    Some(cn) if self.deploys_quietly(cn) => {
                        self.bump_code_epoch();
                        self.rd_note(None);
                    }
                    _ => {
                        self.external_call(vec![], false, span)?;
                    }
                }
                let r = self.decl("created", &Ty::Addr);
                match known {
                    Some(cn) => self.created(&r, &cn),
                    None => {
                        // unknown code: it lands at a codeless address all the same
                        let key = if self.env.contains_key("__created") {
                            "__created"
                        } else {
                            "@host.__created"
                        };
                        if let Some(set) = self.env.get(key).cloned() {
                            self.assume(&format!("(or (= {r} 0) (not (select {} {r})))", set.t));
                        }
                    }
                }
                one(r)
            }
            "staticcall" => {
                want(6)?;
                if self.spec {
                    return unsupported(span, "assembly `staticcall` inside a spec evaluation");
                }
                // the output buffer becomes unknown (or what the call returned, as for `call`);
                // the result is an arbitrary success flag
                let out = self.y_rd_out(&a[4], &a[5]);
                if out.is_none() {
                    self.y_clobber(cx, span, &a[4], &a[5])?;
                }
                let ok = self.decl("staticcall", &Ty::Bool);
                self.y_rd_record(&ok, out, true);
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
        if f.item
            .header
            .modifiers
            .iter()
            .any(|m| self.prog.mod_scope(c, f).modifiers.contains_key(m.name.last().as_str()))
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
            canon: FxHashMap::default(),
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
                "create" | "create2" => {
                    out.insert("__created".into());
                    if !lock {
                        *all = true
                    }
                }
                "tstore" => {
                    out.insert("__traw".into());
                }
                // under a held reentrancy lock a call or creation reaches only the entry points
                // still open, whose writes the caller adds (modset_under_lock)
                "call" if lock => {}
                "call" | "callcode" | "delegatecall" | "selfdestruct" => *all = true,
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

/// Whether an assembly block may remove its contract's code or run other code in its place
/// (`selfdestruct`, `delegatecall`, `callcode`).
pub(super) fn asm_destroys(b: &yul::Block<'_>) -> bool {
    fn ex(e: &yul::Expr<'_>) -> bool {
        match &e.kind {
            yul::ExprKind::Call(c) => {
                matches!(c.name.as_str(), "selfdestruct" | "delegatecall" | "callcode")
                    || c.arguments.iter().any(ex)
            }
            _ => false,
        }
    }
    b.stmts.iter().any(|s| {
        use yul::StmtKind as K;
        match &s.kind {
            K::Block(b) => asm_destroys(b),
            K::AssignSingle(_, e) | K::AssignMulti(_, e) | K::Expr(e) | K::VarDecl(_, Some(e)) => {
                ex(e)
            }
            K::If(c, b) => ex(c) || asm_destroys(b),
            K::Switch(sw) => ex(&sw.selector) || sw.cases.iter().any(|c| asm_destroys(&c.body)),
            K::For(f) => {
                asm_destroys(&f.init)
                    || ex(&f.cond)
                    || asm_destroys(&f.step)
                    || asm_destroys(&f.body)
            }
            K::FunctionDef(f) => asm_destroys(&f.body),
            _ => false,
        }
    })
}

/// Whether an assembly block in a function's body leaves its memory parameter `n`'s object as
/// it was (see `Vc::mem_param_written`). Solidity allocates every memory object at or above
/// 0x80, both in legacy and via-IR code generation, and keeps the free memory pointer at 0x40
/// and the zero slot, which empty arrays point to, at 0x60; so a block whose memory writes all
/// fall in the scratch space `[0, 0x40)` writes no object. Every write must be an `mstore` or
/// `mstore8` at a literal offset, or a call whose output buffer has a literal offset and size,
/// ending at or below 0x40. The copying builtins (`calldatacopy`, `codecopy`, `returndatacopy`,
/// `extcodecopy`, `mcopy`, `datacopy`) and the builtins that write memory or run raw code
/// (`setimmutable`, `verbatim_*`) are refused, as is a call to any name that is neither a builtin
/// known to leave memory alone nor a function the block defines, whose body is checked alike.
///
/// The block must not make a Solidity variable name another object either. An assignment to
/// `n` is refused, and so is one to a Solidity variable that `sol_var` says the function declares
/// in memory (`Some(true)`) or does not declare (`None`): its new value, such as `n` itself
/// or the literal 0x80, may point into `n`'s object, which a later Solidity write through it
/// would change. The block's own variables and Solidity value, storage and calldata variables
/// (`Some(false)`) hold no memory reference the function's Solidity code can write through.
pub(super) fn asm_keeps_memory(
    b: &yul::Block<'_>,
    n: &str,
    sol_var: &dyn Fn(&str) -> Option<bool>,
) -> bool {
    struct Cx<'x> {
        n: &'x str,
        sol_var: &'x dyn Fn(&str) -> Option<bool>,
        /// The names the block declares: `let`, function parameters and returns.
        vars: FxHashSet<String>,
        /// The functions the block defines.
        fns: FxHashSet<String>,
    }

    fn decls(cx: &mut Cx<'_>, b: &yul::Block<'_>) {
        use yul::StmtKind as K;
        for s in b.stmts.iter() {
            match &s.kind {
                K::Block(b) | K::If(_, b) => decls(cx, b),
                K::Switch(sw) => sw.cases.iter().for_each(|c| decls(cx, &c.body)),
                K::For(f) => [&f.init, &f.step, &f.body].into_iter().for_each(|b| decls(cx, b)),
                K::FunctionDef(f) => {
                    cx.fns.insert(f.name.as_str().to_string());
                    let names = f.parameters.iter().chain(f.returns.iter());
                    cx.vars.extend(names.map(|i| i.as_str().to_string()));
                    decls(cx, &f.body);
                }
                K::VarDecl(names, _) => {
                    cx.vars.extend(names.iter().map(|i| i.as_str().to_string()))
                }
                K::AssignSingle(..) | K::AssignMulti(..) | K::Expr(_) => {}
                K::Leave | K::Break | K::Continue => {}
            }
        }
    }

    fn konst(e: &yul::Expr<'_>) -> Option<u64> {
        let yul::ExprKind::Lit(l) = &e.kind else { return None };
        let LitKind::Number(v) = &l.kind else { return None };
        u64::try_from(*v).ok()
    }

    /// Whether `len` bytes at `off`, both literals, lie in the scratch space.
    fn scratch(off: &yul::Expr<'_>, len: Option<u64>) -> bool {
        konst(off).zip(len).and_then(|(o, l)| o.checked_add(l)).is_some_and(|end| end <= 0x40)
    }

    fn ex(cx: &Cx<'_>, e: &yul::Expr<'_>) -> bool {
        let yul::ExprKind::Call(c) = &e.kind else { return true };
        let a = &c.arguments;
        let ok = match (c.name.as_str(), a.len()) {
            ("mstore", 2) => scratch(&a[0], Some(32)),
            ("mstore8", 2) => scratch(&a[0], Some(1)),
            // The output buffer, where up to its size of what the callee returns is copied.
            ("call" | "callcode", 7) => scratch(&a[5], konst(&a[6])),
            ("staticcall" | "delegatecall", 6) => scratch(&a[4], konst(&a[5])),
            ("mstore" | "mstore8" | "call" | "callcode" | "staticcall" | "delegatecall", _) => {
                false
            }
            // Builtins that write no memory: they compute, read memory or other state, write
            // storage, log, create a contract from memory, or end the call.
            (
                "stop" | "add" | "sub" | "mul" | "div" | "sdiv" | "mod" | "smod" | "exp" | "not"
                | "lt" | "gt" | "slt" | "sgt" | "eq" | "iszero" | "and" | "or" | "xor" | "byte"
                | "shl" | "shr" | "sar" | "clz" | "addmod" | "mulmod" | "signextend" | "keccak256"
                | "address" | "balance" | "origin" | "caller" | "callvalue" | "calldataload"
                | "calldatasize" | "codesize" | "gasprice" | "extcodesize" | "returndatasize"
                | "extcodehash" | "blockhash" | "blobhash" | "coinbase" | "timestamp" | "number"
                | "difficulty" | "prevrandao" | "gaslimit" | "chainid" | "selfbalance" | "basefee"
                | "blobbasefee" | "pop" | "mload" | "sload" | "sstore" | "tload" | "tstore"
                | "msize" | "gas" | "log0" | "log1" | "log2" | "log3" | "log4" | "create"
                | "create2" | "return" | "revert" | "selfdestruct" | "invalid" | "datasize"
                | "dataoffset" | "loadimmutable" | "linkersymbol" | "memoryguard",
                _,
            ) => true,
            (name, _) => cx.fns.contains(name),
        };
        ok && a.iter().all(|a| ex(cx, a))
    }

    fn target(cx: &Cx<'_>, p: &ast::AstPath<'_>) -> bool {
        let root = p.segments()[0].as_str();
        root != cx.n
            && match (cx.sol_var)(root) {
                Some(memory) => !memory,
                None => cx.vars.contains(root),
            }
    }

    fn blk(cx: &Cx<'_>, b: &yul::Block<'_>) -> bool {
        use yul::StmtKind as K;
        b.stmts.iter().all(|s| match &s.kind {
            K::Block(b) | K::FunctionDef(yul::Function { body: b, .. }) => blk(cx, b),
            K::AssignSingle(p, e) => target(cx, p) && ex(cx, e),
            K::AssignMulti(ps, e) => ps.iter().all(|p| target(cx, p)) && ex(cx, e),
            K::Expr(e) | K::VarDecl(_, Some(e)) => ex(cx, e),
            K::If(c, b) => ex(cx, c) && blk(cx, b),
            K::Switch(sw) => ex(cx, &sw.selector) && sw.cases.iter().all(|c| blk(cx, &c.body)),
            K::For(f) => {
                blk(cx, &f.init) && ex(cx, &f.cond) && blk(cx, &f.step) && blk(cx, &f.body)
            }
            K::VarDecl(_, None) | K::Leave | K::Break | K::Continue => true,
        })
    }
    let mut cx = Cx { n, sol_var, vars: FxHashSet::default(), fns: FxHashSet::default() };
    decls(&mut cx, b);
    blk(&cx, b)
}

impl<'p, 'a> Vc<'p, 'a> {
    /// A raw storage slot that assembly reaches must lie past the slots the declared state
    /// variables hold, which the executor models apart from raw storage.
    fn raw_slot_clear(&mut self, slot: &str, transient: bool, span: Span) -> R<()> {
        let c = if self.in_library() { self.host } else { self.c };
        if c.custom_layout {
            return unsupported(span, "raw storage in a contract with a custom storage layout");
        }
        let n: BigInt = c
            .state
            .iter()
            .filter(|s| !s.constant && !s.immutable && !s.ghost && s.transient == transient)
            .map(|s| slots_of(self.prog, &s.ty))
            .sum();
        if n.is_zero() {
            return Ok(());
        }
        let phi = format!("(>= {slot} {})", num(&n));
        let kind = if transient { "transient storage" } else { "storage" };
        if self.spec {
            let g = self.guard.clone();
            if !matches!(self.smt.check(&g, &phi, &[]), crate::smt::Res::Unsat) {
                return unsupported(
                    span,
                    format!("raw {kind} slot read in a spec may overlap a declared state variable"),
                );
            }
            return Ok(());
        }
        self.prove(
            &phi,
            &format!(
                "raw {kind} slot may overlap a declared state variable (they take at most slots 0 to {})",
                n - 1
            ),
            span,
            None,
        );
        Ok(())
    }
}

/// How many storage slots a declared variable of type ty may take, without packing.
fn slots_of(prog: &crate::prog::Program<'_>, ty: &Ty) -> BigInt {
    match ty {
        Ty::Arr(el, Some(n)) => BigInt::from(*n) * slots_of(prog, el),
        Ty::Struct(sn) => {
            let fields = prog.struct_fields(sn).cloned().unwrap_or_default();
            let n: BigInt = fields.iter().map(|(_, t)| slots_of(prog, t)).sum();
            n.max(BigInt::one())
        }
        _ => BigInt::one(),
    }
}

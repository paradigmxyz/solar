//! Exact lowering of a solverus query from unbounded Int to fixed-width bitvectors.
//!
//! Every Int subterm of the query gets an interval from the range facts the encoder asserts,
//! SSA definitions, and typing hints (`;@elem` for array elements, `;@ret` for uninterpreted
//! function results). With W = (largest bit length) + 2, two's-complement BV-W arithmetic agrees
//! with Int arithmetic on every subterm, so mapping any Int model of the query into BV-W gives a
//! model of the lowered query: **unsat of the lowered query implies unsat of the original.**
//! Assertions that cannot be lowered (an unbounded subterm, an unknown function) are dropped,
//! which only weakens the lowered query, so the implication still holds. Quantified variables
//! without a range are allowed only where they are effectively universal (an assumption), where
//! restricting their domain also only weakens the query.
use num_bigint::BigInt;
use num_traits::{One, Signed, Zero};
use std::{cell::RefCell, collections::HashMap};

type Iv = (BigInt, BigInt);

#[derive(Debug, Clone)]
enum S {
    A(String),
    L(Vec<Self>),
}

fn tokenize(s: &str, hints: &mut Vec<String>) -> Vec<String> {
    let b: Vec<char> = s.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == ';' {
            let mut j = i;
            while j < b.len() && b[j] != '\n' {
                j += 1;
            }
            let line: String = b[i..j].iter().collect();
            if line.starts_with(";@") {
                hints.push(line);
            }
            i = j;
        } else if c == '(' || c == ')' {
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
        } else if c == '"' {
            let mut j = i + 1;
            while j < b.len() && b[j] != '"' {
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

fn parse_all(toks: &[String]) -> Vec<S> {
    let mut i = 0;
    let mut out = vec![];
    while i < toks.len() {
        match parse_at(toks, &mut i) {
            Some(s) => out.push(s),
            None => break,
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

fn pr(s: &S) -> String {
    fn go(s: &S, o: &mut String) {
        match s {
            S::A(a) => o.push_str(a),
            S::L(v) => {
                o.push('(');
                for (i, x) in v.iter().enumerate() {
                    if i > 0 {
                        o.push(' ');
                    }
                    go(x, o);
                }
                o.push(')');
            }
        }
    }
    let mut o = String::new();
    go(s, &mut o);
    o
}

fn num(s: &S) -> Option<BigInt> {
    match s {
        S::A(a) => a.parse().ok(),
        S::L(v) => match v.as_slice() {
            [S::A(m), x] if m == "-" => num(x).map(|n| -n),
            _ => None,
        },
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Sort {
    Int,
    Bool,
    Bv(u32),
    Arr(Box<Self>, Box<Self>),
    Other,
}

fn sort_of(s: &S) -> Sort {
    match s {
        S::A(a) if a == "Int" => Sort::Int,
        S::A(a) if a == "Bool" => Sort::Bool,
        S::L(v) => match v.as_slice() {
            [S::A(u), S::A(b), S::A(n)] if u == "_" && b == "BitVec" => {
                n.parse().map(Sort::Bv).unwrap_or(Sort::Other)
            }
            [S::A(a), k, e] if a == "Array" => {
                Sort::Arr(Box::new(sort_of(k)), Box::new(sort_of(e)))
            }
            _ => Sort::Other,
        },
        _ => Sort::Other,
    }
}

fn lower_sort(s: &Sort, w: u32) -> Option<String> {
    Some(match s {
        Sort::Int => format!("(_ BitVec {w})"),
        Sort::Bool => "Bool".into(),
        Sort::Bv(n) => format!("(_ BitVec {n})"),
        Sort::Arr(k, e) => format!("(Array {} {})", lower_sort(k, w)?, lower_sort(e, w)?),
        Sort::Other => return None,
    })
}

/// Kind of a term: its bound (Int), or its shape.
#[derive(Debug, Clone)]
enum K {
    Int(Iv),
    Bool,
    Bv(u32),
    Arr(Box<Self>),
}

fn union(a: &K, b: &K) -> Option<K> {
    Some(match (a, b) {
        (K::Int((l1, h1)), K::Int((l2, h2))) => K::Int((l1.min(l2).clone(), h1.max(h2).clone())),
        (K::Bool, K::Bool) => K::Bool,
        (K::Bv(n), K::Bv(m)) if n == m => K::Bv(*n),
        (K::Arr(x), K::Arr(y)) => K::Arr(Box::new(union(x, y)?)),
        _ => return None,
    })
}

fn bits(x: &BigInt) -> u64 {
    x.abs().bits()
}

fn pow2(n: u64) -> BigInt {
    BigInt::one() << n
}

type R<T> = Result<T, String>;

struct Ctx {
    consts: HashMap<String, Sort>,
    funs: HashMap<String, (Vec<Sort>, Sort)>,
    ranges: HashMap<String, Iv>,
    defs: HashMap<String, S>,
    facts: HashMap<String, Iv>,
    elem: HashMap<String, Iv>,
    ret: HashMap<String, Iv>,
    memo: RefCell<HashMap<String, Option<K>>>,
    maxbits: RefCell<u64>,
}

fn intersect(a: Option<Iv>, b: Option<Iv>) -> Option<Iv> {
    match (a, b) {
        (Some((l1, h1)), Some((l2, h2))) => Some((l1.max(l2), h1.min(h2))),
        (x, None) | (None, x) => x,
    }
}

fn nest(depth: usize, iv: Iv) -> K {
    let mut k = K::Int(iv);
    for _ in 0..depth {
        k = K::Arr(Box::new(k));
    }
    k
}

fn arr_depth(s: &Sort) -> usize {
    match s {
        Sort::Arr(_, e) => 1 + arr_depth(e),
        _ => 0,
    }
}

/// (lo <= t <= hi) facts in the encoder's shape.
fn range_fact(s: &S) -> Option<(S, Iv)> {
    let S::L(v) = s else { return None };
    let [S::A(and), a, b] = v.as_slice() else {
        return None;
    };
    if and != "and" {
        return None;
    }
    let (S::L(a), S::L(b)) = (a, b) else {
        return None;
    };
    match (a.as_slice(), b.as_slice()) {
        ([S::A(le1), lo, t1], [S::A(le2), t2, hi])
            if le1 == "<=" && le2 == "<=" && pr(t1) == pr(t2) =>
        {
            Some((t1.clone(), (num(lo)?, num(hi)?)))
        }
        _ => None,
    }
}

impl Ctx {
    fn note(&self, iv: &Iv) {
        let b = bits(&iv.0).max(bits(&iv.1));
        let mut m = self.maxbits.borrow_mut();
        if b > *m {
            *m = b;
        }
    }

    fn atom(&self, a: &str, scope: &HashMap<String, K>) -> R<K> {
        if let Some(k) = scope.get(a) {
            return Ok(k.clone());
        }
        if a == "true" || a == "false" {
            return Ok(K::Bool);
        }
        if let Ok(n) = a.parse::<BigInt>() {
            return Ok(K::Int((n.clone(), n)));
        }
        if let Some(m) = self.memo.borrow().get(a) {
            return m.clone().ok_or_else(|| format!("unbounded {a}"));
        }
        let sort = self.consts.get(a).cloned().ok_or_else(|| format!("unknown symbol {a}"))?;
        let r = match &sort {
            Sort::Bool => Some(K::Bool),
            Sort::Bv(n) => Some(K::Bv(*n)),
            Sort::Int => {
                let def = self.defs.get(a).and_then(|d| match self.bnd(d, &HashMap::new()) {
                    Ok(K::Int(iv)) => Some(iv),
                    _ => None,
                });
                intersect(self.ranges.get(a).cloned(), def).map(K::Int)
            }
            Sort::Arr(..) => {
                let d = arr_depth(&sort);
                let from_def = self.defs.get(a).and_then(|d| self.bnd(d, &HashMap::new()).ok());
                match (self.elem.get(a), from_def) {
                    (Some(iv), _) => Some(nest(d, iv.clone())),
                    (None, Some(k)) => Some(k),
                    _ => None,
                }
            }
            Sort::Other => None,
        };
        self.memo.borrow_mut().insert(a.to_string(), r.clone());
        r.ok_or_else(|| format!("unbounded {a}"))
    }

    fn int(&self, s: &S, sc: &HashMap<String, K>) -> R<Iv> {
        match self.bnd(s, sc)? {
            K::Int(iv) => Ok(iv),
            k => Err(format!("expected Int, got {k:?} in {}", short(s))),
        }
    }

    fn bnd(&self, s: &S, sc: &HashMap<String, K>) -> R<K> {
        let k = self.bnd_inner(s, sc)?;
        if let K::Int(iv) = &k {
            self.note(iv);
        }
        Ok(k)
    }

    fn bnd_inner(&self, s: &S, sc: &HashMap<String, K>) -> R<K> {
        let v = match s {
            S::A(a) => return self.atom(a, sc),
            S::L(v) => v,
        };
        if let Some(n) = num(s) {
            return Ok(K::Int((n.clone(), n)));
        }
        let head = match v.first() {
            Some(S::A(h)) => h.as_str(),
            Some(S::L(h)) => {
                // indexed / qualified heads
                match h.as_slice() {
                    [S::A(u), S::A(f), S::A(n)] if u == "_" && f == "int2bv" => {
                        self.int(&v[1], sc)?;
                        return Ok(K::Bv(n.parse().map_err(|_| "bad width")?));
                    }
                    [S::A(u), S::A(f), S::A(i), S::A(j)] if u == "_" && f == "extract" => {
                        self.bnd(&v[1], sc)?;
                        let (i, j): (u32, u32) =
                            (i.parse().map_err(|_| "w")?, j.parse().map_err(|_| "w")?);
                        return Ok(K::Bv(i - j + 1));
                    }
                    [S::A(u), S::A(f), S::A(k)]
                        if u == "_" && (f == "zero_extend" || f == "sign_extend") =>
                    {
                        let K::Bv(n) = self.bnd(&v[1], sc)? else {
                            return Err("ext".into());
                        };
                        return Ok(K::Bv(n + k.parse::<u32>().map_err(|_| "w")?));
                    }
                    [S::A(a), S::A(c), _] if a == "as" && c == "const" => {
                        return Ok(K::Arr(Box::new(self.bnd(&v[1], sc)?)));
                    }
                    _ => return Err(format!("unsupported head {}", short(s))),
                }
            }
            None => return Err("empty".into()),
        };
        let args = &v[1..];
        let ints = |me: &Self| -> R<Vec<Iv>> { args.iter().map(|a| me.int(a, sc)).collect() };
        Ok(match head {
            "_" => match args {
                [S::A(b), S::A(n)] if b.starts_with("bv") => K::Bv(n.parse().map_err(|_| "w")?),
                _ => return Err("indexed".into()),
            },
            "+" | "-" | "*" => {
                let xs = ints(self)?;
                if head == "-" && xs.len() == 1 {
                    let (l, h) = &xs[0];
                    return Ok(K::Int((-h, -l)));
                }
                let mut acc = xs[0].clone();
                for (l, h) in &xs[1..] {
                    acc = match head {
                        "+" => (acc.0 + l, acc.1 + h),
                        "-" => (acc.0 - h, acc.1 - l),
                        _ => {
                            let c = [&acc.0 * l, &acc.0 * h, &acc.1 * l, &acc.1 * h];
                            (c.iter().min().unwrap().clone(), c.iter().max().unwrap().clone())
                        }
                    };
                }
                K::Int(acc)
            }
            "div" | "mod" => {
                let xs = ints(self)?;
                let ((al, ah), (bl, bh)) = (&xs[0], &xs[1]);
                if !bl.is_positive() {
                    return Err("division by a possibly non-positive value".into());
                }
                if head == "mod" {
                    if !al.is_negative() && ah < bl {
                        K::Int((al.clone(), ah.clone()))
                    } else {
                        K::Int((BigInt::zero(), bh - 1))
                    }
                } else {
                    let m = al.abs().max(ah.abs());
                    if !al.is_negative() {
                        K::Int((al / bh, ah / bl))
                    } else {
                        K::Int((-(&m) - 1, m))
                    }
                }
            }
            "abs" => {
                let (l, h) = self.int(&args[0], sc)?;
                let m = l.abs().max(h.abs());
                K::Int((BigInt::zero(), m))
            }
            "ite" => {
                self.boolean(&args[0], sc)?;
                let (a, b) = (self.bnd(&args[1], sc)?, self.bnd(&args[2], sc)?);
                union(&a, &b).ok_or("ite of mismatched kinds")?
            }
            "<" | "<=" | ">" | ">=" => {
                ints(self)?;
                K::Bool
            }
            "=" | "distinct" => {
                let ks: Vec<K> = args.iter().map(|a| self.bnd(a, sc)).collect::<R<_>>()?;
                if ks.iter().any(|k| matches!(k, K::Arr(_))) {
                    // array (in)equality is not preserved by restricting the index domain
                    return Err("array equality".into());
                }
                K::Bool
            }
            "and" | "or" | "not" | "=>" | "xor" => {
                for a in args {
                    self.boolean(a, sc)?;
                }
                K::Bool
            }
            "select" => {
                let a = self.bnd(&args[0], sc)?;
                self.int(&args[1], sc)?;
                let fact = if sc.is_empty() { self.facts.get(&pr(s)).cloned() } else { None };
                let elem = match a {
                    K::Arr(e) => *e,
                    _ => return Err("select of non-array".into()),
                };
                match (fact, elem) {
                    (Some(f), K::Int(e)) => K::Int(intersect(Some(f), Some(e)).unwrap()),
                    (Some(f), _) => K::Int(f),
                    (None, e) => e,
                }
            }
            "store" => {
                let a = self.bnd(&args[0], sc)?;
                self.int(&args[1], sc)?;
                let x = self.bnd(&args[2], sc)?;
                match a {
                    K::Arr(e) => K::Arr(Box::new(union(&e, &x).ok_or("store kind")?)),
                    _ => return Err("store of non-array".into()),
                }
            }
            "bv2nat" | "bv2int" => {
                let K::Bv(n) = self.bnd(&args[0], sc)? else {
                    return Err("bv2nat".into());
                };
                K::Int((BigInt::zero(), pow2(n as u64) - 1))
            }
            "|pow2|" | "|pow2w|" => {
                let (l, h) = self.int(&args[0], sc)?;
                let cap = BigInt::from(256);
                let neg = l.is_negative();
                let lo: u64 = if l >= cap || (neg && h.is_negative()) {
                    256
                } else if neg {
                    0
                } else {
                    (&l).try_into().unwrap()
                };
                let hi: u64 = if h >= cap || neg { 256 } else { (&h).try_into().unwrap() };
                let lo = if head == "|pow2w|" { BigInt::zero() } else { pow2(lo.min(hi)) };
                K::Int((lo, pow2(hi)))
            }
            "concat" => {
                let mut n = 0;
                for a in args {
                    let K::Bv(m) = self.bnd(a, sc)? else {
                        return Err("concat".into());
                    };
                    n += m;
                }
                K::Bv(n)
            }
            "bvult" | "bvule" | "bvugt" | "bvuge" | "bvslt" | "bvsle" | "bvsgt" | "bvsge" => {
                for a in args {
                    self.bnd(a, sc)?;
                }
                K::Bool
            }
            h if h.starts_with("bv") => {
                let mut w = None;
                for a in args {
                    if let K::Bv(n) = self.bnd(a, sc)? {
                        w = Some(n);
                    }
                }
                K::Bv(w.ok_or("bv op")?)
            }
            "forall" | "exists" => return Err("nested quantifier outside boolean context".into()),
            "!" => self.bnd(&args[0], sc)?,
            f => {
                let (ps, ret) =
                    self.funs.get(f).cloned().ok_or_else(|| format!("unknown function {f}"))?;
                for (a, p) in args.iter().zip(&ps) {
                    let k = self.bnd(a, sc)?;
                    if *p == Sort::Int && !matches!(k, K::Int(_)) {
                        return Err("arg".into());
                    }
                }
                match ret {
                    Sort::Int => {
                        let fact =
                            if sc.is_empty() { self.facts.get(&pr(s)).cloned() } else { None };
                        K::Int(
                            intersect(fact, self.ret.get(f).cloned())
                                .ok_or_else(|| format!("unbounded result of {f}"))?,
                        )
                    }
                    Sort::Bool => K::Bool,
                    Sort::Bv(n) => K::Bv(n),
                    _ => return Err("fun sort".into()),
                }
            }
        })
    }

    /// Boolean structure with quantifiers; `pos` = polarity (Some(true) positive, Some(false)
    /// negative, None both).
    fn boolean(&self, s: &S, sc: &HashMap<String, K>) -> R<()> {
        self.boolean_p(s, sc, Some(true))
    }

    fn boolean_p(&self, s: &S, sc: &HashMap<String, K>, pol: Option<bool>) -> R<()> {
        let flip = |p: Option<bool>| p.map(|b| !b);
        if let S::L(v) = s
            && let Some(S::A(h)) = v.first()
        {
            match h.as_str() {
                "not" => return self.boolean_p(&v[1], sc, flip(pol)),
                "and" | "or" => {
                    for a in &v[1..] {
                        self.boolean_p(a, sc, pol)?;
                    }
                    return Ok(());
                }
                "=>" => {
                    for a in &v[1..v.len() - 1] {
                        self.boolean_p(a, sc, flip(pol))?;
                    }
                    return self.boolean_p(v.last().unwrap(), sc, pol);
                }
                "!" => return self.boolean_p(&v[1], sc, pol),
                "forall" | "exists" => {
                    let universal = match pol {
                        Some(p) => (h == "forall") == p,
                        None => false,
                    };
                    let S::L(bs) = &v[1] else {
                        return Err("binders".into());
                    };
                    let body = &v[2];
                    // the encoder's shape: (=> rng body) / (and rng body)
                    let mut ranges: HashMap<String, Iv> = HashMap::new();
                    let rng = match body {
                        S::L(b)
                            if matches!(b.first(), Some(S::A(x)) if x == "=>" || x == "and")
                                && b.len() == 3 =>
                        {
                            Some(&b[1])
                        }
                        _ => None,
                    };
                    if let Some(r) = rng {
                        let cs: Vec<&S> = match r {
                            S::L(c)
                                if matches!(c.first(), Some(S::A(x)) if x == "and")
                                    && range_fact(r).is_none() =>
                            {
                                c[1..].iter().collect()
                            }
                            _ => vec![r],
                        };
                        for c in cs {
                            if let Some((S::A(x), iv)) = range_fact(c) {
                                ranges.insert(x, iv);
                            }
                        }
                    }
                    let mut sc2 = sc.clone();
                    for b in bs {
                        let S::L(b) = b else {
                            return Err("binder".into());
                        };
                        let (S::A(name), srt) = (&b[0], sort_of(&b[1])) else {
                            return Err("binder".into());
                        };
                        let k = match srt {
                            Sort::Int => match ranges.get(name) {
                                Some(iv) => K::Int(iv.clone()),
                                None if universal => K::Int((-pow2(256), pow2(256))),
                                None => return Err("unranged existential binder".into()),
                            },
                            Sort::Bool => K::Bool,
                            Sort::Bv(n) => K::Bv(n),
                            _ => return Err("binder sort".into()),
                        };
                        sc2.insert(name.clone(), k);
                    }
                    return self.boolean_p(body, &sc2, pol);
                }
                "ite" => {
                    self.boolean_p(&v[1], sc, None)?;
                    self.boolean_p(&v[2], sc, pol)?;
                    return self.boolean_p(&v[3], sc, pol);
                }
                "=" if v.len() == 3 => {
                    if let (Ok(K::Bool), Ok(K::Bool)) =
                        (self.bnd_quiet(&v[1], sc), self.bnd_quiet(&v[2], sc))
                    {
                        self.boolean_p(&v[1], sc, None)?;
                        return self.boolean_p(&v[2], sc, None);
                    }
                }
                _ => {}
            }
        }
        match self.bnd(s, sc)? {
            K::Bool => Ok(()),
            _ => Err("expected Bool".into()),
        }
    }

    fn bnd_quiet(&self, s: &S, sc: &HashMap<String, K>) -> R<K> {
        // shape only (cheap for the Bool test)
        match s {
            S::A(a) if a == "true" || a == "false" => Ok(K::Bool),
            S::A(a) => self.atom(a, sc),
            S::L(v) => match v.first() {
                Some(S::A(h))
                    if [
                        "and", "or", "not", "=>", "<", "<=", ">", ">=", "=", "distinct", "forall",
                        "exists", "xor",
                    ]
                    .contains(&h.as_str())
                        || h.starts_with("bvu")
                        || h.starts_with("bvs") && h.len() == 5 =>
                {
                    Ok(K::Bool)
                }
                _ => self.bnd(s, sc),
            },
        }
    }
}

fn pow2_name(s: &S) -> Option<&str> {
    match s {
        S::L(v) if v.len() == 2 => match &v[0] {
            S::A(h) if h == "|pow2|" || h == "|pow2w|" => Some(h.as_str()),
            _ => None,
        },
        _ => None,
    }
}

fn is_pow2_app(s: &S) -> Option<&S> {
    pow2_name(s)?;
    let S::L(v) = s else { return None };
    Some(&v[1])
}

fn short(s: &S) -> String {
    let p = pr(s);
    if p.len() > 80 { format!("{}…", &p[..80]) } else { p }
}

struct Em<'c> {
    c: &'c Ctx,
    w: u32,
    fresh: RefCell<usize>,
}

impl Em<'_> {
    fn lit(&self, n: &BigInt) -> String {
        let m = pow2(self.w as u64);
        let v = ((n % &m) + &m) % &m;
        format!("(_ bv{v} {})", self.w)
    }

    fn em(&self, s: &S, sc: &HashMap<String, K>) -> R<String> {
        let w = self.w;
        if let Some(n) = num(s) {
            return Ok(self.lit(&n));
        }
        let v = match s {
            S::A(a) => return Ok(a.clone()),
            S::L(v) => v,
        };
        let e = |x: &S| self.em(x, sc);
        if let S::L(h) = &v[0] {
            return Ok(match h.as_slice() {
                [S::A(u), S::A(f), S::A(n)] if u == "_" && f == "int2bv" => {
                    let n: u32 = n.parse().unwrap();
                    let t = e(&v[1])?;
                    if n <= w {
                        format!("((_ extract {} 0) {t})", n - 1)
                    } else {
                        format!("((_ sign_extend {}) {t})", n - w)
                    }
                }
                [S::A(a), S::A(c), srt] if a == "as" && c == "const" => {
                    format!(
                        "((as const {}) {})",
                        lower_sort(&sort_of(srt), w).ok_or("sort")?,
                        e(&v[1])?
                    )
                }
                _ => format!(
                    "({} {})",
                    pr(&v[0]),
                    v[1..].iter().map(e).collect::<R<Vec<_>>>()?.join(" ")
                ),
            });
        }
        let S::A(head) = &v[0] else { unreachable!() };
        let args = &v[1..];
        let ea = || -> R<Vec<String>> { args.iter().map(e).collect() };
        let fold = |op: &str, xs: Vec<String>| -> String {
            let mut acc = xs[0].clone();
            for x in &xs[1..] {
                acc = format!("({op} {acc} {x})");
            }
            acc
        };
        Ok(match head.as_str() {
            "_" => pr(s),
            "+" => fold("bvadd", ea()?),
            "*" => fold("bvmul", ea()?),
            "-" if args.len() == 1 => format!("(bvneg {})", e(&args[0])?),
            "-" => fold("bvsub", ea()?),
            "<" => format!("(bvslt {} {})", e(&args[0])?, e(&args[1])?),
            "<=" => format!("(bvsle {} {})", e(&args[0])?, e(&args[1])?),
            ">" => format!("(bvsgt {} {})", e(&args[0])?, e(&args[1])?),
            ">=" => format!("(bvsge {} {})", e(&args[0])?, e(&args[1])?),
            "abs" => {
                let a = e(&args[0])?;
                format!("(ite (bvslt {a} {}) (bvneg {a}) {a})", self.lit(&BigInt::zero()))
            }
            // a / 2^e with 0 <= a < 2^256  ->  logical shift (2^e for e outside [0,256) is 2^256 or
            // the shift amount is >= w, and both give 0)
            "div"
                if is_pow2_app(&args[1]).is_some()
                    && pow2_name(&args[1]) == Some("|pow2|")
                    && self
                        .c
                        .int(&args[0], sc)
                        .map(|(l, h)| !l.is_negative() && h < pow2(256))
                        .unwrap_or(false) =>
            {
                format!("(bvlshr {} {})", e(&args[0])?, e(is_pow2_app(&args[1]).unwrap())?)
            }
            // x mod 2^k  ->  low k bits (two's complement low bits are the Euclidean remainder)
            "mod"
                if num(&args[1]).is_some_and(|m| {
                    m.is_positive() && (&m & (&m - 1u32)).is_zero() && m.bits() - 1 < w as u64
                }) =>
            {
                let k = num(&args[1]).unwrap().bits() - 1;
                if k == 0 {
                    self.lit(&BigInt::zero())
                } else {
                    format!(
                        "((_ zero_extend {}) ((_ extract {} 0) {}))",
                        w as u64 - k,
                        k - 1,
                        e(&args[0])?
                    )
                }
            }
            "div" | "mod" => {
                let (a, b) = (e(&args[0])?, e(&args[1])?);
                let nonneg =
                    self.c.int(&args[0], sc).map(|iv| !iv.0.is_negative()).unwrap_or(false);
                let z = self.lit(&BigInt::zero());
                if nonneg {
                    format!("({} {a} {b})", if head == "div" { "bvudiv" } else { "bvurem" })
                } else if head == "mod" {
                    format!("(let ((r (bvsrem {a} {b}))) (ite (bvslt r {z}) (bvadd r {b}) r))")
                } else {
                    format!(
                        "(ite (and (bvslt {a} {z}) (not (= (bvsrem {a} {b}) {z}))) (bvsub (bvsdiv {a} {b}) {}) (bvsdiv {a} {b}))",
                        self.lit(&BigInt::one())
                    )
                }
            }
            "bv2nat" | "bv2int" => {
                let K::Bv(n) = self.c.bnd(&args[0], sc)? else {
                    return Err("bv2nat".into());
                };
                let x = e(&args[0])?;
                if n < w {
                    format!("((_ zero_extend {}) {x})", w - n)
                } else {
                    format!("((_ extract {} 0) {x})", w - 1)
                }
            }
            "|pow2|" | "|pow2w|" => {
                let x = e(&args[0])?;
                let z = self.lit(&BigInt::zero());
                let c256 = self.lit(&BigInt::from(256));
                let big = self.lit(&pow2(256));
                let shl = format!("(bvshl {} {x})", self.lit(&BigInt::one()));
                let inr = format!("(and (bvsge {x} {z}) (bvslt {x} {c256}))");
                if head == "|pow2|" {
                    format!("(ite {inr} {shl} {big})")
                } else {
                    format!("(ite {inr} {shl} (ite (bvsge {x} {c256}) {z} {big}))")
                }
            }
            "forall" | "exists" => {
                let S::L(bs) = &args[0] else {
                    return Err("binders".into());
                };
                let mut sc2 = sc.clone();
                let mut out = vec![];
                for b in bs {
                    let S::L(b) = b else {
                        return Err("binder".into());
                    };
                    let S::A(name) = &b[0] else {
                        return Err("binder".into());
                    };
                    let srt = sort_of(&b[1]);
                    out.push(format!("({name} {})", lower_sort(&srt, w).ok_or("sort")?));
                    sc2.insert(name.clone(), K::Bool); // shape is irrelevant while emitting
                }
                format!("({head} ({}) {})", out.join(" "), self.em(&args[1], &sc2)?)
            }
            "!" => e(&args[0])?,
            _ => format!("({head} {})", ea()?.join(" ")),
        })
    }
}

/// Lower a whole query script. Returns the BV script and how many assertions were dropped.
pub(crate) fn lower(script: &str) -> Option<(String, usize, u32)> {
    let mut hints = vec![];
    let toks = tokenize(script, &mut hints);
    let cmds = parse_all(&toks);
    let mut c = Ctx {
        consts: HashMap::new(),
        funs: HashMap::new(),
        ranges: HashMap::new(),
        defs: HashMap::new(),
        facts: HashMap::new(),
        elem: HashMap::new(),
        ret: HashMap::new(),
        memo: RefCell::new(HashMap::new()),
        maxbits: RefCell::new(8),
    };
    for h in &hints {
        let p: Vec<&str> = h.split_whitespace().collect();
        if p.len() == 4
            && let (Ok(lo), Ok(hi)) = (p[2].parse::<BigInt>(), p[3].parse::<BigInt>())
        {
            match p[0] {
                ";@elem" => {
                    c.elem.insert(p[1].to_string(), (lo, hi));
                }
                ";@ret" => {
                    c.ret.insert(p[1].to_string(), (lo, hi));
                }
                _ => {}
            }
        }
    }
    for cmd in &cmds {
        let S::L(v) = cmd else { continue };
        let Some(S::A(h)) = v.first() else { continue };
        match h.as_str() {
            "declare-const" => {
                if let S::A(n) = &v[1] {
                    c.consts.insert(n.clone(), sort_of(&v[2]));
                }
            }
            "declare-fun" => {
                if let (S::A(n), S::L(ps)) = (&v[1], &v[2]) {
                    c.funs.insert(n.clone(), (ps.iter().map(sort_of).collect(), sort_of(&v[3])));
                }
            }
            "assert" => {
                let t = &v[1];
                if let Some((x, iv)) = range_fact(t) {
                    match &x {
                        S::A(a) => {
                            let cur = c.ranges.remove(a);
                            c.ranges.insert(a.clone(), intersect(cur, Some(iv)).unwrap());
                        }
                        _ => {
                            c.facts.insert(pr(&x), iv);
                        }
                    }
                }
                if let S::L(e) = t
                    && let [S::A(eq), S::A(x), d] = e.as_slice()
                    && eq == "="
                    && !c.defs.contains_key(x)
                {
                    c.defs.insert(x.clone(), d.clone());
                }
            }
            _ => {}
        }
    }
    // pass 1: which assertions lower, and the width they need
    let mut keep = vec![];
    let mut dropped = 0;
    for (i, cmd) in cmds.iter().enumerate() {
        if let S::L(v) = cmd
            && matches!(v.first(), Some(S::A(h)) if h == "assert")
        {
            match c.boolean(&v[1], &HashMap::new()) {
                Ok(()) => keep.push(i),
                Err(_) => dropped += 1,
            }
        }
    }
    let w = (*c.maxbits.borrow() + 2) as u32;
    if w > 2048 {
        return None;
    }
    let em = Em { c: &c, w, fresh: RefCell::new(0) };
    let _ = &em.fresh;
    let mut out = String::new();
    let keep: std::collections::HashSet<usize> = keep.into_iter().collect();
    for (i, cmd) in cmds.iter().enumerate() {
        let S::L(v) = cmd else { continue };
        let Some(S::A(h)) = v.first() else { continue };
        match h.as_str() {
            "declare-const" => {
                let srt = sort_of(&v[2]);
                if let Some(ls) = lower_sort(&srt, w) {
                    out.push_str(&format!("(declare-const {} {ls})\n", pr(&v[1])));
                }
            }
            "declare-fun" => {
                let (S::L(ps), r) = (&v[2], sort_of(&v[3])) else {
                    continue;
                };
                let ps: Option<Vec<String>> =
                    ps.iter().map(|p| lower_sort(&sort_of(p), w)).collect();
                if let (Some(ps), Some(r)) = (ps, lower_sort(&r, w)) {
                    out.push_str(&format!("(declare-fun {} ({}) {r})\n", pr(&v[1]), ps.join(" ")));
                }
            }
            "assert" if keep.contains(&i) => {
                if let Ok(t) = em.em(&v[1], &HashMap::new()) {
                    out.push_str(&format!("(assert {t})\n"))
                }
            }
            "push" | "pop" | "check-sat" => {
                out.push_str(&pr(cmd));
                out.push('\n');
            }
            _ => {}
        }
    }
    Some((out, dropped, w))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lowers_min_trick() {
        let q = "(declare-const |x| Int)\n(assert (and (<= 0 |x|) (<= |x| 255)))\n(declare-const |y| Int)\n(assert (and (<= 0 |y|) (<= |y| 255)))\n\
                 (assert (not (= (+ |x| |y|) (+ |y| |x|))))\n(check-sat)\n";
        let (l, dropped, w) = lower(q).unwrap();
        assert_eq!(dropped, 0);
        assert_eq!(w, 11);
        assert!(l.contains("(_ BitVec 11)") && l.contains("bvadd"), "{l}");
    }
    #[test]
    fn drops_unbounded() {
        let q = "(declare-const |x| Int)\n(assert (> |x| 3))\n(check-sat)\n";
        let (_, dropped, _) = lower(q).unwrap();
        assert_eq!(dropped, 1);
    }
}

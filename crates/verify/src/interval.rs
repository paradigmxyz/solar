//! A tiny interval analysis over the SMT-LIB terms the encoder builds. It lets the encoder drop
//! `mod 2^n` wraps and overflow obligations that ranges already settle, which Z3 otherwise has to
//! case-split on (the main cost on arithmetic-heavy code such as Uniswap's math libraries).
use num_bigint::BigInt;
use num_traits::{One, Signed, Zero};
use solar_parse::interface::data_structures::map::FxHashMap;

pub(crate) type Iv = (BigInt, BigInt);

#[derive(Debug)]
enum S {
    Atom(String),
    List(Vec<Self>),
}

fn parse(s: &str) -> Option<S> {
    let toks = tokenize(s);
    let mut i = 0;
    let r = parse_at(&toks, &mut i)?;
    Some(r)
}

fn tokenize(s: &str) -> Vec<String> {
    let mut out = vec![];
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
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
        Some(S::List(v))
    } else {
        Some(S::Atom(tok.clone()))
    }
}

fn pow2(n: u64) -> BigInt {
    BigInt::one() << n
}

fn eval(s: &S, env: &FxHashMap<String, Iv>) -> Option<Iv> {
    match s {
        S::Atom(a) => {
            if let Ok(n) = a.parse::<BigInt>() {
                return Some((n.clone(), n));
            }
            env.get(a).cloned()
        }
        S::List(v) => {
            let S::Atom(op) = v.first()? else { return None };
            let args: Vec<&S> = v[1..].iter().collect();
            let ev = |k: usize| eval(args.get(k)?, env);
            match op.as_str() {
                "-" if args.len() == 1 => {
                    let (l, h) = ev(0)?;
                    Some((-h, -l))
                }
                "+" | "-" => {
                    let mut acc = ev(0)?;
                    for k in 1..args.len() {
                        let (l, h) = ev(k)?;
                        acc =
                            if op == "+" { (acc.0 + l, acc.1 + h) } else { (acc.0 - h, acc.1 - l) };
                    }
                    Some(acc)
                }
                "*" => {
                    let mut acc = ev(0)?;
                    for k in 1..args.len() {
                        let (l, h) = ev(k)?;
                        let c = [&acc.0 * &l, &acc.0 * &h, &acc.1 * &l, &acc.1 * &h];
                        acc = (c.iter().min()?.clone(), c.iter().max()?.clone());
                    }
                    Some(acc)
                }
                "div" => {
                    let (l, h) = ev(0)?;
                    let (dl, dh) = ev(1)?;
                    if dl.is_positive() && !l.is_negative() { Some((l / dh, h / dl)) } else { None }
                }
                "mod" => {
                    let (l, h) = ev(0)?;
                    let (ml, mh) = ev(1)?;
                    if ml == mh && ml.is_positive() {
                        if !l.is_negative() && h < ml {
                            return Some((l, h));
                        }
                        return Some((BigInt::zero(), ml - 1));
                    }
                    None
                }
                "ite" => {
                    let (a, b) = (ev(1)?, ev(2)?);
                    Some((a.0.min(b.0), a.1.max(b.1)))
                }
                "abs" => {
                    let (l, h) = ev(0)?;
                    let m = l.abs().max(h.abs());
                    Some((
                        if l.is_negative() && h.is_positive() {
                            BigInt::zero()
                        } else {
                            l.abs().min(h.abs())
                        },
                        m,
                    ))
                }
                "|pow2|" => {
                    let (l, h) = ev(0)?;
                    let cap = BigInt::from(256);
                    let l = l.max(BigInt::zero()).min(cap.clone());
                    let h = h.min(cap);
                    if h.is_negative() {
                        return None;
                    }
                    let lo: u64 = l.try_into().ok()?;
                    let hi: u64 = h.try_into().ok()?;
                    Some((pow2(lo), pow2(hi)))
                }
                "bv2int" | "bv2nat" => Some((BigInt::zero(), pow2(256) - 1)),
                _ => None,
            }
        }
    }
}

pub(crate) fn interval(term: &str, env: &FxHashMap<String, Iv>) -> Option<Iv> {
    if term.len() > 20_000 {
        return None;
    }
    eval(&parse(term)?, env)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basic() {
        let mut env = FxHashMap::default();
        env.insert("|k@1|".to_string(), (BigInt::from(0), BigInt::from(255)));
        assert_eq!(interval("(* 2 |k@1|)", &env), Some((BigInt::from(0), BigInt::from(510))));
        assert_eq!(
            interval("(mod (* 2 |k@1|) 1000)", &env),
            Some((BigInt::from(0), BigInt::from(510)))
        );
        assert_eq!(interval("(|pow2| |k@1|)", &env), Some((BigInt::from(1), pow2(255))));
        assert_eq!(interval("(+ |x| 1)", &env), None);
    }
}

fn print(s: &S, out: &mut String) {
    match s {
        S::Atom(a) => out.push_str(a),
        S::List(v) => {
            out.push('(');
            for (i, x) in v.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                print(x, out);
            }
            out.push(')');
        }
    }
}

fn pr(s: &S) -> String {
    let mut o = String::new();
    print(s, &mut o);
    o
}

/// Bit width of a bitvector term this encoder produced, when it can tell.
fn bv_width(s: &S) -> Option<u32> {
    let S::List(v) = s else { return None };
    match v.as_slice() {
        [S::List(h), _] if matches!(h.as_slice(), [S::Atom(u), S::Atom(k), _] if u == "_" && k == "int2bv") =>
        {
            let S::Atom(n) = &h[2] else { return None };
            n.parse().ok()
        }
        [S::Atom(u), S::Atom(k), S::Atom(n)] if u == "_" && k.starts_with("bv") => n.parse().ok(),
        [S::Atom(op), a, ..] if op.starts_with("bv") && op != "bv2nat" => bv_width(a),
        [S::Atom(op), _, a, _] if op == "ite" => bv_width(a),
        _ => None,
    }
}

fn to_bv_s(s: &S, n: u32, twin: &mut dyn FnMut(&str) -> Option<String>) -> String {
    match s {
        S::Atom(a) => {
            if let Ok(k) = a.parse::<BigInt>()
                && !k.is_negative()
                && k < pow2(n as u64)
            {
                return format!("(_ bv{k} {n})");
            }
            if let Some(t) = twin(a) {
                return t;
            }
            format!("((_ int2bv {n}) {a})")
        }
        S::List(v) => match v.as_slice() {
            [S::Atom(f), x] if f == "bv2nat" && bv_width(x) == Some(n) => pr(x),
            [S::Atom(f), c, a, b] if f == "ite" => {
                format!("(ite {} {} {})", pr(c), to_bv_s(a, n, twin), to_bv_s(b, n, twin))
            }
            // a / 2^k as a logical shift, when a's bitvector form is its exact value
            [S::Atom(f), a, S::Atom(c)] if f == "div" => {
                let ab = to_bv_s(a, n, twin);
                let exact = !["int2bv", "bvadd", "bvsub", "bvmul"].iter().any(|k| ab.contains(k));
                match c.parse::<BigInt>() {
                    Ok(cv)
                        if exact
                            && cv.is_positive()
                            && (&cv & (&cv - 1u32)).is_zero()
                            && cv.bits() <= n as u64 =>
                    {
                        format!("(bvlshr {ab} (_ bv{} {n}))", cv.bits() - 1)
                    }
                    _ => format!("((_ int2bv {n}) {})", pr(s)),
                }
            }
            // int2bv is mod 2^n, so mod 2^n vanishes and + - * map to bitvector arithmetic
            [S::Atom(f), a, S::Atom(c)]
                if f == "mod" && c.parse::<BigInt>().ok() == Some(pow2(n as u64)) =>
            {
                to_bv_s(a, n, twin)
            }
            [S::Atom(f), a, b] if f == "+" || f == "-" || f == "*" => {
                let op = match f.as_str() {
                    "+" => "bvadd",
                    "-" => "bvsub",
                    _ => "bvmul",
                };
                format!("({op} {} {})", to_bv_s(a, n, twin), to_bv_s(b, n, twin))
            }
            // 2^e (and 2^e mod 2^256) as a shift: exact for every e, including e >= n (gives 0)
            [S::Atom(f), e] if n == 256 && (f == "|pow2|" || f == "|pow2w|") => {
                format!("(bvshl (_ bv1 256) {})", to_bv_s(e, 256, twin))
            }
            _ => format!("((_ int2bv {n}) {})", pr(s)),
        },
    }
}

/// `((_ int2bv n) t)`, without the int->bv->int round trips the encoder would otherwise stack
/// up across chained bitwise operations (and pushed through `ite`), so solvers see plain
/// bitvector terms.
pub(crate) fn to_bv(t: &str, n: u32, twin: &mut dyn FnMut(&str) -> Option<String>) -> String {
    match parse(t) {
        Some(s) if t.len() < 200_000 => to_bv_s(&s, n, twin),
        _ => format!("((_ int2bv {n}) {t})"),
    }
}

#[cfg(test)]
mod bv_tests {
    use super::*;
    #[test]
    fn round_trip() {
        let x = "(bvxor ((_ int2bv 256) |x|) ((_ int2bv 256) |y|))";
        assert_eq!(to_bv(&format!("(bv2nat {x})"), 256, &mut |_| None), x);
        assert_eq!(
            to_bv(&format!("(ite |c| (bv2nat {x}) 0)"), 256, &mut |_| None),
            format!("(ite |c| {x} (_ bv0 256))")
        );
        assert_eq!(
            to_bv(&format!("(bv2nat {x})"), 8, &mut |_| None),
            format!("((_ int2bv 8) (bv2nat {x}))")
        );
    }
}

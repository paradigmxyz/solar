//! A small interval analysis over the SMT-LIB terms the encoder builds, and their bitvector view.
//!
//! [`interval`] bounds an Int term from the bounds of the variables it uses. It lets the encoder
//! drop `mod 2^n` wraps and overflow obligations that the bounds already settle, which Z3
//! otherwise has to case-split on: the main cost on arithmetic-heavy code such as Uniswap's math
//! libraries.
//!
//! [`to_bv`] writes an Int term as an n-bit bitvector term, `((_ int2bv n) term)`, without
//! stacking up the int-to-bitvector round trips that chained bitwise operations would otherwise
//! build, so solvers see plain bitvector terms.

use crate::{
    operator::{IndexedOperator, Operator},
    sexp::{self, Sexp},
};
use num_bigint::BigInt;
use num_traits::{One, Signed, Zero};
use solar_parse::interface::data_structures::map::FxHashMap;

/// The closed range `[low, high]` of an Int term.
pub(crate) type Interval = (BigInt, BigInt);

/// Terms longer than this are not analyzed, to bound the cost per definition.
const MAX_ANALYZED_LEN: usize = 20_000;

/// Terms longer than this keep `int2bv` around them rather than being lowered.
const MAX_LOWERED_LEN: usize = 200_000;

/// The interval of `term` given the intervals of its variables, when the analysis can bound it.
pub(crate) fn interval(term: &str, variables: &FxHashMap<String, Interval>) -> Option<Interval> {
    if term.len() > MAX_ANALYZED_LEN {
        return None;
    }
    interval_of(&parse_term(term)?, variables)
}

/// `((_ int2bv width) term)`, with the round trips removed: `bv2nat` of a `width`-bit term is
/// that term, `ite` and `mod 2^width` are pushed through, `+`, `-` and `*` become bitvector
/// arithmetic, and division by a power of two becomes a shift. `bitvector_view` gives the
/// bitvector term that stands for a variable, such as the bitvector twin the encoder declares for
/// an in-range Int variable; a variable without one is converted with `int2bv`.
pub(crate) fn to_bv(
    term: &str,
    width: u32,
    bitvector_view: &mut dyn FnMut(&str) -> Option<String>,
) -> String {
    match parse_term(term) {
        Some(parsed) if term.len() < MAX_LOWERED_LEN => {
            lower_to_bitvector(&parsed, width, bitvector_view)
        }
        _ => format!("((_ int2bv {width}) {term})"),
    }
}

/// 2^`exponent`.
pub(crate) fn pow2(exponent: u64) -> BigInt {
    BigInt::one() << exponent
}

/// The one term `text` holds.
fn parse_term(text: &str) -> Option<Sexp> {
    sexp::parse(text).next()?.ok()
}

/// The head operator of an application and its arguments.
fn application(term: &Sexp) -> Option<(Operator, &[Sexp])> {
    let (head, arguments) = term.as_list()?.split_first()?;
    Some((Operator::of(head.as_atom()?)?, arguments))
}

fn interval_of(term: &Sexp, variables: &FxHashMap<String, Interval>) -> Option<Interval> {
    if let Sexp::Atom(atom) = term {
        if let Ok(value) = atom.parse::<BigInt>() {
            return Some((value.clone(), value));
        }
        return variables.get(atom).cloned();
    }
    let (operator, arguments) = application(term)?;
    let argument = |index: usize| interval_of(arguments.get(index)?, variables);
    match operator {
        Operator::Sub if arguments.len() == 1 => {
            let (low, high) = argument(0)?;
            Some((-high, -low))
        }
        Operator::Add | Operator::Sub => {
            let mut sum = argument(0)?;
            for index in 1..arguments.len() {
                let (low, high) = argument(index)?;
                sum = if operator == Operator::Add {
                    (sum.0 + low, sum.1 + high)
                } else {
                    (sum.0 - high, sum.1 - low)
                };
            }
            Some(sum)
        }
        Operator::Mul => {
            let mut product = argument(0)?;
            for index in 1..arguments.len() {
                let (low, high) = argument(index)?;
                let corners =
                    [&product.0 * &low, &product.0 * &high, &product.1 * &low, &product.1 * &high];
                product = (corners.iter().min()?.clone(), corners.iter().max()?.clone());
            }
            Some(product)
        }
        Operator::Div => {
            let (low, high) = argument(0)?;
            let (divisor_low, divisor_high) = argument(1)?;
            if divisor_low.is_positive() && !low.is_negative() {
                Some((low / divisor_high, high / divisor_low))
            } else {
                None
            }
        }
        Operator::Mod => {
            let (low, high) = argument(0)?;
            let (modulus, modulus_high) = argument(1)?;
            if modulus != modulus_high || !modulus.is_positive() {
                return None;
            }
            if !low.is_negative() && high < modulus {
                return Some((low, high));
            }
            Some((BigInt::zero(), modulus - 1))
        }
        Operator::Ite => {
            let (then, otherwise) = (argument(1)?, argument(2)?);
            Some((then.0.min(otherwise.0), then.1.max(otherwise.1)))
        }
        Operator::Abs => {
            let (low, high) = argument(0)?;
            let largest = low.abs().max(high.abs());
            let smallest = if low.is_negative() && high.is_positive() {
                BigInt::zero()
            } else {
                low.abs().min(high.abs())
            };
            Some((smallest, largest))
        }
        Operator::Pow2 => {
            let (low, high) = argument(0)?;
            let cap = BigInt::from(256);
            let low = low.max(BigInt::zero()).min(cap.clone());
            let high = high.min(cap);
            if high.is_negative() {
                return None;
            }
            Some((pow2(low.try_into().ok()?), pow2(high.try_into().ok()?)))
        }
        Operator::BvToInt | Operator::BvToNat => Some((BigInt::zero(), pow2(256) - 1)),
        _ => None,
    }
}

/// The width of a bitvector term the encoder wrote, when it can tell.
fn bitvector_width(term: &Sexp) -> Option<u32> {
    let items = term.as_list()?;
    // `((_ int2bv width) x)`
    if let [head, _] = items
        && let Some(IndexedOperator::IntToBv(width)) = IndexedOperator::of(head)
    {
        return Some(width);
    }
    // `(_ bvN width)`
    if let [Sexp::Atom(underscore), Sexp::Atom(value), Sexp::Atom(width)] = items
        && underscore == "_"
        && value.starts_with("bv")
    {
        return width.parse().ok();
    }
    match application(term)? {
        (Operator::Bv | Operator::BvCompare | Operator::BvToInt, [first, ..]) => {
            bitvector_width(first)
        }
        (Operator::Ite, [_, then, _]) => bitvector_width(then),
        _ => None,
    }
}

fn lower_to_bitvector(
    term: &Sexp,
    width: u32,
    bitvector_view: &mut dyn FnMut(&str) -> Option<String>,
) -> String {
    let converted = || format!("((_ int2bv {width}) {term})");
    if let Sexp::Atom(atom) = term {
        if let Ok(value) = atom.parse::<BigInt>()
            && !value.is_negative()
            && value < pow2(width as u64)
        {
            return format!("(_ bv{value} {width})");
        }
        return bitvector_view(atom).unwrap_or_else(converted);
    }
    let Some((operator, arguments)) = application(term) else { return converted() };
    let mut lower = |term: &Sexp| lower_to_bitvector(term, width, bitvector_view);
    match (operator, arguments) {
        (Operator::BvToNat, [bitvector]) if bitvector_width(bitvector) == Some(width) => {
            bitvector.to_string()
        }
        (Operator::Ite, [condition, then, otherwise]) => {
            format!("(ite {condition} {} {})", lower(then), lower(otherwise))
        }
        // `a / 2^k` as a logical shift, when `a`'s bitvector form is its exact value.
        (Operator::Div, [dividend, Sexp::Atom(divisor)]) => {
            let dividend = lower(dividend);
            let exact = !["int2bv", "bvadd", "bvsub", "bvmul"]
                .iter()
                .any(|wrapping| dividend.contains(wrapping));
            match divisor.parse::<BigInt>() {
                Ok(divisor)
                    if exact
                        && divisor.is_positive()
                        && (&divisor & (&divisor - 1u32)).is_zero()
                        && divisor.bits() <= width as u64 =>
                {
                    format!("(bvlshr {dividend} (_ bv{} {width}))", divisor.bits() - 1)
                }
                _ => converted(),
            }
        }
        // `int2bv` is `mod 2^width`, so a `mod 2^width` inside it vanishes.
        (Operator::Mod, [value, Sexp::Atom(modulus)])
            if modulus.parse::<BigInt>().ok() == Some(pow2(width as u64)) =>
        {
            lower(value)
        }
        // Modulo 2^width, `+`, `-` and `*` are bitvector arithmetic.
        (Operator::Add | Operator::Sub | Operator::Mul, [left, right]) => {
            let operator = match operator {
                Operator::Add => "bvadd",
                Operator::Sub => "bvsub",
                _ => "bvmul",
            };
            format!("({operator} {} {})", lower(left), lower(right))
        }
        // 2^e as a shift, exact for every e: e >= 256 gives 0, which is 2^256 mod 2^256.
        (Operator::Pow2, [exponent]) if width == 256 => {
            format!("(bvshl (_ bv1 256) {})", lower(exponent))
        }
        _ => converted(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intervals() {
        let mut variables = FxHashMap::default();
        variables.insert("|k@1|".to_string(), (BigInt::from(0), BigInt::from(255)));
        let interval = |term: &str| interval(term, &variables);
        assert_eq!(interval("(* 2 |k@1|)"), Some((BigInt::from(0), BigInt::from(510))));
        assert_eq!(interval("(mod (* 2 |k@1|) 1000)"), Some((BigInt::from(0), BigInt::from(510))));
        assert_eq!(interval("(|pow2| |k@1|)"), Some((BigInt::from(1), pow2(255))));
        assert_eq!(interval("(+ |x| 1)"), None);
    }

    #[test]
    fn bitvector_round_trips() {
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

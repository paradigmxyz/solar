//! Exact lowering of a solar-verify query from unbounded Int to fixed-width bitvectors.
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
//!
//! [`lower`] works in three steps. It first collects what the script declares and asserts: the
//! sort of every constant and function, the range facts `(and (<= low t) (<= t high))`, the SSA
//! definitions `(= x term)` and the hints. [`Analysis::check_formula`] then finds the [`Kind`] of
//! every subterm of each assertion, an interval for an Int term and a width for a bitvector, and
//! the assertions with a subterm it cannot bound are dropped. Last, [`Emitter`] writes the kept
//! assertions over BV-W.

use crate::{
    interval::{Interval, pow2},
    operator::{IndexedOperator, Operator},
    sexp::{self, Sexp},
};
use num_bigint::BigInt;
use num_traits::{Signed, Zero};
use solar_parse::interface::data_structures::map::{FxHashMap, FxHashSet};
use std::cell::RefCell;

/// Why a term cannot be lowered.
type Lowering<T> = Result<T, String>;

/// The kinds of the variables that quantifiers bind around a subterm.
type Scope = FxHashMap<String, Kind>;

/// The sort of a declared constant, a function's parameter or result, or a bound variable.
#[derive(Debug, Clone, PartialEq)]
enum Sort {
    Int,
    Bool,
    Bitvector(u32),
    /// An array from the index sort to the element sort.
    Array(Box<Self>, Box<Self>),
    /// A sort the lowering does not handle.
    Other,
}

impl Sort {
    /// The sort `term` spells.
    fn of(term: &Sexp) -> Self {
        match term {
            Sexp::Atom(atom) if atom == "Int" => Self::Int,
            Sexp::Atom(atom) if atom == "Bool" => Self::Bool,
            Sexp::List(items) => match items.as_slice() {
                [Sexp::Atom(underscore), Sexp::Atom(bitvec), Sexp::Atom(width)]
                    if underscore == "_" && bitvec == "BitVec" =>
                {
                    width.parse().map(Self::Bitvector).unwrap_or(Self::Other)
                }
                [Sexp::Atom(array), index, element] if array == "Array" => {
                    Self::Array(Box::new(Self::of(index)), Box::new(Self::of(element)))
                }
                _ => Self::Other,
            },
            Sexp::Atom(_) => Self::Other,
        }
    }

    /// The sort over BV-`width`, where Int becomes `(_ BitVec width)`; `None` for a sort the
    /// lowering does not handle.
    fn lowered(&self, width: u32) -> Option<String> {
        Some(match self {
            Self::Int => format!("(_ BitVec {width})"),
            Self::Bool => "Bool".into(),
            Self::Bitvector(bits) => format!("(_ BitVec {bits})"),
            Self::Array(index, element) => {
                format!("(Array {} {})", index.lowered(width)?, element.lowered(width)?)
            }
            Self::Other => return None,
        })
    }

    /// How many arrays the sort nests: 2 for `(Array Int (Array Int Int))`.
    fn array_depth(&self) -> usize {
        match self {
            Self::Array(_, element) => 1 + element.array_depth(),
            _ => 0,
        }
    }
}

/// What the analysis knows about a term: the bounds of an Int, or the shape of any other term.
#[derive(Debug, Clone)]
enum Kind {
    Int(Interval),
    Bool,
    Bitvector(u32),
    /// An array, with the kind of its elements.
    Array(Box<Self>),
}

impl Kind {
    /// An Int in `interval` under `depth` arrays.
    fn nested(depth: usize, interval: Interval) -> Self {
        let mut kind = Self::Int(interval);
        for _ in 0..depth {
            kind = Self::Array(Box::new(kind));
        }
        kind
    }

    /// The kind that covers both, as for the arms of an `ite`; `None` when their shapes differ.
    fn union(&self, other: &Self) -> Option<Self> {
        Some(match (self, other) {
            (Self::Int((low, high)), Self::Int((other_low, other_high))) => {
                Self::Int((low.min(other_low).clone(), high.max(other_high).clone()))
            }
            (Self::Bool, Self::Bool) => Self::Bool,
            (Self::Bitvector(width), Self::Bitvector(other)) if width == other => {
                Self::Bitvector(*width)
            }
            (Self::Array(element), Self::Array(other)) => {
                Self::Array(Box::new(element.union(other)?))
            }
            _ => return None,
        })
    }
}

/// The bits of `value`'s magnitude.
fn bit_length(value: &BigInt) -> u64 {
    value.abs().bits()
}

/// Both bounds where there are two, else the one there is.
fn intersect(first: Option<Interval>, second: Option<Interval>) -> Option<Interval> {
    match (first, second) {
        (Some((low, high)), Some((other_low, other_high))) => {
            Some((low.max(other_low), high.min(other_high)))
        }
        (only, None) | (None, only) => only,
    }
}

/// The value of a numeral, `5` or `(- 5)`.
fn numeral(term: &Sexp) -> Option<BigInt> {
    match term {
        Sexp::Atom(atom) => atom.parse().ok(),
        Sexp::List(items) => match items.as_slice() {
            [Sexp::Atom(minus), value] if minus == "-" => numeral(value).map(|value| -value),
            _ => None,
        },
    }
}

/// Whether `value` is a positive power of two.
fn is_power_of_two(value: &BigInt) -> bool {
    value.is_positive() && (value & (value - 1u32)).is_zero()
}

/// The head operator of an application and its arguments.
fn application(term: &Sexp) -> Option<(Operator, &[Sexp])> {
    let (head, arguments) = term.as_list()?.split_first()?;
    Some((Operator::of(head.as_atom()?)?, arguments))
}

/// A range fact in the encoder's shape, `(and (<= low term) (<= term high))`.
fn range_fact(fact: &Sexp) -> Option<(Sexp, Interval)> {
    let (Operator::And, [lower, upper]) = application(fact)? else { return None };
    match (application(lower)?, application(upper)?) {
        ((Operator::Le, [low, term]), (Operator::Le, [same, high])) if term == same => {
            Some((term.clone(), (numeral(low)?, numeral(high)?)))
        }
        _ => None,
    }
}

/// The exponent of `(|pow2| exponent)`.
fn pow2_exponent(term: &Sexp) -> Option<&Sexp> {
    match application(term)? {
        (Operator::Pow2, [exponent]) => Some(exponent),
        _ => None,
    }
}

/// `term`, cut to 80 bytes for an error message.
fn abbreviate(term: &Sexp) -> String {
    let text = term.to_string();
    if text.len() > 80 { format!("{}…", &text[..80]) } else { text }
}

/// What the script says about its symbols, and the widest bound found so far.
struct Analysis {
    /// The sort of every declared constant.
    constants: FxHashMap<String, Sort>,
    /// The parameter and result sorts of every declared function.
    functions: FxHashMap<String, (Vec<Sort>, Sort)>,
    /// The range facts asserted about constants.
    ranges: FxHashMap<String, Interval>,
    /// The term each SSA constant is asserted equal to, the first time.
    definitions: FxHashMap<String, Sexp>,
    /// The range facts asserted about other terms, keyed by their text.
    term_ranges: FxHashMap<String, Interval>,
    /// `;@elem array low high`: the range of an array constant's elements.
    element_ranges: FxHashMap<String, Interval>,
    /// `;@ret function low high`: the range of an uninterpreted function's results.
    result_ranges: FxHashMap<String, Interval>,
    /// The kind of each constant, once found; `None` when it is unbounded.
    constant_kinds: RefCell<FxHashMap<String, Option<Kind>>>,
    /// The bit length of the widest bound found.
    max_bits: RefCell<u64>,
}

impl Analysis {
    /// Records the bits `interval` needs.
    fn note_bits(&self, interval: &Interval) {
        let bits = bit_length(&interval.0).max(bit_length(&interval.1));
        let mut max_bits = self.max_bits.borrow_mut();
        *max_bits = (*max_bits).max(bits);
    }

    fn symbol_kind(&self, symbol: &str, scope: &Scope) -> Lowering<Kind> {
        if let Some(kind) = scope.get(symbol) {
            return Ok(kind.clone());
        }
        if symbol == "true" || symbol == "false" {
            return Ok(Kind::Bool);
        }
        if let Ok(value) = symbol.parse::<BigInt>() {
            return Ok(Kind::Int((value.clone(), value)));
        }
        if let Some(kind) = self.constant_kinds.borrow().get(symbol) {
            return kind.clone().ok_or_else(|| format!("unbounded {symbol}"));
        }
        let sort = self
            .constants
            .get(symbol)
            .cloned()
            .ok_or_else(|| format!("unknown symbol {symbol}"))?;
        let definition_kind = || {
            self.definitions
                .get(symbol)
                .and_then(|definition| self.kind(definition, &Scope::default()).ok())
        };
        let kind = match &sort {
            Sort::Bool => Some(Kind::Bool),
            Sort::Bitvector(width) => Some(Kind::Bitvector(*width)),
            Sort::Int => {
                let defined = match definition_kind() {
                    Some(Kind::Int(interval)) => Some(interval),
                    _ => None,
                };
                intersect(self.ranges.get(symbol).cloned(), defined).map(Kind::Int)
            }
            Sort::Array(..) => match (self.element_ranges.get(symbol), definition_kind()) {
                (Some(elements), _) => Some(Kind::nested(sort.array_depth(), elements.clone())),
                (None, defined) => defined,
            },
            Sort::Other => None,
        };
        self.constant_kinds.borrow_mut().insert(symbol.to_string(), kind.clone());
        kind.ok_or_else(|| format!("unbounded {symbol}"))
    }

    /// The bounds of an Int term.
    fn interval(&self, term: &Sexp, scope: &Scope) -> Lowering<Interval> {
        match self.kind(term, scope)? {
            Kind::Int(interval) => Ok(interval),
            kind => Err(format!("expected Int, got {kind:?} in {}", abbreviate(term))),
        }
    }

    /// The kind of `term`, recording the bits its bounds need.
    fn kind(&self, term: &Sexp, scope: &Scope) -> Lowering<Kind> {
        let kind = self.compute_kind(term, scope)?;
        if let Kind::Int(interval) = &kind {
            self.note_bits(interval);
        }
        Ok(kind)
    }

    fn compute_kind(&self, term: &Sexp, scope: &Scope) -> Lowering<Kind> {
        let items = match term {
            Sexp::Atom(symbol) => return self.symbol_kind(symbol, scope),
            Sexp::List(items) => items,
        };
        if let Some(value) = numeral(term) {
            return Ok(Kind::Int((value.clone(), value)));
        }
        let Some((head, arguments)) = items.split_first() else { return Err("empty".into()) };
        let Sexp::Atom(symbol) = head else {
            return self.indexed_kind(term, head, arguments, scope);
        };
        let Some(operator) = Operator::of(symbol) else {
            return self.function_kind(term, symbol, arguments, scope);
        };
        let intervals = || -> Lowering<Vec<Interval>> {
            arguments.iter().map(|argument| self.interval(argument, scope)).collect()
        };
        Ok(match operator {
            Operator::Indexed => match arguments {
                // `(_ bvN width)`
                [Sexp::Atom(value), Sexp::Atom(width)] if value.starts_with("bv") => {
                    Kind::Bitvector(width.parse().map_err(|_| "w")?)
                }
                _ => return Err("indexed".into()),
            },
            Operator::Add | Operator::Sub | Operator::Mul => {
                let intervals = intervals()?;
                if operator == Operator::Sub && intervals.len() == 1 {
                    let (low, high) = &intervals[0];
                    return Ok(Kind::Int((-high, -low)));
                }
                let mut result = intervals[0].clone();
                for (low, high) in &intervals[1..] {
                    result = match operator {
                        Operator::Add => (result.0 + low, result.1 + high),
                        Operator::Sub => (result.0 - high, result.1 - low),
                        _ => {
                            let corners = [
                                &result.0 * low,
                                &result.0 * high,
                                &result.1 * low,
                                &result.1 * high,
                            ];
                            let low = corners.iter().min().unwrap().clone();
                            (low, corners.iter().max().unwrap().clone())
                        }
                    };
                }
                Kind::Int(result)
            }
            Operator::Div | Operator::Mod => {
                let intervals = intervals()?;
                let ((low, high), (divisor_low, divisor_high)) = (&intervals[0], &intervals[1]);
                if !divisor_low.is_positive() {
                    return Err("division by a possibly non-positive value".into());
                }
                if operator == Operator::Mod {
                    if !low.is_negative() && high < divisor_low {
                        Kind::Int((low.clone(), high.clone()))
                    } else {
                        Kind::Int((BigInt::zero(), divisor_high - 1))
                    }
                } else if !low.is_negative() {
                    Kind::Int((low / divisor_high, high / divisor_low))
                } else {
                    let magnitude = low.abs().max(high.abs());
                    Kind::Int((-(&magnitude) - 1, magnitude))
                }
            }
            Operator::Abs => {
                let (low, high) = self.interval(&arguments[0], scope)?;
                Kind::Int((BigInt::zero(), low.abs().max(high.abs())))
            }
            Operator::Ite => {
                self.check_formula(&arguments[0], scope)?;
                let then = self.kind(&arguments[1], scope)?;
                let otherwise = self.kind(&arguments[2], scope)?;
                then.union(&otherwise).ok_or("ite of mismatched kinds")?
            }
            Operator::Lt | Operator::Le | Operator::Gt | Operator::Ge => {
                intervals()?;
                Kind::Bool
            }
            Operator::Eq | Operator::Distinct => {
                let kinds: Vec<Kind> = arguments
                    .iter()
                    .map(|argument| self.kind(argument, scope))
                    .collect::<Lowering<_>>()?;
                if kinds.iter().any(|kind| matches!(kind, Kind::Array(_))) {
                    // Restricting the index domain does not preserve array (in)equality.
                    return Err("array equality".into());
                }
                Kind::Bool
            }
            Operator::And | Operator::Or | Operator::Not | Operator::Implies | Operator::Xor => {
                for argument in arguments {
                    self.check_formula(argument, scope)?;
                }
                Kind::Bool
            }
            Operator::Select => {
                let array = self.kind(&arguments[0], scope)?;
                self.interval(&arguments[1], scope)?;
                let fact = self.term_range(term, scope);
                let Kind::Array(element) = array else {
                    return Err("select of non-array".into());
                };
                match (fact, *element) {
                    (Some(fact), Kind::Int(element)) => {
                        Kind::Int(intersect(Some(fact), Some(element)).unwrap())
                    }
                    (Some(fact), _) => Kind::Int(fact),
                    (None, element) => element,
                }
            }
            Operator::Store => {
                let array = self.kind(&arguments[0], scope)?;
                self.interval(&arguments[1], scope)?;
                let value = self.kind(&arguments[2], scope)?;
                let Kind::Array(element) = array else {
                    return Err("store of non-array".into());
                };
                Kind::Array(Box::new(element.union(&value).ok_or("store kind")?))
            }
            Operator::BvToNat | Operator::BvToInt => {
                let Kind::Bitvector(width) = self.kind(&arguments[0], scope)? else {
                    return Err("bv2nat".into());
                };
                Kind::Int((BigInt::zero(), pow2(width as u64) - 1))
            }
            Operator::Pow2 => {
                let (low, high) = self.interval(&arguments[0], scope)?;
                // `|pow2|` is 2^256 from 256 up. Exponents are never negative, but a bound that
                // allows a negative one keeps the result's bounds wide.
                let cap = BigInt::from(256);
                let negative = low.is_negative();
                let low: u64 = if low >= cap || (negative && high.is_negative()) {
                    256
                } else if negative {
                    0
                } else {
                    (&low).try_into().unwrap()
                };
                let high: u64 =
                    if high >= cap || negative { 256 } else { (&high).try_into().unwrap() };
                Kind::Int((pow2(low.min(high)), pow2(high)))
            }
            Operator::Concat => {
                let mut width = 0;
                for argument in arguments {
                    let Kind::Bitvector(part) = self.kind(argument, scope)? else {
                        return Err("concat".into());
                    };
                    width += part;
                }
                Kind::Bitvector(width)
            }
            Operator::BvCompare => {
                for argument in arguments {
                    self.kind(argument, scope)?;
                }
                Kind::Bool
            }
            Operator::Bv => {
                let mut width = None;
                for argument in arguments {
                    if let Kind::Bitvector(bits) = self.kind(argument, scope)? {
                        width = Some(bits);
                    }
                }
                Kind::Bitvector(width.ok_or("bv op")?)
            }
            Operator::Forall | Operator::Exists => {
                return Err("nested quantifier outside boolean context".into());
            }
            Operator::Annotated => self.kind(&arguments[0], scope)?,
        })
    }

    /// The kind of an application with an indexed or qualified head, such as `(_ int2bv 256)`.
    fn indexed_kind(
        &self,
        term: &Sexp,
        head: &Sexp,
        arguments: &[Sexp],
        scope: &Scope,
    ) -> Lowering<Kind> {
        let Some(operator) = IndexedOperator::of(head) else {
            return Err(format!("unsupported head {}", abbreviate(term)));
        };
        Ok(match operator {
            IndexedOperator::IntToBv(width) => {
                self.interval(&arguments[0], scope)?;
                Kind::Bitvector(width)
            }
            IndexedOperator::Extract(high, low) => {
                self.kind(&arguments[0], scope)?;
                Kind::Bitvector(high - low + 1)
            }
            IndexedOperator::ZeroExtend(bits) | IndexedOperator::SignExtend(bits) => {
                let Kind::Bitvector(width) = self.kind(&arguments[0], scope)? else {
                    return Err("ext".into());
                };
                Kind::Bitvector(width + bits)
            }
            IndexedOperator::ConstArray(_) => {
                Kind::Array(Box::new(self.kind(&arguments[0], scope)?))
            }
        })
    }

    /// The kind of an application of a declared function.
    fn function_kind(
        &self,
        term: &Sexp,
        function: &str,
        arguments: &[Sexp],
        scope: &Scope,
    ) -> Lowering<Kind> {
        let (parameters, result) = self
            .functions
            .get(function)
            .cloned()
            .ok_or_else(|| format!("unknown function {function}"))?;
        for (argument, parameter) in arguments.iter().zip(&parameters) {
            let kind = self.kind(argument, scope)?;
            if *parameter == Sort::Int && !matches!(kind, Kind::Int(_)) {
                return Err("arg".into());
            }
        }
        Ok(match result {
            Sort::Int => Kind::Int(
                intersect(self.term_range(term, scope), self.result_ranges.get(function).cloned())
                    .ok_or_else(|| format!("unbounded result of {function}"))?,
            ),
            Sort::Bool => Kind::Bool,
            Sort::Bitvector(width) => Kind::Bitvector(width),
            _ => return Err("fun sort".into()),
        })
    }

    /// The range fact asserted about `term`, which holds only outside quantifiers.
    fn term_range(&self, term: &Sexp, scope: &Scope) -> Option<Interval> {
        if scope.is_empty() { self.term_ranges.get(&term.to_string()).cloned() } else { None }
    }

    /// Checks that `formula` lowers, where it is asserted.
    fn check_formula(&self, formula: &Sexp, scope: &Scope) -> Lowering<()> {
        self.check_formula_with_polarity(formula, scope, Some(true))
    }

    /// Checks that `formula` lowers, where it occurs positively (`Some(true)`), negatively
    /// (`Some(false)`) or both (`None`). The polarity says which quantifiers are universal, whose
    /// unranged variables can be restricted to BV-W.
    fn check_formula_with_polarity(
        &self,
        formula: &Sexp,
        scope: &Scope,
        polarity: Option<bool>,
    ) -> Lowering<()> {
        let flip = |polarity: Option<bool>| polarity.map(|positive| !positive);
        if let Some((operator, arguments)) = application(formula) {
            match operator {
                Operator::Not => {
                    return self.check_formula_with_polarity(&arguments[0], scope, flip(polarity));
                }
                Operator::And | Operator::Or => {
                    for argument in arguments {
                        self.check_formula_with_polarity(argument, scope, polarity)?;
                    }
                    return Ok(());
                }
                Operator::Implies => {
                    let Some((conclusion, premises)) = arguments.split_last() else {
                        return Err("empty implication".into());
                    };
                    for premise in premises {
                        self.check_formula_with_polarity(premise, scope, flip(polarity))?;
                    }
                    return self.check_formula_with_polarity(conclusion, scope, polarity);
                }
                Operator::Annotated => {
                    return self.check_formula_with_polarity(&arguments[0], scope, polarity);
                }
                Operator::Forall | Operator::Exists => {
                    let universal =
                        polarity.is_some_and(|positive| (operator == Operator::Forall) == positive);
                    return self.check_quantifier(arguments, scope, polarity, universal);
                }
                Operator::Ite => {
                    self.check_formula_with_polarity(&arguments[0], scope, None)?;
                    self.check_formula_with_polarity(&arguments[1], scope, polarity)?;
                    return self.check_formula_with_polarity(&arguments[2], scope, polarity);
                }
                // `=` between formulas is an equivalence, and both sides occur both ways.
                Operator::Eq if arguments.len() == 2 => {
                    if let (Ok(Kind::Bool), Ok(Kind::Bool)) = (
                        self.shallow_kind(&arguments[0], scope),
                        self.shallow_kind(&arguments[1], scope),
                    ) {
                        self.check_formula_with_polarity(&arguments[0], scope, None)?;
                        return self.check_formula_with_polarity(&arguments[1], scope, None);
                    }
                }
                _ => {}
            }
        }
        match self.kind(formula, scope)? {
            Kind::Bool => Ok(()),
            _ => Err("expected Bool".into()),
        }
    }

    /// Checks `(forall (binders) body)` or `(exists (binders) body)`. A bound Int needs a range
    /// from the body, in the encoder's shape `(=> ranges body)` or `(and ranges body)`, unless the
    /// quantifier is universal.
    fn check_quantifier(
        &self,
        arguments: &[Sexp],
        scope: &Scope,
        polarity: Option<bool>,
        universal: bool,
    ) -> Lowering<()> {
        let Sexp::List(binders) = &arguments[0] else { return Err("binders".into()) };
        let body = &arguments[1];
        let mut ranges = FxHashMap::default();
        if let Some((Operator::Implies | Operator::And, [guard, _])) = application(body) {
            let conjuncts: Vec<&Sexp> = match application(guard) {
                Some((Operator::And, conjuncts)) if range_fact(guard).is_none() => {
                    conjuncts.iter().collect()
                }
                _ => vec![guard],
            };
            for conjunct in conjuncts {
                if let Some((Sexp::Atom(variable), range)) = range_fact(conjunct) {
                    ranges.insert(variable, range);
                }
            }
        }
        let mut inner = scope.clone();
        for binder in binders {
            let Some([Sexp::Atom(name), sort]) = binder.as_list() else {
                return Err("binder".into());
            };
            let kind = match Sort::of(sort) {
                Sort::Int => match ranges.get(name) {
                    Some(range) => Kind::Int(range.clone()),
                    None if universal => Kind::Int((-pow2(256), pow2(256))),
                    None => return Err("unranged existential binder".into()),
                },
                Sort::Bool => Kind::Bool,
                Sort::Bitvector(width) => Kind::Bitvector(width),
                _ => return Err("binder sort".into()),
            };
            inner.insert(name.clone(), kind);
        }
        self.check_formula_with_polarity(body, &inner, polarity)
    }

    /// The kind of `term`, taking an application of a predicate to be a formula without looking
    /// inside it.
    fn shallow_kind(&self, term: &Sexp, scope: &Scope) -> Lowering<Kind> {
        match term {
            Sexp::Atom(symbol) => self.symbol_kind(symbol, scope),
            _ if application(term).is_some_and(|(operator, _)| operator.is_predicate()) => {
                Ok(Kind::Bool)
            }
            _ => self.kind(term, scope),
        }
    }
}

/// Writes terms the analysis bounded over BV-`width`.
struct Emitter<'a> {
    analysis: &'a Analysis,
    width: u32,
}

impl Emitter<'_> {
    /// `value` modulo 2^width, as a BV-width literal.
    fn literal(&self, value: &BigInt) -> String {
        let modulus = pow2(self.width as u64);
        let value = ((value % &modulus) + &modulus) % &modulus;
        format!("(_ bv{value} {})", self.width)
    }

    /// Whether the Int `term` lies in [0, 2^256).
    fn is_word(&self, term: &Sexp, scope: &Scope) -> bool {
        self.analysis
            .interval(term, scope)
            .is_ok_and(|(low, high)| !low.is_negative() && high < pow2(256))
    }

    /// Whether the Int `term` is non-negative.
    fn is_non_negative(&self, term: &Sexp, scope: &Scope) -> bool {
        self.analysis.interval(term, scope).is_ok_and(|(low, _)| !low.is_negative())
    }

    /// The bits k of a `mod 2^k` this width can take as an extract.
    fn low_bits_modulus(&self, modulus: &Sexp) -> Option<u64> {
        let modulus = numeral(modulus)?;
        let bits = modulus.bits().checked_sub(1)?;
        (is_power_of_two(&modulus) && bits < self.width as u64).then_some(bits)
    }

    fn emit(&self, term: &Sexp, scope: &Scope) -> Lowering<String> {
        let width = self.width;
        if let Some(value) = numeral(term) {
            return Ok(self.literal(&value));
        }
        let items = match term {
            Sexp::Atom(symbol) => return Ok(symbol.clone()),
            Sexp::List(items) => items,
        };
        let emit = |term: &Sexp| self.emit(term, scope);
        let (head, arguments) = items.split_first().ok_or("empty")?;
        let emit_all = || -> Lowering<Vec<String>> { arguments.iter().map(emit).collect() };
        let Sexp::Atom(symbol) = head else {
            return Ok(match IndexedOperator::of(head) {
                Some(IndexedOperator::IntToBv(bits)) => {
                    let value = emit(&arguments[0])?;
                    if bits <= width {
                        format!("((_ extract {} 0) {value})", bits - 1)
                    } else {
                        format!("((_ sign_extend {}) {value})", bits - width)
                    }
                }
                Some(IndexedOperator::ConstArray(sort)) => {
                    let sort = Sort::of(sort).lowered(width).ok_or("sort")?;
                    format!("((as const {sort}) {})", emit(&arguments[0])?)
                }
                _ => format!("({head} {})", emit_all()?.join(" ")),
            });
        };
        let Some(operator) = Operator::of(symbol) else {
            return Ok(format!("({symbol} {})", emit_all()?.join(" ")));
        };
        // `(op (op a b) c)`
        let fold = |operator: &str, operands: Vec<String>| -> String {
            let mut result = operands[0].clone();
            for operand in &operands[1..] {
                result = format!("({operator} {result} {operand})");
            }
            result
        };
        let compare = |operator: &str| -> Lowering<String> {
            Ok(format!("({operator} {} {})", emit(&arguments[0])?, emit(&arguments[1])?))
        };
        let zero = self.literal(&BigInt::zero());
        Ok(match operator {
            Operator::Indexed => term.to_string(),
            Operator::Add => fold("bvadd", emit_all()?),
            Operator::Mul => fold("bvmul", emit_all()?),
            Operator::Sub if arguments.len() == 1 => format!("(bvneg {})", emit(&arguments[0])?),
            Operator::Sub => fold("bvsub", emit_all()?),
            Operator::Lt => compare("bvslt")?,
            Operator::Le => compare("bvsle")?,
            Operator::Gt => compare("bvsgt")?,
            Operator::Ge => compare("bvsge")?,
            Operator::Abs => {
                let value = emit(&arguments[0])?;
                format!("(ite (bvslt {value} {zero}) (bvneg {value}) {value})")
            }
            // `a / 2^e` with 0 <= a < 2^256 is a logical shift: for e outside [0, 256), 2^e is
            // 2^256 or the shift is at least the width, and both give 0.
            Operator::Div
                if pow2_exponent(&arguments[1]).is_some() && self.is_word(&arguments[0], scope) =>
            {
                let exponent = pow2_exponent(&arguments[1]).unwrap();
                format!("(bvlshr {} {})", emit(&arguments[0])?, emit(exponent)?)
            }
            // `x mod 2^k` is the low k bits: in two's complement, they are the Euclidean
            // remainder.
            Operator::Mod if self.low_bits_modulus(&arguments[1]).is_some() => {
                match self.low_bits_modulus(&arguments[1]).unwrap() {
                    0 => zero,
                    bits => format!(
                        "((_ zero_extend {}) ((_ extract {} 0) {}))",
                        width as u64 - bits,
                        bits - 1,
                        emit(&arguments[0])?
                    ),
                }
            }
            Operator::Div | Operator::Mod => {
                let (dividend, divisor) = (emit(&arguments[0])?, emit(&arguments[1])?);
                if self.is_non_negative(&arguments[0], scope) {
                    let operator = if operator == Operator::Div { "bvudiv" } else { "bvurem" };
                    format!("({operator} {dividend} {divisor})")
                } else if operator == Operator::Mod {
                    format!(
                        "(let ((r (bvsrem {dividend} {divisor}))) (ite (bvslt r {zero}) (bvadd r {divisor}) r))"
                    )
                } else {
                    // Euclidean division rounds the inexact quotient of a negative dividend down.
                    format!(
                        "(ite (and (bvslt {dividend} {zero}) (not (= (bvsrem {dividend} {divisor}) {zero}))) (bvsub (bvsdiv {dividend} {divisor}) {}) (bvsdiv {dividend} {divisor}))",
                        self.literal(&BigInt::from(1))
                    )
                }
            }
            Operator::BvToNat | Operator::BvToInt => {
                let Kind::Bitvector(bits) = self.analysis.kind(&arguments[0], scope)? else {
                    return Err("bv2nat".into());
                };
                let value = emit(&arguments[0])?;
                if bits < width {
                    format!("((_ zero_extend {}) {value})", width - bits)
                } else {
                    format!("((_ extract {} 0) {value})", width - 1)
                }
            }
            Operator::Pow2 => {
                let exponent = emit(&arguments[0])?;
                let limit = self.literal(&BigInt::from(256));
                let shifted = format!("(bvshl {} {exponent})", self.literal(&BigInt::from(1)));
                let in_range =
                    format!("(and (bvsge {exponent} {zero}) (bvslt {exponent} {limit}))");
                format!("(ite {in_range} {shifted} {})", self.literal(&pow2(256)))
            }
            Operator::Forall | Operator::Exists => {
                let Sexp::List(binders) = &arguments[0] else { return Err("binders".into()) };
                let mut inner = scope.clone();
                let mut lowered = vec![];
                for binder in binders {
                    let Some([Sexp::Atom(name), sort]) = binder.as_list() else {
                        return Err("binder".into());
                    };
                    let sort = Sort::of(sort).lowered(width).ok_or("sort")?;
                    lowered.push(format!("({name} {sort})"));
                    // The kind does not matter while emitting.
                    inner.insert(name.clone(), Kind::Bool);
                }
                format!("({symbol} ({}) {})", lowered.join(" "), self.emit(&arguments[1], &inner)?)
            }
            Operator::Annotated => emit(&arguments[0])?,
            _ => format!("({symbol} {})", emit_all()?.join(" ")),
        })
    }
}

/// Lowers a whole query script. Returns the bitvector script, how many assertions it dropped,
/// and the width W.
pub(crate) fn lower(script: &str) -> Option<(String, usize, u32)> {
    let commands = sexp::parse(script).collect::<Result<Vec<_>, _>>().ok()?;
    let mut analysis = Analysis {
        constants: FxHashMap::default(),
        functions: FxHashMap::default(),
        ranges: FxHashMap::default(),
        definitions: FxHashMap::default(),
        term_ranges: FxHashMap::default(),
        element_ranges: FxHashMap::default(),
        result_ranges: FxHashMap::default(),
        constant_kinds: RefCell::new(FxHashMap::default()),
        max_bits: RefCell::new(8),
    };
    // The encoder writes its hints as comments at the start of a line.
    for hint in script.lines().map(str::trim_start).filter(|line| line.starts_with(";@")) {
        let words: Vec<&str> = hint.split_whitespace().collect();
        if let [kind, symbol, low, high] = words[..]
            && let (Ok(low), Ok(high)) = (low.parse::<BigInt>(), high.parse::<BigInt>())
        {
            let ranges = match kind {
                ";@elem" => &mut analysis.element_ranges,
                ";@ret" => &mut analysis.result_ranges,
                _ => continue,
            };
            ranges.insert(symbol.to_string(), (low, high));
        }
    }
    for command in &commands {
        let Some((Sexp::Atom(name), arguments)) = command.as_list().and_then(<[_]>::split_first)
        else {
            continue;
        };
        match name.as_str() {
            "declare-const" => {
                if let Sexp::Atom(constant) = &arguments[0] {
                    analysis.constants.insert(constant.clone(), Sort::of(&arguments[1]));
                }
            }
            "declare-fun" => {
                if let (Sexp::Atom(function), Sexp::List(parameters)) =
                    (&arguments[0], &arguments[1])
                {
                    let parameters = parameters.iter().map(Sort::of).collect();
                    let result = Sort::of(&arguments[2]);
                    analysis.functions.insert(function.clone(), (parameters, result));
                }
            }
            "assert" => {
                let assertion = &arguments[0];
                if let Some((term, range)) = range_fact(assertion) {
                    match &term {
                        Sexp::Atom(constant) => {
                            let known = analysis.ranges.remove(constant);
                            let range = intersect(known, Some(range)).unwrap();
                            analysis.ranges.insert(constant.clone(), range);
                        }
                        _ => {
                            analysis.term_ranges.insert(term.to_string(), range);
                        }
                    }
                }
                // `(= x definition)`
                if let Some((Operator::Eq, [Sexp::Atom(constant), definition])) =
                    application(assertion)
                    && !analysis.definitions.contains_key(constant)
                {
                    analysis.definitions.insert(constant.clone(), definition.clone());
                }
            }
            _ => {}
        }
    }
    // Which assertions lower, and the width they need.
    let mut kept = FxHashSet::default();
    let mut dropped = 0;
    for (index, command) in commands.iter().enumerate() {
        if command.head() != Some("assert") {
            continue;
        }
        match analysis.check_formula(&command.as_list().unwrap()[1], &Scope::default()) {
            Ok(()) => {
                kept.insert(index);
            }
            Err(_) => dropped += 1,
        }
    }
    let width = (*analysis.max_bits.borrow() + 2) as u32;
    if width > 2048 {
        return None;
    }
    let emitter = Emitter { analysis: &analysis, width };
    let mut out = String::new();
    for (index, command) in commands.iter().enumerate() {
        let Some((Sexp::Atom(name), arguments)) = command.as_list().and_then(<[_]>::split_first)
        else {
            continue;
        };
        match name.as_str() {
            "declare-const" => {
                if let Some(sort) = Sort::of(&arguments[1]).lowered(width) {
                    out.push_str(&format!("(declare-const {} {sort})\n", arguments[0]));
                }
            }
            "declare-fun" => {
                let Sexp::List(parameters) = &arguments[1] else { continue };
                let parameters: Option<Vec<String>> =
                    parameters.iter().map(|parameter| Sort::of(parameter).lowered(width)).collect();
                if let (Some(parameters), Some(result)) =
                    (parameters, Sort::of(&arguments[2]).lowered(width))
                {
                    out.push_str(&format!(
                        "(declare-fun {} ({}) {result})\n",
                        arguments[0],
                        parameters.join(" ")
                    ));
                }
            }
            "assert" if kept.contains(&index) => {
                if let Ok(assertion) = emitter.emit(&arguments[0], &Scope::default()) {
                    out.push_str(&format!("(assert {assertion})\n"));
                }
            }
            "push" | "pop" | "check-sat" => {
                out.push_str(&command.to_string());
                out.push('\n');
            }
            _ => {}
        }
    }
    Some((out, dropped, width))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowers_commutativity() {
        let query = "(declare-const |x| Int)\n(assert (and (<= 0 |x|) (<= |x| 255)))\n\
                     (declare-const |y| Int)\n(assert (and (<= 0 |y|) (<= |y| 255)))\n\
                     (assert (not (= (+ |x| |y|) (+ |y| |x|))))\n(check-sat)\n";
        let (lowered, dropped, width) = lower(query).unwrap();
        assert_eq!(dropped, 0);
        assert_eq!(width, 11);
        assert!(lowered.contains("(_ BitVec 11)") && lowered.contains("bvadd"), "{lowered}");
    }

    #[test]
    fn drops_unbounded() {
        let query = "(declare-const |x| Int)\n(assert (> |x| 3))\n(check-sat)\n";
        let (_, dropped, _) = lower(query).unwrap();
        assert_eq!(dropped, 1);
    }
}

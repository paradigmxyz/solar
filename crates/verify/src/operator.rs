//! The operators of the SMT-LIB terms the encoder writes, named in one place.
//!
//! The interval analysis and the bitvector lowering read the encoder's terms back as
//! [`Sexp`]s. They classify the head of an application with [`Operator::of`], and an indexed
//! head such as `(_ int2bv 256)` with [`IndexedOperator::of`], then match on the enums rather
//! than on symbols.

use crate::sexp::Sexp;

/// The function symbol at the head of an application.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operator {
    /// `+`.
    Add,
    /// `-`: subtraction, or negation with one argument.
    Sub,
    /// `*`.
    Mul,
    /// Integer `div`, rounding as SMT-LIB's Euclidean division does.
    Div,
    /// Integer `mod`, the Euclidean remainder.
    Mod,
    /// `abs`.
    Abs,
    /// `<`.
    Lt,
    /// `<=`.
    Le,
    /// `>`.
    Gt,
    /// `>=`.
    Ge,
    /// `=`.
    Eq,
    /// `distinct`.
    Distinct,
    /// `and`.
    And,
    /// `or`.
    Or,
    /// `not`.
    Not,
    /// `=>`.
    Implies,
    /// `xor`.
    Xor,
    /// `ite`.
    Ite,
    /// Array `select`.
    Select,
    /// Array `store`.
    Store,
    /// `forall`.
    Forall,
    /// `exists`.
    Exists,
    /// `!`, a term with attributes such as `:pattern`.
    Annotated,
    /// `_`, an indexed identifier such as the bitvector literal `(_ bv5 256)`.
    Indexed,
    /// `bv2nat`, a bitvector's unsigned value.
    BvToNat,
    /// `bv2int`, Z3's name for `bv2nat`.
    BvToInt,
    /// `|pow2|`, the encoder's 2^e: exact for 0 <= e < 256 and 2^256 above.
    Pow2,
    /// Bitvector `concat`.
    Concat,
    /// A bitvector comparison: `bvult`, `bvule`, `bvugt`, `bvuge` and their signed forms.
    BvCompare,
    /// Any other bitvector operation, such as `bvadd` or `bvand`.
    Bv,
}

impl Operator {
    /// The operator `symbol` names, or `None` for a declared function.
    pub(crate) fn of(symbol: &str) -> Option<Self> {
        Some(match symbol {
            "+" => Self::Add,
            "-" => Self::Sub,
            "*" => Self::Mul,
            "div" => Self::Div,
            "mod" => Self::Mod,
            "abs" => Self::Abs,
            "<" => Self::Lt,
            "<=" => Self::Le,
            ">" => Self::Gt,
            ">=" => Self::Ge,
            "=" => Self::Eq,
            "distinct" => Self::Distinct,
            "and" => Self::And,
            "or" => Self::Or,
            "not" => Self::Not,
            "=>" => Self::Implies,
            "xor" => Self::Xor,
            "ite" => Self::Ite,
            "select" => Self::Select,
            "store" => Self::Store,
            "forall" => Self::Forall,
            "exists" => Self::Exists,
            "!" => Self::Annotated,
            "_" => Self::Indexed,
            "bv2nat" => Self::BvToNat,
            "bv2int" => Self::BvToInt,
            "|pow2|" => Self::Pow2,
            "concat" => Self::Concat,
            "bvult" | "bvule" | "bvugt" | "bvuge" | "bvslt" | "bvsle" | "bvsgt" | "bvsge" => {
                Self::BvCompare
            }
            _ if symbol.starts_with("bv") => Self::Bv,
            _ => return None,
        })
    }

    /// Whether an application of the operator is a formula.
    pub(crate) fn is_predicate(self) -> bool {
        matches!(
            self,
            Self::Lt
                | Self::Le
                | Self::Gt
                | Self::Ge
                | Self::Eq
                | Self::Distinct
                | Self::And
                | Self::Or
                | Self::Not
                | Self::Implies
                | Self::Xor
                | Self::Forall
                | Self::Exists
                | Self::BvCompare
        )
    }
}

/// An indexed or qualified head, the list at the head of an application such as
/// `((_ int2bv 256) x)`.
#[derive(Clone, Copy, Debug)]
pub(crate) enum IndexedOperator<'a> {
    /// `(_ int2bv width)`: an Int modulo 2^width, as a bitvector.
    IntToBv(u32),
    /// `(_ extract high low)`: bits `high` down to `low`.
    Extract(u32, u32),
    /// `(_ zero_extend bits)`.
    ZeroExtend(u32),
    /// `(_ sign_extend bits)`.
    SignExtend(u32),
    /// `(as const sort)`: the array of that sort holding one value everywhere.
    ConstArray(&'a Sexp),
}

impl<'a> IndexedOperator<'a> {
    /// The operator `head` spells, or `None` for one the readers do not handle or whose indices
    /// are not numerals.
    pub(crate) fn of(head: &'a Sexp) -> Option<Self> {
        let index = |index: &Sexp| index.as_atom()?.parse().ok();
        Some(match head.as_list()? {
            [Sexp::Atom(underscore), Sexp::Atom(name), width] if underscore == "_" => {
                match name.as_str() {
                    "int2bv" => Self::IntToBv(index(width)?),
                    "zero_extend" => Self::ZeroExtend(index(width)?),
                    "sign_extend" => Self::SignExtend(index(width)?),
                    _ => return None,
                }
            }
            [Sexp::Atom(underscore), Sexp::Atom(name), high, low]
                if underscore == "_" && name == "extract" =>
            {
                Self::Extract(index(high)?, index(low)?)
            }
            [Sexp::Atom(as_), Sexp::Atom(name), sort] if as_ == "as" && name == "const" => {
                Self::ConstArray(sort)
            }
            _ => return None,
        })
    }
}

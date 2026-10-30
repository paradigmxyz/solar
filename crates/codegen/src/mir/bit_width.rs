//! Tests of whether a word fits in `bits` bits.
//!
//! Such a test has two spellings that hold for the same words, a shift against zero and a
//! comparison with the limit:
//!
//! ```text
//! x fits in k bits      eq (shr k, x), 0   lt x, 2^k
//! x has a bit above k   ne (shr k, x), 0   gt x, 2^k - 1
//! ```
//!
//! The spellings arose where code was written for one consumer. A branch reads a word without a
//! test, so a check that aborts on `ne (shr k, x), 0` pays no `ISZERO`; a materialized boolean, or
//! a width whose limit is a cheap push, favors the comparison. [`FunctionBuilder::fits_in_bits`]
//! and [`FunctionBuilder::exceeds_bits`] choose one by price when they emit the test, and
//! [`BitWidthTest::of`] reads either back, so no pass depends on one spelling.
//!
//! [`FunctionBuilder::fits_in_bits`]: super::FunctionBuilder::fits_in_bits
//! [`FunctionBuilder::exceeds_bits`]: super::FunctionBuilder::exceeds_bits

use super::{Function, InstKind, ValueId};
use alloy_primitives::U256;

/// A condition that holds exactly when `word` fits in `bits` bits (`fits`), or when it does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BitWidthTest {
    pub(crate) word: ValueId,
    /// The width, 1 to 255 bits.
    pub(crate) bits: u32,
    /// Whether the condition holds for a word that fits.
    pub(crate) fits: bool,
}

impl BitWidthTest {
    /// Reads the test that the instruction defining `value` spells, in either spelling.
    pub(crate) fn of(func: &Function, value: ValueId) -> Option<Self> {
        Self::of_kind(func, func.value_inst_kind(value)?)
    }

    /// Reads the test that `kind` spells, in either spelling.
    pub(crate) fn of_kind(func: &Function, kind: &InstKind) -> Option<Self> {
        let literal = |value| func.value_u256(value);
        // `word < 2^k` fits, `word > 2^k - 1` does not; the other operand is the literal.
        let compare = |word, limit, fits| {
            let limit: U256 = literal(limit)?;
            if literal(word).is_some() {
                return None;
            }
            let power = if fits { limit } else { limit.checked_add(U256::ONE)? };
            Self::new(word, power, fits)
        };
        match *kind {
            InstKind::Lt(a, b) => compare(a, b, true).or_else(|| compare(b, a, false)),
            InstKind::Gt(a, b) => compare(a, b, false).or_else(|| compare(b, a, true)),
            _ => {
                let (test, fits) =
                    match (kind.zero_test_operand(func), kind.nonzero_test_operand(func)) {
                        (Some(test), _) => (test, true),
                        (_, Some(test)) => (test, false),
                        _ => return None,
                    };
                let &InstKind::Shr(bits, word) = func.value_inst_kind(test)? else { return None };
                let bits = u32::try_from(literal(bits)?).ok()?;
                (1..256).contains(&bits).then_some(Self { word, bits, fits })
            }
        }
    }

    fn new(word: ValueId, power: U256, fits: bool) -> Option<Self> {
        // A limit of one is a zero test, not a width.
        (power > U256::ONE && power.is_power_of_two()).then(|| Self {
            word,
            bits: power.trailing_zeros() as u32,
            fits,
        })
    }

    /// The largest word that fits, `2^bits - 1`.
    pub(crate) fn limit(self) -> U256 {
        (U256::ONE << self.bits) - U256::ONE
    }
}

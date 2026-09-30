//! Unsigned interval domain.
//!
//! An [`Interval`] bounds a 256-bit word by an inclusive unsigned range. Arithmetic follows EVM
//! wrapping semantics: a result whose bounds cannot wrap stays exact, and one that may wrap
//! becomes the full range. Checked arithmetic reverts instead of wrapping, so its result keeps
//! the non-wrapping part of the range. Comparisons evaluate to `0`, `1`, or `[0, 1]`, and their
//! refinement narrows both operands on each branch edge. Widening jumps a growing bound to the
//! end of the word, which bounds loops.
//!
//! This is the numeric domain other passes can query interprocedurally; the intraprocedural
//! `check-elim` pass keeps its own relational range facts.

use super::{
    lattice::JoinSemiLattice,
    value::{DomainCx, Seeded, ValueDomain},
};
use crate::mir::{CheckedOp, Op};
use alloy_primitives::U256;
use std::fmt;

/// An inclusive unsigned range; `lo > hi` is the empty interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Interval {
    /// Smallest possible value.
    pub(crate) lo: U256,
    /// Largest possible value.
    pub(crate) hi: U256,
}

impl Interval {
    /// Every word.
    pub(crate) const FULL: Self = Self { lo: U256::ZERO, hi: U256::MAX };

    /// No word.
    pub(crate) const EMPTY: Self = Self { lo: U256::MAX, hi: U256::ZERO };

    const BOOL: Self = Self { lo: U256::ZERO, hi: U256::from_limbs([1, 0, 0, 0]) };

    /// Returns the interval `[lo, hi]`.
    pub(crate) const fn new(lo: U256, hi: U256) -> Self {
        Self { lo, hi }
    }

    fn singleton(value: U256) -> Self {
        Self::new(value, value)
    }

    fn is_empty_range(self) -> bool {
        self.lo > self.hi
    }

    fn intersect(self, other: Self) -> Self {
        Self::new(self.lo.max(other.lo), self.hi.min(other.hi))
    }

    fn truth(value: bool) -> Self {
        Self::singleton(U256::from(u8::from(value)))
    }

    fn add(self, other: Self) -> Self {
        match (self.lo.checked_add(other.lo), self.hi.checked_add(other.hi)) {
            (Some(lo), Some(hi)) => Self::new(lo, hi),
            _ => Self::FULL,
        }
    }

    fn checked_add(self, other: Self) -> Self {
        match self.lo.checked_add(other.lo) {
            Some(lo) => Self::new(lo, self.hi.saturating_add(other.hi)),
            None => Self::EMPTY,
        }
    }

    fn sub(self, other: Self) -> Self {
        if self.lo >= other.hi {
            Self::new(self.lo - other.hi, self.hi - other.lo)
        } else {
            Self::FULL
        }
    }

    fn checked_sub(self, other: Self) -> Self {
        if self.hi < other.lo {
            return Self::EMPTY;
        }
        Self::new(self.lo.saturating_sub(other.hi), self.hi - other.lo)
    }

    fn mul(self, other: Self) -> Self {
        match (self.lo.checked_mul(other.lo), self.hi.checked_mul(other.hi)) {
            (Some(lo), Some(hi)) => Self::new(lo, hi),
            _ => Self::FULL,
        }
    }

    fn checked_mul(self, other: Self) -> Self {
        match self.lo.checked_mul(other.lo) {
            Some(lo) => Self::new(lo, self.hi.saturating_mul(other.hi)),
            None => Self::EMPTY,
        }
    }

    fn div(self, other: Self) -> Self {
        // EVM division by zero yields zero.
        let lo = if other.hi.is_zero() { U256::ZERO } else { self.lo / other.hi };
        let hi = if other.lo.is_zero() { self.hi } else { self.hi / other.lo };
        Self::new(if other.lo.is_zero() { U256::ZERO } else { lo }, hi)
    }

    fn rem(self, other: Self) -> Self {
        if other.hi.is_zero() {
            return Self::singleton(U256::ZERO);
        }
        Self::new(U256::ZERO, self.hi.min(other.hi - U256::from(1)))
    }
}

impl JoinSemiLattice for Interval {
    fn join(&mut self, other: &Self) -> bool {
        if other.is_empty_range() {
            return false;
        }
        if self.is_empty_range() {
            *self = *other;
            return true;
        }
        let joined = Self::new(self.lo.min(other.lo), self.hi.max(other.hi));
        let changed = joined != *self;
        *self = joined;
        changed
    }

    fn widen(&mut self, other: &Self) -> bool {
        if other.is_empty_range() {
            return false;
        }
        if self.is_empty_range() {
            *self = *other;
            return true;
        }
        let lo = if other.lo < self.lo { U256::ZERO } else { self.lo };
        let hi = if other.hi > self.hi { U256::MAX } else { self.hi };
        let widened = Self::new(lo, hi);
        let changed = widened != *self;
        *self = widened;
        changed
    }
}

impl fmt::Display for Interval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let word = |f: &mut fmt::Formatter<'_>, value: U256| {
            if value == U256::MAX {
                f.write_str("max")
            } else if let Ok(small) = u64::try_from(value)
                && small < 1 << 32
            {
                write!(f, "{small}")
            } else {
                write!(f, "{value:#x}")
            }
        };
        if self.is_empty_range() {
            return f.write_str("empty");
        }
        if self.lo == self.hi {
            return word(f, self.lo);
        }
        f.write_str("[")?;
        word(f, self.lo)?;
        f.write_str(", ")?;
        word(f, self.hi)?;
        f.write_str("]")
    }
}

impl ValueDomain for Interval {
    const NAME: &'static str = "intervals";

    fn top() -> Self {
        Self::FULL
    }

    fn constant(value: U256) -> Self {
        Self::singleton(value)
    }

    fn is_empty(&self) -> bool {
        self.is_empty_range()
    }

    fn transfer(_cx: &DomainCx<'_>, op: Op, operands: &[Self]) -> Self {
        let arg = |i: usize| operands.get(i).copied().unwrap_or(Self::FULL);
        let (a, b) = (arg(0), arg(1));
        match op {
            Op::Add { .. } => a.add(b),
            Op::Sub { .. } => a.sub(b),
            Op::Mul { .. } => a.mul(b),
            Op::Div { .. } => a.div(b),
            Op::Mod { .. } => a.rem(b),
            Op::And { .. } => Self::new(U256::ZERO, a.hi.min(b.hi)),
            Op::Or { .. } | Op::Xor { .. } => {
                let bits = a.hi.max(b.hi).bit_len();
                let hi =
                    if bits >= 256 { U256::MAX } else { (U256::from(1) << bits) - U256::from(1) };
                Self::new(if matches!(op, Op::Or { .. }) { a.lo.max(b.lo) } else { U256::ZERO }, hi)
            }
            Op::Shr { .. } => {
                match (a.lo == a.hi).then_some(a.lo).and_then(|s| usize::try_from(s).ok()) {
                    Some(shift) if shift < 256 => Self::new(b.lo >> shift, b.hi >> shift),
                    Some(_) => Self::singleton(U256::ZERO),
                    None => Self::new(U256::ZERO, b.hi),
                }
            }
            Op::Shl { .. } => {
                match (a.lo == a.hi).then_some(a.lo).and_then(|s| usize::try_from(s).ok()) {
                    Some(shift) if shift < 256 && b.hi.leading_zeros() >= shift => {
                        Self::new(b.lo << shift, b.hi << shift)
                    }
                    _ => Self::FULL,
                }
            }
            Op::Lt { .. } => {
                if a.hi < b.lo {
                    Self::truth(true)
                } else if a.lo >= b.hi {
                    Self::truth(false)
                } else {
                    Self::BOOL
                }
            }
            Op::Gt { .. } => {
                if a.lo > b.hi {
                    Self::truth(true)
                } else if a.hi <= b.lo {
                    Self::truth(false)
                } else {
                    Self::BOOL
                }
            }
            Op::Eq { .. } | Op::Ne { .. } => {
                let equal = a.lo == a.hi && b.lo == b.hi && a.lo == b.lo;
                let disjoint = a.hi < b.lo || b.hi < a.lo;
                let eq = matches!(op, Op::Eq { .. });
                if equal {
                    Self::truth(eq)
                } else if disjoint {
                    Self::truth(!eq)
                } else {
                    Self::BOOL
                }
            }
            Op::Zext { .. } | Op::Bitcast { .. } => a,
            Op::Trunc { bits, .. } => {
                if bits >= 256 || a.hi.bit_len() <= bits as usize {
                    a
                } else {
                    Self::new(U256::ZERO, (U256::from(1) << bits as usize) - U256::from(1))
                }
            }
            Op::Select { .. } => {
                let mut joined = arg(1);
                joined.join(&arg(2));
                joined
            }
            Op::CheckedBinary { op, .. } => match op {
                CheckedOp::Add => a.checked_add(b),
                CheckedOp::Sub => a.checked_sub(b),
                CheckedOp::Mul => a.checked_mul(b),
                CheckedOp::Div | CheckedOp::WrappingDiv if b.lo.is_zero() && b.hi.is_zero() => {
                    Self::EMPTY
                }
                CheckedOp::Div | CheckedOp::WrappingDiv => a.div(b),
                CheckedOp::Rem => a.rem(b),
                CheckedOp::Pow => Self::FULL,
            },
            _ => Self::FULL,
        }
    }

    fn refine(op: Op, operands: &mut [Self], taken: bool) -> bool {
        let [a, b] = operands else { return true };
        match op {
            Op::Lt { .. } if taken => less(a, b),
            Op::Lt { .. } => at_most(b, a),
            Op::Gt { .. } if taken => less(b, a),
            Op::Gt { .. } => at_most(a, b),
            Op::Eq { .. } | Op::Ne { .. } if taken == matches!(op, Op::Eq { .. }) => {
                let both = a.intersect(*b);
                *a = both;
                *b = both;
            }
            Op::Eq { .. } | Op::Ne { .. } => {
                exclude(a, *b);
                exclude(b, *a);
            }
            _ => {}
        }
        !a.is_empty_range() && !b.is_empty_range()
    }
}

/// Narrows both sides of `a < b`.
fn less(a: &mut Interval, b: &mut Interval) {
    if b.hi.is_zero() {
        *a = Interval::EMPTY;
        return;
    }
    *a = a.intersect(Interval::new(U256::ZERO, b.hi - U256::from(1)));
    if !a.is_empty_range() {
        *b = b.intersect(Interval::new(a.lo.saturating_add(U256::from(1)), U256::MAX));
    }
}

/// Narrows both sides of `a <= b`.
fn at_most(a: &mut Interval, b: &mut Interval) {
    *a = a.intersect(Interval::new(U256::ZERO, b.hi));
    if !a.is_empty_range() {
        *b = b.intersect(Interval::new(a.lo, U256::MAX));
    }
}

/// Removes a known value from an endpoint of `range`.
fn exclude(range: &mut Interval, other: Interval) {
    if other.lo != other.hi || range.is_empty_range() {
        return;
    }
    let value = other.lo;
    if range.lo == value && range.hi == value {
        *range = Interval::EMPTY;
    } else if range.lo == value {
        range.lo += U256::from(1);
    } else if range.hi == value {
        range.hi -= U256::from(1);
    }
}

impl Seeded for Interval {}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(lo: u64, hi: u64) -> Interval {
        Interval::new(U256::from(lo), U256::from(hi))
    }

    #[test]
    fn wrapping_and_checked_addition() {
        assert_eq!(range(1, 2).add(range(3, 4)), range(4, 6));
        assert_eq!(Interval::new(U256::MAX, U256::MAX).add(range(1, 1)), Interval::FULL);
        let near = Interval::new(U256::MAX - U256::from(1), U256::MAX);
        assert_eq!(
            near.checked_add(range(0, 1)),
            Interval::new(U256::MAX - U256::from(1), U256::MAX)
        );
    }

    #[test]
    fn refinement_narrows_both_sides() {
        let mut operands = [range(0, 100), range(10, 10)];
        let op = Op::Lt { a: crate::mir::ValueId::new(0), b: crate::mir::ValueId::new(1) };
        assert!(Interval::refine(op, &mut operands, true));
        assert_eq!(operands[0], range(0, 9));
        let mut operands = [range(0, 100), range(10, 10)];
        assert!(Interval::refine(op, &mut operands, false));
        assert_eq!(operands[0], range(10, 100));
        let mut impossible = [range(5, 5), range(0, 0)];
        assert!(!Interval::refine(op, &mut impossible, true));
    }

    #[test]
    fn widening_jumps_growing_bounds() {
        let mut value = range(0, 1);
        assert!(value.widen(&range(0, 2)));
        assert_eq!(value, Interval::new(U256::ZERO, U256::MAX));
    }
}

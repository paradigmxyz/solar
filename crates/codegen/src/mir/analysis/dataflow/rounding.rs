//! Rounding-direction domain.
//!
//! Following the rounding analysis on Slither's `dev-data-flow-rounding` branch and
//! [roundme](https://github.com/crytic/roundme), a value carries the set of directions its
//! computation may round: up, down, or exact. Division rounds down unless it is the ceiling
//! idiom `(a + b - 1) / b` or `a / b` with a numerator rounded up; subtracting or dividing
//! by a value inverts its direction. Combining opposite directions is an inconsistency: the
//! result's error bound is unknown, and the analysis reports it. Dividing by a value rounded
//! in the same direction as the numerator is reported too, because a smaller denominator
//! rounds the quotient the other way.
//!
//! Internal calls are summarized like any other domain. Library calls named for their
//! direction, such as `mulDivUp` or `divWadDown`, seed their results by name.

use super::{
    lattice::JoinSemiLattice,
    value::{DomainCx, Seeded, ValueDomain},
};
use crate::mir::{CheckedOp, InstKind, Op, ValueId};
use alloy_primitives::U256;
use std::fmt;

/// A set of rounding directions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Rounding(u8);

impl Rounding {
    const UP: u8 = 1;
    const DOWN: u8 = 2;
    const EXACT: u8 = 4;
    const UNKNOWN: u8 = 8;

    const fn one(tag: u8) -> Self {
        Self(tag)
    }

    fn tags(self) -> impl Iterator<Item = u8> {
        [Self::UP, Self::DOWN, Self::EXACT, Self::UNKNOWN]
            .into_iter()
            .filter(move |&tag| self.0 & tag != 0)
    }

    fn invert(self) -> Self {
        let mut inverted = self.0 & (Self::EXACT | Self::UNKNOWN);
        if self.0 & Self::UP != 0 {
            inverted |= Self::DOWN;
        }
        if self.0 & Self::DOWN != 0 {
            inverted |= Self::UP;
        }
        Self(inverted)
    }

    /// Combines the directions of two operands; opposite directions become unknown.
    fn combine(self, other: Self) -> Self {
        let mut result = 0;
        for a in self.tags() {
            for b in other.tags() {
                result |= match (a, b) {
                    (Self::EXACT, tag) | (tag, Self::EXACT) => tag,
                    (a, b) if a == b => a,
                    _ => Self::UNKNOWN,
                };
            }
        }
        Self(result)
    }

    /// Returns whether every pair of directions conflicts.
    fn conflicts(self, other: Self) -> bool {
        let pairs = self.tags().flat_map(|a| other.tags().map(move |b| (a, b)));
        let mut any = false;
        for (a, b) in pairs {
            any = true;
            if !matches!((a, b), (Self::UP, Self::DOWN) | (Self::DOWN, Self::UP)) {
                return false;
            }
        }
        any
    }

    /// Returns the single non-exact direction, if the set has exactly one.
    fn direction(self) -> Option<u8> {
        match self.0 {
            Self::UP | Self::DOWN => Some(self.0),
            _ => None,
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        let lower = name.to_ascii_lowercase();
        let up = [
            "divup",
            "mulup",
            "mulwadup",
            "divwadup",
            "ceildiv",
            "roundup",
            "muldivup",
            "muldivroundingup",
            "divroundingup",
            "rpowup",
        ];
        let down = [
            "divdown",
            "muldown",
            "mulwaddown",
            "divwaddown",
            "divfloor",
            "rounddown",
            "muldivdown",
            "muldiv",
            "divroundingdown",
            "rpowdown",
        ];
        if up.contains(&lower.as_str()) {
            Some(Self::one(Self::UP))
        } else if down.contains(&lower.as_str()) {
            Some(Self::one(Self::DOWN))
        } else {
            None
        }
    }
}

impl JoinSemiLattice for Rounding {
    fn join(&mut self, other: &Self) -> bool {
        let joined = self.0 | other.0;
        // A set with a direction drops the exact tag, as in the reference analysis.
        let joined = if joined & (Self::UP | Self::DOWN | Self::UNKNOWN) != 0 {
            joined & !Self::EXACT
        } else {
            joined
        };
        let changed = joined != self.0;
        self.0 = joined;
        changed
    }
}

impl fmt::Display for Rounding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names = [
            (Self::UP, "up"),
            (Self::DOWN, "down"),
            (Self::EXACT, "exact"),
            (Self::UNKNOWN, "unknown"),
        ];
        let mut first = true;
        for (tag, name) in names {
            if self.0 & tag != 0 {
                if !first {
                    f.write_str("|")?;
                }
                first = false;
                f.write_str(name)?;
            }
        }
        if first {
            f.write_str("none")?;
        }
        Ok(())
    }
}

/// Returns whether `numerator / divisor` is the ceiling idiom `(a + divisor - 1) / divisor`.
fn is_ceiling(cx: &DomainCx<'_>, numerator: ValueId, divisor: ValueId) -> bool {
    let sub_one = |value: ValueId| match cx.def(value) {
        Some(&InstKind::Sub(x, one))
        | Some(&InstKind::CheckedBinary { op: CheckedOp::Sub, lhs: x, rhs: one, .. })
            if cx.func.value_u256(one) == Some(U256::from(1)) =>
        {
            Some(x)
        }
        _ => None,
    };
    let add = |value: ValueId| match cx.def(value) {
        Some(&InstKind::Add(x, y))
        | Some(&InstKind::CheckedBinary { op: CheckedOp::Add, lhs: x, rhs: y, .. }) => Some((x, y)),
        _ => None,
    };
    // (a + b) - 1, or a + (b - 1).
    if let Some(sum) = sub_one(numerator)
        && let Some((x, y)) = add(sum)
    {
        return x == divisor || y == divisor;
    }
    if let Some((x, y)) = add(numerator) {
        return sub_one(x) == Some(divisor) || sub_one(y) == Some(divisor);
    }
    false
}

impl ValueDomain for Rounding {
    const NAME: &'static str = "rounding";

    fn top() -> Self {
        // Unannotated inputs count as exact, as in the reference analysis.
        Self::one(Self::EXACT)
    }

    fn constant(_value: U256) -> Self {
        Self::one(Self::EXACT)
    }

    fn call_seed(callee: &str) -> Option<Self> {
        Self::from_name(callee)
    }

    fn transfer(cx: &DomainCx<'_>, op: Op, operands: &[Self]) -> Self {
        let arg = |i: usize| operands.get(i).copied().unwrap_or_else(Self::top);
        let (a, b) = (arg(0), arg(1));
        let divide = |numerator: ValueId, divisor: ValueId| {
            if is_ceiling(cx, numerator, divisor) {
                return Self::one(Self::UP);
            }
            let combined = a.combine(b.invert());
            // Truncating division of exact operands rounds down.
            if combined == Self::one(Self::EXACT) { Self::one(Self::DOWN) } else { combined }
        };
        match op {
            Op::Add { .. } | Op::Mul { .. } => a.combine(b),
            Op::Sub { .. } => a.combine(b.invert()),
            Op::Div { a: numerator, b: divisor } => divide(numerator, divisor),
            Op::CheckedBinary { op, lhs, rhs, .. } => match op {
                CheckedOp::Add | CheckedOp::Mul => a.combine(b),
                CheckedOp::Sub => a.combine(b.invert()),
                CheckedOp::Div | CheckedOp::WrappingDiv => divide(lhs, rhs),
                CheckedOp::Rem | CheckedOp::Pow => Self::top(),
            },
            Op::Zext { .. } | Op::Trunc { .. } | Op::Bitcast { .. } => a,
            Op::Select { .. } => {
                let mut joined = arg(1);
                joined.join(&arg(2));
                joined
            }
            _ => Self::top(),
        }
    }

    fn check(cx: &DomainCx<'_>, op: Op, operands: &[Self], findings: &mut Vec<String>) {
        #[derive(PartialEq)]
        enum Kind {
            Combine,
            Subtract,
            Divide,
        }
        let arg = |i: usize| operands.get(i).copied().unwrap_or_else(Self::top);
        let (a, b) = (arg(0), arg(1));
        let kind = match op {
            Op::Add { .. } | Op::Mul { .. } => Kind::Combine,
            Op::Sub { .. } => Kind::Subtract,
            Op::Div { .. } => Kind::Divide,
            Op::CheckedBinary { op, .. } => match op {
                CheckedOp::Add | CheckedOp::Mul => Kind::Combine,
                CheckedOp::Sub => Kind::Subtract,
                CheckedOp::Div | CheckedOp::WrappingDiv => Kind::Divide,
                CheckedOp::Rem | CheckedOp::Pow => return,
            },
            _ => return,
        };
        let text = crate::mir::display::display_instruction(cx.func, Some(cx.module), cx.inst);
        let effective = if kind == Kind::Combine { b } else { b.invert() };
        if kind == Kind::Divide
            && let Some(direction) = a.direction()
            && b.direction() == Some(direction)
        {
            findings.push(format!(
                "`{text}` divides a value rounded {a} by a value rounded the same way, which rounds the quotient the other way"
            ));
        } else if a.conflicts(effective) {
            findings.push(match kind {
                Kind::Combine => format!("`{text}` combines values rounded {a} and {b}"),
                Kind::Subtract => {
                    format!("`{text}` subtracts a value rounded {b} from a value rounded {a}")
                }
                Kind::Divide => {
                    format!("`{text}` divides a value rounded {a} by a value rounded {b}")
                }
            });
        }
    }
}

impl Seeded for Rounding {
    /// Reads a direction from a `_UP` or `_DOWN` name suffix or from "rounds up" and
    /// "rounds down" in the documentation.
    fn parse_seed(text: &str) -> Option<Self> {
        let lower = text.to_ascii_lowercase();
        let name = lower.split_whitespace().next().unwrap_or_default();
        if name.ends_with("_up") || lower.contains("rounds up") || lower.contains("rounded up") {
            Some(Self::one(Self::UP))
        } else if name.ends_with("_down")
            || lower.contains("rounds down")
            || lower.contains("rounded down")
        {
            Some(Self::one(Self::DOWN))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combination_and_inversion() {
        let up = Rounding::one(Rounding::UP);
        let down = Rounding::one(Rounding::DOWN);
        let exact = Rounding::top();
        assert_eq!(up.combine(exact), up);
        assert_eq!(up.combine(down), Rounding::one(Rounding::UNKNOWN));
        assert_eq!(down.invert(), up);
        assert!(up.conflicts(down));
        assert!(!up.conflicts(exact));
        assert_eq!(Rounding::from_name("mulDivUp"), Some(up));
        assert_eq!(Rounding::from_name("mulDiv"), Some(down));
    }
}

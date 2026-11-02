//! Canonical operand order, applied to every instruction when it is allocated.
//!
//! The middle end keeps one spelling of each equivalent instruction so analyses and rewrites
//! match a single pattern, and the backend picks the cheapest spelling late. Nothing here prices
//! a choice.
//!
//! - A commutative pair orders its operands by [`OperandRank`], the more complex operand first and
//!   equal ranks in their written order, so constants end up on the right. This follows LLVM's
//!   `getComplexity`, with unary operations and casts ranked as one class.
//! - A comparison with a constant on the left swaps its operands and flips its predicate, so `lt 5,
//!   x` becomes `gt x, 5`.
//! - A comparison of two non-constants is always `lt` or `slt`: `gt a, b` becomes `lt b, a`. LLVM
//!   keeps both directions; we keep one because every pass that tests an index against a bound
//!   already matches `lt index, bound`, and the EVM IR peephole re-spells it for the stack.
//!
//! A pass that rewrites an instruction in place passes the new kind through
//! [`Function::canonical_kind`]. Replacing the uses of a value does not re-establish the order;
//! the e-graph restores it through [`Function::canonical_op`] when it next runs.

use super::{Function, InstKind, Op, OpTraits, Value, ValueId};
use std::{borrow::Cow, cmp::Reverse};

/// How complex an operand is, ordered from the simplest to the most complex.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum OperandRank {
    Constant,
    /// An argument, phi, or any other value not produced by a computation.
    Leaf,
    /// A unary operation or cast.
    Unary,
    Other,
}

impl Function {
    /// Returns `op` with its operands in canonical order.
    #[must_use]
    pub fn canonical_op(&self, op: Op) -> Op {
        let rank = |value| self.operand_rank(value);
        let is_constant = |value| matches!(self.value(value), Value::Immediate(_));
        let op = op.canonicalize_commutative_by_key(|value| Reverse(rank(value)));
        match op {
            // lt constant, value => gt value, constant
            Op::Lt { a, b } if is_constant(a) && !is_constant(b) => Op::Gt { a: b, b: a },
            Op::SLt { a, b } if is_constant(a) && !is_constant(b) => Op::SGt { a: b, b: a },
            // gt a, value => lt value, a
            Op::Gt { b, .. } | Op::SGt { b, .. } if !is_constant(b) => op.less_than_form(),
            other => other,
        }
    }

    /// Returns `kind` with its operands in canonical order.
    #[must_use]
    pub fn canonical_kind(&self, kind: InstKind) -> InstKind {
        let ordered = kind.op_def().traits.contains(OpTraits::COMMUTATIVE)
            || matches!(
                kind,
                InstKind::Lt(..) | InstKind::Gt(..) | InstKind::SLt(..) | InstKind::SGt(..)
            );
        if ordered && let Some(canonical) = self.canonical_op(kind.op()).into_kind() {
            return canonical;
        }
        kind
    }

    fn operand_rank(&self, value: ValueId) -> OperandRank {
        match self.value(value) {
            Value::Immediate(_) => OperandRank::Constant,
            Value::Inst(inst) => match self.inst(*inst).kind {
                InstKind::Phi(_) => OperandRank::Leaf,
                InstKind::Not(_) | InstKind::Zext(_) | InstKind::Trunc(..) | InstKind::Sext(..) => {
                    OperandRank::Unary
                }
                _ => OperandRank::Other,
            },
            Value::Arg(_) | Value::Undef(_) | Value::Error(_) => OperandRank::Leaf,
        }
    }
}

impl Op {
    /// Respells `gt a, b` as `lt b, a`, and `sgt` as `slt`, so both directions share one form.
    #[must_use]
    pub fn less_than_form(self) -> Self {
        match self {
            Self::Gt { a, b } => Self::Lt { a: b, b: a },
            Self::SGt { a, b } => Self::SLt { a: b, b: a },
            other => other,
        }
    }
}

impl InstKind {
    /// Returns the instruction with a greater-than comparison respelled as the swapped
    /// less-than, as [`Op::less_than_form`] does.
    #[must_use]
    pub fn less_than_form(&self) -> Cow<'_, Self> {
        match self {
            Self::Gt(..) | Self::SGt(..) => {
                Cow::Owned(self.op().less_than_form().into_kind().expect("compare has no payload"))
            }
            _ => Cow::Borrowed(self),
        }
    }
}

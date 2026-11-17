//! Canonical operand order, applied to every instruction when it is allocated or edited.
//!
//! The middle end keeps one spelling of each equivalent instruction so analyses and rewrites
//! match a single pattern, and the backend picks the cheapest spelling late. Nothing here prices
//! a choice.
//!
//! - A commutative pair orders its operands by [`OperandRank`], the more complex operand first and
//!   equal ranks in their written order, so constants end up on the right. This follows LLVM's
//!   `getComplexity`, with unary operations and casts ranked as one class. The pair is the one the
//!   operation schema declares, so `addmod` and `mulmod` order their addends and factors and leave
//!   the modulus in place. Two constants order by value, the larger first, which puts `0` and `1`
//!   on the right where the rules look for them.
//! - A comparison with a constant on the left swaps its operands and flips its predicate: the
//!   constant moves to the right of `gt`.
//! - A comparison of two non-constants is always `lt` or `slt`: `gt a, b` becomes `lt b, a`. LLVM
//!   keeps both directions; we keep one because every pass that tests an index against a bound
//!   already matches `lt index, bound`, and the EVM IR stack rescheduler may mirror it where the
//!   stack favors that.
//!
//! Every `Function` entry point that creates or changes an instruction restores the order, so it
//! holds between passes: [`Function::alloc_value_inst`] for a new instruction,
//! [`Function::set_inst_kind`] and [`Function::replace_inst_kind`] for a new kind, and
//! [`Function::rewrite_inst_operands`], [`Function::replace_uses`] and
//! [`Function::replace_uses_canonicalized`] for new operands. A raw write through
//! [`Function::inst_mut`] bypasses them, and the validator rejects the instruction when
//! [`Function::has_canonical_operands`] refuses it.
//!
//! The order among non-constant operands is only as current as the last write to the instruction.
//! Rewriting a value's producer in place can change its [`OperandRank`] and leave its users in the
//! old order, so the validator checks only what holds regardless of rank: constants and the
//! comparison form.

use super::{Function, InstId, InstKind, Op, OpTraits, Value, ValueId};
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
        self.canonical_op_ranked(op, |value| self.operand_rank(value))
    }

    /// Returns `kind` with its operands in canonical order.
    #[must_use]
    pub fn canonical_kind(&self, kind: InstKind) -> InstKind {
        if is_ordered(&kind)
            && let Some(canonical) = self.canonical_op(kind.op()).into_kind()
        {
            return canonical;
        }
        kind
    }

    /// Sets the kind of an existing instruction, in canonical operand order, and leaves its
    /// metadata and result type to the caller. This is for a rewrite that manages both itself,
    /// such as a lowering; an equivalent rewrite uses [`Self::replace_inst_kind`].
    pub fn set_inst_kind(&mut self, inst: InstId, kind: InstKind) {
        self.inst_mut(inst).kind = self.canonical_kind(kind);
    }

    /// Replaces an instruction with an equivalent one, in canonical operand order, preserving its
    /// result and provenance.
    ///
    /// Memory regions, storage aliases, and effect overrides describe the old operation and are
    /// dropped. ABI validation dependencies and allocation obligations survive an equivalent
    /// rewrite. The caller must prove equivalence and preserve the result representation.
    pub fn replace_inst_kind(&mut self, inst: InstId, kind: InstKind) {
        let kind = self.canonical_kind(kind);
        self.inst_mut(inst).replace_kind(kind);
    }

    /// Rewrites an instruction's operands in place and restores their canonical order if any
    /// changed. Returns whether any changed.
    pub fn rewrite_inst_operands(&mut self, inst: InstId, mut f: impl FnMut(&mut ValueId)) -> bool {
        let changed = self.inst_mut(inst).rewrite_operands(&mut f);
        if changed {
            self.canonicalize_inst(inst);
        }
        changed
    }

    /// Puts every instruction in canonical operand order.
    ///
    /// For a producer that builds a whole function from operands it does not yet know to be
    /// final, such as the text parser, whose forward references have no producer to rank.
    pub fn canonicalize_operands(&mut self) {
        for block in self.blocks.indices() {
            for index in 0..self.blocks[block].instructions.len() {
                self.canonicalize_inst(self.blocks[block].instructions[index]);
            }
        }
    }

    fn canonicalize_inst(&mut self, inst: InstId) {
        // Only these kinds clone: the payloads of other kinds are not worth copying.
        if is_ordered(&self.inst(inst).kind) {
            self.set_inst_kind(inst, self.inst(inst).kind.clone());
        }
    }

    /// Whether `kind` has its constants and comparison in canonical form, whatever the rank of
    /// its non-constant operands, see the module documentation.
    #[must_use]
    pub fn has_canonical_operands(&self, kind: &InstKind) -> bool {
        let rank = |value| match self.value(value) {
            Value::Immediate(_) => OperandRank::Constant,
            _ => OperandRank::Leaf,
        };
        !is_ordered(kind)
            || self.canonical_op_ranked(kind.op(), rank).into_kind().as_ref() == Some(kind)
    }

    fn canonical_op_ranked(&self, op: Op, rank: impl Fn(ValueId) -> OperandRank) -> Op {
        let is_constant = |value| matches!(self.value(value), Value::Immediate(_));
        let op = op.canonicalize_commutative_by_key(|value| {
            (Reverse(rank(value)), Reverse(self.value_u256(value)))
        });
        match op {
            // lt constant, value => gt value, constant
            Op::Lt { a, b } if is_constant(a) && !is_constant(b) => Op::Gt { a: b, b: a },
            Op::SLt { a, b } if is_constant(a) && !is_constant(b) => Op::SGt { a: b, b: a },
            // gt a, value => lt value, a
            Op::Gt { b, .. } | Op::SGt { b, .. } if !is_constant(b) => op.less_than_form(),
            other => other,
        }
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

/// Whether canonical order constrains `kind`. Only these kinds convert: `op()` of other kinds
/// would copy their payloads.
fn is_ordered(kind: &InstKind) -> bool {
    kind.op_def().traits.contains(OpTraits::COMMUTATIVE)
        || matches!(
            kind,
            InstKind::Lt(..) | InstKind::Gt(..) | InstKind::SLt(..) | InstKind::SGt(..)
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Immediate, Module, validate};
    use alloy_primitives::U256;
    use solar_data_structures::map::FxHashMap;
    use solar_interface::{ColorChoice, Session, source_map::FileName};

    const MODULE: &str = "@module M\nfn @f(arg0: i256, arg1: i256) -> i256 {\n  bb0:\n    v0 = add arg0, arg1\n    ret v0\n}\n";

    // The text parser canonicalizes, so only a raw write can build a non-canonical instruction.
    #[test]
    fn validator_rejects_a_constant_on_the_left_until_the_order_is_restored() {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.enter_sequential(|| {
            let file = sess.source_map().new_source_file(FileName::Custom("m.mir".into()), MODULE);
            let mut module = Module::parse(&sess, &file.unwrap()).unwrap();
            let function = module.functions.indices().next().unwrap();
            let func = &mut module.functions[function];
            let inst = func.instructions().next().unwrap();
            let InstKind::Add(arg0, arg1) = func.inst(inst).kind else { unreachable!() };
            let five = func.alloc_value(Value::Immediate(Immediate::I256(U256::from(5))));

            // An edit that bypasses `set_inst_kind` leaves the constant on the left.
            func.inst_mut(inst).kind = InstKind::Add(five, arg0);
            validate(&sess.dcx, &module);
            let errors = sess.dcx.emitted_errors().unwrap().unwrap_err().to_string();
            snapbox::assert_data_eq!(
                errors.lines().next().unwrap(),
                snapbox::str!["error: [fn0] [bb0, inst0] `add` operands are not in canonical order; a pass must rebuild it with `set_inst_kind` or `rewrite_inst_operands`"]
            );

            // Replacing a use puts the instruction back in order.
            let func = &mut module.functions[function];
            func.inst_mut(inst).kind = InstKind::Add(arg1, arg0);
            func.replace_uses(&FxHashMap::from_iter([(arg1, five)]));
            assert_eq!(func.inst(inst).kind, InstKind::Add(arg0, five));
            assert!(func.has_canonical_operands(&func.inst(inst).kind));
        });
    }
}

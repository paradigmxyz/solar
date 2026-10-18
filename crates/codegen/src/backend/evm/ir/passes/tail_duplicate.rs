//! Duplicate small straight-line joins after final loop placement.
//!
//! When both the incoming and outgoing jumps are explicit, copying the join into its predecessor
//! removes one destination push, jump, and label entry per execution. The outgoing transfer stays
//! intact. Added bytes must cost less to deposit than the removed transfers cost over the
//! configured optimizer runs. This runs after structural sharing and layout: before loop placement
//! these joins can still have fallthrough successors. Size mode does not duplicate joins.
//!
//! Only immediate pushes, physical stack operations, and position-independent computations qualify.
//! Immutable patches and protected predecessor boundaries are excluded. Cold tails stay shared.
//! Straight-line loop joins qualify, but no branches are copied: header tests and loop rotation
//! remain with loop layout. Block order and existing label identities stay intact; assembly omits
//! labels whose last reference disappears. Debug origins remain on the copied operations and never
//! affect eligibility.
//! Opcodes awaiting fork legalization stay shared because their expansion is not priced here.
//! Copies start beyond the one-byte address range, using an instruction-only lower bound on the
//! prefix size, so they cannot widen the labels and indexed tables packed by block layout.

use super::{
    EvmPass,
    utils::{instruction_size_lower_bound, is_split_point},
};
use crate::{
    backend::evm::{
        ir::{BlockId, Module, TerminatorKind},
        op,
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use solar_sema::Gcx;

pub(super) struct TailDuplicate;

impl EvmPass for TailDuplicate {
    fn name(&self) -> &'static str {
        "tail-duplicate"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        if !gcx.sess.opts.optimization.is_gas() {
            return false;
        }
        let target = Target::new(gcx);
        // Bound the removed transfer by its smallest encoding: even a non-entry label can
        // resolve to zero when the entry block falls through without emitting instructions.
        let transfer = target.push(U256::ZERO) + target.opcode(op::JUMP);
        let saved_gas = transfer.gas + target.opcode_gas(op::JUMPDEST);
        let copied_transfer = target.label_push() + target.opcode(op::JUMP);
        let mut changed = false;
        let mut prefix_bytes = 0;
        for caller in module.blocks.indices() {
            let block = &module.blocks[caller];
            if prefix_bytes <= usize::from(u8::MAX) {
                prefix_bytes += block
                    .instructions
                    .iter()
                    .map(|inst| instruction_size_lower_bound(gcx, inst))
                    .sum::<usize>();
                continue;
            }
            let Some(&TerminatorKind::Jump(callee)) =
                block.terminator.as_ref().map(|term| &term.kind)
            else {
                continue;
            };
            if callee == BlockId::ENTRY
                || callee == caller
                || Some(callee) == module.next_block(caller)
                || block.metadata.hotness.is_cold()
                || !is_split_point(&block.instructions, block.instructions.len())
            {
                continue;
            }
            let tail = &module.blocks[callee];
            let Some(term) = &tail.terminator else { continue };
            let TerminatorKind::Jump(successor) = term.kind else { continue };
            if tail.metadata.hotness.is_cold()
                || successor == callee
                || successor == caller
                || Some(successor) == module.next_block(callee)
            {
                continue;
            }
            let profitable = |bytes| {
                target.lifetime_gas(Cost::new(0, bytes))
                    < target.lifetime_gas(Cost::new(saved_gas, 0))
            };
            let growth = tail.instructions.iter().try_fold(
                copied_transfer.bytes - transfer.bytes,
                |n, inst| {
                    let bytes = if inst.is_encoded_push() {
                        target.push(inst.concrete_immediate()?).bytes
                    } else if let Some(stack) = inst.as_stack_op() {
                        target.stack_op(stack)?.bytes
                    } else if op::is_unaffected_by_preceding_push(inst.opcode)
                        && op::is_available(inst.opcode, target.evm_version())
                    {
                        target.opcode(inst.opcode).bytes
                    } else {
                        return None;
                    };
                    let growth = n + bytes;
                    profitable(growth).then_some(growth)
                },
            );
            if !growth.is_some_and(profitable) {
                continue;
            }
            let instructions = tail.instructions.clone();
            let terminator = term.clone();
            // NOTE: Removed transfer and label-entry checkpoints are dropped, as in inline-returns.
            // jump tail -> tail.instructions; jump successor
            module.blocks[caller].instructions.extend(instructions);
            module.blocks[caller].terminator = Some(terminator);
            changed = true;
        }
        changed
    }
}

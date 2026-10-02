//! Inline return tails that are no larger than the transfer used to reach them.
//!
//! Sharing a one- or two-instruction return tail can cost more than spelling it out: an explicit
//! jump needs a destination push, a `JUMP`, and a destination `JUMPDEST`. This pass replaces direct
//! non-fallthrough jumps with the target's complete terminal body when its encoded size fits the
//! transfer budget. Gas mode budgets a `PUSH1` and `JUMP`; size mode uses the smallest push
//! available on the selected fork, including a zero-address `PUSH0`. Gas mode can therefore spend
//! one byte when a destination ultimately resolves to zero. The target remains available to other
//! callers; ordinary CFG cleanup removes it only when no references remain.
//!
//! Only immediate pushes, physical stack operations, and position-independent computations are
//! copied. Deferred values and immutable patches are excluded, and `keep_with_next` boundaries
//! remain intact. Entry targets and current fallthrough edges are excluded because their transfer
//! may cost fewer bytes. The rewrite removes transfer gas independently of execution frequency and
//! does not depend on debug metadata. It runs after sharing, which can create these tiny tails.

use super::{EvmPass, utils::is_split_point};
use crate::{
    backend::evm::{
        ir::{BlockId, Module, PushValue, TerminatorKind},
        op,
    },
    target::Target,
};
use alloy_primitives::U256;
use solar_sema::Gcx;

pub(super) struct InlineReturns;

impl EvmPass for InlineReturns {
    fn name(&self) -> &'static str {
        "inline-returns"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let target = Target::new(gcx);
        let address_bytes = if gcx.sess.opts.optimization.is_gas() {
            target.opcode(op::PUSH1).bytes
        } else {
            target.push(U256::ZERO).bytes
        };
        let transfer_bytes = address_bytes + target.opcode(op::JUMP).bytes;
        let mut changed = false;
        for caller in module.blocks.indices() {
            let block = &module.blocks[caller];
            let Some(TerminatorKind::Jump(callee)) =
                block.terminator.as_ref().map(|term| term.kind.clone())
            else {
                continue;
            };
            if callee == BlockId::ENTRY
                || callee == caller
                || Some(callee) == module.next_block(caller)
                || !is_split_point(&block.instructions, block.instructions.len())
            {
                continue;
            }
            let tail = &module.blocks[callee];
            let Some(term) = &tail.terminator else { continue };
            let TerminatorKind::Op(opcode) = term.kind else { continue };
            if !op::is_terminal(opcode) {
                continue;
            }
            let size = tail.instructions.iter().try_fold(target.opcode(opcode).bytes, |n, inst| {
                let bytes = if inst.is_encoded_push() {
                    if inst.deferred_push().is_some() || inst.immutable_push().is_some() {
                        return None;
                    }
                    let Some(PushValue::Immediate(value)) = inst.value else { return None };
                    target.push(value).bytes
                } else if let Some(stack) = inst.as_stack_op() {
                    stack.assembled_len(target.evm_version())? as u32
                } else if op::is_unaffected_by_preceding_push(inst.opcode) {
                    target.opcode(inst.opcode).bytes
                } else {
                    return None;
                };
                let size = n + bytes;
                (size <= transfer_bytes).then_some(size)
            });
            if !size.is_some_and(|size| size <= transfer_bytes) {
                continue;
            }
            let instructions = tail.instructions.clone();
            let terminator = term.clone();
            // NOTE: The removed transfer and label-entry checkpoints are intentionally dropped.
            // Keeping an executable jump for them would defeat the rewrite; moving a label's
            // invocation onto an instruction could overwrite that instruction's own transition.
            // The copied operations retain their original source origins and function events.
            // jump tail -> tail.instructions; tail.terminator
            module.blocks[caller].instructions.extend(instructions);
            module.blocks[caller].terminator = Some(terminator);
            changed = true;
        }
        changed
    }
}

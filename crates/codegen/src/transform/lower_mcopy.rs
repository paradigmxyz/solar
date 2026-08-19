//! Lower `mcopy` for EVM versions that predate Cancun.

use crate::{
    mir::{BlockId, Function, FunctionBuilder, InstKind, Module},
    pass::MirPass,
    transform::utils::redirect_successor_predecessors,
};
use solar_sema::Gcx;

/// Lowers `mcopy` to an ascending word-copy loop when the target has no
/// `MCOPY` opcode, like solc.
///
/// The identity precompile would be smaller, but calling it is observable:
/// tooling that keys behavior on "the next call" — Foundry's `vm.prank` and
/// `vm.expectRevert` — consumes the precompile call instead of the intended
/// one, breaking every pre-Cancun test that pranks before an operation
/// involving a memory copy.
///
/// The trailing partial word is copied whole. Memory objects are word-aligned
/// and word-granular, so the overshoot lands in the padding.
pub(crate) struct LowerMCopy;

impl MirPass for LowerMCopy {
    fn name(&self) -> &'static str {
        "lower-mcopy"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        if gcx.sess.opts.evm_version.has_mcopy() {
            return false;
        }

        let mut changed = false;
        for func in module.functions.iter_mut() {
            if !func.blocks.is_empty() {
                changed |= lower_function(func);
            }
        }
        changed
    }
}

fn lower_function(func: &mut Function) -> bool {
    let mut changed = false;
    let mut block_index = 0;
    while block_index < func.blocks.len() {
        let block = BlockId::from_usize(block_index);
        let mcopy = func.blocks[block]
            .instructions
            .iter()
            .copied()
            .enumerate()
            .find(|(_, inst)| matches!(func.inst(*inst).kind, InstKind::MCopy(_, _, _)));
        if let Some((position, inst)) = mcopy {
            lower_mcopy(func, block, position, inst);
            changed = true;
        }
        block_index += 1;
    }
    changed
}

fn lower_mcopy(func: &mut Function, block: BlockId, position: usize, inst: crate::mir::InstId) {
    let InstKind::MCopy(dest, src, len) = func.inst(inst).kind else { unreachable!() };

    let mut instructions = std::mem::take(&mut func.blocks[block].instructions);
    let tail = instructions.split_off(position + 1);
    let removed = instructions.pop();
    debug_assert_eq!(removed, Some(inst));
    func.blocks[block].instructions = instructions;
    let old_terminator = func.blocks[block].terminator.take();

    let continuation = func.alloc_block();
    func.blocks[continuation].instructions = tail;
    func.blocks[continuation].terminator = old_terminator;
    redirect_successor_predecessors(func, block, continuation);

    let loop_head = func.alloc_block();
    let loop_body = func.alloc_block();
    let mut builder = FunctionBuilder::new(func);

    builder.switch_to_block(block);
    let zero = builder.imm_u64(0);
    builder.jump(loop_head);

    builder.switch_to_block(loop_head);
    let offset = builder.phi(vec![(block, zero)]);
    let remaining = builder.lt(offset, len);
    builder.branch(remaining, loop_body, continuation);

    builder.switch_to_block(loop_body);
    let src_ptr = builder.add(src, offset);
    let word = builder.mload(src_ptr);
    let dest_ptr = builder.add(dest, offset);
    builder.mstore(dest_ptr, word);
    let word_size = builder.imm_u64(32);
    let next = builder.add(offset, word_size);
    builder.add_phi_incoming(offset, loop_body, next);
    builder.jump(loop_head);
}

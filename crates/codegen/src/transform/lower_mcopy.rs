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
    let (old_terminator, metadata) = func.blocks[block].take_terminator();

    // continuation: tail; old_terminator !metadata(original block)
    let continuation = func.alloc_block();
    func.blocks[continuation].instructions = tail;
    if let Some(terminator) = old_terminator {
        func.blocks[continuation].set_terminator(terminator, metadata);
    }
    redirect_successor_predecessors(func, block, continuation);

    let loop_head = func.alloc_block();
    let loop_body = func.alloc_block();
    let tail_check = func.alloc_block();
    let tail_block = func.alloc_block();
    let mut builder = FunctionBuilder::new(func);

    // Copy the full words in a loop, then merge the partial tail word with a
    // byte mask so exactly `len` bytes change, like the identity precompile.
    builder.switch_to_block(block);
    let zero = builder.imm(0);
    let word_size = builder.imm(32);
    let thirty_one = builder.imm(31);
    let not_thirty_one = builder.not(thirty_one);
    let full = builder.and(len, not_thirty_one);
    let entry = builder.current_block();
    builder.jump(loop_head);

    builder.switch_to_block(loop_head);
    let offset = builder.phi(vec![(entry, zero)]);
    let remaining = builder.lt(offset, full);
    builder.branch(remaining, loop_body, tail_check);

    builder.switch_to_block(loop_body);
    let src_ptr = builder.add(src, offset);
    let word = builder.mload(src_ptr);
    let dest_ptr = builder.add(dest, offset);
    builder.mstore(dest_ptr, word);
    let next = builder.add(offset, word_size);
    builder.add_phi_incoming(offset, loop_body, next);
    builder.jump(loop_head);

    // A word-multiple length has no tail. Besides avoiding a redundant store,
    // skipping it keeps the mask calculation away from its 256-bit shift
    // boundary on pre-Cancun targets.
    builder.switch_to_block(tail_check);
    let has_partial = builder.lt(full, len);
    builder.branch(has_partial, tail_block, continuation);

    // Merge the partial source word with the bytes beyond `len` already at
    // the destination.
    builder.switch_to_block(tail_block);
    let partial = builder.and(len, thirty_one);
    let gap = builder.sub(word_size, partial);
    let three = builder.imm(3);
    let shift = builder.shl(three, gap);
    let src_tail_ptr = builder.add(src, full);
    let src_word = builder.mload(src_tail_ptr);
    let src_shifted = builder.shr(shift, src_word);
    let src_top = builder.shl(shift, src_shifted);
    let dest_tail_ptr = builder.add(dest, full);
    let dest_word = builder.mload(dest_tail_ptr);
    let one = builder.imm(1);
    let low_bound = builder.shl(shift, one);
    let low_mask = builder.sub(low_bound, one);
    let dest_low = builder.and(dest_word, low_mask);
    let merged = builder.or(src_top, dest_low);
    builder.mstore(dest_tail_ptr, merged);
    builder.jump(continuation);
}

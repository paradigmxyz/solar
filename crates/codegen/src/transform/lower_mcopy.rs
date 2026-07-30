//! Lower `mcopy` for EVM versions that predate Cancun.

use crate::{
    mir::{BlockId, Function, FunctionBuilder, InstKind, Module, ValueId},
    pass::MirPass,
    transform::utils::redirect_successor_predecessors,
};
use solar_sema::Gcx;

/// Lowers `mcopy` to an overlap-safe memory loop when the target has no
/// `MCOPY` opcode.
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

    let forward_word_cond = func.alloc_block();
    let forward_word_body = func.alloc_block();
    let forward_byte_cond = func.alloc_block();
    let forward_byte_body = func.alloc_block();
    let backward_word_cond = func.alloc_block();
    let backward_word_body = func.alloc_block();
    let backward_byte_cond = func.alloc_block();
    let backward_byte_body = func.alloc_block();

    let mut builder = FunctionBuilder::new(func);
    builder.switch_to_block(block);
    let zero = builder.imm_u64(0);
    let src_end = builder.add(src, len);
    let dest_after_src = builder.gt(dest, src);
    let dest_before_end = builder.lt(dest, src_end);
    let overlaps_after = builder.and(dest_after_src, dest_before_end);
    builder.branch(overlaps_after, backward_word_cond, forward_word_cond);

    builder.switch_to_block(forward_word_cond);
    let forward_offset = builder.phi(vec![(block, zero)]);
    let forward_remaining = builder.sub(len, forward_offset);
    let word_size = builder.imm_u64(32);
    let byte_mask = builder.imm_u64(31);
    let forward_short = builder.lt(forward_remaining, word_size);
    builder.branch(forward_short, forward_byte_cond, forward_word_body);

    builder.switch_to_block(forward_word_body);
    let forward_src = builder.add(src, forward_offset);
    let word = builder.mload(forward_src);
    let forward_dest = builder.add(dest, forward_offset);
    builder.mstore(forward_dest, word);
    let next_forward_offset = builder.add(forward_offset, word_size);
    let forward_word_latch = builder.current_block();
    builder.jump(forward_word_cond);
    builder.add_phi_incoming(forward_offset, forward_word_latch, next_forward_offset);

    builder.switch_to_block(forward_byte_cond);
    let forward_byte_offset = builder.phi(vec![(forward_word_cond, forward_offset)]);
    let has_forward_byte = builder.lt(forward_byte_offset, len);
    builder.branch(has_forward_byte, forward_byte_body, continuation);

    builder.switch_to_block(forward_byte_body);
    let forward_src = builder.add(src, forward_byte_offset);
    let byte = load_byte(&mut builder, forward_src, byte_mask);
    let forward_dest = builder.add(dest, forward_byte_offset);
    builder.mstore8(forward_dest, byte);
    let one = builder.imm_u64(1);
    let next_forward_byte = builder.add(forward_byte_offset, one);
    let forward_byte_latch = builder.current_block();
    builder.jump(forward_byte_cond);
    builder.add_phi_incoming(forward_byte_offset, forward_byte_latch, next_forward_byte);

    builder.switch_to_block(backward_word_cond);
    let backward_remaining = builder.phi(vec![(block, len)]);
    let backward_short = builder.lt(backward_remaining, word_size);
    builder.branch(backward_short, backward_byte_cond, backward_word_body);

    builder.switch_to_block(backward_word_body);
    let next_backward_remaining = builder.sub(backward_remaining, word_size);
    let backward_src = builder.add(src, next_backward_remaining);
    let word = builder.mload(backward_src);
    let backward_dest = builder.add(dest, next_backward_remaining);
    builder.mstore(backward_dest, word);
    let backward_word_latch = builder.current_block();
    builder.jump(backward_word_cond);
    builder.add_phi_incoming(backward_remaining, backward_word_latch, next_backward_remaining);

    builder.switch_to_block(backward_byte_cond);
    let backward_byte_remaining = builder.phi(vec![(backward_word_cond, backward_remaining)]);
    let has_backward_byte = builder.gt(backward_byte_remaining, zero);
    builder.branch(has_backward_byte, backward_byte_body, continuation);

    builder.switch_to_block(backward_byte_body);
    let one = builder.imm_u64(1);
    let next_backward_byte = builder.sub(backward_byte_remaining, one);
    let backward_src = builder.add(src, next_backward_byte);
    let byte = load_byte(&mut builder, backward_src, byte_mask);
    let backward_dest = builder.add(dest, next_backward_byte);
    builder.mstore8(backward_dest, byte);
    let backward_byte_latch = builder.current_block();
    builder.jump(backward_byte_cond);
    builder.add_phi_incoming(backward_byte_remaining, backward_byte_latch, next_backward_byte);
}

fn load_byte(builder: &mut FunctionBuilder<'_>, address: ValueId, byte_mask: ValueId) -> ValueId {
    // Avoid expanding memory past the source byte's containing word.
    let byte_index = builder.and(address, byte_mask);
    let aligned_address = builder.sub(address, byte_index);
    let word = builder.mload(aligned_address);
    builder.byte(byte_index, word)
}

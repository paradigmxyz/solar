//! Lower `mcopy` for EVM versions that predate Cancun.

use crate::{
    memory::EvmMemoryLayout,
    mir::{BlockId, Function, FunctionBuilder, FunctionId, InstId, InstKind, MirType, Module},
    pass::MirPass,
    target::{Cost, Target},
    transform::utils::redirect_successor_predecessors,
};
use solar_interface::{Ident, sym};
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
/// The loop is expanded at every site, or, when the objective ranks the
/// bytes of the copies above the gas of the call protocol, built once as the
/// internal function `mcopy_words(dest, src, len)` that every site calls, like
/// solc's shared `copy_memory_to_memory` routine.
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
        let sites = module
            .functions
            .iter()
            .map(|func| func.instructions().filter(|&inst| is_mcopy(func, inst)).count())
            .sum::<usize>();
        if sites == 0 {
            return false;
        }

        let target = Target::new(gcx);
        let helper = shared_copy_helper(target, sites);
        let helper = helper.map(|function| module.add_function(function));
        for func in module.functions.iter_mut() {
            if !func.blocks.is_empty() {
                lower_function(func, helper);
            }
        }
        true
    }
}

fn is_mcopy(func: &Function, inst: InstId) -> bool {
    matches!(func.inst(inst).kind, InstKind::MCopy(_, _, _))
}

/// Builds the shared copy helper when the objective ranks `sites` calls to it, with the
/// protocol gas they run, above `sites` expanded loops.
fn shared_copy_helper(target: Target, sites: usize) -> Option<Function> {
    if sites < 2 {
        return None;
    }
    let mut function = Function::new(Ident::with_dummy_span(sym::mcopy_words));
    {
        let mut builder = FunctionBuilder::new(&mut function);
        let dest = builder.add_param(MirType::MemPtr);
        let src = builder.add_param(MirType::MemPtr);
        let len = builder.add_param(MirType::uint256());
        let exit = builder.create_block();
        emit_copy_loop(&mut builder, dest, src, len, exit);
        builder.switch_to_block(exit);
        builder.ret([]);
    }
    let params = function.params.len();
    let body = target.code_estimate(&function);
    let sites = u32::try_from(sites).unwrap_or(u32::MAX);
    // The loop itself runs in both shapes; the call protocol is the price of sharing it, and
    // the copies of the loop are the price of expanding it.
    let frame_words =
        EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE / EvmMemoryLayout::WORD_SIZE + params as u64;
    let call = target.internal_call(params, 0, frame_words);
    let ret = target.internal_return(params, 0);
    let shared = Cost::new(0, body.bytes).plus(ret).plus(call.times(sites));
    let expanded = Cost::new(0, body.bytes.saturating_mul(sites));
    target.cmp(shared, expanded).is_lt().then_some(function)
}

fn lower_function(func: &mut Function, helper: Option<FunctionId>) -> bool {
    if let Some(helper) = helper {
        let sites = func.instructions().filter(|&inst| is_mcopy(func, inst)).collect::<Vec<_>>();
        for &inst in &sites {
            call_copy_helper(func, inst, helper);
        }
        return !sites.is_empty();
    }
    // Expanding a copy splits its block at the copy, so the rest of the block, with any later
    // copy, is visited as the continuation.
    let mut changed = false;
    let mut block_index = 0;
    while block_index < func.blocks.len() {
        let block = BlockId::from_usize(block_index);
        let mcopy = func.blocks[block]
            .instructions
            .iter()
            .copied()
            .enumerate()
            .find(|&(_, inst)| is_mcopy(func, inst));
        if let Some((position, inst)) = mcopy {
            lower_mcopy(func, block, position, inst);
            changed = true;
        }
        block_index += 1;
    }
    changed
}

/// Replaces an `mcopy` with a call of the shared helper.
fn call_copy_helper(func: &mut Function, inst: InstId, helper: FunctionId) {
    let InstKind::MCopy(dest, src, len) = func.inst(inst).kind else { unreachable!() };
    // internal_call @mcopy_words, 0, dest, src, len
    func.inst_mut(inst).kind =
        InstKind::InternalCall { function: helper, args: vec![dest, src, len].into(), returns: 0 };
}

fn lower_mcopy(func: &mut Function, block: BlockId, position: usize, inst: InstId) {
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

    let mut builder = FunctionBuilder::new(func);
    builder.switch_to_block(block);
    emit_copy_loop(&mut builder, dest, src, len, continuation);
}

/// Emits the word-copy loop from the builder's current block, continuing at `continuation`.
fn emit_copy_loop(
    builder: &mut FunctionBuilder<'_>,
    dest: crate::mir::ValueId,
    src: crate::mir::ValueId,
    len: crate::mir::ValueId,
    continuation: BlockId,
) {
    let loop_head = builder.create_block();
    let loop_body = builder.create_block();
    let tail_check = builder.create_block();
    let tail_block = builder.create_block();

    // Copy the full words in a loop, then merge the partial tail word with a
    // byte mask so exactly `len` bytes change, like the identity precompile.
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

//! Lower `mcopy` for EVM versions that predate Cancun.

use crate::{
    mir::{
        BlockId, EffectKind, Function, FunctionBuilder, FunctionId, InstId, InstKind, MirType,
        Module, Value, ValueId,
        analysis::{
            AliasAnalysis, AliasResult, CallGraphInfo, LocationSize, MemoryBase, MemoryLocation,
        },
        memory::EvmMemoryLayout,
        pass::MirPass,
        transform::utils::redirect_successor_predecessors,
    },
    target::{Cost, Target},
};
use solar_data_structures::bit_set::DenseBitSet;
use solar_interface::{Ident, sym};
use solar_sema::Gcx;

/// Lowers `mcopy` to overlap-safe word-copy loops when the target has no
/// `MCOPY` opcode.
///
/// The identity precompile would be smaller, but calling it is observable:
/// tooling that keys behavior on "the next call" — Foundry's `vm.prank` and
/// `vm.expectRevert` — consumes the precompile call instead of the intended
/// one, breaking every pre-Cancun test that pranks before an operation
/// involving a memory copy.
///
/// The loop is expanded at every site, or, when the objective ranks the
/// bytes of the copies above the gas of the call protocol, built once as the
/// internal function `mcopy_words(dest, src, len)` that eligible runtime sites
/// call, like solc's shared `copy_memory_to_memory` routine. Constructor-reachable
/// sites remain inline because their ABI output may occupy the free-memory
/// pointer where an internal call would stage its frame. Copies through raw or
/// symbolic memory bases also remain inline because the helper's argument frame
/// could overlap either copied range.
///
/// Copies whose destination starts above their source run backward; all other
/// copies run forward. A masked partial-word merge ensures that the lowering
/// changes exactly `len` bytes. Pointer provenance selects a direction at
/// compile time for disjoint allocations and constant offsets; unknown pointer
/// relationships retain a runtime direction check.
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
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        if gcx.sess.opts.evm_version.has_mcopy() {
            return false;
        }
        let call_graph = CallGraphInfo::new(module);
        let constructor_roots = module
            .functions
            .iter_enumerated()
            .filter_map(|(id, func)| func.attributes.is_constructor.then_some(id));
        let mut constructor_reachable = call_graph.reachable_callees_from(constructor_roots);
        for (id, func) in module.functions.iter_enumerated() {
            if func.attributes.is_constructor {
                constructor_reachable.insert(id);
            }
        }
        let has_runtime_sites = module
            .functions
            .iter_enumerated()
            .filter(|(id, _)| !constructor_reachable.contains(*id))
            .any(|(_, func)| func.instructions().any(|inst| is_mcopy(func, inst)));
        let constructor_sites = module
            .functions
            .iter_enumerated()
            .filter(|(id, _)| constructor_reachable.contains(*id))
            .any(|(_, func)| func.instructions().any(|inst| is_mcopy(func, inst)));
        if !has_runtime_sites && !constructor_sites {
            return false;
        }

        let target = Target::new(gcx);
        let fresh_returns = super::lower_abi_encode::fresh_object_returning_functions(module);
        let summaries = analyses.call_summaries(module);
        let helper_sites = module
            .functions
            .iter_enumerated()
            .filter(|(id, _)| !constructor_reachable.contains(*id))
            .map(|(_, func)| {
                let alias = AliasAnalysis::with_call_summaries(func, summaries.clone());
                func.instructions()
                    .filter(|&inst| copy_helper_eligible(func, &alias, &fresh_returns, inst))
                    .count()
            })
            .sum::<usize>();
        let helper = shared_copy_helper(target, helper_sites);
        let helper = helper.map(|function| module.add_function(function));
        for (func_id, func) in module.functions.iter_mut_enumerated() {
            if !func.blocks.is_empty() {
                let constructor_reachable = func_id.index() < constructor_reachable.domain_size()
                    && constructor_reachable.contains(func_id);
                let helper = (!constructor_reachable).then_some(helper).flatten();
                lower_function(func, helper, &fresh_returns, &summaries);
            }
        }
        CallGraphInfo::assert_runtime_helpers(module, helper);
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
        emit_copy_loop(&mut builder, dest, src, len, exit, CopyDirection::Dynamic);
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
    let call = target.icall(params, 0, frame_words);
    let ret = target.internal_return(params, 0);
    let shared = Cost::new(0, body.bytes).plus(ret).plus(call.times(sites));
    let expanded = Cost::new(0, body.bytes.saturating_mul(sites));
    target.cmp(shared, expanded).is_lt().then_some(function)
}

fn lower_function(
    func: &mut Function,
    helper: Option<FunctionId>,
    fresh_returns: &DenseBitSet<FunctionId>,
    summaries: &std::sync::Arc<crate::mir::analysis::MemoryCallSummaries>,
) -> bool {
    let alias = AliasAnalysis::with_call_summaries(func, summaries.clone());
    let directions = func
        .instructions()
        .filter_map(|inst| {
            let InstKind::MCopy(dest, src, len) = func.inst(inst).kind else { return None };
            Some((inst, copy_direction(func, &alias, fresh_returns, dest, src, len)))
        })
        .collect::<solar_data_structures::map::FxHashMap<_, _>>();
    let helper_sites = helper.map(|helper| {
        directions
            .keys()
            .copied()
            .filter(|&inst| copy_helper_eligible(func, &alias, fresh_returns, inst))
            .map(|inst| (inst, helper))
            .collect::<solar_data_structures::map::FxHashMap<_, _>>()
    });
    if let Some(helper_sites) = &helper_sites {
        for (&inst, &helper) in helper_sites {
            call_copy_helper(func, inst, helper);
        }
    }

    // Expanding a copy splits its block at the copy, so the rest of the block,
    // with any later copy, is visited as the continuation.
    let mut changed = false;
    let mut block_index = 0;
    while block_index < func.blocks.len() {
        let block = BlockId::from_usize(block_index);
        let mcopy =
            func.blocks[block].instructions.iter().copied().enumerate().find(|&(_, inst)| {
                is_mcopy(func, inst)
                    && helper_sites.as_ref().is_none_or(|sites| !sites.contains_key(&inst))
            });
        if let Some((position, inst)) = mcopy {
            lower_mcopy(func, block, position, inst, directions[&inst]);
            changed = true;
        }
        block_index += 1;
    }
    changed
}

/// Returns whether a helper call frame is provably disjoint from both copy ranges.
fn copy_helper_eligible(
    func: &Function,
    alias: &AliasAnalysis,
    fresh_returns: &DenseBitSet<FunctionId>,
    inst: InstId,
) -> bool {
    let InstKind::MCopy(dest, src, _) = func.inst(inst).kind else { return false };
    [dest, src].into_iter().all(|pointer| {
        alias
            .memory_address(func, pointer)
            .is_some_and(|address| helper_owned_base(func, address.base, fresh_returns))
    })
}

/// Returns whether a base belongs to compiler-managed memory outside a callee's frame.
fn helper_owned_base(
    func: &Function,
    base: MemoryBase,
    fresh_returns: &DenseBitSet<FunctionId>,
) -> bool {
    match base {
        MemoryBase::InternalFrame
        | MemoryBase::Allocation(_)
        | MemoryBase::DynamicAllocation(_) => true,
        MemoryBase::Value(value) => {
            let Value::Inst(inst) = func.value(value) else { return false };
            matches!(
                func.inst(*inst).kind,
                InstKind::ICall { function, returns: 1, .. }
                    if fresh_returns.contains(function)
            )
        }
        MemoryBase::Absolute => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CopyDirection {
    Forward,
    Reverse,
    Dynamic,
}

/// Selects a copy direction from pointer provenance when possible.
fn copy_direction(
    func: &Function,
    alias: &AliasAnalysis,
    fresh_returns: &DenseBitSet<FunctionId>,
    dest: ValueId,
    src: ValueId,
    len: ValueId,
) -> CopyDirection {
    let Some(dest) = alias.memory_address(func, dest) else { return CopyDirection::Dynamic };
    let Some(src) = alias.memory_address(func, src) else { return CopyDirection::Dynamic };
    let dest_location = MemoryLocation::new(dest, LocationSize::Dynamic(len));
    let src_location = MemoryLocation::new(src, LocationSize::Dynamic(len));
    if alias.memory_alias(dest_location, src_location) == AliasResult::NoAlias {
        return CopyDirection::Forward;
    }
    let dest_fresh = fresh_base(func, dest.base, fresh_returns);
    let src_fresh = fresh_base(func, src.base, fresh_returns);
    if dest_fresh.is_some() && src_fresh.is_some() && dest_fresh != src_fresh {
        return CopyDirection::Forward;
    }
    if dest.base == src.base {
        if dest.offset > src.offset { CopyDirection::Reverse } else { CopyDirection::Forward }
    } else {
        CopyDirection::Dynamic
    }
}

/// Returns the instruction that created a fresh memory base.
fn fresh_base(
    func: &Function,
    base: MemoryBase,
    fresh_returns: &DenseBitSet<FunctionId>,
) -> Option<InstId> {
    match base {
        MemoryBase::Allocation(inst) | MemoryBase::DynamicAllocation(inst) => Some(inst),
        MemoryBase::Value(value) => fresh_value_base(func, value, fresh_returns, 0),
        MemoryBase::Absolute | MemoryBase::InternalFrame => None,
    }
}

/// Traces constant and dynamic offsets back to one fresh allocation.
fn fresh_value_base(
    func: &Function,
    value: ValueId,
    fresh_returns: &DenseBitSet<FunctionId>,
    depth: usize,
) -> Option<InstId> {
    if depth > 8 {
        return None;
    }
    let Value::Inst(inst) = func.value(value) else { return None };
    match func.inst(*inst).kind {
        InstKind::Alloc { .. } => Some(*inst),
        InstKind::ICall { function, returns: 1, .. } if fresh_returns.contains(function) => {
            Some(*inst)
        }
        InstKind::Add(first, second) => {
            let first = fresh_value_base(func, first, fresh_returns, depth + 1);
            let second = fresh_value_base(func, second, fresh_returns, depth + 1);
            match (first, second) {
                (Some(first), None) | (None, Some(first)) => Some(first),
                _ => None,
            }
        }
        InstKind::Sub(base, offset)
            if fresh_value_base(func, offset, fresh_returns, depth + 1).is_none() =>
        {
            fresh_value_base(func, base, fresh_returns, depth + 1)
        }
        InstKind::MLoad(address)
            if func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT)
                && func.inst(*inst).metadata.effect() == Some(EffectKind::MemoryWrite) =>
        {
            Some(*inst)
        }
        _ => None,
    }
}

/// Replaces an `mcopy` with a call of the shared helper.
fn call_copy_helper(func: &mut Function, inst: InstId, helper: FunctionId) {
    let InstKind::MCopy(dest, src, len) = func.inst(inst).kind else { unreachable!() };
    // icall @mcopy_words, 0, dest, src, len
    func.inst_mut(inst).kind =
        InstKind::ICall { function: helper, args: vec![dest, src, len].into(), returns: 0 };
}

fn lower_mcopy(
    func: &mut Function,
    block: BlockId,
    position: usize,
    inst: InstId,
    direction: CopyDirection,
) {
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

    let mut builder = FunctionBuilder::new(func);
    builder.switch_to_block(block);
    emit_copy_loop(&mut builder, dest, src, len, continuation, direction);
}

/// Emits the word-copy loop from the builder's current block, continuing at `continuation`.
fn emit_copy_loop(
    builder: &mut FunctionBuilder<'_>,
    dest: ValueId,
    src: ValueId,
    len: ValueId,
    continuation: BlockId,
    direction: CopyDirection,
) {
    let copy = builder.create_block();

    // empty = len == 0
    // branch empty, continuation, copy
    let empty = builder.iszero(len);
    builder.branch(empty, continuation, copy);
    builder.switch_to_block(copy);

    match direction {
        CopyDirection::Forward => emit_forward_copy(builder, dest, src, len, continuation, copy),
        CopyDirection::Reverse => emit_reverse_copy(builder, dest, src, len, continuation),
        CopyDirection::Dynamic => emit_dynamic_copy(builder, dest, src, len, continuation),
    }
}

/// Emits an ascending copy after overlap analysis has proved it safe.
fn emit_forward_copy(
    builder: &mut FunctionBuilder<'_>,
    dest: ValueId,
    src: ValueId,
    len: ValueId,
    continuation: BlockId,
    entry: BlockId,
) {
    let forward_head = builder.create_block();
    let forward_body = builder.create_block();
    let forward_tail_check = builder.create_block();
    let partial_block = builder.create_block();

    // full = len & ~31
    // jump forward_head
    let zero = builder.imm(0);
    let word_size = builder.imm(32);
    let thirty_one = builder.imm(31);
    let not_thirty_one = builder.not(thirty_one);
    let full = builder.and(len, not_thirty_one);
    builder.jump(forward_head);

    // forward_offset = phi(entry: 0, forward_body: forward_next)
    // remaining = forward_offset < full
    // branch remaining, forward_body, forward_tail_check
    builder.switch_to_block(forward_head);
    let forward_offset = builder.phi(vec![(entry, zero)]);
    let remaining = builder.lt(forward_offset, full);
    builder.branch(remaining, forward_body, forward_tail_check);

    // word = mload(src + forward_offset)
    // mstore(dest + forward_offset, word)
    // forward_next = forward_offset + 32
    // jump forward_head
    builder.switch_to_block(forward_body);
    let src_ptr = builder.add(src, forward_offset);
    let word = builder.mload(src_ptr);
    let dest_ptr = builder.add(dest, forward_offset);
    builder.mstore(dest_ptr, word);
    let forward_next = builder.add(forward_offset, word_size);
    builder.add_phi_incoming(forward_offset, forward_body, forward_next);
    builder.jump(forward_head);

    // has_partial = full < len
    // branch has_partial, partial_block, continuation
    builder.switch_to_block(forward_tail_check);
    let has_partial = builder.lt(full, len);
    builder.branch(has_partial, partial_block, continuation);

    builder.switch_to_block(partial_block);
    emit_partial_copy(builder, dest, src, len, full, continuation);
}

/// Emits a descending copy for an upward-overlapping range.
fn emit_reverse_copy(
    builder: &mut FunctionBuilder<'_>,
    dest: ValueId,
    src: ValueId,
    len: ValueId,
    continuation: BlockId,
) {
    let partial_check = builder.create_block();
    let reverse_head = builder.create_block();
    let reverse_body = builder.create_block();
    let partial_block = builder.create_block();

    // full = len & ~31
    // has_partial = full < len
    // branch has_partial, partial_block, partial_check
    let word_size = builder.imm(32);
    let thirty_one = builder.imm(31);
    let not_thirty_one = builder.not(thirty_one);
    let full = builder.and(len, not_thirty_one);
    let has_partial = builder.lt(full, len);
    builder.branch(has_partial, partial_block, partial_check);

    // jump reverse_head
    builder.switch_to_block(partial_check);
    builder.jump(reverse_head);

    builder.switch_to_block(partial_block);
    emit_partial_copy(builder, dest, src, len, full, reverse_head);

    // reverse_offset = phi(partial_check: full, partial_block: full, reverse_body: reverse_next)
    // done = reverse_offset == 0
    // branch done, continuation, reverse_body
    builder.switch_to_block(reverse_head);
    let reverse_offset = builder.phi(vec![(partial_check, full), (partial_block, full)]);
    let done = builder.iszero(reverse_offset);
    builder.branch(done, continuation, reverse_body);

    // reverse_next = reverse_offset - 32
    // word = mload(src + reverse_next)
    // mstore(dest + reverse_next, word)
    // jump reverse_head
    builder.switch_to_block(reverse_body);
    let reverse_next = builder.sub(reverse_offset, word_size);
    let src_ptr = builder.add(src, reverse_next);
    let word = builder.mload(src_ptr);
    let dest_ptr = builder.add(dest, reverse_next);
    builder.mstore(dest_ptr, word);
    builder.add_phi_incoming(reverse_offset, reverse_body, reverse_next);
    builder.jump(reverse_head);
}

/// Emits a runtime direction check when pointer provenance is inconclusive.
fn emit_dynamic_copy(
    builder: &mut FunctionBuilder<'_>,
    dest: ValueId,
    src: ValueId,
    len: ValueId,
    continuation: BlockId,
) {
    let forward = builder.create_block();
    let reverse = builder.create_block();

    // copy_backward = src < dest
    // branch copy_backward, reverse, forward
    let copy_backward = builder.lt(src, dest);
    builder.branch(copy_backward, reverse, forward);

    builder.switch_to_block(forward);
    emit_forward_copy(builder, dest, src, len, continuation, forward);

    builder.switch_to_block(reverse);
    emit_reverse_copy(builder, dest, src, len, continuation);
}

/// Emits an exact masked copy of the final partial word.
fn emit_partial_copy(
    builder: &mut FunctionBuilder<'_>,
    dest: ValueId,
    src: ValueId,
    len: ValueId,
    partial_offset: ValueId,
    continuation: BlockId,
) {
    // partial = len & 31
    // shift = (32 - (len & 31)) << 3
    // source_top = (mload(src + partial_offset) >> shift) << shift
    // destination_low = mload(dest + partial_offset) & ((1 << shift) - 1)
    // mstore(dest + partial_offset, source_top | destination_low)
    // jump continuation
    let word_size = builder.imm(32);
    let thirty_one = builder.imm(31);
    let partial = builder.and(len, thirty_one);
    let gap = builder.sub(word_size, partial);
    let three = builder.imm(3);
    let shift = builder.shl(three, gap);
    let src_tail_ptr = builder.add(src, partial_offset);
    let src_word = builder.mload(src_tail_ptr);
    let src_shifted = builder.shr(shift, src_word);
    let src_top = builder.shl(shift, src_shifted);
    let dest_tail_ptr = builder.add(dest, partial_offset);
    let dest_word = builder.mload(dest_tail_ptr);
    let one = builder.imm(1);
    let low_bound = builder.shl(shift, one);
    let low_mask = builder.sub(low_bound, one);
    let dest_low = builder.and(dest_word, low_mask);
    let merged = builder.or(src_top, dest_low);
    builder.mstore(dest_tail_ptr, merged);
    builder.jump(continuation);
}

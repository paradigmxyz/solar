//! Lower `mcopy` for EVM versions that predate Cancun.
//!
//! Each copy becomes word-copy loops, since the target has no `MCOPY` opcode.
//! The identity precompile would be smaller, but calling it is observable:
//! tooling that keys behavior on "the next call" — Foundry's `vm.prank` and
//! `vm.expectRevert` — consumes the precompile call instead of the intended
//! one, breaking every pre-Cancun test that pranks before an operation
//! involving a memory copy.
//!
//! The loop is expanded at every site, or built once per loop shape (direction
//! and whole-word length) as an internal function `mcopy_words(dest, src, len)`
//! that eligible runtime sites of that shape call, like solc's shared
//! `copy_memory_to_memory` routine. A helper is built when the objective ranks
//! the calls above the expanded loops: bytes first in size mode, and in gas mode
//! the lifetime cost of the call protocol's gas over the optimizer runs against
//! the deposit of the repeated bytes. Constructor-reachable sites remain inline
//! because their ABI output may occupy the free-memory pointer where an internal
//! call would stage its frame. Copies through raw or symbolic memory bases also
//! remain inline because the helper's argument frame could overlap either copied
//! range, unless the copy is marked `disjoint`: ABI encoding copies from heap
//! objects into heap output, clear of every frame.
//!
//! Copies marked `disjoint`, such as ABI encoding's copies of source data into
//! its output, run forward. Otherwise pointer provenance picks the direction at
//! compile time: backward when the destination starts above the source in the
//! same base, forward for disjoint allocations. Unknown relationships keep a
//! runtime direction check. A masked partial-word merge ensures that the
//! lowering changes exactly `len` bytes; a length that is provably a multiple
//! of 32, such as a word array's `len << 5`, copies whole words and needs no
//! merge.
//!
//! The pass runs before `lower-alloc`, while allocations are still symbolic
//! and provenance can tell them apart.

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
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
use solar_interface::{Ident, sym};
use solar_sema::Gcx;

/// Lowers `mcopy` to overlap-safe word-copy loops without an `MCOPY` opcode.
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
        let is_runtime = |id: FunctionId| {
            id.index() >= constructor_reachable.domain_size() || !constructor_reachable.contains(id)
        };
        let sites = module
            .functions
            .iter_enumerated()
            .map(|(id, func)| copy_sites(func, is_runtime(id), &fresh_returns, &summaries))
            .collect::<IndexVec<FunctionId, _>>();

        // One helper per copy shape, when its sites are worth sharing.
        let mut counts = FxHashMap::<CopyShape, u32>::default();
        for site in sites.iter().flatten().filter(|site| site.shareable) {
            *counts.entry(site.shape).or_default() += 1;
        }
        let mut shapes = counts.into_iter().collect::<Vec<_>>();
        shapes.sort_by_key(|&(shape, _)| shape);
        let mut helpers = FxHashMap::default();
        for (shape, count) in shapes {
            if let Some(helper) = shared_copy_helper(target, shape, count) {
                helpers.insert(shape, module.add_function(helper));
            }
        }

        for (func_id, sites) in sites.into_iter_enumerated() {
            lower_function(module.function_mut(func_id), &sites, &helpers);
        }
        CallGraphInfo::assert_runtime_helpers(module, helpers.into_values());
        true
    }
}

fn is_mcopy(func: &Function, inst: InstId) -> bool {
    matches!(func.inst(inst).kind, InstKind::MCopy(_, _, _))
}

/// The loop an `mcopy` expands to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct CopyShape {
    direction: CopyDirection,
    whole_words: bool,
}

/// One `mcopy` and the loop it expands to.
struct CopySite {
    inst: InstId,
    shape: CopyShape,
    /// Whether a runtime helper call can replace the expansion.
    shareable: bool,
}

/// Finds the copies of a function, with the loop shape each expands to.
fn copy_sites(
    func: &Function,
    runtime: bool,
    fresh_returns: &DenseBitSet<FunctionId>,
    summaries: &std::sync::Arc<crate::mir::analysis::MemoryCallSummaries>,
) -> Vec<CopySite> {
    if func.blocks.is_empty() || !func.instructions().any(|inst| is_mcopy(func, inst)) {
        return Vec::new();
    }
    let alias = AliasAnalysis::with_call_summaries(func, summaries.clone());
    func.instructions()
        .filter_map(|inst| {
            let InstKind::MCopy(dest, src, len) = func.inst(inst).kind else { return None };
            let disjoint = func.inst(inst).metadata.disjoint();
            let direction = if disjoint {
                CopyDirection::Forward
            } else {
                copy_direction(func, &alias, fresh_returns, dest, src, len)
            };
            let shape = CopyShape { direction, whole_words: is_whole_words(func, len, 0) };
            let shareable = runtime && (disjoint || copy_helper_eligible(func, &alias, inst));
            Some(CopySite { inst, shape, shareable })
        })
        .collect()
}

/// Builds the helper for `sites` copies of one shape when the objective ranks
/// calling it above expanding the loop at every site.
fn shared_copy_helper(target: Target, shape: CopyShape, sites: u32) -> Option<Function> {
    if sites < 2 {
        return None;
    }
    // fn @mcopy_words(dest, src, len) { copy_loop<shape>(dest, src, len); ret }
    let mut function = Function::new(Ident::with_dummy_span(sym::mcopy_words));
    {
        let mut builder = FunctionBuilder::new(&mut function);
        let dest = builder.add_param(MirType::I256);
        let src = builder.add_param(MirType::I256);
        let len = builder.add_param(MirType::I256);
        let exit = builder.create_block();
        emit_copy_loop(&mut builder, dest, src, len, exit, shape);
        builder.switch_to_block(exit);
        builder.ret([]);
    }
    let params = function.params.len();
    let body = target.code_estimate(&function);
    // The loop itself runs in both shapes. Sharing it costs a call and a return at every
    // execution and a call at every site; expanding it repeats its bytes at every site.
    let frame_words =
        EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE / EvmMemoryLayout::WORD_SIZE + params as u64;
    let call = target.icall(params, 0, frame_words);
    let ret = target.internal_return(params, 0);
    let shared = Cost::new(
        call.gas.saturating_add(ret.gas).saturating_mul(sites),
        body.bytes.saturating_add(ret.bytes).saturating_add(call.bytes.saturating_mul(sites)),
    );
    let expanded = Cost::new(0, body.bytes.saturating_mul(sites));
    let shares = if target.optimization().is_gas() {
        target.lifetime_gas(shared) < target.lifetime_gas(expanded)
    } else {
        target.cmp(shared, expanded).is_lt()
    };
    shares.then_some(function)
}

fn lower_function(
    func: &mut Function,
    sites: &[CopySite],
    helpers: &FxHashMap<CopyShape, FunctionId>,
) {
    let mut shapes = FxHashMap::default();
    for site in sites {
        match helpers.get(&site.shape) {
            Some(&helper) if site.shareable => call_copy_helper(func, site.inst, helper),
            _ => {
                shapes.insert(site.inst, site.shape);
            }
        }
    }

    // Expanding a copy splits its block at the copy, so the rest of the block,
    // with any later copy, is visited as the continuation.
    let mut block_index = 0;
    while block_index < func.blocks.len() {
        let block = BlockId::from_usize(block_index);
        let mcopy = func.blocks[block]
            .instructions
            .iter()
            .copied()
            .enumerate()
            .find(|&(_, inst)| shapes.contains_key(&inst));
        if let Some((position, inst)) = mcopy {
            lower_mcopy(func, block, position, inst, shapes[&inst]);
        }
        block_index += 1;
    }
}

/// Returns whether a helper call frame is provably disjoint from both copy ranges.
fn copy_helper_eligible(func: &Function, alias: &AliasAnalysis, inst: InstId) -> bool {
    let InstKind::MCopy(dest, src, _) = func.inst(inst).kind else { return false };
    [dest, src].into_iter().all(|pointer| {
        alias.memory_address(func, pointer).is_some_and(|address| helper_owned_base(address.base))
    })
}

/// Returns whether a base belongs to compiler-managed memory outside a callee's frame.
fn helper_owned_base(base: MemoryBase) -> bool {
    matches!(
        base,
        MemoryBase::InternalFrame | MemoryBase::Allocation(_) | MemoryBase::DynamicAllocation(_)
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
        InstKind::ICall { function: crate::mir::Callee::Function(function), .. }
            if fresh_returns.contains(function) =>
        {
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
    let instruction = func.inst_mut(inst);
    instruction.kind = InstKind::ICall {
        function: crate::mir::Callee::Function(helper),
        args: vec![dest, src, len].into(),
    };
    instruction.metadata.set_disjoint(false);
}

fn lower_mcopy(
    func: &mut Function,
    block: BlockId,
    position: usize,
    inst: InstId,
    shape: CopyShape,
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
    emit_copy_loop(&mut builder, dest, src, len, continuation, shape);
}

/// Emits the word-copy loop from the builder's current block, continuing at `continuation`.
fn emit_copy_loop(
    builder: &mut FunctionBuilder<'_>,
    dest: ValueId,
    src: ValueId,
    len: ValueId,
    continuation: BlockId,
    shape: CopyShape,
) {
    let copy = builder.create_block();

    // empty = len == 0
    // branch empty, continuation, copy
    let empty = builder.eq_zero(len);
    builder.branch(empty, continuation, copy);
    builder.switch_to_block(copy);

    // full = len & ~31, or len when it is whole words
    let full = if shape.whole_words {
        len
    } else {
        let thirty_one = builder.imm(31);
        let not_thirty_one = builder.not(thirty_one);
        builder.and(len, not_thirty_one)
    };
    let copy = WordCopy { dest, src, len, full };
    match shape.direction {
        CopyDirection::Forward => emit_forward_copy(builder, copy, continuation),
        CopyDirection::Reverse => emit_reverse_copy(builder, copy, continuation),
        CopyDirection::Dynamic => emit_dynamic_copy(builder, copy, continuation),
    }
}

/// The operands of one expanded copy, with `full` the length of its whole words.
#[derive(Clone, Copy)]
struct WordCopy {
    dest: ValueId,
    src: ValueId,
    len: ValueId,
    full: ValueId,
}

impl WordCopy {
    /// Returns whether a partial word follows the whole words.
    fn has_partial_word(self) -> bool {
        self.full != self.len
    }
}

/// Returns whether `len` is provably a multiple of 32.
fn is_whole_words(func: &Function, len: ValueId, depth: usize) -> bool {
    if let Some(len) = func.value_u256(len) {
        return len.as_limbs()[0] % 32 == 0;
    }
    let Value::Inst(inst) = func.value(len) else { return false };
    if depth >= 4 {
        return false;
    }
    let whole = |value| is_whole_words(func, value, depth + 1);
    match func.inst(*inst).kind {
        InstKind::Shl(shift, _) => func.value_u64(shift).is_some_and(|shift| shift >= 5),
        InstKind::Mul(a, b) | InstKind::And(a, b) => whole(a) || whole(b),
        InstKind::Add(a, b) | InstKind::Sub(a, b) => whole(a) && whole(b),
        _ => false,
    }
}

/// Emits an ascending copy after overlap analysis has proved it safe.
fn emit_forward_copy(builder: &mut FunctionBuilder<'_>, copy: WordCopy, continuation: BlockId) {
    let WordCopy { dest, src, len, full } = copy;
    let entry = builder.current_block();
    let forward_head = builder.create_block();
    let forward_body = builder.create_block();

    // jump forward_head
    let zero = builder.imm(0);
    let word_size = builder.imm(32);
    builder.jump(forward_head);

    // forward_offset = phi(entry: 0, forward_body: forward_next)
    // remaining = forward_offset < full
    // branch remaining, forward_body, exit
    builder.switch_to_block(forward_head);
    let forward_offset = builder.phi(vec![(entry, zero)]);
    let remaining = builder.lt(forward_offset, full);
    let exit = if copy.has_partial_word() { builder.create_block() } else { continuation };
    builder.branch(remaining, forward_body, exit);

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
    if !copy.has_partial_word() {
        return;
    }

    // exit: has_partial = full < len
    // branch has_partial, partial_block, continuation
    builder.switch_to_block(exit);
    let partial_block = builder.create_block();
    let has_partial = builder.lt(full, len);
    builder.branch(has_partial, partial_block, continuation);

    builder.switch_to_block(partial_block);
    emit_partial_copy(builder, dest, src, len, full, continuation);
}

/// Emits a descending copy for an upward-overlapping range.
fn emit_reverse_copy(builder: &mut FunctionBuilder<'_>, copy: WordCopy, continuation: BlockId) {
    let WordCopy { dest, src, len, full } = copy;
    let reverse_head = builder.create_block();
    let reverse_body = builder.create_block();
    let word_size = builder.imm(32);

    let incoming = if copy.has_partial_word() {
        let partial_check = builder.create_block();
        let partial_block = builder.create_block();

        // has_partial = full < len
        // branch has_partial, partial_block, partial_check
        let has_partial = builder.lt(full, len);
        builder.branch(has_partial, partial_block, partial_check);

        // jump reverse_head
        builder.switch_to_block(partial_check);
        builder.jump(reverse_head);

        builder.switch_to_block(partial_block);
        emit_partial_copy(builder, dest, src, len, full, reverse_head);
        vec![(partial_check, full), (partial_block, full)]
    } else {
        // jump reverse_head
        let entry = builder.current_block();
        builder.jump(reverse_head);
        vec![(entry, full)]
    };

    // reverse_offset = phi(incoming..., reverse_body: reverse_next)
    // done = reverse_offset == 0
    // branch done, continuation, reverse_body
    builder.switch_to_block(reverse_head);
    let reverse_offset = builder.phi(incoming);
    let done = builder.eq_zero(reverse_offset);
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
fn emit_dynamic_copy(builder: &mut FunctionBuilder<'_>, copy: WordCopy, continuation: BlockId) {
    let forward = builder.create_block();
    let reverse = builder.create_block();

    // copy_backward = src < dest
    // branch copy_backward, reverse, forward
    let copy_backward = builder.lt(copy.src, copy.dest);
    builder.branch(copy_backward, reverse, forward);

    builder.switch_to_block(forward);
    emit_forward_copy(builder, copy, continuation);

    builder.switch_to_block(reverse);
    emit_reverse_copy(builder, copy, continuation);
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

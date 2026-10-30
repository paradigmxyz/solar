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
//! that eligible runtime sites call. A site may call the helper of a more
//! general shape, whose loop checks the direction at runtime or merges a partial
//! word, so copies of different shapes can share one loop. The pass prices every
//! set of helpers and keeps the cheapest under the objective: bytes first in size
//! mode, and in gas mode the lifetime cost of the call protocol's and the more
//! general loops' gas over the optimizer runs against the deposit of the
//! repeated bytes.
//! Constructor-reachable sites remain inline because their ABI output may
//! occupy the free-memory pointer where an internal call would stage its frame.
//! Copies between Solidity memory objects, whatever their bases, may call the helper: its frame
//! lies below the initial free memory pointer, which only assembly that names constant addresses
//! reaches (CODEGEN-009). Pointers computed from constants or calldata alone, and ranges that
//! leave the scratch space, remain inline. Copies marked `disjoint` may call it too: ABI
//! encoding copies from heap objects into heap output, which no frame overlaps.
//!
//! Copies marked `disjoint`, such as ABI encoding's copies of source data into
//! its output, run forward. Constant lengths of at most one word also copy
//! forward because they read the source before their only store. Otherwise provenance
//! picks the direction at compile time: backward when the destination starts
//! above the source in the same base, forward when both complete ranges fit
//! disjoint allocations.
//! Allocation origins alone do not prove disjointness. Unknown relationships keep a
//! runtime direction check. A masked partial-word merge ensures that the
//! lowering changes exactly `len` bytes; a length that is provably a multiple
//! of 32, such as a word array's `len << 5`, copies whole words and needs no
//! merge. A constant one-byte tail uses `MSTORE8` instead of reading and masking
//! the destination word.
//!
//! At `-O none` the pass skips the memory-summary and alias analyses and takes the
//! conservative answers: a runtime direction check for every copy that is not
//! `disjoint` or at most one word, and no helper call for a copy that is not `disjoint`.
//!
//! The pass runs before `lower-alloc`, while allocations are still symbolic
//! and provenance can tell them apart.

use crate::{
    mir::{
        BlockId, EffectKind, Function, FunctionBuilder, FunctionId, InstId, InstKind, Instruction,
        MirModuleTarget, MirType, Module, PtrLocation, Value, ValueId,
        analysis::{AliasAnalysis, AliasResult, MemoryBase, MemoryCallSummaries},
        memory::EvmMemoryLayout,
        pass::MirPass,
        transform::utils::expand_block,
    },
    target::{Cost, Target},
};
use solar_data_structures::{
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use solar_interface::{Ident, sym};
use solar_sema::Gcx;
use std::{collections::BTreeMap, sync::Arc};

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
        if !module.functions.iter().any(|func| func.instructions().any(|inst| is_mcopy(func, inst)))
        {
            return false;
        }

        let constructor_reachable = module.constructor_reachable_functions();
        let target = module.target(gcx);
        let summaries = module.is_optimized().then(|| analyses.call_summaries(module));
        let sites = module
            .functions
            .iter_enumerated()
            .map(|(id, func)| {
                let summaries = summaries.as_ref().filter(|_| !module.objective(id).is_none());
                copy_sites(func, !constructor_reachable.contains(id), summaries)
            })
            .collect::<IndexVec<FunctionId, _>>();

        // Sites of each shape call one helper when the objective ranks the shared loops above
        // the expanded ones.
        let mut counts = BTreeMap::<CopyShape, u32>::new();
        for site in sites.iter().flatten().filter(|site| site.shareable) {
            *counts.entry(site.shape).or_default() += 1;
        }
        let mut functions = BTreeMap::new();
        let helpers = choose_copy_helpers(&target, &counts)
            .into_iter()
            .map(|(site, helper)| {
                let function = *functions
                    .entry(helper)
                    .or_insert_with(|| module.add_function(copy_helper(helper)));
                (site, function)
            })
            .collect::<FxHashMap<_, _>>();

        for (func_id, sites) in sites.into_iter_enumerated() {
            lower_function(module.function_mut(func_id), &sites, &helpers);
        }
        module.assert_runtime_helpers(functions.into_values());
        true
    }
}

fn is_mcopy(func: &Function, inst: InstId) -> bool {
    matches!(func.inst(inst).kind, InstKind::Copy(_, _, _, PtrLocation::Memory))
}

/// The loop a `mcopy` expands to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct CopyShape {
    direction: CopyDirection,
    whole_words: bool,
}

impl CopyShape {
    /// Every shape, more specific loops first.
    const ALL: [Self; 6] = [
        Self { direction: CopyDirection::Forward, whole_words: true },
        Self { direction: CopyDirection::Forward, whole_words: false },
        Self { direction: CopyDirection::Reverse, whole_words: true },
        Self { direction: CopyDirection::Reverse, whole_words: false },
        Self { direction: CopyDirection::Dynamic, whole_words: true },
        Self { direction: CopyDirection::Dynamic, whole_words: false },
    ];

    /// Returns whether this shape's loop also performs copies of `site`'s shape.
    fn covers(self, site: Self) -> bool {
        (self.direction == site.direction || self.direction == CopyDirection::Dynamic)
            && (site.whole_words || !self.whole_words)
    }
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
    summaries: Option<&Arc<MemoryCallSummaries>>,
) -> Vec<CopySite> {
    if func.blocks.is_empty() || !func.instructions().any(|inst| is_mcopy(func, inst)) {
        return Vec::new();
    }
    let alias =
        summaries.map(|summaries| AliasAnalysis::with_call_summaries(func, summaries.clone()));
    func.instructions()
        .filter_map(|inst| {
            let InstKind::Copy(dest, src, len, PtrLocation::Memory) = func.inst(inst).kind else {
                return None;
            };
            let disjoint = func.inst(inst).metadata.disjoint();
            let direction = if disjoint {
                CopyDirection::Forward
            } else {
                copy_direction(func, alias.as_ref(), dest, src, len)
            };
            let shape = CopyShape { direction, whole_words: is_whole_words(func, len, 0) };
            // NOTE: a disjoint copy reads a Solidity memory object, which lies outside every
            // static frame unless assembly placed it there; see CODEGEN-009 in
            // `docs/SOLC_DIVERGENCE.md`.
            let shareable =
                runtime && (disjoint || copy_helper_eligible(func, alias.as_ref(), inst));
            Some(CopySite { inst, shape, shareable })
        })
        .collect()
}

/// Builds the helper `mcopy_words(dest, src, len)` that copies with the loop of `shape`.
fn copy_helper(shape: CopyShape) -> Function {
    // fn @mcopy_words(dest, src, len) { copy_loop<shape>(dest, src, len); ret }
    let mut function = Function::new(Ident::with_dummy_span(sym::mcopy_words));
    let mut builder = FunctionBuilder::new(&mut function);
    let dest = builder.add_param(MirType::I256);
    let src = builder.add_param(MirType::I256);
    let len = builder.add_param(MirType::I256);
    let exit = builder.create_block();
    emit_copy_loop(&mut builder, dest, src, len, exit, shape);
    builder.switch_to_block(exit);
    builder.ret([]);
    function
}

/// Maps each site shape in `counts` to the shape of the helper its sites call; unmapped shapes
/// expand at every site.
///
/// A site may call the helper of a more general shape: the runtime direction check covers both
/// directions, and the partial-word merge covers whole-word lengths. Every set of helpers is
/// priced with each site calling the covering helper that adds the least estimated gas over its
/// own loop, and the cheapest set under the objective wins. The loops run in every alternative,
/// so a helper costs its call protocol and its bytes once, and a more general loop costs the
/// difference of the estimates at every execution. The estimate counts the code a call skips,
/// such as the other direction's loop, which overstates that difference.
fn choose_copy_helpers(
    target: &Target,
    counts: &BTreeMap<CopyShape, u32>,
) -> BTreeMap<CopyShape, CopyShape> {
    let bodies = CopyShape::ALL.map(|shape| target.code_estimate(&copy_helper(shape)));
    let index = |shape| CopyShape::ALL.iter().position(|&other| other == shape).unwrap();
    let params = 3;
    let mut best_cost = counts
        .iter()
        .map(|(&site, &count)| Cost::new(0, bodies[index(site)].bytes).times(count))
        .sum::<Cost>();
    let mut best = BTreeMap::new();
    for set in 1u32..1 << CopyShape::ALL.len() {
        let in_set = |helper: usize| set & (1 << helper) != 0;
        let mut users = [0; CopyShape::ALL.len()];
        let mut cost = Cost::ZERO;
        let mut choice = BTreeMap::new();
        for (&site, &count) in counts {
            let own = bodies[index(site)];
            let helper = (0..CopyShape::ALL.len())
                .filter(|&helper| in_set(helper) && CopyShape::ALL[helper].covers(site))
                .min_by_key(|&helper| target.runtime_gas(bodies[helper]));
            if let Some(helper) = helper {
                users[helper] += count;
                cost += Cost { bytes: 0, ..bodies[helper].saturating_sub(own) }.times(count);
                choice.insert(site, CopyShape::ALL[helper]);
            } else {
                cost += Cost::new(0, own.bytes).times(count);
            }
        }
        if (0..CopyShape::ALL.len()).any(|helper| in_set(helper) && users[helper] == 0) {
            continue;
        }
        for helper in (0..CopyShape::ALL.len()).filter(|&helper| in_set(helper)) {
            cost += target.helper_sharing(bodies[helper].bytes, params, 0, users[helper]).0;
        }
        if target.cmp_lifetime(cost, best_cost).is_lt() {
            best_cost = cost;
            best = choice;
        }
    }
    best
}

fn lower_function(
    func: &mut Function,
    sites: &[CopySite],
    helpers: &FxHashMap<CopyShape, FunctionId>,
) {
    let mut expanded = FxHashMap::default();
    let mut casts = FxHashMap::default();
    for site in sites {
        match helpers.get(&site.shape) {
            Some(&helper) if site.shareable => {
                casts.insert(site.inst, call_copy_helper(func, site.inst, helper));
            }
            _ => {
                expanded.insert(site.inst, site.shape);
            }
        }
    }
    if !casts.is_empty() {
        let mut instructions = Vec::new();
        for block in func.blocks.iter_mut() {
            if block.instructions.iter().any(|inst| casts.contains_key(inst)) {
                instructions.extend(block.instructions.drain(..).flat_map(|inst| {
                    casts.remove(&inst).unwrap_or_default().into_iter().chain([inst])
                }));
                std::mem::swap(&mut block.instructions, &mut instructions);
            }
        }
    }

    if expanded.is_empty() {
        return;
    }

    // Expanding a copy splits its block at the copy, so the rest of the block,
    // with any later copy, is visited as the continuation.
    let mut builder = FunctionBuilder::new(func);
    let mut block_index = 0;
    while block_index < builder.func().blocks.len() {
        let block = BlockId::from_usize(block_index);
        block_index += 1;
        if !builder.func().blocks[block].instructions.iter().any(|inst| expanded.contains_key(inst))
        {
            continue;
        }
        let mut done = false;
        expand_block(&mut builder, block, |builder, inst| {
            let Some(&shape) = expanded.get(&inst).filter(|_| !done) else { return false };
            done = true;
            lower_mcopy(builder, inst, shape);
            true
        });
    }
}

/// Returns whether a helper call frame does not overlap either copy range.
///
/// Static frames lie between the scratch space and the initial free memory pointer, and only
/// assembly that names constant addresses there can reach them (CODEGEN-009). This pass runs
/// only where `mcopy` is not an opcode, so every copy is the compiler's own, between Solidity
/// memory objects. A pointer qualifies unless constants or calldata alone compute it, or it
/// names a constant range that leaves the scratch space.
///
/// The caller's frame is live, so the helper's frame does not overlap it; and heap objects,
/// call results, and allocations start at or above the initial free memory pointer.
fn copy_helper_eligible(func: &Function, alias: Option<&AliasAnalysis>, inst: InstId) -> bool {
    let Some(alias) = alias else { return false };
    let InstKind::Copy(dest, src, len, PtrLocation::Memory) = func.inst(inst).kind else {
        return false;
    };
    let size = alias.location_size(func, len);
    [dest, src].into_iter().all(|pointer| {
        let Some(location) = alias.bare_memory_location(func, pointer, size) else {
            return false;
        };
        let address = location.address;
        let scratch = address.base == MemoryBase::Absolute
            && size
                .as_const()
                .and_then(|size| address.offset.checked_add(size))
                .is_some_and(|end| end <= EvmMemoryLayout::HEAP_START);
        scratch || !is_constant_address(func, pointer)
    })
}

/// Returns whether a pointer is computed from constants and calldata alone, so that it can name
/// any static address.
///
/// Every pure operation passes constants through, so an operation added later cannot hide a
/// constant. Anything else, such as a load, a call result, an allocation, or a frame address, is
/// not constant.
fn is_constant_address(func: &Function, pointer: ValueId) -> bool {
    let mut pending = vec![pointer];
    let mut seen = FxHashSet::default();
    while let Some(value) = pending.pop() {
        if !seen.insert(value) {
            continue;
        }
        let inst = match func.value(value) {
            Value::Immediate(_) => continue,
            Value::Inst(inst) => func.inst(*inst),
            _ => return false,
        };
        match &inst.kind {
            InstKind::Load(.., PtrLocation::Calldata) | InstKind::CalldataSize => {}
            InstKind::Select(_, lhs, rhs) => pending.extend([*lhs, *rhs]),
            InstKind::InternalFrameAddr(_) => return false,
            kind if kind.op_def().effect == EffectKind::Pure => pending.extend(kind.operands()),
            _ => return false,
        }
    }
    true
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
    alias: Option<&AliasAnalysis>,
    dest: ValueId,
    src: ValueId,
    len: ValueId,
) -> CopyDirection {
    if func.value_u64(len).is_some_and(|len| len <= EvmMemoryLayout::WORD_SIZE) {
        return CopyDirection::Forward;
    }
    let Some(alias) = alias else { return CopyDirection::Dynamic };
    let size = alias.location_size(func, len);
    let Some(dest_location) = alias.bare_memory_location(func, dest, size) else {
        return CopyDirection::Dynamic;
    };
    let Some(src_location) = alias.bare_memory_location(func, src, size) else {
        return CopyDirection::Dynamic;
    };
    if alias.memory_alias(func, dest_location, src_location) == AliasResult::NoAlias {
        return CopyDirection::Forward;
    }
    let Some(dest) = alias.memory_address(func, dest) else { return CopyDirection::Dynamic };
    let Some(src) = alias.memory_address(func, src) else { return CopyDirection::Dynamic };
    if dest.base == src.base {
        if dest.offset > src.offset { CopyDirection::Reverse } else { CopyDirection::Forward }
    } else {
        CopyDirection::Dynamic
    }
}

/// Replaces a `mcopy` with a call of the shared helper and returns the pointer casts that must
/// precede it.
fn call_copy_helper(func: &mut Function, inst: InstId, helper: FunctionId) -> Vec<InstId> {
    let InstKind::Copy(dest, src, len, PtrLocation::Memory) = func.inst(inst).kind else {
        unreachable!()
    };
    // word = ptrtoint pointer to i256, for each pointer address of the word helper
    let mut casts = Vec::new();
    let mut word = |func: &mut Function, value: ValueId| {
        if func.value_ty(value) != Some(MirType::MEMPTR) {
            return value;
        }
        let instruction = Instruction::new(InstKind::PtrToInt(value, 256), Some(MirType::I256));
        let (cast, word) = func.alloc_value_inst(instruction.with_debug_info_dropped());
        casts.push(cast);
        word
    };
    let (dest, src) = (word(func, dest), word(func, src));
    // icall @mcopy_words, 0, dest, src, len
    let instruction = func.inst_mut(inst);
    instruction.kind = InstKind::ICall {
        function: crate::mir::Callee::Function(helper),
        args: vec![dest, src, len].into(),
    };
    instruction.metadata.set_disjoint(false);
    casts
}

/// Emits the word-copy loop of `inst` and leaves the builder in the continuation block.
fn lower_mcopy(builder: &mut FunctionBuilder<'_>, inst: InstId, shape: CopyShape) {
    let InstKind::Copy(dest, src, len, PtrLocation::Memory) = builder.func().inst(inst).kind else {
        unreachable!()
    };

    // continuation: the rest of the block, then the original terminator
    let continuation = builder.create_block();
    emit_copy_loop(builder, dest, src, len, continuation, shape);
    builder.switch_to_block(continuation);
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
    let full = if shape.whole_words { len } else { builder.mask_padded_size(len) };
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
    /// How deep to look through the arithmetic that computes a length.
    const MAX_DEPTH: usize = 4;
    const WORD_BITS: u64 = EvmMemoryLayout::WORD_SIZE.trailing_zeros() as u64;
    if let Some(len) = func.value_u256(len) {
        return len.as_limbs()[0] % EvmMemoryLayout::WORD_SIZE == 0;
    }
    let Value::Inst(inst) = func.value(len) else { return false };
    if depth >= MAX_DEPTH {
        return false;
    }
    let whole = |value| is_whole_words(func, value, depth + 1);
    match func.inst(*inst).kind {
        InstKind::Shl(shift, _) => func.value_u64(shift).is_some_and(|shift| shift >= WORD_BITS),
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
    let word = builder.load(src_ptr, PtrLocation::Memory);
    let dest_ptr = builder.add(dest, forward_offset);
    builder.store(dest_ptr, word);
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
    let word = builder.load(src_ptr, PtrLocation::Memory);
    let dest_ptr = builder.add(dest, reverse_next);
    builder.store(dest_ptr, word);
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

/// Emits an exact copy of the final partial word.
fn emit_partial_copy(
    builder: &mut FunctionBuilder<'_>,
    dest: ValueId,
    src: ValueId,
    len: ValueId,
    partial_offset: ValueId,
    continuation: BlockId,
) {
    if builder
        .func()
        .value_u256(len)
        .is_some_and(|len| len.as_limbs()[0] % EvmMemoryLayout::WORD_SIZE == 1)
    {
        // word = mload(src + partial_offset)
        // mstore8(dest + partial_offset, byte(0, word))
        // jump continuation
        let src_tail_ptr = builder.add(src, partial_offset);
        let word = builder.load(src_tail_ptr, PtrLocation::Memory);
        let zero = builder.imm(0);
        let byte = builder.byte(zero, word);
        let dest_tail_ptr = builder.add(dest, partial_offset);
        builder.store8(dest_tail_ptr, byte);
        builder.jump(continuation);
        return;
    }

    // partial = len & 31
    // shift = (32 - (len & 31)) << 3
    // source = mload(src + partial_offset)
    // destination = mload(dest + partial_offset)
    // changed_top = ((source ^ destination) >> shift) << shift
    // mstore(dest + partial_offset, destination ^ changed_top)
    // jump continuation
    let word_size = builder.imm(32);
    let thirty_one = builder.imm(31);
    let partial = builder.and_folded(len, thirty_one);
    let gap = builder.sub_folded(word_size, partial);
    let three = builder.imm(3);
    let shift = builder.shl_folded(gap, three);
    let src_tail_ptr = builder.add(src, partial_offset);
    let src_word = builder.load(src_tail_ptr, PtrLocation::Memory);
    let dest_tail_ptr = builder.add(dest, partial_offset);
    let dest_word = builder.load(dest_tail_ptr, PtrLocation::Memory);
    let changed = builder.xor(src_word, dest_word);
    let changed_shifted = builder.shr(changed, shift);
    let changed_top = builder.shl(changed_shifted, shift);
    let merged = builder.xor(dest_word, changed_top);
    builder.store(dest_tail_ptr, merged);
    builder.jump(continuation);
}

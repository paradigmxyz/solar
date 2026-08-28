//! Static placement of provably local heap allocations.
//!
//! When a constant-size `alloc` executes at most once per call and its pointer
//! never escapes the function, the allocation does not need a runtime
//! free-memory-pointer bump. This module proves those properties for the EVM
//! backend and retains a conservative MIR rewrite for the explicit
//! `static-alloc` pass.
//!
//! Safety contract:
//! - external entries only: their locals are absolute low-memory addresses;
//! - the block cannot re-execute, so the reused static region cannot expose a previous iteration's
//!   contents where fresh zeroed memory was expected;
//! - every use of the pointer is an in-bounds address derivation into exact loads, stores, hashes,
//!   copies, logs, or external-data terminators; non-recursive internal calls are allowed only when
//!   their interprocedural summaries prove that the pointer is not captured, that the callee never
//!   resets the free-memory pointer or reads `msize`, and that it never relates the pointer to a
//!   value derived from the free-memory pointer, since memory-safe assembly could otherwise derive
//!   aliases from where the object lies relative to the heap;
//! - functions observing `msize` are skipped: eliding a bump changes the high-water mark.
//! - allocations marked as source-visible FMP advances are never placed statically.

use crate::{
    analysis::{AliasAnalysis, CallGraphInfo, CfgInfo, MemoryCallSummaries},
    memory::{EvmMemoryLayout, MemoryLayoutPolicy},
    mir::{
        ArgIdx, BlockId, Function, FunctionId, Immediate, InstId, InstKind, MemoryObjectKind,
        MemoryObjectLayout, Module, Terminator, Value, ValueId,
    },
    pass::{MirPass, ModuleAnalyses},
};
use alloy_primitives::U256;
use solar_data_structures::{
    index::{IndexVec, index_vec},
    map::FxHashMap,
};

/// Pass that places provably local allocations statically.
pub(crate) struct StaticAlloc;

impl MirPass for StaticAlloc {
    fn name(&self) -> &'static str {
        "static-alloc"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut ModuleAnalyses,
    ) -> bool {
        // Every entry's locals share the same low-memory region — only one
        // entry runs per call — so the tallest entry's frame top is a shadow
        // the others can grow into without moving the shared static-frame
        // region or any spill base above it. Placements stay inside it.
        let shadow = module
            .functions
            .iter()
            .filter(|func| is_entry(func))
            .map(|func| {
                EvmMemoryLayout::HEAP_START
                    + func.internal_frame_size.max(func.external_static_return_size)
            })
            .max()
            .unwrap_or(EvmMemoryLayout::HEAP_START);

        let mut changed = false;
        let calls = CallGraphInfo::new(module);
        let summaries = MemoryCallSummaries::new(module);
        for (func_id, func) in module.functions.iter_mut_enumerated() {
            if !is_entry(func) {
                continue;
            }
            changed |= run_on_entry(func_id, func, shadow, &calls, &summaries);
        }
        changed
    }
}

/// Pass that defers eligible allocations until exact backend layout is known.
pub(crate) struct DeferAlloc;

impl MirPass for DeferAlloc {
    fn name(&self) -> &'static str {
        "defer-alloc"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut ModuleAnalyses,
    ) -> bool {
        let calls = CallGraphInfo::new(module);
        let summaries = MemoryCallSummaries::new(module);
        let mut candidates = Vec::new();
        for (func_id, func) in module.functions.iter_enumerated() {
            if !func.instructions().any(|inst_id| {
                matches!(
                    func.inst(inst_id).kind,
                    InstKind::Alloc { semantics: crate::mir::AllocationSemantics::INTERNAL, .. }
                )
            }) {
                continue;
            }
            let eligible = eligible_static_allocations(func_id, func, &calls, &summaries);
            candidates.extend(eligible.into_iter().map(|candidate| (func_id, candidate.alloc)));
        }

        let mut changed = false;
        for (func_id, alloc) in candidates {
            let metadata = &mut module.functions[func_id].inst_mut(alloc).metadata;
            if !metadata.deferred_alloc() {
                metadata.set_deferred_alloc();
                changed = true;
            }
        }
        changed
    }
}

fn is_entry(func: &Function) -> bool {
    !func.attributes.is_constructor
        && (func.selector.is_some() || func.attributes.is_receive || func.attributes.is_fallback)
}

fn fmp_write_has_future_observer(func: &Function, cfg: &CfgInfo, inst_id: InstId) -> bool {
    let Some((block, position)) = func.blocks.iter_enumerated().find_map(|(block, block_data)| {
        block_data
            .instructions
            .iter()
            .position(|&candidate| candidate == inst_id)
            .map(|position| (block, position))
    }) else {
        return true;
    };

    if func.blocks[block].instructions[position + 1..]
        .iter()
        .any(|&inst| instruction_observes_fmp(func, inst))
    {
        return true;
    }
    if cfg.transitive_reachability().get(&block).into_iter().flat_map(|blocks| blocks.iter()).any(
        |block| {
            func.blocks[block]
                .instructions
                .iter()
                .copied()
                .any(|inst| instruction_observes_fmp(func, inst))
                || func.blocks[block]
                    .terminator
                    .as_ref()
                    .is_some_and(|term| matches!(term, Terminator::TailCall { .. }))
        },
    ) {
        return true;
    }

    false
}

fn instruction_observes_fmp(func: &Function, inst_id: InstId) -> bool {
    match func.inst(inst_id).kind {
        InstKind::Alloc { .. }
        | InstKind::Fmp
        | InstKind::SetFmp(_)
        | InstKind::InternalCall { .. } => true,
        InstKind::MLoad(address) => func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT),
        _ => false,
    }
}

fn run_on_entry(
    func_id: FunctionId,
    func: &mut Function,
    shadow: u64,
    calls: &CallGraphInfo,
    summaries: &MemoryCallSummaries,
) -> bool {
    let mut changed = false;
    for cand in eligible_static_allocations(func_id, func, calls, summaries) {
        changed |= apply_candidate(func, &cand, shadow);
    }
    changed
}

/// One constant-size allocation eligible for static placement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StaticAllocCandidate {
    block: BlockId,
    alloc: InstId,
    ptr: ValueId,
    size: u64,
}

/// Returns constant-size, non-escaping allocations that the backend may place
/// in an entry-local static region.
fn eligible_static_allocations(
    func_id: FunctionId,
    func: &Function,
    calls: &CallGraphInfo,
    summaries: &MemoryCallSummaries,
) -> Vec<StaticAllocCandidate> {
    if !is_entry(func) || calls.is_recursive(func_id) {
        return Vec::new();
    }

    let mut has_alloc = false;
    for inst_id in func.instructions() {
        match func.inst(inst_id).kind {
            InstKind::MSize => return Vec::new(),
            InstKind::Alloc { semantics: crate::mir::AllocationSemantics::INTERNAL, .. } => {
                has_alloc = true
            }
            _ => {}
        }
    }
    if !has_alloc {
        return Vec::new();
    }

    let cfg = CfgInfo::new(func);
    // A raw FMP write can move the dynamic heap below a statically placed
    // object. Compiler-owned allocations are still abstract at this point;
    // only direct writes from source lowering can appear here. Keep the
    // placement conservative when a later operation can observe the write.
    let aa = AliasAnalysis::new(func);
    if func.instructions().any(|inst_id| {
        let kind = &func.inst(inst_id).kind;
        let bad = !matches!(kind, InstKind::InternalCall { .. })
            && aa.instruction_may_reset_fmp(func, inst_id);
        bad && fmp_write_has_future_observer(func, &cfg, inst_id)
    }) {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    for block in func.blocks.indices() {
        for &alloc in &func.blocks[block].instructions {
            let InstKind::Alloc { size, semantics, .. } = func.inst(alloc).kind else {
                continue;
            };
            if semantics != crate::mir::AllocationSemantics::INTERNAL
                || func.inst(alloc).metadata.preserves_fmp()
            {
                continue;
            }
            let Some(size) = func.value_u64(size) else { continue };
            if size == 0
                || size > 0x1000
                || !size.is_multiple_of(32)
                || !cfg.is_reachable(block)
                || cfg.cyclic_blocks().contains(block)
            {
                continue;
            }
            let ptr = func.inst_result_value(alloc).expect("allocation must produce a value");
            candidates.push(StaticAllocCandidate { block, alloc, ptr, size });
        }
    }
    if candidates.is_empty() {
        return candidates;
    }
    let uses = ValueUses::new(func);
    // The bounded-use proof rejects every unrecognized use, so it also proves non-escape.
    candidates
        .retain(|candidate| candidate_uses_are_safe(func, candidate, &uses, calls, summaries));
    candidates
}

struct ValueUses {
    instructions: IndexVec<ValueId, Vec<InstId>>,
    terminators: IndexVec<ValueId, Vec<BlockId>>,
}

impl ValueUses {
    fn new(func: &Function) -> Self {
        let mut instructions = index_vec![Vec::new(); func.num_values()];
        let mut terminators = index_vec![Vec::new(); func.num_values()];
        for inst_id in func.instructions() {
            for operand in func.inst(inst_id).operands() {
                instructions[operand].push(inst_id);
            }
        }
        for (block_id, block) in func.blocks.iter_enumerated() {
            if let Some(terminator) = &block.terminator {
                for operand in terminator.operands() {
                    terminators[operand].push(block_id);
                }
            }
        }
        Self { instructions, terminators }
    }
}

/// Verifies every use of the pointer stays in bounds and never escapes.
fn candidate_uses_are_safe(
    func: &Function,
    cand: &StaticAllocCandidate,
    uses: &ValueUses,
    calls: &CallGraphInfo,
    summaries: &MemoryCallSummaries,
) -> bool {
    // In-bounds address derivations from the pointer, to a fixpoint so
    // definition order does not matter.
    let mut derived: FxHashMap<ValueId, u64> = FxHashMap::default();
    derived.insert(cand.ptr, 0);
    let mut pending = vec![cand.ptr];
    while let Some(value) = pending.pop() {
        for &inst_id in &uses.instructions[value] {
            let Some(result) = func.inst_result_value(inst_id) else { continue };
            if derived.contains_key(&result) {
                continue;
            }
            let kind = func.inst(inst_id).kind.clone();
            let offset = match kind {
                InstKind::Add(a, b) => {
                    let (base, offset) = if derived.contains_key(&a) { (a, b) } else { (b, a) };
                    let (Some(base_offset), Some(offset)) =
                        (derived.get(&base).copied(), func.value_u64(offset))
                    else {
                        return false;
                    };
                    base_offset.checked_add(offset)
                }
                InstKind::MemoryObjectData(object, kind) if object == value => derived
                    .get(&value)
                    .and_then(|&base| base.checked_add(EvmMemoryLayout::object_data_offset(kind))),
                InstKind::MemoryObjectFieldAddr { object, layout, field } if object == value => {
                    derived.get(&value).and_then(|&base| {
                        EvmMemoryLayout::field_offset(layout, field)?.checked_add(base)
                    })
                }
                InstKind::MemoryObjectElementAddr { object, layout, index } if object == value => {
                    derived.get(&value).and_then(|&base| {
                        object_element_offset(layout, func.value_u64(index))?.checked_add(base)
                    })
                }
                _ => continue,
            };
            let Some(total) = offset else { return false };
            if total >= cand.size {
                return false;
            }
            derived.insert(result, total);
            pending.push(result);
        }
    }

    // Every use of every derived address must be a bounded memory access.
    let in_range = |off: u64, len: u64| off.checked_add(len).is_some_and(|end| end <= cand.size);
    let in_range_at = |off: u64, offset: u64, len: u64| {
        off.checked_add(offset).is_some_and(|address| in_range(address, len))
    };
    for (&operand, &off) in &derived {
        for &inst_id in &uses.instructions[operand] {
            if inst_id == cand.alloc {
                continue;
            }
            let kind = func.inst(inst_id).kind.clone();
            let ok = match kind.clone() {
                InstKind::MLoad(addr) => operand == addr && in_range(off, 32),
                InstKind::MStore(addr, value) => {
                    operand == addr && !derived.contains_key(&value) && in_range(off, 32)
                }
                InstKind::Keccak256(addr, size)
                | InstKind::Log0(addr, size)
                | InstKind::MemoryZero(addr, size)
                | InstKind::CalldataCopy(addr, _, size)
                | InstKind::ReturnDataCopy(addr, _, size)
                | InstKind::CodeCopy(addr, _, size) => {
                    operand == addr && func.value_u64(size).is_some_and(|len| in_range(off, len))
                }
                InstKind::Log1(addr, size, _)
                | InstKind::Log2(addr, size, _, _)
                | InstKind::Log3(addr, size, _, _, _)
                | InstKind::Log4(addr, size, _, _, _, _) => {
                    operand == addr && func.value_u64(size).is_some_and(|len| in_range(off, len))
                }
                InstKind::MCopy(dest, src, size) => {
                    (operand == dest || operand == src)
                        && func.value_u64(size).is_some_and(|len| in_range(off, len))
                }
                // In-bounds derivations were collected above; anything
                // else consuming an address is an escape.
                InstKind::Add(_, _) => {
                    func.inst_result_value(inst_id).is_some_and(|r| derived.contains_key(&r))
                }
                InstKind::MemoryObjectData(_, _)
                | InstKind::MemoryObjectFieldAddr { .. }
                | InstKind::MemoryObjectElementAddr { .. } => {
                    func.inst_result_value(inst_id).is_some_and(|r| derived.contains_key(&r))
                }
                InstKind::MemoryObjectLen(object, kind) => {
                    operand == object
                        && EvmMemoryLayout::object_length_offset(kind)
                            .is_some_and(|offset| in_range_at(off, offset, 32))
                }
                InstKind::SetMemoryObjectLen(object, len, kind) => {
                    operand == object
                        && !derived.contains_key(&len)
                        && EvmMemoryLayout::object_length_offset(kind)
                            .is_some_and(|offset| in_range_at(off, offset, 32))
                }
                InstKind::MemoryObjectLoadField { object, layout, field } => {
                    operand == object
                        && EvmMemoryLayout::field_offset(layout, field)
                            .is_some_and(|offset| in_range_at(off, offset, 32))
                }
                InstKind::MemoryObjectStoreField { object, layout, field, value } => {
                    operand == object
                        && !derived.contains_key(&value)
                        && EvmMemoryLayout::field_offset(layout, field)
                            .is_some_and(|offset| in_range_at(off, offset, 32))
                }
                InstKind::MemoryObjectLoadElement { object, layout, index } => {
                    operand == object
                        && object_element_offset(layout, func.value_u64(index))
                            .is_some_and(|offset| in_range_at(off, offset, 32))
                }
                InstKind::MemoryObjectLoadByte { object, index } => {
                    operand == object
                        && func.value_u64(index).is_some_and(|index| {
                            object_byte_offset(index)
                                .is_some_and(|offset| in_range_at(off, offset, 1))
                        })
                }
                InstKind::MemoryObjectStoreElement { object, layout, index, value } => {
                    operand == object
                        && !derived.contains_key(&value)
                        && object_element_offset(layout, func.value_u64(index))
                            .is_some_and(|offset| in_range_at(off, offset, 32))
                }
                InstKind::MemoryObjectStoreByte { object, index, value } => {
                    operand == object
                        && !derived.contains_key(&value)
                        && func.value_u64(index).is_some_and(|index| {
                            object_byte_offset(index)
                                .is_some_and(|offset| in_range_at(off, offset, 1))
                        })
                }
                InstKind::MemoryObjectStoreWord { object, offset, value } => {
                    operand == object
                        && !derived.contains_key(&value)
                        && func.value_u64(offset).is_some_and(|offset| {
                            EvmMemoryLayout::object_data_offset(MemoryObjectKind::Bytes)
                                .checked_add(offset)
                                .is_some_and(|offset| in_range_at(off, offset, 32))
                        })
                }
                InstKind::InternalCall { function, args, .. } => {
                    call_use_is_safe(function, &args, operand, calls, summaries)
                }
                _ => false,
            };
            if !ok {
                return false;
            }
        }
        for &block in &uses.terminators[operand] {
            let term = func.blocks[block].terminator.as_ref().expect("indexed terminator use");
            let ok = match term {
                Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                    operand == *offset
                        && func.value_u64(*size).is_some_and(|len| in_range(off, len))
                }
                Terminator::TailCall { function, args } => {
                    call_use_is_safe(*function, args, operand, calls, summaries)
                }
                _ => false,
            };
            if !ok {
                return false;
            }
        }
    }

    true
}

fn object_element_offset(layout: MemoryObjectLayout, index: Option<u64>) -> Option<u64> {
    let index = index?;
    let stride = EvmMemoryLayout::element_stride(layout)?;
    EvmMemoryLayout::object_data_offset(layout.kind()).checked_add(index.checked_mul(stride)?)
}

fn object_byte_offset(index: u64) -> Option<u64> {
    EvmMemoryLayout::object_data_offset(MemoryObjectKind::Bytes).checked_add(index)
}

fn call_use_is_safe(
    function: FunctionId,
    args: &[ValueId],
    operand: ValueId,
    calls: &CallGraphInfo,
    summaries: &MemoryCallSummaries,
) -> bool {
    if calls.is_recursive(function) {
        return false;
    }
    let Some(summary) = summaries.get(function) else { return false };
    // A callee can only relate the object to the heap by combining the pointer with a value
    // derived from the free-memory pointer; dereferencing it, or comparing it against its own
    // derivations, is placement-agnostic. `msize` sees the elided bump regardless.
    !summary.may_recycle_fmp()
        && !summary.may_observe_msize()
        && args.iter().enumerate().filter(|(_, arg)| **arg == operand).all(|(index, _)| {
            let index = ArgIdx::new(index);
            !summary.captures_param(index)
                && !(summary.may_observe_fmp() && summary.observes_param(index))
        })
        && args.contains(&operand)
}

/// Rewrites an eligible allocation using the conservative placement retained
/// for the explicit `static-alloc` MIR pass.
fn apply_candidate(func: &mut Function, cand: &StaticAllocCandidate, shadow: u64) -> bool {
    // The region lives past the locals and the static return
    // buffer. It must stay inside the tallest entry's shadow — growing past
    // it pushes the shared static-frame region and can widen every helper
    // and spill push behind it — and must not drag this entry's own spill
    // base across the one-byte address boundary.
    let base = EvmMemoryLayout::HEAP_START
        + func.internal_frame_size.max(func.external_static_return_size);
    if base + cand.size > shadow || (base < 0x100 && base + cand.size > 0x100) {
        return false;
    }
    func.internal_frame_size = (base - EvmMemoryLayout::HEAP_START) + cand.size;
    let replacement = func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(base))));
    let mut replacements = FxHashMap::default();
    replacements.insert(cand.ptr, replacement);
    func.replace_uses_canonicalized(&replacements);
    let block = &mut func.blocks[cand.block];
    block.instructions.retain(|&inst| inst != cand.alloc);
    true
}

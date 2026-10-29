//! Static placement of provably local heap allocations.
//!
//! When a constant-size `alloc` executes at most once per call and its pointer
//! never escapes the function, the allocation does not need a runtime
//! free-memory-pointer bump. The `defer-alloc` pass proves those properties
//! and marks each such allocation `deferred_alloc`, so the backend places it
//! once the exact frame layout is known.
//!
//! Safety contract:
//! - external entries only: their locals are absolute low-memory addresses;
//! - the block cannot re-execute, so the reused static region cannot expose a previous iteration's
//!   contents where fresh zeroed memory was expected;
//! - every use of the pointer is an in-bounds address derivation into exact loads, stores, hashes,
//!   copies, logs, or external-data terminators; non-recursive internal calls are allowed only when
//!   their interprocedural summaries prove that the pointer is not captured, that the callee never
//!   resets the free-memory pointer, and that it never relates the pointer to a value derived from
//!   the free-memory pointer, since memory-safe assembly could otherwise derive aliases from where
//!   the object lies relative to the heap;
//! - allocations marked as source-visible FMP advances are never placed statically.

use crate::mir::{
    ArgIdx, BlockId, Callee, Function, FunctionId, InstId, InstKind, MirType, Module, PtrLocation,
    Terminator, ValueId,
    analysis::{AliasAnalysis, CallGraphInfo, CfgInfo, MemoryCallSummaries},
    memory::EvmMemoryLayout,
    pass::{MirPass, ModuleAnalyses},
};
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::FxHashMap,
};

/// Pass that defers eligible allocations until exact backend layout is known.
pub(crate) struct DeferAlloc;

impl MirPass for DeferAlloc {
    fn name(&self) -> &'static str {
        "defer-alloc"
    }

    fn is_contract_wide(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut ModuleAnalyses,
    ) -> bool {
        let calls = CallGraphInfo::new(module);
        let summaries = analyses.call_summaries(module);
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
    // Search the blocks reachable through at least one edge, without the
    // quadratic all-pairs reachability table.
    let mut seen = DenseBitSet::new_empty(cfg.num_blocks());
    let mut stack = cfg.successors(block).to_vec();
    while let Some(block) = stack.pop() {
        if !seen.insert(block) {
            continue;
        }
        if func.blocks[block].instructions.iter().any(|&inst| instruction_observes_fmp(func, inst))
            || func.blocks[block]
                .terminator
                .as_ref()
                .is_some_and(|term| matches!(term, Terminator::TailCall { .. }))
        {
            return true;
        }
        stack.extend_from_slice(cfg.successors(block));
    }

    false
}

fn instruction_observes_fmp(func: &Function, inst_id: InstId) -> bool {
    match func.inst(inst_id).kind {
        InstKind::Alloc { .. }
        | InstKind::Fmp
        | InstKind::SetFmp(_)
        | InstKind::ICall { function: Callee::Function(_), .. } => true,
        InstKind::Load(address, PtrLocation::Memory) => {
            func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT)
        }
        _ => false,
    }
}

/// One constant-size allocation eligible for static placement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StaticAllocCandidate {
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
    if !is_entry(func)
        || calls.is_recursive(func_id)
        || !func.instructions().any(|inst_id| {
            matches!(
                func.inst(inst_id).kind,
                InstKind::Alloc { semantics: crate::mir::AllocationSemantics::INTERNAL, .. }
            )
        })
    {
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
        let bad = !matches!(kind, InstKind::ICall { function: Callee::Function(_), .. })
            && aa.instruction_may_reset_fmp(func, inst_id);
        bad && fmp_write_has_future_observer(func, &cfg, inst_id)
    }) {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    for block in func.blocks.indices() {
        for &alloc in &func.blocks[block].instructions {
            let InstKind::Alloc { size, semantics } = func.inst(alloc).kind else {
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
            candidates.push(StaticAllocCandidate { alloc, ptr, size });
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
            func.inst(inst_id).visit_operands(|operand| {
                instructions[operand].push(inst_id);
            });
        }
        for (block_id, block) in func.blocks.iter_enumerated() {
            if let Some(terminator) = &block.terminator {
                terminator.visit_operands(|operand| {
                    terminators[operand].push(block_id);
                });
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
    let mut pending = Vec::new();
    pending.push(cand.ptr);
    while let Some(value) = pending.pop() {
        for &inst_id in &uses.instructions[value] {
            let Some(result) = func.inst_result_value(inst_id) else { continue };
            if derived.contains_key(&result) {
                continue;
            }
            let kind = func.inst(inst_id).kind.clone();
            let offset = match kind {
                InstKind::PtrToInt(base, 256) | InstKind::IntToPtr(base) => {
                    derived.get(&base).copied()
                }
                InstKind::Add(a, b) => {
                    let (base, offset) = if derived.contains_key(&a) { (a, b) } else { (b, a) };
                    let (Some(base_offset), Some(offset)) =
                        (derived.get(&base).copied(), func.value_u64(offset))
                    else {
                        return false;
                    };
                    base_offset.checked_add(offset)
                }
                InstKind::PtrAdd(base, offset) => {
                    let (Some(base_offset), Some(offset)) =
                        (derived.get(&base).copied(), func.value_u64(offset))
                    else {
                        return false;
                    };
                    base_offset.checked_add(offset)
                }
                InstKind::ExtractValue {
                    ty: MirType::Slice(_),
                    aggregate: slice,
                    index: MirType::SLICE_PTR_FIELD,
                } => {
                    let Some(&data) = derived.get(&slice) else { continue };
                    derived.insert(result, data);
                    pending.push(result);
                    continue;
                }
                InstKind::MakeSlice { ptr, location: PtrLocation::Memory, .. } if ptr == value => {
                    derived.get(&value).copied()
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
    for (&operand, &off) in &derived {
        for &inst_id in &uses.instructions[operand] {
            if inst_id == cand.alloc {
                continue;
            }
            let kind = func.inst(inst_id).kind.clone();
            let ok = match kind {
                InstKind::Load(addr, PtrLocation::Memory) => operand == addr && in_range(off, 32),
                InstKind::Store(addr, value, PtrLocation::Memory) => {
                    operand == addr && !derived.contains_key(&value) && in_range(off, 32)
                }
                InstKind::Store8(addr, value) => {
                    operand == addr && !derived.contains_key(&value) && in_range(off, 1)
                }
                InstKind::Keccak256(addr, size)
                | InstKind::Log { offset: addr, size, .. }
                | InstKind::MemoryZero(addr, size)
                | InstKind::Copy(addr, _, size, PtrLocation::Calldata | PtrLocation::Returndata)
                | InstKind::DataCopy(_, addr, size)
                | InstKind::CodeCopy(addr, _, size) => {
                    operand == addr && func.value_u64(size).is_some_and(|len| in_range(off, len))
                }
                InstKind::Copy(dest, src, size, PtrLocation::Memory) => {
                    (operand == dest || operand == src)
                        && func.value_u64(size).is_some_and(|len| in_range(off, len))
                }
                // In-bounds derivations were collected above; anything
                // else consuming an address is an escape.
                InstKind::Add(_, _)
                | InstKind::PtrAdd(_, _)
                | InstKind::PtrToInt(_, 256)
                | InstKind::IntToPtr(_)
                | InstKind::ExtractValue {
                    ty: MirType::Slice(_),
                    index: MirType::SLICE_PTR_FIELD,
                    ..
                }
                | InstKind::MakeSlice { .. } => {
                    func.inst_result_value(inst_id).is_some_and(|r| derived.contains_key(&r))
                }
                InstKind::ExtractValue {
                    ty: MirType::Slice(_),
                    index: MirType::SLICE_LEN_FIELD,
                    ..
                } => true,
                InstKind::ICall { function: Callee::Function(function), args, .. } => {
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
    // derivations, is placement-agnostic.
    !summary.may_recycle_fmp()
        && args.iter().enumerate().filter(|(_, arg)| **arg == operand).all(|(index, _)| {
            let index = ArgIdx::new(index);
            !summary.captures_param(index)
                && !(summary.may_observe_fmp() && summary.observes_param(index))
        })
        && args.contains(&operand)
}

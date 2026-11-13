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
//!
//! With `-Zcheck-analyses`, the block of each allocation the pass defers reads a guard word just
//! before the allocation, records it zero, `assume eq defer_alloc, guard, 0`, and sets it to one:
//! the block runs at most once per call, so it never finds the guard set. The guard is a word of
//! the entry's local region past its locals and return buffer, which the frame layout reserves by
//! growing the local region, and which the call starts with zeroed like all of its memory. The
//! entry block also records the heap floor at or above the end of the guard,
//! `assume ule defer_alloc, guard + 32, heap_floor`: the backend places the heap above the local
//! region, and the record lets `-Zprove-analyses` keep the guard apart from the heap. Memory
//! dead store elimination drops the guard's store when no path leads from it back to the guard's
//! read; one that keeps it found the cycle this pass's analysis missed, or a pass between them
//! made. A deferral the second run of the pass makes again keeps its first guard.

use crate::mir::{
    AllocationSemantics, ArgIdx, AssumePredicate, BlockId, Callee, FactSource, Function,
    FunctionId, Immediate, InstId, InstKind, Instruction, MirType, Module, PtrLocation, Terminator,
    Value, ValueId,
    analysis::{AliasAnalysis, CallGraphInfo, CfgInfo, MemoryCallSummaries},
    memory::EvmMemoryLayout,
    pass::{MirPass, ModuleAnalyses},
    transform::{
        lower_assumes::{record_fact, should_check_analyses},
        storage_promotion::allocate_temp_addr,
    },
};
use alloy_primitives::U256;
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
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut ModuleAnalyses,
    ) -> bool {
        let record = should_check_analyses(gcx.sess);
        let calls = CallGraphInfo::new(module);
        let summaries = analyses.call_summaries(module);
        let entries = module.runtime_entries();
        let mut candidates = Vec::new();
        for (func_id, func) in module.functions.iter_enumerated() {
            let eligible = eligible_static_allocations(func_id, func, &entries, &calls, &summaries);
            candidates.extend(eligible.into_iter().map(|candidate| (func_id, candidate.alloc)));
        }

        let mut changed = false;
        for (func_id, alloc) in candidates {
            let func = &mut module.functions[func_id];
            let metadata = &mut func.inst_mut(alloc).metadata;
            if !metadata.deferred_alloc() {
                metadata.set_deferred_alloc();
                changed = true;
                if record {
                    guard_once(func, alloc);
                }
            }
        }
        changed
    }
}

fn fmp_write_has_future_observer(
    func: &Function,
    cfg: &CfgInfo,
    block: BlockId,
    position: usize,
) -> bool {
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
    entries: &DenseBitSet<FunctionId>,
    calls: &CallGraphInfo,
    summaries: &MemoryCallSummaries,
) -> Vec<StaticAllocCandidate> {
    if !entries.contains(func_id)
        || calls.is_recursive(func_id)
        || !func.instructions().any(|inst_id| {
            matches!(
                func.inst(inst_id).kind,
                InstKind::Alloc { semantics: AllocationSemantics::INTERNAL, .. }
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
    if func.blocks.iter_enumerated().any(|(block, data)| {
        data.instructions.iter().enumerate().any(|(position, &inst_id)| {
            !matches!(
                func.inst(inst_id).kind,
                InstKind::ICall { function: Callee::Function(_), .. }
            ) && aa.instruction_may_reset_fmp(func, inst_id)
                && fmp_write_has_future_observer(func, &cfg, block, position)
        })
    }) {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    for block in func.blocks.indices() {
        for &alloc in &func.blocks[block].instructions {
            let InstKind::Alloc { size, semantics } = func.inst(alloc).kind else {
                continue;
            };
            if semantics != AllocationSemantics::INTERNAL
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
    let mut derived = FxHashMap::default();
    let mut pending = Vec::new();
    candidates.retain(|candidate| {
        candidate_uses_are_safe(
            func,
            candidate,
            &uses,
            calls,
            summaries,
            &mut derived,
            &mut pending,
        )
    });
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
    derived: &mut FxHashMap<ValueId, u64>,
    pending: &mut Vec<ValueId>,
) -> bool {
    // In-bounds address derivations from the pointer, to a fixpoint so
    // definition order does not matter.
    derived.clear();
    derived.insert(cand.ptr, 0);
    pending.clear();
    pending.push(cand.ptr);
    while let Some(value) = pending.pop() {
        for &inst_id in &uses.instructions[value] {
            let Some(result) = func.inst_result_value(inst_id) else { continue };
            if derived.contains_key(&result) {
                continue;
            }
            let kind = &func.inst(inst_id).kind;
            let offset = match *kind {
                InstKind::PtrToInt(base, 256) | InstKind::IntToPtr(base) => {
                    derived.get(&base).copied()
                }
                InstKind::Add(base, offset) | InstKind::PtrAdd(base, offset) => {
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
    for (&operand, &off) in derived.iter() {
        for &inst_id in &uses.instructions[operand] {
            if inst_id == cand.alloc {
                continue;
            }
            let ok = match &func.inst(inst_id).kind {
                InstKind::Load(addr, PtrLocation::Memory) => operand == *addr && in_range(off, 32),
                InstKind::Store(addr, value, PtrLocation::Memory) => {
                    operand == *addr && !derived.contains_key(value) && in_range(off, 32)
                }
                InstKind::Store8(addr, value) => {
                    operand == *addr && !derived.contains_key(value) && in_range(off, 1)
                }
                InstKind::Keccak256(addr, size)
                | InstKind::Log { offset: addr, size, .. }
                | InstKind::MemoryZero(addr, size)
                | InstKind::Copy(addr, _, size, PtrLocation::Calldata | PtrLocation::Returndata)
                | InstKind::DataCopy(_, addr, size)
                | InstKind::CodeCopy(addr, _, size) => {
                    operand == *addr && func.value_u64(*size).is_some_and(|len| in_range(off, len))
                }
                InstKind::Copy(dest, src, size, PtrLocation::Memory) => {
                    (operand == *dest || operand == *src)
                        && func.value_u64(*size).is_some_and(|len| in_range(off, len))
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
                    call_use_is_safe(*function, args, operand, calls, summaries)
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
}

/// Records that the block of the deferred `alloc` runs at most once per call: a guard word of the
/// entry's local region, zero when the call starts, is still zero just before the allocation,
/// which then sets it.
fn guard_once(func: &mut Function, alloc: InstId) {
    let Some((block, position)) = func.blocks.iter_enumerated().find_map(|(block, data)| {
        data.instructions.iter().position(|&inst| inst == alloc).map(|position| (block, position))
    }) else {
        return;
    };
    let guard = allocate_temp_addr(func);
    let one = func.alloc_value(Value::Immediate(Immediate::I256(U256::ONE)));
    let zero = func.alloc_value(Value::Immediate(Immediate::I256(U256::ZERO)));
    // seen = mload guard; mstore guard, 1; ...; v = alloc size
    let (load, seen) = func.alloc_value_inst(
        Instruction::new(InstKind::Load(guard, PtrLocation::Memory), Some(MirType::I256))
            .with_debug_loc_dropped(),
    );
    let store = func.alloc_inst(
        Instruction::new(InstKind::Store(guard, one, PtrLocation::Memory), None)
            .with_debug_loc_dropped(),
    );
    func.blocks[block].instructions.splice(position..position, [load, store]);
    // assume eq defer_alloc, seen, 0
    record_fact(func, block, FactSource::DeferAlloc, AssumePredicate::Eq, seen, zero);
    let Some(end) = func.value_u64(guard).and_then(|guard| guard.checked_add(32)) else {
        return;
    };
    let end = func.alloc_value(Value::Immediate(Immediate::I256(U256::from(end))));
    // floor = heap_floor; assume ule defer_alloc, guard + 32, floor; ...the entry block
    let (floor_inst, floor) = func.alloc_value_inst(
        Instruction::new(InstKind::HeapFloor, Some(MirType::I256)).with_debug_loc_dropped(),
    );
    let kind = InstKind::Assume {
        predicate: AssumePredicate::Ule,
        source: FactSource::DeferAlloc,
        lhs: end,
        rhs: floor,
    };
    let record = func.alloc_inst(Instruction::new(kind, None).with_debug_loc_dropped());
    let entry = &mut func.blocks[BlockId::ENTRY].instructions;
    entry.splice(0..0, [floor_inst, record]);
}

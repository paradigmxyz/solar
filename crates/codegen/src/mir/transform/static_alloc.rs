//! Placement of provably local heap allocations without a free-memory-pointer bump.
//!
//! An `alloc` normally reserves its bytes by advancing the free-memory pointer (FMP). Two
//! placements avoid that bump when the object is local to one function. `defer-alloc` proves the
//! properties and rewrites or marks the allocation. Static placement runs before ABI encoding is
//! expanded and again once memory objects are physical. Unreserved placement runs only in the
//! second run, after physical inlining has moved helpers' allocations into the entries where static
//! placement can take them, and before `coalesce-allocs` and `lower-alloc` expand the rest.
//!
//! # Static placement
//!
//! A constant-size `alloc` that executes at most once per call and whose pointer never escapes
//! the function is marked `deferred_alloc`, and the backend places it once the exact frame layout
//! is known.
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
//! # Unreserved placement
//!
//! An allocation of any size, in any function and any loop, becomes an unreserved region at the
//! FMP when nothing else can observe the missing bump. The pass rewrites it to `fmp`, the way
//! `abi_encode, scratch` stages call data at the pointer without reserving it, so the next
//! allocation reuses the memory and the `mstore 64` disappears. A zeroed allocation keeps an
//! explicit `memory_zero`, since memory past the pointer can hold old scratch data. A checked
//! allocation keeps its bump: dropping it would leave an inline overflow check at every site, which
//! costs more code than the checked-allocation helper that `lower-alloc` shares between them.
//!
//! The pointer must not escape. Every use is an address derivation (`ptrtoint`, `inttoptr`,
//! constant `add`, `ptradd`, slice fields) into a raw memory access, an external call range, a
//! `revert` or `returndata`, or a call whose summary proves it neither captures the pointer nor
//! touches the FMP. Derived accesses need no bounds proof: memory past the object is the same
//! memory with or without the reservation.
//!
//! The lifetime ends before the FMP is used again. Take every instruction on a path from the
//! allocation to a use of the pointer, without re-entering the allocation, which defines a new
//! pointer. None of them may read or advance the FMP (`fmp`, `set_fmp`, another `alloc`, a load or
//! store of slot 64, an operation that stages at the pointer such as `abi_encode`, or an internal
//! call that does any of those), and none may write memory that is not addressed from the
//! allocation, since that could alias the region: only constant ranges in the scratch words below
//! slot 64 qualify. Memory-safe assembly may write past the FMP only after reading it, so the
//! first rule covers it, and the second covers assembly writing absolute addresses.
//!
//! Assembly can see a missing bump anywhere in the contract, as `mload(0x40)` before and after a
//! call that allocates, and the test suite pins such deltas. So one read that may cover slot 64,
//! in any function of the module, keeps every bump. A read counts unless its address is at or above
//! the heap start by `AliasAnalysis`'s heap-pointer assumption, so a variable address or size
//! counts too. The exception is the bubbled revert data of a failed call, which a copy has just
//! overwritten. An `fmp` whose result is used as anything but a memory address counts too, since
//! a removed bump changes it. A write that may reach the slot counts as well, since a partial one
//! keeps the old pointer's other bytes, unless it ends the execution. Allocations marked as
//! source-visible FMP advances and those that static placement takes are left alone. `msize` is
//! not tracked.

use crate::mir::{
    AllocationFailure, AllocationInitialization, AllocationSemantics, ArgIdx, BlockId, Callee,
    EffectKind, Function, FunctionBuilder, FunctionId, InstId, InstKind, MirType, Module,
    PtrLocation, Terminator, Value, ValueId,
    analysis::{
        Access, AddressSpace, AliasAnalysis, CallGraphInfo, CfgInfo, Location, MemoryBase,
        MemoryCallSummaries,
    },
    memory::EvmMemoryLayout,
    op_schema::{RawMemoryAccess, RawMemorySize},
    pass::{MirPass, ModuleAnalyses},
};
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::FxHashMap,
};
use std::sync::Arc;

/// Pass that defers eligible allocations until exact backend layout is known.
pub(crate) enum DeferAlloc {
    /// Static placement only, before ABI encoding is expanded.
    Early,
    /// Static placement, then unreserved placement of what remains, once memory objects are
    /// physical and inlining has finished.
    Late,
}

impl MirPass for DeferAlloc {
    fn name(&self) -> &'static str {
        "defer-alloc"
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
        let mut unreserved = Vec::new();
        // Assembly can observe a missing bump anywhere in the contract, as `mload(0x40)` before
        // and after a call, so one such access anywhere keeps every bump.
        let unreserved_allowed =
            matches!(self, Self::Late) && !module.functions.iter().any(accesses_fmp_slot);
        for (func_id, func) in module.functions.iter_enumerated() {
            let eligible = eligible_static_allocations(func_id, func, &calls, &summaries);
            if unreserved_allowed {
                let placed = eligible.iter().map(|candidate| candidate.alloc).collect::<Vec<_>>();
                unreserved.extend(
                    eligible_unreserved_allocations(func, &calls, &summaries, &placed)
                        .into_iter()
                        .map(|placement| (func_id, placement)),
                );
            }
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
        for &(func_id, placement) in &unreserved {
            place_at_fmp(&mut module.functions[func_id], placement);
        }
        changed | !unreserved.is_empty()
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

/// Collects the in-bounds address derivations from `root`, with each one's constant offset, to a
/// fixpoint so definition order does not matter. `None` means a derivation that cannot be bounded
/// by a constant: a non-constant `add`, or a derivation past `bound` bytes. Without a `bound`,
/// `ptradd` takes a runtime offset, and the offsets in the map mean nothing.
fn derive_pointers(
    func: &Function,
    root: ValueId,
    uses: &ValueUses,
    bound: Option<u64>,
) -> Option<FxHashMap<ValueId, u64>> {
    let mut derived: FxHashMap<ValueId, u64> = FxHashMap::default();
    derived.insert(root, 0);
    let mut pending = Vec::new();
    pending.push(root);
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
                        return None;
                    };
                    base_offset.checked_add(offset)
                }
                InstKind::PtrAdd(base, offset) => {
                    let base_offset = derived.get(&base).copied();
                    match (base_offset, func.value_u64(offset)) {
                        (Some(base_offset), Some(offset)) => base_offset.checked_add(offset),
                        (Some(base_offset), None) if bound.is_none() => Some(base_offset),
                        _ => return None,
                    }
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
            let total = offset?;
            if bound.is_some_and(|bound| total >= bound) {
                return None;
            }
            derived.insert(result, total);
            pending.push(result);
        }
    }
    Some(derived)
}

/// For an address derivation or a slice length read, whether the use stays tracked: its pointer
/// result is among the `derived` addresses. `None` for any other instruction.
fn derivation_is_tracked(
    func: &Function,
    inst_id: InstId,
    kind: &InstKind,
    derived: &FxHashMap<ValueId, u64>,
) -> Option<bool> {
    match kind {
        InstKind::Add(..)
        | InstKind::PtrAdd(..)
        | InstKind::PtrToInt(_, 256)
        | InstKind::IntToPtr(_)
        | InstKind::ExtractValue {
            ty: MirType::Slice(_), index: MirType::SLICE_PTR_FIELD, ..
        } => Some(func.inst_result_value(inst_id).is_some_and(|r| derived.contains_key(&r))),
        // A derived length would leak the pointer as an integer through the length field.
        InstKind::MakeSlice { len, .. } => Some(
            !derived.contains_key(len)
                && func.inst_result_value(inst_id).is_some_and(|r| derived.contains_key(&r)),
        ),
        InstKind::ExtractValue {
            ty: MirType::Slice(_), index: MirType::SLICE_LEN_FIELD, ..
        } => Some(true),
        _ => None,
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
    let Some(derived) = derive_pointers(func, cand.ptr, uses, Some(cand.size)) else {
        return false;
    };

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
                _ if let Some(tracked) = derivation_is_tracked(func, inst_id, &kind, &derived) => {
                    tracked
                }
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

/// An allocation whose region can stay unreserved, with its place in the function.
#[derive(Clone, Copy)]
struct Placement {
    alloc: InstId,
    block: BlockId,
    position: usize,
}

/// Returns allocations whose region can stay unreserved at the free-memory pointer: nothing
/// reads or advances the pointer, or writes memory the allocation does not address, between the
/// allocation and the last use of its pointer. `placed` holds those that placement takes first.
fn eligible_unreserved_allocations(
    func: &Function,
    calls: &CallGraphInfo,
    summaries: &Arc<MemoryCallSummaries>,
    placed: &[InstId],
) -> Vec<Placement> {
    let mut allocs = Vec::new();
    for (block, data) in func.blocks.iter_enumerated() {
        for (position, &alloc) in data.instructions.iter().enumerate() {
            let inst = func.inst(alloc);
            if matches!(
                inst.kind,
                InstKind::Alloc {
                    semantics: AllocationSemantics { failure: AllocationFailure::Infallible, .. },
                    ..
                }
            ) && !inst.metadata.preserves_fmp()
                && !inst.metadata.deferred_alloc()
                && !placed.contains(&alloc)
            {
                allocs.push(Placement { alloc, block, position });
            }
        }
    }
    if allocs.is_empty() {
        return allocs;
    }

    let cfg = CfgInfo::new(func);
    allocs.retain(|placement| cfg.is_reachable(placement.block));

    let mut positions = index_vec![(BlockId::ENTRY, 0); func.num_insts()];
    for (block, data) in func.blocks.iter_enumerated() {
        for (position, &inst) in data.instructions.iter().enumerate() {
            positions[inst] = (block, position);
        }
    }
    let analysis = UnreservedAnalysis {
        func,
        cfg,
        aa: AliasAnalysis::with_call_summaries(func, Arc::clone(summaries)),
        uses: ValueUses::new(func),
        calls,
        summaries,
        positions,
    };
    allocs.retain(|&placement| analysis.is_unreserved(placement));
    allocs
}

struct UnreservedAnalysis<'a> {
    func: &'a Function,
    cfg: CfgInfo,
    aa: AliasAnalysis,
    uses: ValueUses,
    calls: &'a CallGraphInfo,
    summaries: &'a MemoryCallSummaries,
    /// The block and position of every placed instruction.
    positions: IndexVec<InstId, (BlockId, usize)>,
}

impl UnreservedAnalysis<'_> {
    fn is_unreserved(&self, Placement { alloc, block: alloc_block, position }: Placement) -> bool {
        let func = self.func;
        let ptr = func.inst_result_value(alloc).expect("allocation must produce a value");
        let Some(derived) = derive_pointers(func, ptr, &self.uses, None) else { return false };
        let Some(use_ends) = self.pointer_uses(alloc, &derived) else { return false };

        // Blocks that still reach a use. A path back into the allocation's block runs the
        // allocation again, which defines a new pointer, so those edges do not count.
        let mut reaches_use = DenseBitSet::new_empty(self.cfg.num_blocks());
        let mut pending = use_ends.keys().copied().collect::<Vec<_>>();
        for &block in &pending {
            reaches_use.insert(block);
        }
        while let Some(block) = pending.pop() {
            if block == alloc_block {
                continue;
            }
            for &pred in self.cfg.predecessors(block) {
                if reaches_use.insert(pred) {
                    pending.push(pred);
                }
            }
        }

        let mut visited = DenseBitSet::new_empty(self.cfg.num_blocks());
        visited.insert(alloc_block);
        let mut stack = vec![alloc_block];
        while let Some(block) = stack.pop() {
            let start = if block == alloc_block { position + 1 } else { 0 };
            let reaches_later_use = self
                .cfg
                .successors(block)
                .iter()
                .any(|&succ| succ != alloc_block && reaches_use.contains(succ));
            let instructions = &func.blocks[block].instructions;
            let end = if reaches_later_use {
                instructions.len()
            } else {
                use_ends.get(&block).copied().unwrap_or(start)
            };
            if instructions
                .get(start..end)
                .unwrap_or_default()
                .iter()
                .any(|&inst| self.touches_region(inst, alloc, &derived))
            {
                return false;
            }
            for &succ in self.cfg.successors(block) {
                if succ != alloc_block && reaches_use.contains(succ) && visited.insert(succ) {
                    stack.push(succ);
                }
            }
        }
        true
    }

    /// Verifies the pointer never escapes and returns, for each block that uses it, the end of
    /// its last use as a bound on instruction positions. Address derivations are not uses: only
    /// memory accesses, calls, and terminators end the lifetime.
    fn pointer_uses(
        &self,
        alloc: InstId,
        derived: &FxHashMap<ValueId, u64>,
    ) -> Option<FxHashMap<BlockId, usize>> {
        let func = self.func;
        let mut use_ends = FxHashMap::<BlockId, usize>::default();
        for &operand in derived.keys() {
            for &inst_id in &self.uses.instructions[operand] {
                if inst_id == alloc {
                    continue;
                }
                let kind = &func.inst(inst_id).kind;
                if let Some(tracked) = derivation_is_tracked(func, inst_id, kind, derived) {
                    if !tracked {
                        return None;
                    }
                    continue;
                }
                let is_use = match kind {
                    InstKind::ICall { function: Callee::Function(function), args, .. } => {
                        if !call_use_is_safe(*function, args, operand, self.calls, self.summaries) {
                            return None;
                        }
                        true
                    }
                    _ => {
                        if !is_address_only_use(kind, operand) {
                            return None;
                        }
                        true
                    }
                };
                if is_use {
                    let (block, position) = self.positions[inst_id];
                    let end = use_ends.entry(block).or_default();
                    *end = (*end).max(position + 1);
                }
            }
            for &block in &self.uses.terminators[operand] {
                let term = func.blocks[block].terminator.as_ref().expect("indexed terminator use");
                match term {
                    Terminator::Revert { offset, size }
                    | Terminator::ReturnData { offset, size }
                        if operand == *offset && operand != *size => {}
                    _ => return None,
                }
                use_ends.insert(block, func.blocks[block].instructions.len());
            }
        }
        Some(use_ends)
    }

    /// Whether the instruction may read or advance the free-memory pointer, or write memory the
    /// allocation does not address.
    fn touches_region(
        &self,
        inst_id: InstId,
        alloc: InstId,
        derived: &FxHashMap<ValueId, u64>,
    ) -> bool {
        let func = self.func;
        if inst_id == alloc {
            return false;
        }
        let kind = &func.inst(inst_id).kind;
        let mut raw = false;
        let mut blocked = false;
        kind.visit_raw_memory(|access, address, size| {
            raw = true;
            let len = match size {
                RawMemorySize::Bytes(len) => Some(len),
                RawMemorySize::Value(size) => func.value_u64(size),
            };
            blocked |= !derived.contains_key(&address)
                && match access {
                    RawMemoryAccess::Read => {
                        AliasAnalysis::range_may_overlap_fmp(func, address, len)
                    }
                    RawMemoryAccess::Write | RawMemoryAccess::PartialWrite => {
                        !writes_scratch_only(func.value_u64(address), len)
                    }
                };
        });
        if raw {
            return blocked;
        }
        let effects = self.aa.instruction_mod_ref(func, inst_id);
        effects.reads().iter().any(|&access| !self.access_is_clear(access, false, alloc, derived))
            || effects
                .writes()
                .iter()
                .any(|&access| !self.access_is_clear(access, true, alloc, derived))
    }

    /// Whether an access neither touches the free-memory pointer nor, for a write, memory the
    /// allocation does not address.
    fn access_is_clear(
        &self,
        access: Access,
        write: bool,
        alloc: InstId,
        derived: &FxHashMap<ValueId, u64>,
    ) -> bool {
        let location = match access {
            Access::Any(AddressSpace::Memory) => return false,
            Access::Any(_) => return true,
            Access::Location(Location::Memory(location)) => location,
            Access::Location(_) => return true,
        };
        let address = location.address;
        match address.base {
            MemoryBase::Absolute if write => {
                writes_scratch_only(Some(address.offset), location.size.as_const())
            }
            MemoryBase::Absolute => {
                !AliasAnalysis::memory_alias_locations(location, AliasAnalysis::fmp_location())
                    .may_alias()
            }
            MemoryBase::InternalFrame => true,
            MemoryBase::Allocation(site) | MemoryBase::DynamicAllocation(site) => {
                !write || site == alloc
            }
            MemoryBase::Value(value) => {
                derived.contains_key(&value)
                    || (!write && !AliasAnalysis::range_may_overlap_fmp(self.func, value, None))
            }
        }
    }
}

/// Whether the function may read the free-memory pointer's slot the way assembly does, or patch
/// part of it. A read counts unless its address is at or above the heap start by the established
/// heap-pointer assumption; a constant range that misses the slot, or an unsized one, does not
/// escape that rule. A write that may reach the slot counts too, since a partial one keeps the old
/// pointer's other bytes, unless it ends the execution: nothing reads the pointer after it. A
/// write of the whole word does not count: the entry's initialization writes it, and it ignores
/// the old pointer.
fn accesses_fmp_slot(func: &Function) -> bool {
    func.blocks.iter().any(|block| {
        // Whether execution ends after the instruction without another call, creation, or
        // instruction that reads the free-memory pointer.
        let mut ends = matches!(
            block.terminator,
            Some(
                Terminator::Revert { .. }
                    | Terminator::RevertReturndata
                    | Terminator::ReturnData { .. }
                    | Terminator::Stop
                    | Terminator::SelfDestruct { .. }
                    | Terminator::Invalid
            )
        );
        block.instructions.iter().rev().any(|&inst| {
            let kind = &func.inst(inst).kind;
            let mut hit = false;
            kind.visit_raw_memory(|access, address, size| {
                let len = match size {
                    RawMemorySize::Bytes(len) => Some(len),
                    RawMemorySize::Value(size) => func.value_u64(size),
                };
                hit |= match access {
                    RawMemoryAccess::Read => {
                        AliasAnalysis::range_may_overlap_fmp(func, address, len)
                    }
                    RawMemoryAccess::Write | RawMemoryAccess::PartialWrite => {
                        !ends
                            && AliasAnalysis::range_may_overlap_fmp(func, address, len)
                            && !(func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT)
                                && len == Some(EvmMemoryLayout::WORD_SIZE))
                    }
                };
            });
            ends &= !(instruction_observes_fmp(func, inst)
                || matches!(kind, InstKind::AbiEncode { .. } | InstKind::AbiDecode { .. })
                || matches!(
                    kind.effect_kind(),
                    EffectKind::ICall | EffectKind::ExternalCall | EffectKind::Create
                ));
            hit
        })
    }) || func.blocks.iter().any(|block| {
        matches!(
            block.terminator,
            Some(Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size })
                if AliasAnalysis::range_may_overlap_fmp(func, offset, func.value_u64(size))
                    && !reads_copied_returndata(func, &block.instructions, offset, size)
        )
    }) || fmp_value_escapes(func)
}

/// Whether the function reads the free-memory pointer into anything but memory addresses, such as
/// an integer that a later `fmp` can be subtracted from. A removed bump changes that value.
fn fmp_value_escapes(func: &Function) -> bool {
    let uses = ValueUses::new(func);
    func.instructions().any(|inst| {
        if !matches!(func.inst(inst).kind, InstKind::Fmp) {
            return false;
        }
        let ptr = func.inst_result_value(inst).expect("fmp must produce a value");
        let Some(derived) = derive_pointers(func, ptr, &uses, None) else { return true };
        derived.keys().any(|&operand| {
            uses.instructions[operand].iter().any(|&user| {
                let kind = &func.inst(user).kind;
                match derivation_is_tracked(func, user, kind, &derived) {
                    Some(tracked) => !tracked,
                    None => !is_address_only_use(kind, operand),
                }
            }) || uses.terminators[operand].iter().any(|&block| {
                !matches!(
                    func.blocks[block].terminator,
                    Some(Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size })
                        if operand == offset && operand != size
                )
            })
        })
    })
}

/// Whether a terminator reads exactly the returndata that a copy at the same constant address
/// wrote, as a failed call does when it bubbles up its revert data: `returndatacopy 0, 0, n;
/// revert 0, n` with both sizes `returndatasize`. Both sizes must be read in the block, with no
/// instruction in between that writes memory or calls out, so they come from the same returndata.
fn reads_copied_returndata(
    func: &Function,
    instructions: &[InstId],
    offset: ValueId,
    size: ValueId,
) -> bool {
    let returndata_size = |value| match func.value(value) {
        Value::Inst(inst) if matches!(func.inst(*inst).kind, InstKind::ReturnDataSize) => {
            Some(*inst)
        }
        _ => None,
    };
    let (Some(offset), Some(size)) = (func.value_u64(offset), returndata_size(size)) else {
        return false;
    };
    // The terminator's size is read after the copy, and the copy's size before it, with only
    // pure and environment reads between them.
    let mut size_read = false;
    let mut copy_size = None;
    for &inst in instructions.iter().rev() {
        if copy_size.is_none() {
            size_read |= inst == size;
        } else if Some(inst) == copy_size {
            return true;
        }
        match func.inst(inst).kind {
            InstKind::Copy(dest, _, length, PtrLocation::Returndata)
                if copy_size.is_none() && size_read && func.value_u64(dest) == Some(offset) =>
            {
                let Some(length) = returndata_size(length) else { return false };
                copy_size = Some(length);
            }
            ref kind
                if matches!(kind.effect_kind(), EffectKind::Pure | EffectKind::EnvironmentRead) => {
            }
            _ => return false,
        }
    }
    false
}

/// Whether a write of `len` bytes at `address` stays inside the scratch words below the
/// free-memory pointer's slot, the only memory outside the allocation that it may write.
fn writes_scratch_only(address: Option<u64>, len: Option<u64>) -> bool {
    len == Some(0)
        || address
            .zip(len)
            .and_then(|(address, len)| address.checked_add(len))
            .is_some_and(|end| end <= EvmMemoryLayout::FMP_SLOT)
}

/// Whether every operand slot holding `operand` is the address of a raw memory range the
/// instruction accesses, so the pointer never leaves as data or as a size.
fn is_address_only_use(kind: &InstKind, operand: ValueId) -> bool {
    let mut addresses = 0;
    kind.visit_raw_memory(|_, address, _| addresses += usize::from(address == operand));
    let mut operands = 0;
    kind.visit_operands(|value| operands += usize::from(value == operand));
    addresses != 0 && addresses == operands
}

/// Rewrites an allocation to read the free-memory pointer without advancing it.
fn place_at_fmp(func: &mut Function, Placement { alloc, block, .. }: Placement) {
    let InstKind::Alloc { size, semantics } = func.inst(alloc).kind else { unreachable!() };
    let ptr = func.inst_result_value(alloc).expect("allocation must produce a value");
    // ptr = fmp
    func.inst_mut(alloc).replace_kind(InstKind::Fmp);
    if semantics.initialization != AllocationInitialization::Zeroed {
        return;
    }

    // memory_zero ptr, size
    let metadata = func.inst(alloc).metadata.clone();
    // Earlier placements in the block may have shifted the position.
    let position = func.blocks[block]
        .instructions
        .iter()
        .position(|&inst| inst == alloc)
        .expect("allocation belongs to its block");
    let tail = func.blocks[block].instructions.split_off(position + 1);
    let mut builder = FunctionBuilder::new(func);
    builder.switch_to_block(block);
    builder.set_debug_context(&metadata);
    builder.memory_zero(ptr, size);
    func.blocks[block].instructions.extend(tail);
}

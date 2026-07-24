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
//!   copies, logs, or external-data terminators — the pointer value never escapes into stored data,
//!   call arguments, or unbounded arithmetic;
//! - functions observing `msize` are skipped: eliding a bump changes the high-water mark.

use crate::{
    analysis::{AliasAnalysis, CfgInfo, MemoryCallSummaries},
    memory::EvmMemoryLayout,
    mir::{BlockId, Function, Immediate, InstId, InstKind, Module, Terminator, Value, ValueId},
    pass::MirPass,
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use std::sync::Arc;

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
        _analyses: &mut crate::pass::ModuleAnalyses,
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

        let summaries = Arc::new(MemoryCallSummaries::new(module));
        let mut changed = false;
        for func in module.functions.iter_mut() {
            if !is_entry(func) || has_msize(func) {
                continue;
            }
            let aa = AliasAnalysis::with_call_summaries(func, Arc::clone(&summaries));
            changed |= run_on_entry(func, shadow, &aa);
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
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        let summaries = Arc::new(MemoryCallSummaries::new(module));
        let mut candidates = Vec::new();
        for (func_id, func) in module.functions.iter_enumerated() {
            let aa = AliasAnalysis::with_call_summaries(func, Arc::clone(&summaries));
            candidates.extend(
                eligible_static_allocations(func, &aa)
                    .into_iter()
                    .map(|candidate| (func_id, candidate.alloc)),
            );
        }

        let mut changed = false;
        for (func_id, alloc) in candidates {
            let metadata = &mut module.functions[func_id].instructions[alloc].metadata;
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

fn run_on_entry(func: &mut Function, shadow: u64, aa: &AliasAnalysis) -> bool {
    let mut changed = false;
    for cand in eligible_static_allocations(func, aa) {
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
fn eligible_static_allocations(func: &Function, aa: &AliasAnalysis) -> Vec<StaticAllocCandidate> {
    if !is_entry(func) || has_msize(func) {
        return Vec::new();
    }

    let inst_results = func.inst_results();
    let cfg = CfgInfo::new(func);
    let mut cyclic = FxHashMap::default();
    let mut candidates = Vec::new();
    for block in func.blocks.indices() {
        for &alloc in &func.blocks[block].instructions {
            let InstKind::Alloc { size, semantics, .. } = func.instructions[alloc].kind else {
                continue;
            };
            if semantics != crate::mir::AllocationSemantics::INTERNAL {
                continue;
            }
            let Some(size) = func.value_u64(size) else { continue };
            if size == 0
                || size > 0x1000
                || !size.is_multiple_of(32)
                || !cfg.is_reachable(block)
                || *cyclic.entry(block).or_insert_with(|| block_in_cycle(func, block))
            {
                continue;
            }
            let candidate = StaticAllocCandidate { block, alloc, ptr: inst_results[&alloc], size };
            if !aa.value_escapes(func, candidate.ptr) && candidate_uses_are_safe(func, &candidate) {
                candidates.push(candidate);
            }
        }
    }
    candidates
}

fn has_msize(func: &Function) -> bool {
    func.blocks.iter().any(|block| {
        block
            .instructions
            .iter()
            .any(|&inst| matches!(func.instructions[inst].kind, InstKind::MSize))
    })
}

/// Returns true when `block` can execute more than once: it can reach itself.
fn block_in_cycle(func: &Function, block: BlockId) -> bool {
    let mut stack = vec![block];
    let mut seen = DenseBitSet::new_empty(func.blocks.len());
    while let Some(current) = stack.pop() {
        let Some(term) = func.blocks[current].terminator.as_ref() else { continue };
        for succ in term.successors() {
            if succ == block {
                return true;
            }
            if seen.insert(succ) {
                stack.push(succ);
            }
        }
    }
    false
}

/// Verifies every use of the pointer stays in bounds and never escapes.
fn candidate_uses_are_safe(func: &Function, cand: &StaticAllocCandidate) -> bool {
    let inst_results = func.inst_results();

    // In-bounds address derivations from the pointer, to a fixpoint so
    // definition order does not matter.
    let mut derived: FxHashMap<ValueId, u64> = FxHashMap::default();
    derived.insert(cand.ptr, 0);
    loop {
        let mut grew = false;
        for block in func.blocks.iter() {
            for &inst_id in &block.instructions {
                if let InstKind::Add(a, b) = func.instructions[inst_id].kind
                    && let Some(&result) = inst_results.get(&inst_id)
                    && !derived.contains_key(&result)
                {
                    let (base, offset) = if derived.contains_key(&a) {
                        (a, b)
                    } else if derived.contains_key(&b) {
                        (b, a)
                    } else {
                        continue;
                    };
                    let (Some(base_off), Some(off)) =
                        (derived.get(&base).copied(), func.value_u64(offset))
                    else {
                        return false;
                    };
                    let Some(total) = base_off.checked_add(off) else { return false };
                    if total >= cand.size {
                        return false;
                    }
                    derived.insert(result, total);
                    grew = true;
                }
            }
        }
        if !grew {
            break;
        }
    }

    // Every use of every derived address must be a bounded memory access.
    let in_range = |off: u64, len: u64| off.checked_add(len).is_some_and(|end| end <= cand.size);
    for block in func.blocks.iter() {
        for &inst_id in &block.instructions {
            if inst_id == cand.alloc {
                continue;
            }
            let kind = &func.instructions[inst_id].kind;
            for &operand in kind.operands().iter() {
                let Some(&off) = derived.get(&operand) else { continue };
                let ok = match *kind {
                    InstKind::MLoad(addr) => operand == addr && in_range(off, 32),
                    InstKind::MStore(addr, value) => {
                        operand == addr && value != operand && in_range(off, 32)
                    }
                    InstKind::Keccak256(addr, size)
                    | InstKind::Log0(addr, size)
                    | InstKind::CalldataCopy(addr, _, size)
                    | InstKind::ReturnDataCopy(addr, _, size)
                    | InstKind::CodeCopy(addr, _, size) => {
                        operand == addr
                            && func.value_u64(size).is_some_and(|len| in_range(off, len))
                    }
                    InstKind::Log1(addr, size, _)
                    | InstKind::Log2(addr, size, _, _)
                    | InstKind::Log3(addr, size, _, _, _)
                    | InstKind::Log4(addr, size, _, _, _, _) => {
                        operand == addr
                            && func.value_u64(size).is_some_and(|len| in_range(off, len))
                    }
                    InstKind::MCopy(dest, src, size) => {
                        (operand == dest || operand == src)
                            && func.value_u64(size).is_some_and(|len| in_range(off, len))
                    }
                    // In-bounds derivations were collected above; anything
                    // else consuming an address is an escape.
                    InstKind::Add(_, _) => {
                        inst_results.get(&inst_id).is_some_and(|r| derived.contains_key(r))
                    }
                    _ => false,
                };
                if !ok {
                    return false;
                }
            }
        }
        if let Some(term) = &block.terminator {
            for &operand in term.operands().iter() {
                let Some(&off) = derived.get(&operand) else { continue };
                let ok = match term {
                    Terminator::Revert { offset, size }
                    | Terminator::ReturnData { offset, size } => {
                        operand == *offset
                            && func.value_u64(*size).is_some_and(|len| in_range(off, len))
                    }
                    _ => false,
                };
                if !ok {
                    return false;
                }
            }
        }
    }

    true
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

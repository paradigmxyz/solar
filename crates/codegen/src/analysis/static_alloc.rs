//! Eligibility analysis for deferred and static heap allocations.

use super::{AliasAnalysis, CfgInfo, MemoryCallSummaries};
use crate::mir::{BlockId, Function, FunctionId, InstId, InstKind, Module, Terminator, ValueId};
use solar_data_structures::{
    bit_set::DenseBitSet,
    map::{FxHashMap, FxHashSet},
};
use std::sync::Arc;

/// One constant-size allocation eligible for static placement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StaticAllocCandidate {
    pub(crate) block: BlockId,
    pub(crate) alloc: InstId,
    pub(crate) ptr: ValueId,
    pub(crate) size: u64,
}

/// Returns allocations eligible for deferred placement in the given functions.
pub(crate) fn eligible_deferred_allocations(
    module: &Module,
    functions: &[FunctionId],
) -> FxHashSet<(FunctionId, InstId)> {
    let summaries = Arc::new(MemoryCallSummaries::new(module));
    functions
        .iter()
        .flat_map(|&func_id| {
            let func = module.function(func_id);
            let aa = AliasAnalysis::with_call_summaries(func, Arc::clone(&summaries));
            eligible_static_allocations(func, &aa)
                .into_iter()
                .map(move |item| (func_id, item.alloc))
        })
        .collect()
}

/// Returns constant-size, non-escaping allocations that the backend may place
/// in an entry-local static region.
pub(crate) fn eligible_static_allocations(
    func: &Function,
    aa: &AliasAnalysis,
) -> Vec<StaticAllocCandidate> {
    if !is_entry(func) || has_msize(func) {
        return Vec::new();
    }

    let cfg = CfgInfo::new(func);
    let mut cyclic = FxHashMap::default();
    let mut candidates = Vec::new();
    for block in func.blocks.indices() {
        for &alloc in &func.blocks[block].instructions {
            let InstKind::Alloc { size, semantics, .. } = func.inst(alloc).kind else {
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
            let ptr = func.inst_result_value(alloc).expect("allocation must produce a value");
            let candidate = StaticAllocCandidate { block, alloc, ptr, size };
            if !aa.value_escapes(func, candidate.ptr) && candidate_uses_are_safe(func, &candidate) {
                candidates.push(candidate);
            }
        }
    }
    candidates
}

pub(crate) fn is_entry(func: &Function) -> bool {
    !func.attributes.is_constructor
        && (func.selector.is_some() || func.attributes.is_receive || func.attributes.is_fallback)
}

pub(crate) fn has_msize(func: &Function) -> bool {
    func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::MSize))
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
    let mut derived: FxHashMap<ValueId, u64> = FxHashMap::default();
    derived.insert(cand.ptr, 0);
    loop {
        let mut grew = false;
        for inst_id in func.instructions() {
            if let InstKind::Add(a, b) = func.inst(inst_id).kind
                && let Some(result) = func.inst_result_value(inst_id)
                && !derived.contains_key(&result)
            {
                let (base_offset, offset) = if let Some(&base_offset) = derived.get(&a) {
                    (base_offset, b)
                } else if let Some(&base_offset) = derived.get(&b) {
                    (base_offset, a)
                } else {
                    continue;
                };
                let Some(offset) = func.value_u64(offset) else { return false };
                let Some(total) = base_offset.checked_add(offset) else { return false };
                if total >= cand.size {
                    return false;
                }
                derived.insert(result, total);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }

    let in_range = |off: u64, len: u64| off.checked_add(len).is_some_and(|end| end <= cand.size);
    for block in func.blocks.iter() {
        for &inst_id in &block.instructions {
            if inst_id == cand.alloc {
                continue;
            }
            let kind = &func.inst(inst_id).kind;
            for &operand in kind.operands().iter() {
                let Some(&off) = derived.get(&operand) else { continue };
                let ok = match *kind {
                    InstKind::MLoad(addr) => operand == addr && in_range(off, 32),
                    InstKind::MStore(addr, value) => {
                        operand == addr && value != operand && in_range(off, 32)
                    }
                    InstKind::Keccak256(addr, size)
                    | InstKind::Log0(addr, size)
                    | InstKind::MemoryZero(addr, size)
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
                    InstKind::Add(_, _) => func
                        .inst_result_value(inst_id)
                        .is_some_and(|result| derived.contains_key(&result)),
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

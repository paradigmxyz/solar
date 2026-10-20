//! Memory facts for the checks of the `@custom:solar-*` tags that run on a contract's MIR right
//! after lowering, before any pass.

use super::*;
use crate::mir::{
    InstId,
    analysis::{AliasAnalysis, MemoryBase, MemoryCallSummaries},
};

/// A function prepared for a tag check: a copy whose trivial phis are resolved, with its alias
/// analysis.
///
/// Lowering gives a loop header a phi for every variable in scope, including the ones the loop
/// never assigns. The alias analysis cannot see through a phi that merges a pointer with
/// itself, so every write through such a variable would reach anything.
pub(super) struct Analyzed {
    pub(super) func: Function,
    /// The value each trivial phi merges, by the phi's result.
    replacements: FxHashMap<ValueId, ValueId>,
    pub(super) aa: AliasAnalysis,
    /// The reads of the free memory pointer that no reset of it may precede.
    fresh_fmps: FxHashSet<InstId>,
}

impl Analyzed {
    pub(super) fn new(func: &Function, calls: &Arc<MemoryCallSummaries>) -> Self {
        let mut replacements = FxHashMap::default();
        // A phi is trivial when every incoming value other than the phi itself is one value.
        loop {
            let mut changed = false;
            for inst in func.instructions() {
                let InstKind::Phi(incoming) = &func.inst(inst).kind else { continue };
                let Some(result) = func.inst_result_value(inst) else { continue };
                if replacements.contains_key(&result) {
                    continue;
                }
                let mut merged = None;
                let trivial = incoming.iter().all(|&(_, value)| {
                    let value = crate::mir::utils::resolve_replacement(value, &replacements);
                    value == result || *merged.get_or_insert(value) == value
                });
                if trivial && let Some(value) = merged {
                    replacements.insert(result, value);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let replacements = replacements
            .keys()
            .map(|&value| (value, crate::mir::utils::resolve_replacement(value, &replacements)))
            .collect::<FxHashMap<_, _>>();
        let mut func = func.clone();
        func.replace_uses(&replacements);
        let aa = AliasAnalysis::with_call_summaries(&func, Arc::clone(calls));
        let fresh_fmps = fresh_fmps(&func, &aa);
        Self { func, replacements, aa, fresh_fmps }
    }

    /// The value `value` stands for once trivial phis are resolved.
    pub(super) fn resolve(&self, value: ValueId) -> ValueId {
        self.replacements.get(&value).copied().unwrap_or(value)
    }

    /// The instruction `value` is the result of, when no object that exists before it runs lies
    /// at or past `value`: a fresh allocation, or a read of the free memory pointer that no reset
    /// of it may precede.
    ///
    /// The alias analysis bases a write past a fresh allocation's known extent, or anywhere past
    /// the free memory pointer, on that value. Such a write may reach objects allocated after the
    /// instruction, but none that existed before it.
    pub(super) fn fresh_start(&self, value: ValueId) -> Option<InstId> {
        let Value::Inst(inst) = *self.func.value(value) else { return None };
        if self.fresh_fmps.contains(&inst) {
            return Some(inst);
        }
        match self.aa.memory_address(&self.func, value)?.base {
            MemoryBase::Allocation(site) | MemoryBase::DynamicAllocation(site) if site == inst => {
                Some(site)
            }
            _ => None,
        }
    }

    /// The object `pointer` points into: the pointer it offsets, through every `ptradd` and
    /// pointer cast.
    ///
    /// Lowering emits `ptradd` only for addresses inside the object its base points to, so the
    /// base names the object a store through `pointer` writes.
    pub(super) fn object(&self, mut pointer: ValueId) -> ValueId {
        while let Value::Inst(inst) = *self.func.value(pointer) {
            pointer = match self.func.inst(inst).kind {
                InstKind::PtrAdd(base, _)
                | InstKind::IntToPtr(base)
                | InstKind::PtrToInt(base, 256) => base,
                _ => break,
            };
        }
        pointer
    }
}

/// The reads of the free memory pointer in `func` that no instruction that may reset it precedes
/// on any path.
fn fresh_fmps(func: &Function, aa: &AliasAnalysis) -> FxHashSet<InstId> {
    let mut fresh = FxHashSet::default();
    if !func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::Fmp)) {
        return fresh;
    }
    let mut resets = DenseBitSet::new_empty(func.blocks.len());
    for (id, block) in func.blocks.iter_enumerated() {
        if block.instructions.iter().any(|&inst| aa.instruction_may_reset_fmp(func, inst)) {
            resets.insert(id);
        }
    }
    // Blocks some path into which may reset the free memory pointer.
    let mut reached = DenseBitSet::new_empty(func.blocks.len());
    let mut poisoned = DenseBitSet::new_empty(func.blocks.len());
    let mut worklist = vec![BlockId::ENTRY];
    reached.insert(BlockId::ENTRY);
    while let Some(block) = worklist.pop() {
        let out = poisoned.contains(block) || resets.contains(block);
        let Some(terminator) = &func.blocks[block].terminator else { continue };
        for successor in terminator.successors() {
            let mut changed = reached.insert(successor);
            if out {
                changed |= poisoned.insert(successor);
            }
            if changed {
                worklist.push(successor);
            }
        }
    }
    for (block, data) in func.blocks.iter_enumerated() {
        if !reached.contains(block) {
            continue;
        }
        let mut reset = poisoned.contains(block);
        for &inst in &data.instructions {
            if !reset && matches!(func.inst(inst).kind, InstKind::Fmp) {
                fresh.insert(inst);
            }
            reset |= aa.instruction_may_reset_fmp(func, inst);
        }
    }
    fresh
}

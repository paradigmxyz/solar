//! Memory facts shared by the checks of the `@custom:solar-*` tags that run on lowered MIR.

use super::*;
use crate::mir::{
    InstId,
    analysis::{AliasAnalysis, MemoryBase, MemoryCallSummaries, MemoryLocation},
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
        Self { func, replacements, aa }
    }

    /// The value `value` stands for once trivial phis are resolved.
    pub(super) fn resolve(&self, value: ValueId) -> ValueId {
        self.replacements.get(&value).copied().unwrap_or(value)
    }

    /// The allocation `value` is the result of, when the allocation is fresh.
    ///
    /// The alias analysis bases a write past a fresh allocation's known extent on the
    /// allocation's result. Such a write may reach objects allocated after the allocation, but
    /// none that existed before it.
    pub(super) fn fresh_allocation(&self, value: ValueId) -> Option<InstId> {
        let Value::Inst(inst) = *self.func.value(value) else { return None };
        match self.aa.memory_address(&self.func, value)?.base {
            MemoryBase::Allocation(site) | MemoryBase::DynamicAllocation(site) if site == inst => {
                Some(site)
            }
            _ => None,
        }
    }
}

/// Whether `location` lies in the reserved words below the heap, which hold no object's payload.
pub(super) fn below_heap(location: MemoryLocation) -> bool {
    location
        .size
        .as_const()
        .and_then(|size| location.address.offset.checked_add(size))
        .is_some_and(|end| end <= EvmMemoryLayout::HEAP_START)
}

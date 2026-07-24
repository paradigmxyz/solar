//! Module-level call graph facts for MIR.

use crate::mir::{Function, FunctionId, InstKind, Module, Terminator};
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use std::collections::VecDeque;

/// Module-level internal-call graph facts.
#[derive(Clone, Debug)]
pub(crate) struct CallGraphInfo {
    callees: FxHashMap<FunctionId, DenseBitSet<FunctionId>>,
    reachable_from_entries: DenseBitSet<FunctionId>,
    recursive_functions: DenseBitSet<FunctionId>,
}

impl CallGraphInfo {
    /// Computes call graph facts for `module`.
    #[must_use]
    pub(crate) fn new(module: &Module) -> Self {
        let function_count = module.function_count();
        let mut callees = FxHashMap::default();
        let mut entry_functions = DenseBitSet::new_empty(function_count);

        for (func_id, func) in module.iter_functions() {
            if Self::is_entry_function(func) {
                entry_functions.insert(func_id);
            }

            let direct_callees = Self::collect_internal_callees(func, function_count);
            if !direct_callees.is_empty() {
                callees.insert(func_id, direct_callees);
            }
        }

        let reachable_from_entries =
            Self::reachable_from_roots_in_graph(&callees, &entry_functions);
        let recursive_functions = Self::recursive_functions_in_graph(&callees, function_count);

        Self { callees, reachable_from_entries, recursive_functions }
    }

    /// Returns all functions reachable from entry functions.
    #[must_use]
    pub(crate) fn reachable_from_entries(&self) -> &DenseBitSet<FunctionId> {
        &self.reachable_from_entries
    }

    /// Returns true if `func` is directly or indirectly recursive.
    #[must_use]
    pub(crate) fn is_recursive(&self, func: FunctionId) -> bool {
        self.recursive_functions.contains(func)
    }

    /// Returns functions reachable from `roots` through MIR call edges.
    #[must_use]
    pub(crate) fn reachable_callees_from(
        &self,
        roots: impl IntoIterator<Item = FunctionId>,
    ) -> DenseBitSet<FunctionId> {
        let mut reachable = DenseBitSet::new_empty(self.reachable_from_entries.domain_size());
        let mut worklist: VecDeque<_> = roots.into_iter().collect();

        while let Some(func) = worklist.pop_front() {
            let Some(callees) = self.callees.get(&func) else { continue };
            for callee in callees {
                if reachable.insert(callee) {
                    worklist.push_back(callee);
                }
            }
        }

        reachable
    }

    fn collect_internal_callees(func: &Function, function_count: usize) -> DenseBitSet<FunctionId> {
        let mut callees = DenseBitSet::new_empty(function_count);
        for inst in func.instructions() {
            if let InstKind::InternalCall { function, .. } = inst.kind {
                callees.insert(function);
            }
        }
        // Tail calls transfer control to another function body: for
        // reachability and recursion purposes they are call edges.
        for block in func.blocks() {
            if let Some(Terminator::TailCall { function, .. }) = &block.terminator {
                callees.insert(*function);
            }
        }
        callees
    }

    fn is_entry_function(func: &Function) -> bool {
        func.selector.is_some()
            || func.attributes.is_constructor
            || func.attributes.is_fallback
            || func.attributes.is_receive
    }

    fn reachable_from_roots_in_graph(
        callees: &FxHashMap<FunctionId, DenseBitSet<FunctionId>>,
        roots: &DenseBitSet<FunctionId>,
    ) -> DenseBitSet<FunctionId> {
        let mut reachable = DenseBitSet::new_empty(roots.domain_size());
        let mut worklist = VecDeque::new();
        for root in roots {
            reachable.insert(root);
            worklist.push_back(root);
        }

        while let Some(func) = worklist.pop_front() {
            let Some(callees) = callees.get(&func) else { continue };
            for callee in callees {
                if reachable.insert(callee) {
                    worklist.push_back(callee);
                }
            }
        }

        reachable
    }

    fn recursive_functions_in_graph(
        callees: &FxHashMap<FunctionId, DenseBitSet<FunctionId>>,
        function_count: usize,
    ) -> DenseBitSet<FunctionId> {
        let mut recursive = DenseBitSet::new_empty(function_count);
        for &func_id in callees.keys() {
            if Self::has_cycle_from(func_id, callees, &mut DenseBitSet::new_empty(function_count)) {
                recursive.insert(func_id);
            }
        }
        recursive
    }

    fn has_cycle_from(
        func_id: FunctionId,
        callees: &FxHashMap<FunctionId, DenseBitSet<FunctionId>>,
        visiting: &mut DenseBitSet<FunctionId>,
    ) -> bool {
        if !visiting.insert(func_id) {
            return true;
        }

        let recursive = callees.get(&func_id).is_some_and(|direct_callees| {
            direct_callees.iter().any(|callee| Self::has_cycle_from(callee, callees, visiting))
        });
        visiting.remove(func_id);
        recursive
    }
}

//! Module-level call graph facts for MIR.

use super::CfgInfo;
use crate::mir::{Function, FunctionId, InstKind, Module, Terminator};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
use std::collections::VecDeque;

/// Module-level internal-call graph facts.
#[derive(Clone, Debug)]
pub(crate) struct CallGraphInfo {
    callees: FxHashMap<FunctionId, DenseBitSet<FunctionId>>,
    reachable_from_entries: DenseBitSet<FunctionId>,
    recursive_functions: DenseBitSet<FunctionId>,
    callee_first: Vec<FunctionId>,
}

/// Call edges that the backend can frame as non-returning tail calls.
pub(crate) struct TailCallEligibility {
    callees: DenseBitSet<FunctionId>,
    constructor_reachable: DenseBitSet<FunctionId>,
    recursive: DenseBitSet<FunctionId>,
    callee_first: Vec<FunctionId>,
}

impl TailCallEligibility {
    pub(crate) fn new(module: &Module) -> Self {
        let call_graph = CallGraphInfo::new(module);
        let mut callees = DenseBitSet::new_empty(module.functions.len());
        for (func_id, func) in module.functions.iter_enumerated() {
            if func.selector.is_none()
                && !func.attributes.is_receive
                && !func.attributes.is_fallback
                && !call_graph.is_recursive(func_id)
                && function_cannot_return(func)
            {
                callees.insert(func_id);
            }
        }

        let mut constructor_reachable = call_graph.reachable_callees_from(
            module
                .functions
                .iter_enumerated()
                .filter_map(|(id, func)| func.attributes.is_constructor.then_some(id)),
        );
        for (id, func) in module.functions.iter_enumerated() {
            if func.attributes.is_constructor {
                constructor_reachable.insert(id);
            }
        }
        let recursive = call_graph.recursive_functions;
        let callee_first = call_graph.callee_first;
        Self { callees, constructor_reachable, recursive, callee_first }
    }

    pub(crate) fn contains(&self, caller: FunctionId, callee: FunctionId) -> bool {
        self.callees.contains(callee) && !self.constructor_reachable.contains(caller)
    }

    pub(crate) fn callee_first(&self) -> &[FunctionId] {
        &self.callee_first
    }

    pub(crate) fn refresh_callee(&mut self, module: &Module, func_id: FunctionId) {
        let func = module.function(func_id);
        if func.selector.is_none()
            && !func.attributes.is_receive
            && !func.attributes.is_fallback
            && !self.recursive.contains(func_id)
            && function_cannot_return(func)
        {
            self.callees.insert(func_id);
        }
    }

    pub(crate) fn same_eligible_calls(&self, other: &Self) -> bool {
        self.callees == other.callees && self.constructor_reachable == other.constructor_reachable
    }
}

impl CallGraphInfo {
    /// Computes call graph facts for `module`.
    #[must_use]
    pub(crate) fn new(module: &Module) -> Self {
        let function_count = module.functions.len();
        let mut callees = FxHashMap::default();
        let mut entry_functions = DenseBitSet::new_empty(function_count);

        for (func_id, func) in module.functions.iter_enumerated() {
            if Self::is_entry_function(func) {
                entry_functions.insert(func_id);
            }

            let direct_callees = Self::direct_callees(func, function_count);
            if !direct_callees.is_empty() {
                callees.insert(func_id, direct_callees);
            }
        }

        let reachable_from_entries =
            Self::reachable_from_roots_in_graph(&callees, &entry_functions);
        let (recursive_functions, callee_first) =
            Self::recursive_functions_in_graph(&callees, function_count);

        Self { callees, reachable_from_entries, recursive_functions, callee_first }
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

    /// Returns the strongly connected recursive component containing `root`.
    #[must_use]
    pub(crate) fn recursive_component(&self, root: FunctionId) -> DenseBitSet<FunctionId> {
        let mut component = DenseBitSet::new_empty(self.reachable_from_entries.domain_size());
        if !self.is_recursive(root) {
            return component;
        }

        let reachable = self.reachable_callees_from([root]);
        for func in self.recursive_functions.iter() {
            if (func == root || reachable.contains(func))
                && self.reachable_callees_from([func]).contains(root)
            {
                component.insert(func);
            }
        }
        component
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

    pub(crate) fn direct_callees(
        func: &Function,
        function_count: usize,
    ) -> DenseBitSet<FunctionId> {
        let mut callees = DenseBitSet::new_empty(function_count);
        for inst_id in func.instructions() {
            if let InstKind::InternalCall { function, .. } = func.inst(inst_id).kind {
                callees.insert(function);
            }
        }
        // Tail calls transfer control to another function body: for
        // reachability and recursion purposes they are call edges.
        for block in func.blocks.iter() {
            if let Some(Terminator::TailCall { function, .. }) = &block.terminator {
                callees.insert(*function);
            }
        }
        callees
    }

    fn is_entry_function(func: &Function) -> bool {
        func.selector.is_some()
            || func.attributes.is_dispatch_entry
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
    ) -> (DenseBitSet<FunctionId>, Vec<FunctionId>) {
        // Find strongly connected components with Kosaraju's algorithm. The previous search
        // started a fresh DFS at every function, making repeated call-graph analyses quadratic on
        // large modules even when their call graph was sparse.
        let mut reverse = IndexVec::<FunctionId, Vec<FunctionId>>::with_capacity(function_count);
        for _ in 0..function_count {
            reverse.push(Vec::new());
        }
        for (&caller, direct_callees) in callees {
            for callee in direct_callees {
                reverse[callee].push(caller);
            }
        }

        let mut visited = DenseBitSet::new_empty(function_count);
        let mut finish_order = Vec::with_capacity(function_count);
        for root in (0..function_count).map(FunctionId::from_usize) {
            if visited.contains(root) {
                continue;
            }
            let mut stack = vec![(root, false)];
            while let Some((func, expanded)) = stack.pop() {
                if expanded {
                    finish_order.push(func);
                    continue;
                }
                // Mark a node when its DFS frame is entered, not when the parent discovers it.
                // Pre-marking all siblings can violate postorder when one sibling reaches another.
                if !visited.insert(func) {
                    continue;
                }
                stack.push((func, true));
                if let Some(direct_callees) = callees.get(&func) {
                    for callee in direct_callees {
                        if !visited.contains(callee) {
                            stack.push((callee, false));
                        }
                    }
                }
            }
        }

        let mut assigned = DenseBitSet::new_empty(function_count);
        let mut recursive = DenseBitSet::new_empty(function_count);
        for root in finish_order.iter().rev().copied() {
            if !assigned.insert(root) {
                continue;
            }
            let mut component = Vec::new();
            let mut stack = vec![root];
            while let Some(func) = stack.pop() {
                component.push(func);
                for &caller in &reverse[func] {
                    if assigned.insert(caller) {
                        stack.push(caller);
                    }
                }
            }
            if component.len() > 1 || callees.get(&root).is_some_and(|direct| direct.contains(root))
            {
                for func in component {
                    recursive.insert(func);
                }
            }
        }
        (recursive, finish_order)
    }
}

/// Whether a function can never return to an internal caller.
fn function_cannot_return(func: &Function) -> bool {
    if func.blocks.is_empty() {
        return false;
    }
    let cfg = CfgInfo::new(func);
    !cfg.reachable().iter().any(|block| {
        matches!(func.blocks[block].terminator, Some(Terminator::Return { .. } | Terminator::Stop))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recursion_excludes_callers_outside_the_cycle() {
        let mut callees = FxHashMap::default();
        for (caller, callee) in [(0, 1), (1, 2), (2, 1)] {
            callees
                .entry(FunctionId::from_usize(caller))
                .or_insert_with(|| DenseBitSet::new_empty(3))
                .insert(FunctionId::from_usize(callee));
        }

        let (recursive, _) = CallGraphInfo::recursive_functions_in_graph(&callees, 3);
        assert!(!recursive.contains(FunctionId::from_usize(0)));
        assert!(recursive.contains(FunctionId::from_usize(1)));
        assert!(recursive.contains(FunctionId::from_usize(2)));
    }

    #[test]
    fn recursion_excludes_reachable_siblings_in_acyclic_graph() {
        let mut callees = FxHashMap::default();
        for (caller, callee) in [(0, 1), (0, 2), (2, 1)] {
            callees
                .entry(FunctionId::from_usize(caller))
                .or_insert_with(|| DenseBitSet::new_empty(3))
                .insert(FunctionId::from_usize(callee));
        }

        let (recursive, _) = CallGraphInfo::recursive_functions_in_graph(&callees, 3);
        assert!(recursive.is_empty());
    }
}

//! Module-level call graph facts for MIR.

use crate::mir::{BlockId, Function, FunctionId, InstKind, Module, Terminator};
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    index::IndexVec,
    map::FxHashMap,
};
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
        let function_count = module.functions.len();
        let mut callees = FxHashMap::default();
        let mut entry_functions = DenseBitSet::new_empty(function_count);
        if let Some(entry) = module.dispatch_entry() {
            entry_functions.insert(entry);
        }

        for (func_id, func) in module.functions.iter_enumerated() {
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

    /// Finds functions whose reachable exits all abort, including chains of
    /// calls to other cold functions.
    pub(crate) fn collect_cold_functions(module: &Module) -> DenseBitSet<FunctionId> {
        let mut cold = DenseBitSet::new_empty(module.functions.len());
        let mut worklist = Vec::new();
        let mut visited = GrowableBitSet::new_empty();
        loop {
            let mut changed = false;
            for (function_id, func) in module.functions.iter_enumerated() {
                if cold.contains(function_id) {
                    continue;
                }
                worklist.clear();
                worklist.push(BlockId::ENTRY);
                visited.clear();
                let mut saw_exit = false;
                let mut all_exits_cold = true;
                while let Some(block_id) = worklist.pop()
                    && all_exits_cold
                {
                    if !visited.insert(block_id) {
                        continue;
                    }
                    let block = &func.blocks[block_id];
                    if block.instructions.iter().any(|&inst_id| {
                        matches!(
                            func.inst(inst_id).kind,
                            InstKind::ICall { function, .. } if cold.contains(function)
                        )
                    }) {
                        saw_exit = true;
                        continue;
                    }
                    let Some(term) = block.terminator.as_ref() else {
                        all_exits_cold = false;
                        continue;
                    };
                    match term {
                        Terminator::Revert { .. }
                        | Terminator::RevertReturndata
                        | Terminator::Invalid => {
                            saw_exit = true;
                        }
                        Terminator::TailCall { function, .. } if cold.contains(*function) => {
                            saw_exit = true;
                        }
                        _ => {
                            let successors = term.successors();
                            if successors.is_empty() {
                                all_exits_cold = false;
                            } else {
                                worklist.extend(successors);
                            }
                        }
                    }
                }
                if saw_exit && all_exits_cold {
                    cold.insert(function_id);
                    changed = true;
                }
            }
            if !changed {
                return cold;
            }
        }
    }

    /// Returns functions sharing a call context with unrestricted source memory.
    /// The dispatcher is excluded: independent external entries have separate memory
    /// lifetimes, while a shared helper inherits the strictest caller's convention.
    pub(crate) fn source_only_memory_contexts(&self, module: &Module) -> DenseBitSet<FunctionId> {
        let mut unrestricted = DenseBitSet::new_empty(module.functions.len());
        for (id, func) in module.functions.iter_enumerated() {
            if func.attributes.unrestricted_memory {
                unrestricted.insert(id);
            }
        }
        let mut required = unrestricted.clone();
        if unrestricted.is_empty() {
            return required;
        }
        for id in module.functions.indices() {
            if Some(id) == module.dispatch_entry() {
                continue;
            }
            let mut reachable = self.reachable_callees_from([id]);
            reachable.insert(id);
            if unrestricted.iter().any(|callee| reachable.contains(callee)) {
                required.union(&reachable);
            }
        }
        required
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
        for inst_id in func.instructions() {
            if let InstKind::ICall { function, .. } = func.inst(inst_id).kind {
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
        for root in finish_order.into_iter().rev() {
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
        recursive
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::FunctionBuilder;
    use solar_interface::Ident;

    #[test]
    fn cold_calls_exclude_normally_returning_tails() {
        let mut module = Module::new(Ident::DUMMY);
        let mut abort = Function::new(Ident::DUMMY);
        // abort: revert 0, 0
        let mut builder = FunctionBuilder::new(&mut abort);
        let zero = builder.imm(0);
        builder.revert(zero, zero);
        let abort = module.add_function(abort);

        let mut returning = Function::new(Ident::DUMMY);
        // returning: ret
        FunctionBuilder::new(&mut returning).ret([]);
        let returning = module.add_function(returning);

        let mut cold_tail = Function::new(Ident::DUMMY);
        // cold_tail: tail_call abort
        FunctionBuilder::new(&mut cold_tail).tail_call(abort, Vec::new());
        let cold_tail = module.add_function(cold_tail);
        let mut warm_tail = Function::new(Ident::DUMMY);
        // warm_tail: tail_call returning
        FunctionBuilder::new(&mut warm_tail).tail_call(returning, Vec::new());
        module.add_function(warm_tail);

        assert_eq!(
            CallGraphInfo::collect_cold_functions(&module).iter().collect::<Vec<_>>(),
            [abort, cold_tail]
        );
    }

    #[test]
    fn recursion_excludes_callers_outside_the_cycle() {
        let mut callees = FxHashMap::default();
        for (caller, callee) in [(0, 1), (1, 2), (2, 1)] {
            callees
                .entry(FunctionId::from_usize(caller))
                .or_insert_with(|| DenseBitSet::new_empty(3))
                .insert(FunctionId::from_usize(callee));
        }

        let recursive = CallGraphInfo::recursive_functions_in_graph(&callees, 3);
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

        let recursive = CallGraphInfo::recursive_functions_in_graph(&callees, 3);
        assert!(recursive.is_empty());
    }
}

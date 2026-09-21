//! Module-level call graph facts for MIR.

use crate::mir::{Callee, Function, FunctionId, InstKind, Module, Terminator};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
use std::collections::VecDeque;

/// Module-level internal-call graph facts.
#[derive(Clone, Debug)]
pub(crate) struct CallGraphInfo {
    callees: FxHashMap<FunctionId, DenseBitSet<FunctionId>>,
    reachable_from_entries: DenseBitSet<FunctionId>,
    recursive_functions: DenseBitSet<FunctionId>,
}

impl CallGraphInfo {
    /// Checks the phase contract for helpers that require runtime static frames.
    /// Constructor ABI staging may already occupy the free-memory pointer, so
    /// these helpers must not be reachable through any constructor call chain.
    pub(crate) fn assert_runtime_helpers(
        module: &Module,
        helpers: impl IntoIterator<Item = FunctionId>,
    ) {
        if cfg!(debug_assertions) {
            let helpers = helpers.into_iter().collect::<Vec<_>>();
            if helpers.is_empty() {
                return;
            }
            let graph = Self::new(module);
            let reachable = graph.reachable_callees_from(
                module
                    .functions
                    .iter_enumerated()
                    .filter_map(|(id, function)| function.attributes.is_constructor.then_some(id)),
            );
            for helper in helpers {
                assert!(
                    !reachable.contains(helper)
                        && !module.function(helper).attributes.is_constructor,
                    "runtime helper `{}` is constructor-reachable and may overlap ABI staging",
                    module.function(helper).name
                );
            }
        }
    }

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

    /// A caller processing order that visits callees before their callers.
    ///
    /// Both LLVM's inliner (post-order SCC walk) and GCC's IPA inliner process
    /// the call graph bottom-up, so a shared wrapper's single-use inner helper
    /// is consumed before the wrapper's body is duplicated at its call sites;
    /// otherwise every clone deposits its own copy of the inner call. Recursive
    /// components have no topological order; their members are rejected by the
    /// recursion check anyway, so ties break by function index.
    #[must_use]
    pub(crate) fn bottom_up_order(&self, module: &Module) -> Vec<FunctionId> {
        let mut visited = DenseBitSet::new_empty(module.functions.len());
        let mut order = Vec::with_capacity(module.functions.len());
        let mut stack = Vec::new();
        for root in module.functions.indices() {
            stack.push((root, false));
            while let Some((func, expanded)) = stack.pop() {
                if expanded {
                    order.push(func);
                } else if visited.insert(func) {
                    stack.push((func, true));
                    if let Some(callees) = self.callees.get(&func) {
                        // Reverse the pending siblings to retain index-ordered DFS.
                        // Mark on entry, not discovery: siblings may reach each other.
                        let start = stack.len();
                        stack.extend(callees.iter().map(|callee| (callee, false)));
                        stack[start..].reverse();
                    }
                }
            }
        }
        order
    }

    /// Collects direct call targets, including tail calls, without building graph analyses.
    pub(crate) fn collect_internal_callees(
        func: &Function,
        function_count: usize,
    ) -> DenseBitSet<FunctionId> {
        let mut callees = DenseBitSet::new_empty(function_count);
        for inst_id in func.instructions() {
            if let InstKind::ICall { function: Callee::Function(function), .. } =
                func.inst(inst_id).kind
            {
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

    #[test]
    fn bottom_up_order_handles_deep_chains_and_sibling_edges() {
        use solar_interface::Ident;

        // A small thread stack catches recursive DFS without relying on the
        // machine's default stack size. The extra sibling edge checks postorder.
        std::thread::Builder::new()
            .stack_size(128 * 1024)
            .spawn(|| {
                let count = 2048;
                let mut module = Module::new(Ident::DUMMY);
                let mut callees = FxHashMap::default();
                for i in 0..count {
                    module.functions.push(Function::new(Ident::DUMMY));
                    if i + 1 < count {
                        let mut next = DenseBitSet::new_empty(count);
                        next.insert(FunctionId::from_usize(i + 1));
                        if i == 0 {
                            next.insert(FunctionId::from_usize(2));
                        }
                        callees.insert(FunctionId::from_usize(i), next);
                    }
                }
                let graph = CallGraphInfo {
                    callees,
                    reachable_from_entries: DenseBitSet::new_empty(count),
                    recursive_functions: DenseBitSet::new_empty(count),
                };
                let order = graph.bottom_up_order(&module);
                let expected: Vec<_> = (0..count).rev().map(FunctionId::from_usize).collect();
                assert_eq!(order, expected);
            })
            .unwrap()
            .join()
            .unwrap();
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

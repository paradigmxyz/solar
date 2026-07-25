//! Module-level call graph facts for MIR.

use crate::mir::{Function, FunctionId, InstKind, Module, Terminator};
use solar_data_structures::{bit_set::DenseBitSet, index::index_vec, map::FxHashMap};
use std::{cell::OnceCell, collections::VecDeque};

/// Module-level internal-call graph facts.
#[derive(Clone, Debug)]
pub(crate) struct CallGraphInfo {
    callees: FxHashMap<FunctionId, DenseBitSet<FunctionId>>,
    reachable_from_entries: DenseBitSet<FunctionId>,
    recursion: OnceCell<(DenseBitSet<FunctionId>, DenseBitSet<FunctionId>)>,
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

            let direct_callees = Self::collect_internal_callees(func, function_count);
            if !direct_callees.is_empty() {
                callees.insert(func_id, direct_callees);
            }
        }

        let reachable_from_entries =
            Self::reachable_from_roots_in_graph(&callees, &entry_functions);

        Self { callees, reachable_from_entries, recursion: OnceCell::new() }
    }

    /// Returns all functions reachable from entry functions.
    #[must_use]
    pub(crate) fn reachable_from_entries(&self) -> &DenseBitSet<FunctionId> {
        &self.reachable_from_entries
    }

    /// Returns true if `func` is directly or indirectly recursive.
    #[must_use]
    pub(crate) fn is_recursive(&self, func: FunctionId) -> bool {
        self.recursion().1.contains(func)
    }

    /// Returns true if `func` belongs to a recursive call-graph component.
    #[must_use]
    pub(crate) fn is_recursive_cycle_member(&self, func: FunctionId) -> bool {
        self.recursion().0.contains(func)
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

    fn recursion(&self) -> &(DenseBitSet<FunctionId>, DenseBitSet<FunctionId>) {
        self.recursion.get_or_init(|| {
            Self::recursive_functions_in_graph(
                &self.callees,
                self.reachable_from_entries.domain_size(),
            )
        })
    }

    fn collect_internal_callees(func: &Function, function_count: usize) -> DenseBitSet<FunctionId> {
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
    ) -> (DenseBitSet<FunctionId>, DenseBitSet<FunctionId>) {
        let mut outgoing = index_vec![Vec::new(); function_count];
        let mut callers = index_vec![Vec::new(); function_count];
        for (&caller, direct_callees) in callees {
            for callee in direct_callees {
                outgoing[caller].push(callee);
                callers[callee].push(caller);
            }
        }

        let mut visited = DenseBitSet::new_empty(function_count);
        let mut postorder = Vec::with_capacity(function_count);
        let mut stack = Vec::new();
        for root in outgoing.indices() {
            if !visited.insert(root) {
                continue;
            }
            stack.push((root, 0));
            while let Some((function, next)) = stack.last_mut() {
                if let Some(&callee) = outgoing[*function].get(*next) {
                    *next += 1;
                    if visited.insert(callee) {
                        stack.push((callee, 0));
                    }
                } else {
                    postorder.push(*function);
                    stack.pop();
                }
            }
        }

        let mut assigned = DenseBitSet::new_empty(function_count);
        let mut recursive_cycle_members = DenseBitSet::new_empty(function_count);
        let mut component = Vec::new();
        for root in postorder.into_iter().rev() {
            if !assigned.insert(root) {
                continue;
            }
            component.clear();
            component.push(root);
            stack.push((root, 0));
            while let Some((function, next)) = stack.last_mut() {
                if let Some(&caller) = callers[*function].get(*next) {
                    *next += 1;
                    if assigned.insert(caller) {
                        component.push(caller);
                        stack.push((caller, 0));
                    }
                } else {
                    stack.pop();
                }
            }
            if component.len() > 1 || outgoing[root].contains(&root) {
                for &function in &component {
                    recursive_cycle_members.insert(function);
                }
            }
        }

        let mut recursive = recursive_cycle_members.clone();
        let mut pending = recursive.iter().collect::<Vec<_>>();
        while let Some(function) = pending.pop() {
            for &caller in &callers[function] {
                if recursive.insert(caller) {
                    pending.push(caller);
                }
            }
        }
        (recursive_cycle_members, recursive)
    }
}

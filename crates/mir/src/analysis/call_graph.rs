//! Module-level call graph facts for MIR.

use crate::{Callee, Function, FunctionId, InstKind, Module, Terminator};
use solar_data_structures::{bit_set::DenseBitSet, graph::sccs, map::FxHashMap};
use std::collections::VecDeque;

/// Module-level internal-call graph facts.
#[derive(Clone, Debug)]
pub struct CallGraphInfo {
    callees: FxHashMap<FunctionId, DenseBitSet<FunctionId>>,
    reachable_from_entries: DenseBitSet<FunctionId>,
    recursive_functions: DenseBitSet<FunctionId>,
}

impl CallGraphInfo {
    /// Computes call graph facts for `module`.
    #[must_use]
    pub fn new(module: &Module) -> Self {
        let function_count = module.functions.len();
        let mut callees = FxHashMap::default();
        let mut entry_functions = DenseBitSet::new_empty(function_count);
        if let Some(entry) = module.dispatch_entry() {
            entry_functions.insert(entry);
        }

        for (func_id, func) in module.functions.iter_enumerated() {
            if func.is_external_entry() {
                entry_functions.insert(func_id);
            }

            let direct_callees = Self::collect_internal_callees(func, function_count);
            if !direct_callees.is_empty() {
                callees.insert(func_id, direct_callees);
            }
        }

        let mut reachable_from_entries =
            reachable_in_graph(&callees, &entry_functions, function_count);
        reachable_from_entries.union(&entry_functions);
        let recursive_functions = Self::recursive_functions_in_graph(&callees, function_count);

        Self { callees, reachable_from_entries, recursive_functions }
    }

    /// Returns all functions reachable from entry functions.
    #[must_use]
    pub fn reachable_from_entries(&self) -> &DenseBitSet<FunctionId> {
        &self.reachable_from_entries
    }

    /// Returns true if `func` is directly or indirectly recursive.
    #[must_use]
    pub fn is_recursive(&self, func: FunctionId) -> bool {
        self.recursive_functions.contains(func)
    }

    /// Returns the strongly connected recursive component containing `root`.
    #[must_use]
    pub fn recursive_component(&self, root: FunctionId) -> DenseBitSet<FunctionId> {
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

    /// Returns the functions `func` calls or tail-calls directly, in ascending order.
    pub fn callees(&self, func: FunctionId) -> impl Iterator<Item = FunctionId> + '_ {
        self.callees.get(&func).into_iter().flatten()
    }

    /// Returns functions reachable from `roots` through MIR call edges.
    #[must_use]
    pub fn reachable_callees_from(
        &self,
        roots: impl IntoIterator<Item = FunctionId>,
    ) -> DenseBitSet<FunctionId> {
        reachable_in_graph(&self.callees, roots, self.reachable_from_entries.domain_size())
    }

    /// Collects direct call targets, including tail calls, without building graph analyses.
    pub fn collect_internal_callees(
        func: &Function,
        function_count: usize,
    ) -> DenseBitSet<FunctionId> {
        let mut callees = DenseBitSet::new_empty(function_count);
        for function in direct_callees(func) {
            callees.insert(function);
        }
        callees
    }

    fn recursive_functions_in_graph(
        callees: &FxHashMap<FunctionId, DenseBitSet<FunctionId>>,
        function_count: usize,
    ) -> DenseBitSet<FunctionId> {
        let mut recursive = DenseBitSet::new_empty(function_count);
        let components = sccs(function_count, |func| callees.get(&func).into_iter().flatten());
        for scc in components {
            // A component is recursive with several functions or one function that calls itself.
            if scc.len() > 1 || callees.get(&scc[0]).is_some_and(|direct| direct.contains(scc[0])) {
                for func in scc {
                    recursive.insert(func);
                }
            }
        }
        recursive
    }
}

/// Returns the functions reachable from `roots` through `callees`; a root is included only if a
/// path leads back to it.
fn reachable_in_graph(
    callees: &FxHashMap<FunctionId, DenseBitSet<FunctionId>>,
    roots: impl IntoIterator<Item = FunctionId>,
    function_count: usize,
) -> DenseBitSet<FunctionId> {
    let mut reachable = DenseBitSet::new_empty(function_count);
    let mut worklist: VecDeque<_> = roots.into_iter().collect();

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

/// Returns the functions `func` calls directly, then those it tail-calls, with repeats.
///
/// Tail calls transfer control to another function body: for reachability and recursion
/// purposes they are call edges.
pub fn direct_callees(func: &Function) -> impl Iterator<Item = FunctionId> + '_ {
    let calls = func.instructions().filter_map(|inst| match func.inst(inst).kind {
        InstKind::ICall { function: Callee::Function(function), .. } => Some(function),
        _ => None,
    });
    let tail_calls = func.blocks.iter().filter_map(|block| match block.terminator {
        Some(Terminator::TailCall { function, .. }) => Some(function),
        _ => None,
    });
    calls.chain(tail_calls)
}

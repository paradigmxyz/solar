//! Module-level call graph facts for MIR.

use crate::mir::{Function, FunctionId, InstKind, Module};
use solar_data_structures::map::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

/// Module-level internal-call graph facts.
#[derive(Clone, Debug)]
pub struct CallGraphInfo {
    callees: FxHashMap<FunctionId, FxHashSet<FunctionId>>,
    callers: FxHashMap<FunctionId, FxHashSet<FunctionId>>,
    entry_functions: FxHashSet<FunctionId>,
    reachable_from_entries: FxHashSet<FunctionId>,
    recursive_functions: FxHashSet<FunctionId>,
    has_body: FxHashSet<FunctionId>,
}

impl CallGraphInfo {
    /// Computes call graph facts for `module`.
    #[must_use]
    pub fn new(module: &Module) -> Self {
        let mut callees: FxHashMap<FunctionId, FxHashSet<FunctionId>> = FxHashMap::default();
        let mut callers: FxHashMap<FunctionId, FxHashSet<FunctionId>> = FxHashMap::default();
        let mut entry_functions = FxHashSet::default();
        let mut has_body = FxHashSet::default();

        for (func_id, func) in module.functions.iter_enumerated() {
            if Self::is_entry_function(func) {
                entry_functions.insert(func_id);
            }
            if Self::has_body(func) {
                has_body.insert(func_id);
            }

            let direct_callees = Self::collect_internal_callees(func);
            if !direct_callees.is_empty() {
                for &callee in &direct_callees {
                    callers.entry(callee).or_default().insert(func_id);
                }
                callees.insert(func_id, direct_callees);
            }
        }

        let reachable_from_entries =
            Self::reachable_from_roots_in_graph(&callees, &entry_functions);
        let recursive_functions = Self::recursive_functions_in_graph(&callees);

        Self {
            callees,
            callers,
            entry_functions,
            reachable_from_entries,
            recursive_functions,
            has_body,
        }
    }

    /// Returns functions directly called by `func`.
    #[must_use]
    pub fn callees(&self, func: FunctionId) -> Option<&FxHashSet<FunctionId>> {
        self.callees.get(&func)
    }

    /// Returns functions that directly call `func`.
    #[must_use]
    pub fn callers(&self, func: FunctionId) -> Option<&FxHashSet<FunctionId>> {
        self.callers.get(&func)
    }

    /// Returns entry functions: external ABI entries, constructor, fallback, and receive.
    #[must_use]
    pub fn entry_functions(&self) -> &FxHashSet<FunctionId> {
        &self.entry_functions
    }

    /// Returns all functions reachable from entry functions.
    #[must_use]
    pub fn reachable_from_entries(&self) -> &FxHashSet<FunctionId> {
        &self.reachable_from_entries
    }

    /// Returns true if `func` is directly or indirectly recursive.
    #[must_use]
    pub fn is_recursive(&self, func: FunctionId) -> bool {
        self.recursive_functions.contains(&func)
    }

    /// Returns functions reachable from `roots` that have MIR bodies.
    #[must_use]
    pub fn reachable_bodies_from(
        &self,
        roots: impl IntoIterator<Item = FunctionId>,
    ) -> FxHashSet<FunctionId> {
        let mut reachable = FxHashSet::default();
        let mut worklist: VecDeque<_> = roots.into_iter().collect();

        while let Some(func) = worklist.pop_front() {
            let Some(callees) = self.callees.get(&func) else { continue };
            for &callee in callees {
                if self.has_body.contains(&callee) && reachable.insert(callee) {
                    worklist.push_back(callee);
                }
            }
        }

        reachable
    }

    /// Returns all functions reachable from `roots`.
    #[must_use]
    pub fn reachable_from_roots(
        &self,
        roots: impl IntoIterator<Item = FunctionId>,
    ) -> FxHashSet<FunctionId> {
        let roots: FxHashSet<_> = roots.into_iter().collect();
        Self::reachable_from_roots_in_graph(&self.callees, &roots)
    }

    fn collect_internal_callees(func: &Function) -> FxHashSet<FunctionId> {
        let mut callees = FxHashSet::default();
        for inst in &func.instructions {
            if let InstKind::InternalCall { function, .. } = inst.kind {
                callees.insert(function);
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

    fn has_body(func: &Function) -> bool {
        !func.blocks.is_empty()
    }

    fn reachable_from_roots_in_graph(
        callees: &FxHashMap<FunctionId, FxHashSet<FunctionId>>,
        roots: &FxHashSet<FunctionId>,
    ) -> FxHashSet<FunctionId> {
        let mut reachable = FxHashSet::default();
        let mut worklist = VecDeque::new();
        for &root in roots {
            reachable.insert(root);
            worklist.push_back(root);
        }

        while let Some(func) = worklist.pop_front() {
            let Some(callees) = callees.get(&func) else { continue };
            for &callee in callees {
                if reachable.insert(callee) {
                    worklist.push_back(callee);
                }
            }
        }

        reachable
    }

    fn recursive_functions_in_graph(
        callees: &FxHashMap<FunctionId, FxHashSet<FunctionId>>,
    ) -> FxHashSet<FunctionId> {
        let mut recursive = FxHashSet::default();
        for &func_id in callees.keys() {
            if Self::has_cycle_from(func_id, callees, &mut FxHashSet::default()) {
                recursive.insert(func_id);
            }
        }
        recursive
    }

    fn has_cycle_from(
        func_id: FunctionId,
        callees: &FxHashMap<FunctionId, FxHashSet<FunctionId>>,
        visiting: &mut FxHashSet<FunctionId>,
    ) -> bool {
        if !visiting.insert(func_id) {
            return true;
        }

        let recursive = callees.get(&func_id).is_some_and(|direct_callees| {
            direct_callees.iter().any(|&callee| Self::has_cycle_from(callee, callees, visiting))
        });
        visiting.remove(&func_id);
        recursive
    }
}

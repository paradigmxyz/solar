//! Functions that never return normally.
//!
//! A function is cold when every exit it can reach aborts: a revert, an
//! invalid instruction, or a call or tail call to another cold function. The
//! analysis iterates to a fixpoint over the module so chains of panic helpers
//! become cold together. Optimizations use it to tell an abort edge, such as an
//! arithmetic panic inside a loop, from control flow that continues.

use crate::mir::{BlockId, Callee, FunctionId, InstKind, Module, Terminator};
use solar_data_structures::bit_set::{DenseBitSet, GrowableBitSet};

/// Finds functions whose reachable exits all abort, including chains of
/// calls to other cold functions.
pub(crate) fn cold_functions(module: &Module) -> DenseBitSet<FunctionId> {
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
                        InstKind::ICall { function: Callee::Function(function), .. } if cold.contains(function)
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

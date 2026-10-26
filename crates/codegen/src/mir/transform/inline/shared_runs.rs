//! Bytes of a function that the backend shares with other code.
//!
//! The backend's sharing passes, tail merge and outlining, turn repeated runs of instructions
//! into one body, so a family of wrappers that differ only in a selector or a slot costs little
//! more than one of them. That sharing relies on the wrappers staying functions: consuming one
//! into its single caller surrounds its code with that caller's values and control flow, the
//! runs no longer match, and the body is emitted again. The inliner adds these bytes to the body
//! it inlines, so a single-call callee whose code occurs elsewhere is consumed only when the
//! call protocol it removes outweighs the lost sharing.
//!
//! Instructions match by operation, result type, and the distance to the in-block definition of
//! each operand. Immediates, arguments, and values from other blocks are masked, as the outliner
//! turns differing pushes into parameters. Only runs of `Target::SHARED_RUN_INSTRUCTIONS`
//! consecutive instructions count. The index is built before a round of inlining and not
//! updated as callees are consumed, so a consumed twin still counts as a partner for the rest
//! of the round, which errs toward keeping calls.

use super::estimate_inst_cost;
use crate::{
    mir::{BlockId, Function, FunctionId, Module, Value},
    target::Target,
};
use solar_data_structures::map::{FxHashMap, FxHasher};
use std::hash::{Hash, Hasher};

/// How often each run of instructions occurs in the module.
pub(super) struct SharedRuns {
    counts: FxHashMap<u64, u32>,
}

impl SharedRuns {
    pub(super) fn new(module: &Module) -> Self {
        let mut counts = FxHashMap::<u64, u32>::default();
        for func in &module.functions {
            for block in func.blocks.indices() {
                for run in runs(func, block) {
                    *counts.entry(run).or_default() += 1;
                }
            }
        }
        Self { counts }
    }

    /// The estimated bytes of `callee` that lie in a run occurring more than once in the module.
    pub(super) fn shared_bytes(
        &self,
        target: &Target,
        module: &Module,
        callee: FunctionId,
    ) -> usize {
        let func = module.function(callee);
        let mut bytes = 0;
        for block in func.blocks.indices() {
            let mut runs = runs(func, block);
            // Instructions before `shared_until` lie in a run that occurs elsewhere.
            let mut shared_until = 0;
            for (index, &inst) in func.blocks[block].instructions.iter().enumerate() {
                if let Some(run) = runs.next()
                    && self.counts.get(&run).is_some_and(|&count| count > 1)
                {
                    shared_until = index + Target::SHARED_RUN_INSTRUCTIONS;
                }
                if index < shared_until {
                    bytes += estimate_inst_cost(target, module, func, &func.inst(inst).kind).0.bytes
                        as usize;
                }
            }
        }
        bytes
    }
}

/// Hashes each run of `Target::SHARED_RUN_INSTRUCTIONS` consecutive instructions of a block.
fn runs(func: &Function, block: BlockId) -> std::vec::IntoIter<u64> {
    const RUN: usize = Target::SHARED_RUN_INSTRUCTIONS;
    let instructions = &func.blocks[block].instructions;
    if instructions.len() < RUN {
        return Vec::new().into_iter();
    }
    let position = instructions
        .iter()
        .enumerate()
        .map(|(index, &inst)| (inst, index))
        .collect::<FxHashMap<_, _>>();
    let tokens = instructions
        .iter()
        .enumerate()
        .map(|(index, &inst)| {
            let inst = func.inst(inst);
            let mut key = FxHasher::default();
            inst.kind.clone_without_operands().hash(&mut key);
            inst.result_ty.hash(&mut key);
            inst.kind.visit_operands(|value| match func.value(value) {
                Value::Inst(def) => {
                    position.get(def).map(|&def| index.wrapping_sub(def)).hash(&mut key)
                }
                _ => None::<usize>.hash(&mut key),
            });
            key.finish()
        })
        .collect::<Vec<_>>();
    tokens
        .windows(RUN)
        .map(|run| {
            let mut key = FxHasher::default();
            run.hash(&mut key);
            key.finish()
        })
        .collect::<Vec<_>>()
        .into_iter()
}

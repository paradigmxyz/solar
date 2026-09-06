//! Bounded evaluator for closed, pure MIR functions.
//!
//! This pass executes no-argument functions whose reachable instructions are pure and whose control
//! flow becomes deterministic under the evaluator. It is intentionally fuel-limited and only
//! rewrites functions that end in a raw `Return`, so ABI-returning external entries are left to the
//! normal encoder path.

use crate::{
    mir::{
        BlockId, Function, Immediate, InstKind, InstructionMetadata, Module, Terminator, Value,
        ValueId,
    },
    pass::{MirPass, run_function_pass},
    utils::eval,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::map::FxHashMap;

/// Function pass for bounded pure MIR evaluation.
pub(crate) struct PureEval;

impl MirPass for PureEval {
    fn name(&self) -> &'static str {
        "pure-eval"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| {
            let changed = PureEvaluator::new().run(func).functions_folded != 0;
            let repaired = crate::mir::utils::repair_reachability_phis(func);
            changed || repaired
        })
    }
}

const DEFAULT_FUEL: usize = 10_000;

/// Statistics from bounded pure evaluation.
#[derive(Clone, Debug, Default)]
struct PureEvalStats {
    /// Number of functions folded to constant returns.
    functions_folded: usize,
}

/// Bounded pure MIR evaluator.
#[derive(Debug)]
struct PureEvaluator {
    fuel: usize,
    stats: PureEvalStats,
}

impl Default for PureEvaluator {
    fn default() -> Self {
        Self { fuel: DEFAULT_FUEL, stats: PureEvalStats::default() }
    }
}

impl PureEvaluator {
    /// Creates a new evaluator with the default fuel.
    #[must_use]
    fn new() -> Self {
        Self::default()
    }

    /// Runs the evaluator on one function.
    fn run(&mut self, func: &mut Function) -> &PureEvalStats {
        self.stats = PureEvalStats::default();
        if !func.params.is_empty() || !self.is_side_effect_free(func) {
            return &self.stats;
        }

        let Some(values) = self.evaluate(func) else {
            return &self.stats;
        };
        if values.len() != func.returns.len() {
            return &self.stats;
        }
        if self.is_already_folded(func, &values) {
            return &self.stats;
        }
        self.rewrite_to_return(func, &values);
        self.stats.functions_folded = 1;
        &self.stats
    }

    /// Returns true when the function is already in the exact shape
    /// [`Self::rewrite_to_return`] would produce, so rewriting again would
    /// report a change (and allocate fresh immediates) without progress.
    fn is_already_folded(&self, func: &Function, values: &[U256]) -> bool {
        let entry = BlockId::ENTRY;
        for (block_id, block) in func.blocks.iter_enumerated() {
            if !block.instructions.is_empty() {
                return false;
            }
            if block_id != entry && !matches!(block.terminator, Some(Terminator::Invalid)) {
                return false;
            }
        }
        let Some(Terminator::Return { values: ret }) = &func.blocks[entry].terminator else {
            return false;
        };
        ret.len() == values.len()
            && ret.iter().zip(values).all(|(&ret_value, expected)| {
                matches!(
                    func.value(ret_value),
                    Value::Immediate(imm) if imm.as_u256() == Some(*expected)
                )
            })
    }

    fn is_side_effect_free(&self, func: &Function) -> bool {
        func.instructions().all(|inst_id| !func.inst(inst_id).kind.has_side_effects())
    }

    fn evaluate(&self, func: &Function) -> Option<Vec<U256>> {
        let mut env = FxHashMap::default();
        let mut insert_immediate = |value_id| {
            if let Value::Immediate(imm) = func.value(value_id)
                && let Some(value) = imm.as_u256()
            {
                env.insert(value_id, value);
            }
        };
        for value in func.live_values() {
            insert_immediate(value);
        }

        let mut current = BlockId::ENTRY;
        let mut predecessor = None;
        let mut fuel = self.fuel;
        while fuel != 0 {
            fuel -= 1;
            let block = &func.blocks[current];

            let mut phis = SmallVec::<[(ValueId, U256); 2]>::new();
            for &inst_id in &block.instructions {
                let inst = func.inst(inst_id);
                if let InstKind::Phi(incoming) = &inst.kind {
                    let pred = predecessor?;
                    let (_, value) = incoming.iter().find(|(block, _)| *block == pred)?;
                    phis.push((func.inst_result_value(inst_id)?, self.value_const(&env, *value)?));
                }
            }
            env.extend(phis);

            for &inst_id in &block.instructions {
                let inst = func.inst(inst_id);
                if matches!(inst.kind, InstKind::Phi(..)) {
                    continue;
                }
                let result = self.eval_inst(&inst.kind, &env)?;
                if let Some(value_id) = func.inst_result_value(inst_id) {
                    env.insert(value_id, result);
                }
            }

            match block.terminator.as_ref()? {
                Terminator::Jump(target) => {
                    predecessor = Some(current);
                    current = *target;
                }
                Terminator::Branch { condition, then_block, else_block } => {
                    let condition = self.value_const(&env, *condition)?;
                    predecessor = Some(current);
                    current = if condition.is_zero() { *else_block } else { *then_block };
                }
                Terminator::Switch { value, default, cases } => {
                    let value = self.value_const(&env, *value)?;
                    predecessor = Some(current);
                    current = *default;
                    for (case, target) in cases {
                        if self.value_const(&env, *case)? == value {
                            current = *target;
                            break;
                        }
                    }
                }
                Terminator::Return { values } => {
                    return values
                        .iter()
                        .map(|&value| self.value_const(&env, value))
                        .collect::<Option<Vec<_>>>();
                }
                Terminator::ReturnData { .. }
                | Terminator::Revert { .. }
                | Terminator::RevertReturndata
                | Terminator::Stop
                | Terminator::SelfDestruct { .. }
                | Terminator::TailCall { .. }
                | Terminator::Invalid => return None,
            }
        }
        None
    }

    fn value_const(&self, env: &FxHashMap<ValueId, U256>, value: ValueId) -> Option<U256> {
        env.get(&value).copied()
    }

    fn eval_inst(&self, kind: &InstKind, env: &FxHashMap<ValueId, U256>) -> Option<U256> {
        let get = |value| self.value_const(env, value);
        if let InstKind::Select(condition, then_value, else_value) = *kind {
            return if get(condition)?.is_zero() { get(else_value) } else { get(then_value) };
        }
        eval::eval_inst(kind, |value| get(value).ok_or(())).ok().flatten()
    }

    fn rewrite_to_return(&self, func: &mut Function, values: &[U256]) {
        let entry = BlockId::ENTRY;
        // Folded paths retain all return origins, excluding unrelated analysis facts.
        let mut origins = func
            .blocks
            .iter()
            .filter(|block| matches!(block.terminator, Some(Terminator::Return { .. })));
        let mut metadata = origins.next().map_or_else(
            || InstructionMetadata::EMPTY.debug_context(),
            |block| block.terminator_metadata.debug_context(),
        );
        for block in origins {
            metadata.merge_debug_context(&block.terminator_metadata);
        }
        let block_ids = func.blocks.indices();
        for block_id in block_ids {
            let block = &mut func.blocks[block_id];
            block.instructions.clear();
            if block_id == entry {
                block.predecessors.clear();
            } else {
                block.predecessors.clear();
                // Dead block -> invalid !metadata(intentionally dropped)
                block.set_generated_terminator(Terminator::Invalid);
            }
        }

        let returns = std::mem::take(&mut func.returns);
        let values = values
            .iter()
            .zip(&returns)
            .map(|(&value, ty)| {
                func.alloc_value(Value::Immediate(Immediate::for_type(Some(*ty), value)))
            })
            .collect();
        func.returns = returns;
        // entry: ret constants !metadata(union of return origins)
        func.blocks[entry].set_terminator(Terminator::Return { values }, metadata);
    }
}

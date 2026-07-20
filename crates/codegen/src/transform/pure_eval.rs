//! Bounded evaluator for closed, pure MIR functions.
//!
//! This pass executes no-argument functions whose reachable instructions are pure and whose control
//! flow becomes deterministic under the evaluator. It is intentionally fuel-limited and only
//! rewrites functions that end in a raw `Return`, so ABI-returning external entries are left to the
//! normal encoder path.

use crate::{
    mir::{Function, Immediate, InstKind, Terminator, Value, ValueId},
    pass::FunctionPass,
    utils::evm_word,
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;

const DEFAULT_FUEL: usize = 10_000;

/// Statistics from bounded pure evaluation.
#[derive(Clone, Debug, Default)]
pub(crate) struct PureEvalStats {
    /// Number of functions folded to constant returns.
    pub functions_folded: usize,
}

/// Bounded pure MIR evaluator.
#[derive(Debug)]
pub(crate) struct PureEvaluator {
    fuel: usize,
    stats: PureEvalStats,
}

/// Function pass for bounded pure MIR evaluation.
pub(crate) struct PureEvalPass;

impl FunctionPass for PureEvalPass {
    fn run_on_function(&mut self, func: &mut Function) -> bool {
        let changed = PureEvaluator::new().run(func).functions_folded != 0;
        let repaired = crate::mir::utils::repair_reachability_phis(func);
        changed || repaired
    }
}

impl Default for PureEvaluator {
    fn default() -> Self {
        Self { fuel: DEFAULT_FUEL, stats: PureEvalStats::default() }
    }
}

impl PureEvaluator {
    /// Creates a new evaluator with the default fuel.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Runs the evaluator on one function.
    pub(crate) fn run(&mut self, func: &mut Function) -> &PureEvalStats {
        self.stats = PureEvalStats::default();
        if !func.params.is_empty() || !self.is_side_effect_free(func) {
            return &self.stats;
        }

        let Some(values) = self.evaluate(func) else {
            return &self.stats;
        };
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
        let entry = func.entry_block;
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
        for block in &func.blocks {
            for &inst_id in &block.instructions {
                if func.instructions[inst_id].kind.has_side_effects() {
                    return false;
                }
            }
        }
        true
    }

    fn evaluate(&self, func: &Function) -> Option<Vec<U256>> {
        let mut env = FxHashMap::default();
        for (value_id, value) in func.values.iter_enumerated() {
            if let Value::Immediate(imm) = value
                && let Some(value) = imm.as_u256()
            {
                env.insert(value_id, value);
            }
        }

        let mut current = func.entry_block;
        let mut predecessor = None;
        let mut fuel = self.fuel;
        while fuel != 0 {
            fuel -= 1;
            let block = &func.blocks[current];

            for &inst_id in &block.instructions {
                let inst = &func.instructions[inst_id];
                let result = match &inst.kind {
                    InstKind::Phi(incoming) => {
                        let pred = predecessor?;
                        let (_, value) = incoming.iter().find(|(block, _)| *block == pred)?;
                        self.value_const(&env, *value)?
                    }
                    kind => self.eval_inst(kind, &env)?,
                };
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
                    current = cases
                        .iter()
                        .find_map(|(case, target)| {
                            (self.value_const(&env, *case)? == value).then_some(*target)
                        })
                        .unwrap_or(*default);
                }
                Terminator::Return { values } => {
                    return values
                        .iter()
                        .map(|&value| self.value_const(&env, value))
                        .collect::<Option<Vec<_>>>();
                }
                Terminator::ReturnData { .. }
                | Terminator::Revert { .. }
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
        Some(match *kind {
            InstKind::Add(a, b) => get(a)?.wrapping_add(get(b)?),
            InstKind::Sub(a, b) => get(a)?.wrapping_sub(get(b)?),
            InstKind::Mul(a, b) => get(a)?.wrapping_mul(get(b)?),
            InstKind::Div(a, b) => {
                let b = get(b)?;
                if b.is_zero() { U256::ZERO } else { get(a)? / b }
            }
            InstKind::Mod(a, b) => {
                let b = get(b)?;
                if b.is_zero() { U256::ZERO } else { get(a)? % b }
            }
            InstKind::Exp(a, b) => get(a)?.wrapping_pow(get(b)?),
            InstKind::And(a, b) => get(a)? & get(b)?,
            InstKind::Or(a, b) => get(a)? | get(b)?,
            InstKind::Xor(a, b) => get(a)? ^ get(b)?,
            InstKind::Not(a) => !get(a)?,
            InstKind::Shl(shift, value) => {
                let shift = get(shift)?;
                if shift >= U256::from(256) {
                    U256::ZERO
                } else {
                    get(value)? << shift.to::<usize>()
                }
            }
            InstKind::Shr(shift, value) => {
                let shift = get(shift)?;
                if shift >= U256::from(256) {
                    U256::ZERO
                } else {
                    get(value)? >> shift.to::<usize>()
                }
            }
            InstKind::Sar(shift, value) => evm_word::sar(get(value)?, get(shift)?),
            InstKind::Byte(index, value) => evm_word::byte(get(index)?, get(value)?),
            InstKind::SignExtend(size, value) => evm_word::signextend(get(size)?, get(value)?),
            InstKind::Lt(a, b) => U256::from(get(a)? < get(b)?),
            InstKind::Gt(a, b) => U256::from(get(a)? > get(b)?),
            InstKind::Eq(a, b) => U256::from(get(a)? == get(b)?),
            InstKind::IsZero(a) => U256::from(get(a)?.is_zero()),
            InstKind::Select(condition, then_value, else_value) => {
                if get(condition)?.is_zero() {
                    get(else_value)?
                } else {
                    get(then_value)?
                }
            }
            InstKind::Phi(_) => unreachable!("phis are handled by the block interpreter"),
            _ => return None,
        })
    }

    fn rewrite_to_return(&self, func: &mut Function, values: &[U256]) {
        let entry = func.entry_block;
        let block_ids: Vec<_> = func.blocks.indices().collect();
        for block_id in block_ids {
            let block = &mut func.blocks[block_id];
            block.instructions.clear();
            if block_id == entry {
                block.predecessors.clear();
            } else {
                block.predecessors.clear();
                block.terminator = Some(Terminator::Invalid);
            }
        }

        let values = values
            .iter()
            .map(|&value| func.alloc_value(Value::Immediate(Immediate::uint256(value))))
            .collect();
        func.blocks[entry].terminator = Some(Terminator::Return { values });
    }
}

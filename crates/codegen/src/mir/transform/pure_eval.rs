//! Bounded evaluator for closed, pure MIR functions.
//!
//! This pass executes no-argument functions whose reachable instructions are pure and whose control
//! flow becomes deterministic under the evaluator. It is intentionally fuel-limited and only
//! rewrites functions that end in a raw `Return`, so ABI-returning external entries are left to the
//! normal encoder path.

use crate::mir::{
    BlockId, Function, Immediate, InstKind, InstructionMetadata, Module, Terminator, Value,
    ValueId,
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
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| fold(func))
    }
}

const DEFAULT_FUEL: usize = 10_000;

/// Folds one function to a constant return. Returns whether it changed.
fn fold(func: &mut Function) -> bool {
    if !func.params.is_empty() || !is_side_effect_free(func) {
        return false;
    }
    let Some(values) = evaluate(func) else { return false };
    if values.len() != func.return_components().len() || is_already_folded(func, &values) {
        return false;
    }
    rewrite_to_return(func, &values);
    true
}

/// Returns true when the function is already in the exact shape
/// [`rewrite_to_return`] would produce, so rewriting again would
/// report a change (and allocate fresh immediates) without progress.
fn is_already_folded(func: &Function, values: &[U256]) -> bool {
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

fn is_side_effect_free(func: &Function) -> bool {
    func.instructions().all(|inst_id| !func.inst(inst_id).kind.has_side_effects())
}

fn evaluate(func: &Function) -> Option<Vec<U256>> {
    let mut env = FxHashMap::default();
    let mut current = BlockId::ENTRY;
    let mut predecessor = None;
    for _ in 0..DEFAULT_FUEL {
        let block = &func.blocks[current];

        let mut phis = SmallVec::<[(ValueId, U256); 2]>::new();
        for &inst_id in &block.instructions {
            let inst = func.inst(inst_id);
            if let InstKind::Phi(incoming) = &inst.kind {
                let pred = predecessor?;
                let (_, value) = incoming.iter().find(|(block, _)| *block == pred)?;
                phis.push((func.inst_result_value(inst_id)?, value_const(func, &env, *value)?));
            }
        }
        env.extend(phis);

        for &inst_id in &block.instructions {
            let inst = func.inst(inst_id);
            if matches!(inst.kind, InstKind::Phi(..)) {
                continue;
            }
            let result = eval_inst(func, &inst.kind, &env)?;
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
                let condition = value_const(func, &env, *condition)?;
                predecessor = Some(current);
                current = if condition.is_zero() { *else_block } else { *then_block };
            }
            Terminator::Switch { value, default, cases } => {
                let value = value_const(func, &env, *value)?;
                predecessor = Some(current);
                current = *default;
                for (case, target) in cases {
                    if value_const(func, &env, *case)? == value {
                        current = *target;
                        break;
                    }
                }
            }
            Terminator::Return { values } => {
                return values
                    .iter()
                    .map(|&value| value_const(func, &env, value))
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

fn eval_inst(func: &Function, kind: &InstKind, env: &FxHashMap<ValueId, U256>) -> Option<U256> {
    let get = |value| value_const(func, env, value);
    if let InstKind::Select(condition, then_value, else_value) = *kind {
        return if get(condition)?.is_zero() { get(else_value) } else { get(then_value) };
    }
    eval::eval_typed_inst(func, kind, |value| get(value).ok_or(())).ok().flatten()
}

fn rewrite_to_return(func: &mut Function, values: &[U256]) {
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
    for block_id in func.blocks.indices() {
        let block = &mut func.blocks[block_id];
        block.instructions.clear();
        block.predecessors.clear();
        if block_id != entry {
            // Dead block -> invalid !metadata(intentionally dropped)
            block.set_generated_terminator(Terminator::Invalid);
        }
    }

    let values = values
        .iter()
        .enumerate()
        .map(|(index, &value)| {
            let ty = func.return_components()[index];
            func.alloc_value(Value::Immediate(Immediate::for_type(Some(ty), value)))
        })
        .collect();
    // entry: ret constants !metadata(union of return origins)
    func.blocks[entry].set_terminator(Terminator::Return { values }, metadata);
}

fn value_const(func: &Function, env: &FxHashMap<ValueId, U256>, value: ValueId) -> Option<U256> {
    env.get(&value).copied().or_else(|| func.value_u256(value))
}

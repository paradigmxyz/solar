//! Local MIR instruction simplification.
//!
//! This pass removes algebraic no-ops and rewrites a few equivalent EVM
//! instruction patterns before stack scheduling. It is intentionally local and
//! conservative: it only applies identities that are exact for EVM word
//! semantics.
//!
//! Safety contract:
//! - do not remove or reorder side effects
//! - replace an instruction with a value only when the equality is exact for all 256-bit EVM words
//! - preserve boolean-only rewrites behind explicit MIR boolean type checks
//!
//! Rules that replace an instruction with an existing or immediate value are
//! written in ISLE in `isle/inst_simplify.isle`; constant folding, in-place
//! instruction rewrites, and terminator rewrites stay here.

use crate::{
    mir::{
        Function, Immediate, InstId, InstKind, Module, Terminator, ToUint, Value, ValueId,
        utils as mir_utils,
    },
    pass::{MirPass, run_function_pass},
    utils::eval,
};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};

pub(crate) mod isle;

/// Function pass for local instruction simplification.
pub(crate) struct InstSimplify;

impl MirPass for InstSimplify {
    fn name(&self) -> &'static str {
        "inst-simplify"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| {
            InstSimplifier::new(gcx.sess.opts.evm_version).run_to_fixpoint(func) != 0
        })
    }
}

/// Local MIR instruction simplification pass.
#[derive(Debug)]
pub(crate) struct InstSimplifier {
    /// Number of instructions simplified in the last run.
    simplified_count: usize,
    evm_version: EvmVersion,
}

struct RunState {
    replacements: FxHashMap<ValueId, ValueId>,
    dead: DenseBitSet<InstId>,
}

impl RunState {
    fn new(func: &Function) -> Self {
        Self { replacements: FxHashMap::default(), dead: DenseBitSet::new_empty(func.num_insts()) }
    }
}

impl InstSimplifier {
    /// Creates a new instruction simplifier.
    fn new(evm_version: EvmVersion) -> Self {
        Self { simplified_count: 0, evm_version }
    }

    fn run_with_state(&mut self, func: &mut Function, state: &mut RunState) -> usize {
        self.simplified_count = 0;

        state.replacements.clear();
        state.dead.clear();
        let block_ids = func.blocks.indices();

        for block_id in block_ids {
            let instruction_count = func.blocks[block_id].instructions.len();
            for index in 0..instruction_count {
                let inst_id = func.blocks[block_id].instructions[index];
                loop {
                    let kind = func.inst(inst_id).kind.clone();

                    if self.is_dead_noop_inst(func, &kind, &state.replacements) {
                        tracing::trace!(
                            target: "solar::codegen::mir::inst_simplify",
                            function = %func.name,
                            action = "delete",
                            instruction = %kind,
                            "mir_inst_simplify"
                        );
                        state.dead.insert(inst_id);
                        self.simplified_count += 1;
                        break;
                    }

                    if let Some(new_kind) = self.rewrite_inst(func, &kind, &state.replacements) {
                        tracing::trace!(
                            target: "solar::codegen::mir::inst_simplify",
                            function = %func.name,
                            action = "rewrite",
                            input = %kind,
                            output = %new_kind,
                            "mir_inst_simplify"
                        );
                        func.inst_mut(inst_id).kind = new_kind;
                        self.simplified_count += 1;
                        continue;
                    }

                    let Some(result) = func.inst_result_value(inst_id) else {
                        break;
                    };
                    let Some(replacement) = self.simplify_inst(func, &kind, &state.replacements)
                    else {
                        break;
                    };
                    let replacement =
                        mir_utils::resolve_replacement(replacement, &state.replacements);
                    if replacement != result {
                        tracing::trace!(
                            target: "solar::codegen::mir::inst_simplify",
                            function = %func.name,
                            action = "replace",
                            instruction = %kind,
                            ?result,
                            ?replacement,
                            "mir_inst_simplify"
                        );
                        state.replacements.insert(result, replacement);
                        state.dead.insert(inst_id);
                        self.simplified_count += 1;
                    }
                    break;
                }
            }
        }

        if !state.replacements.is_empty() {
            func.replace_uses_canonicalized(&state.replacements);
        }
        if !state.dead.is_empty() {
            for block in func.blocks.iter_mut() {
                block.instructions.retain(|&id| !state.dead.contains(id));
            }
        }
        self.simplified_count += self.rewrite_terminators(func, &state.replacements);

        self.simplified_count
    }

    /// Runs instruction simplification until no more changes are found.
    fn run_to_fixpoint(&mut self, func: &mut Function) -> usize {
        let mut total = 0;
        let mut state = RunState::new(func);
        for round in 1.. {
            let simplified = self.run_with_state(func, &mut state);
            tracing::trace!(
                target: "solar::codegen::mir::inst_simplify",
                function = %func.name,
                round,
                simplified,
                "mir_inst_simplify_round"
            );
            if simplified == 0 {
                break;
            }
            total += simplified;
        }
        total
    }

    fn rewrite_inst(
        &mut self,
        func: &mut Function,
        kind: &InstKind,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> Option<InstKind> {
        let resolve = |value| mir_utils::resolve_replacement(value, replacements);
        let op = kind.op().map_values(resolve);
        let rewritten = isle::RuleContext::new(func, self.evm_version).rewrite(&op)?;
        Some(rewritten.into_kind().expect("rewrite rules produce operand-only operations"))
    }

    fn simplify_inst(
        &mut self,
        func: &mut Function,
        kind: &InstKind,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> Option<ValueId> {
        let resolve = |value| mir_utils::resolve_replacement(value, replacements);

        if let Some(value) = Self::const_fold_inst(func, kind, replacements) {
            return Some(value);
        }

        // Variable-length payloads are not visible to the rules.
        if let InstKind::Phi(incoming) = kind {
            let &(_, first) = incoming.first()?;
            let first = resolve(first);
            return incoming
                .iter()
                .all(|&(_, value)| Self::same_value(func, resolve(value), first))
                .then_some(first);
        }

        // The rules see the instruction with its operands resolved and inspect
        // the operands of defining instructions as written.
        let op = kind.op().map_values(resolve);
        isle::RuleContext::new(func, self.evm_version).simplify(&op)
    }

    /// Folds an instruction over immediate operands to an immediate result.
    pub(crate) fn const_fold_inst(
        func: &mut Function,
        kind: &InstKind,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> Option<ValueId> {
        let resolve = |value| mir_utils::resolve_replacement(value, replacements);
        if let InstKind::Select(condition, then_value, else_value) = *kind {
            let condition = func.value_u256(resolve(condition))?;
            return Some(if condition.is_zero() {
                resolve(else_value)
            } else {
                resolve(then_value)
            });
        }

        let value = eval::eval_inst(kind, |value| func.value_u256(resolve(value)).ok_or(()))
            .ok()
            .flatten()?;
        match kind {
            InstKind::Lt(..)
            | InstKind::Gt(..)
            | InstKind::SLt(..)
            | InstKind::SGt(..)
            | InstKind::Eq(..)
            | InstKind::IsZero(..) => Some(Self::imm_bool(func, !value.is_zero())),
            _ => Some(Self::imm(func, value)),
        }
    }

    fn is_dead_noop_inst(
        &self,
        func: &Function,
        kind: &InstKind,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> bool {
        let resolve = |value| mir_utils::resolve_replacement(value, replacements);
        match kind {
            InstKind::MCopy(_, _, size)
            | InstKind::CalldataCopy(_, _, size)
            | InstKind::DataCopy(_, _, size)
            | InstKind::CodeCopy(_, _, size) => Self::is_zero(func, resolve(*size)),
            InstKind::ReturnDataCopy(_, offset, size) => {
                Self::is_zero(func, resolve(*offset)) && Self::is_zero(func, resolve(*size))
            }
            _ => false,
        }
    }

    fn imm(func: &mut Function, value: impl ToUint) -> ValueId {
        func.alloc_value(Value::Immediate(Immediate::uint256(value.to_uint())))
    }

    fn imm_bool(func: &mut Function, value: bool) -> ValueId {
        func.alloc_value(Value::Immediate(Immediate::bool(value)))
    }

    fn is_const(func: &Function, value: ValueId, expected: U256) -> bool {
        func.value_u256(value) == Some(expected)
    }

    fn is_zero(func: &Function, value: ValueId) -> bool {
        Self::is_const(func, value, U256::ZERO)
    }

    pub(crate) fn same_value(func: &Function, a: ValueId, b: ValueId) -> bool {
        a == b
            || match (func.value(a), func.value(b)) {
                (Value::Immediate(a), Value::Immediate(b)) => a == b,
                _ => false,
            }
    }

    fn rewrite_terminators(
        &mut self,
        func: &mut Function,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> usize {
        let externally_terminating =
            func.selector.is_some() || func.attributes.is_receive || func.attributes.is_fallback;
        let mut rewrites = 0;
        for block_id in func.blocks.indices() {
            loop {
                let Some(Terminator::Branch { condition, .. }) = func.blocks[block_id].terminator
                else {
                    break;
                };
                let condition = mir_utils::resolve_replacement(condition, replacements);
                let (inner, swap) = if let Some(inner) = Self::iszero_operand(func, condition) {
                    (inner, true)
                } else if let Some(inner) = Self::nonzero_test_operand(func, condition) {
                    // `branch gt(x, 0)` / `branch lt(0, x)` test exactly `x != 0`,
                    // which is what `branch x` already does.
                    (inner, false)
                } else {
                    break;
                };
                let inner = mir_utils::resolve_replacement(inner, replacements);
                let Some(Terminator::Branch { condition, then_block, else_block }) =
                    &mut func.blocks[block_id].terminator
                else {
                    unreachable!()
                };
                *condition = inner;
                if swap {
                    std::mem::swap(then_block, else_block);
                }
                rewrites += 1;
                tracing::trace!(
                    target: "solar::codegen::mir::inst_simplify",
                    function = %func.name,
                    action = "rewrite_terminator",
                    ?block_id,
                    swap,
                    "mir_inst_simplify"
                );
            }

            if externally_terminating
                && let Some(Terminator::ReturnData { size, .. }) = func.blocks[block_id].terminator
                && Self::is_zero(func, mir_utils::resolve_replacement(size, replacements))
            {
                func.blocks[block_id].terminator = Some(Terminator::Stop);
                rewrites += 1;
            }
        }

        rewrites
    }

    /// Returns `x` when `value` computes `gt(x, 0)` or `lt(0, x)`, both of
    /// which are the unsigned nonzero test.
    fn nonzero_test_operand(func: &Function, value: ValueId) -> Option<ValueId> {
        match func.value(value) {
            Value::Inst(inst_id) => match func.inst(*inst_id).kind {
                InstKind::Gt(a, b) if Self::is_zero(func, b) => Some(a),
                InstKind::Lt(a, b) if Self::is_zero(func, a) => Some(b),
                _ => None,
            },
            _ => None,
        }
    }

    fn iszero_operand(func: &Function, value: ValueId) -> Option<ValueId> {
        match func.value(value) {
            Value::Inst(inst_id) => match func.inst(*inst_id).kind {
                InstKind::IsZero(inner) => Some(inner),
                _ => None,
            },
            _ => None,
        }
    }
}

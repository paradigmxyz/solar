//! Partial redundancy elimination for pure MIR expressions.
//!
//! This pass handles the conservative PRE case that CSE cannot: an expression
//! is recomputed in a join block, but is already available along at least as
//! many incoming edges as the number of edges where it must be inserted. We
//! only move pure word expressions. A jump-terminated insertion predecessor
//! receives the computation directly; a branch- or switch-terminated one ends
//! a critical edge, which is split first so the computation runs only on the
//! edge into the join.
//!
//! Availability at a predecessor's end is checked in the predecessor itself and
//! then up its dominator tree: a def of the translated expression in any
//! dominator is available with no further checks, so it can feed the join phi
//! without inserting a duplicate computation.
//!
//! # Termination
//!
//! Joins that are mutual predecessors can ping-pong an expression between each
//! other forever: each rewrite is net-zero and re-creates a candidate in the
//! other block. Three rules guarantee termination:
//! 1. An instruction inserted by this run is never picked as an elimination candidate, so every
//!    rewrite retires an instruction that existed when the run started, bounding rewrites by the
//!    initial instruction count.
//! 2. An expression key is never inserted into a block it was previously eliminated from in the
//!    same run.
//! 3. A function-size-derived rewrite limit backstops the above.
//!
//! Edge splitting does not weaken these rules: split blocks have a single
//! predecessor, so they are never join targets, and the only instructions they
//! hold are inserted-this-run and excluded by rule 1.

use crate::{
    analysis::{CfgInfo, DominatorTree},
    mir::{
        BlockId, Function, Immediate, InstId, InstKind, Instruction, InstructionMetadata, MirType,
        Module, Terminator, Value, ValueId,
        utils::{repair_reachability_phis, split_edge},
    },
    pass::{MirPass, run_function_pass},
};
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    map::{FxHashMap, FxHashSet},
};
use std::cmp::Ordering;

/// Function pass for pure expression PRE.
pub(crate) struct PrePass;

impl MirPass for PrePass {
    fn name(&self) -> &'static str {
        "pre"
    }

    fn run_pass(&self, _gcx: solar_sema::Gcx<'_>, module: &mut Module) -> bool {
        run_function_pass(module, |func| PartialRedundancyEliminator::new().run(func).total() != 0)
    }

    fn is_required(&self) -> bool {
        false
    }
}

const MAX_INSERTIONS_PER_REWRITE: usize = 2;

/// Statistics for pure expression PRE.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PreStats {
    /// Number of join-block expressions replaced by PRE phis.
    pub expressions_eliminated: usize,
    /// Number of predecessor computations inserted.
    pub expressions_inserted: usize,
}

impl PreStats {
    /// Returns the total number of MIR edits made by this pass.
    pub(crate) const fn total(self) -> usize {
        self.expressions_eliminated + self.expressions_inserted
    }
}

/// Partial redundancy eliminator for pure expressions.
#[derive(Debug, Default)]
pub(crate) struct PartialRedundancyEliminator {
    stats: PreStats,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ExprKey {
    Add(OperandKey, OperandKey),
    Sub(OperandKey, OperandKey),
    Mul(OperandKey, OperandKey),
    Div(OperandKey, OperandKey),
    SDiv(OperandKey, OperandKey),
    Mod(OperandKey, OperandKey),
    SMod(OperandKey, OperandKey),
    Exp(OperandKey, OperandKey),
    AddMod(OperandKey, OperandKey, OperandKey),
    MulMod(OperandKey, OperandKey, OperandKey),
    And(OperandKey, OperandKey),
    Or(OperandKey, OperandKey),
    Xor(OperandKey, OperandKey),
    Not(OperandKey),
    Shl(OperandKey, OperandKey),
    Shr(OperandKey, OperandKey),
    Sar(OperandKey, OperandKey),
    Byte(OperandKey, OperandKey),
    Lt(OperandKey, OperandKey),
    Gt(OperandKey, OperandKey),
    SLt(OperandKey, OperandKey),
    SGt(OperandKey, OperandKey),
    Eq(OperandKey, OperandKey),
    IsZero(OperandKey),
    Select(OperandKey, OperandKey, OperandKey),
    SignExtend(OperandKey, OperandKey),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum OperandKey {
    Value(ValueId),
    Immediate(Immediate),
}

struct PreCandidate {
    target: BlockId,
    inst: InstId,
    result: ValueId,
    result_ty: MirType,
    metadata: InstructionMetadata,
    incoming: Vec<(BlockId, ValueId)>,
    insertions: Vec<(BlockId, InstKind)>,
}

impl PartialRedundancyEliminator {
    /// Creates a new PRE pass.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Runs PRE to a fixed point.
    pub(crate) fn run(&mut self, func: &mut Function) -> PreStats {
        self.stats = PreStats::default();
        repair_reachability_phis(func);

        let mut inst_results = func.inst_results();
        let mut inst_blocks = func.inst_blocks();

        let mut eliminated_keys = FxHashSet::default();
        let mut inserted_insts = GrowableBitSet::with_capacity(func.instructions.len());
        let rewrite_limit = func.instructions.len().saturating_mul(2).max(64);
        let mut rewrites = 0usize;

        while rewrites < rewrite_limit {
            // Edge splitting grows the CFG between batches, so the dominator
            // tree is recomputed before each scan.
            let cfg = CfgInfo::new(func);
            let batch = self.collect_candidates(
                func,
                cfg.dominators(),
                &inst_results,
                &inst_blocks,
                &eliminated_keys,
                &inserted_insts,
                rewrite_limit - rewrites,
            );
            if batch.is_empty() {
                break;
            }
            rewrites += batch.len();
            for candidate in batch {
                self.apply_candidate(
                    func,
                    candidate,
                    &mut inst_results,
                    &mut inst_blocks,
                    &mut eliminated_keys,
                    &mut inserted_insts,
                );
            }
            repair_reachability_phis(func);
        }

        self.stats
    }

    /// Collects non-interfering candidates from a single scan over the
    /// function so they can be applied as one batch.
    #[allow(clippy::too_many_arguments)]
    fn collect_candidates(
        &self,
        func: &Function,
        dominators: &DominatorTree,
        inst_results: &FxHashMap<InstId, ValueId>,
        inst_blocks: &FxHashMap<InstId, BlockId>,
        eliminated_keys: &FxHashSet<(ExprKey, BlockId)>,
        inserted_insts: &GrowableBitSet<InstId>,
        limit: usize,
    ) -> Vec<PreCandidate> {
        let mut batch = Vec::new();
        // Candidates whose analysis would be invalidated by an earlier
        // candidate in this batch are deferred to the next scan.
        let mut modified_blocks = DenseBitSet::new_empty(func.blocks.len());
        let mut eliminated_values = DenseBitSet::new_empty(func.values.len());

        'targets: for target in func.blocks.indices() {
            let predecessors = func.unique_predecessors(target);
            if predecessors.len() < 2 {
                continue;
            }

            for &inst in &func.blocks[target].instructions {
                if batch.len() >= limit {
                    break 'targets;
                }
                // Termination rule 1: never re-eliminate an instruction this
                // run inserted.
                if inserted_insts.contains(inst) {
                    continue;
                }
                let instruction = &func.instructions[inst];
                if !Self::is_pre_expression(&instruction.kind) {
                    continue;
                }
                let Some(result_ty) = instruction.result_ty else {
                    continue;
                };
                let Some(&result) = inst_results.get(&inst) else {
                    continue;
                };

                let Some(candidate) = self.candidate_for_inst(
                    func,
                    target,
                    inst,
                    result,
                    result_ty,
                    instruction.metadata.clone(),
                    &predecessors,
                    inst_results,
                    inst_blocks,
                    dominators,
                    eliminated_keys,
                ) else {
                    continue;
                };

                if Self::interferes_with_batch(&candidate, &modified_blocks, &eliminated_values) {
                    continue;
                }
                modified_blocks.insert(candidate.target);
                for &(block, _) in &candidate.insertions {
                    modified_blocks.insert(block);
                }
                eliminated_values.insert(candidate.result);
                batch.push(candidate);
            }
        }

        batch
    }

    /// Returns true if applying earlier candidates in the batch invalidates
    /// this candidate's analysis: its blocks were already rewritten, or it
    /// references a value whose defining instruction the batch removes.
    fn interferes_with_batch(
        candidate: &PreCandidate,
        modified_blocks: &DenseBitSet<BlockId>,
        eliminated_values: &DenseBitSet<ValueId>,
    ) -> bool {
        modified_blocks.contains(candidate.target)
            || candidate.insertions.iter().any(|&(block, _)| modified_blocks.contains(block))
            || candidate.incoming.iter().any(|&(_, value)| eliminated_values.contains(value))
            || candidate.insertions.iter().any(|(_, kind)| {
                kind.operands().into_iter().any(|value| eliminated_values.contains(value))
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn candidate_for_inst(
        &self,
        func: &Function,
        target: BlockId,
        inst: InstId,
        result: ValueId,
        result_ty: MirType,
        metadata: InstructionMetadata,
        predecessors: &[BlockId],
        inst_results: &FxHashMap<InstId, ValueId>,
        inst_blocks: &FxHashMap<InstId, BlockId>,
        dominators: &DominatorTree,
        eliminated_keys: &FxHashSet<(ExprKey, BlockId)>,
    ) -> Option<PreCandidate> {
        let original = &func.instructions[inst].kind;
        let mut incoming = Vec::with_capacity(predecessors.len());
        let mut insertions = Vec::new();
        let mut available = 0usize;

        for &pred in predecessors {
            let translated =
                Self::translate_kind_for_predecessor(func, original, target, pred, inst_blocks)?;
            if !Self::operands_available_at_end(func, &translated, pred, inst_blocks, dominators) {
                return None;
            }
            let key = Self::make_expr_key(func, &translated)?;
            if let Some(value) =
                Self::available_value_at_end(func, dominators, pred, &key, inst_results)
            {
                available += 1;
                incoming.push((pred, value));
                continue;
            }

            // Termination rule 2: never insert an expression into a block it
            // was previously eliminated from, which would ping-pong it between
            // mutually-preceding join blocks.
            if eliminated_keys.contains(&(key, pred)) {
                return None;
            }
            insertions.push((pred, translated));
        }

        // Every insertion must be paid for by a predecessor where the
        // expression is already available, so no path computes it more often
        // than before; paths through available predecessors compute it
        // strictly less often. The constant bounds code growth at joins with
        // many predecessors.
        if insertions.len() > available
            || insertions.len() > MAX_INSERTIONS_PER_REWRITE.max(available)
        {
            return None;
        }

        Some(PreCandidate { target, inst, result, result_ty, metadata, incoming, insertions })
    }

    fn apply_candidate(
        &mut self,
        func: &mut Function,
        candidate: PreCandidate,
        inst_results: &mut FxHashMap<InstId, ValueId>,
        inst_blocks: &mut FxHashMap<InstId, BlockId>,
        eliminated_keys: &mut FxHashSet<(ExprKey, BlockId)>,
        inserted_insts: &mut GrowableBitSet<InstId>,
    ) {
        let PreCandidate { target, inst, result, result_ty, metadata, mut incoming, insertions } =
            candidate;

        if let Some(key) = Self::make_expr_key(func, &func.instructions[inst].kind) {
            eliminated_keys.insert((key, target));
        }

        let fully_available = insertions.is_empty();
        for (pred, kind) in insertions {
            // A jump-terminated predecessor owns its single outgoing edge, so
            // the computation can go at its end. Any other terminator makes
            // the edge critical: split it so the computation runs only on the
            // edge into the join. The split block sits on that edge, so the
            // per-edge phi translation that held for `pred` holds for it too.
            let block = match func.blocks[pred].terminator {
                Some(Terminator::Jump(jump_target)) => {
                    debug_assert_eq!(jump_target, target);
                    pred
                }
                _ => split_edge(func, pred, target),
            };
            let new_inst = func.alloc_inst(Instruction {
                kind,
                result_ty: Some(result_ty),
                metadata: metadata.clone(),
            });
            let value = func.alloc_value(Value::Inst(new_inst));
            func.blocks[block].instructions.push(new_inst);
            incoming.push((block, value));
            inst_results.insert(new_inst, value);
            inst_blocks.insert(new_inst, block);
            inserted_insts.insert(new_inst);
            self.stats.expressions_inserted += 1;
        }
        incoming.sort_by_key(|(block, _)| block.index());

        // A fully-available expression whose predecessors all reuse the same
        // value needs no phi: that value's def dominates every predecessor and
        // therefore the join itself.
        let replacement = match incoming.first() {
            Some(&(_, first))
                if fully_available
                    && first != result
                    && incoming.iter().all(|&(_, value)| value == first) =>
            {
                first
            }
            _ => {
                let phi_inst =
                    func.alloc_inst(Instruction::new(InstKind::Phi(incoming), Some(result_ty)));
                let phi_value = func.alloc_value(Value::Inst(phi_inst));
                let phi_count = func.blocks[target]
                    .instructions
                    .iter()
                    .take_while(|&&inst_id| {
                        matches!(func.instructions[inst_id].kind, InstKind::Phi(_))
                    })
                    .count();
                func.blocks[target].instructions.insert(phi_count, phi_inst);
                inst_results.insert(phi_inst, phi_value);
                inst_blocks.insert(phi_inst, target);
                phi_value
            }
        };

        let replacements = FxHashMap::from_iter([(result, replacement)]);
        func.replace_uses(&replacements);
        func.blocks[target].instructions.retain(|&inst_id| inst_id != inst);
        inst_results.remove(&inst);
        inst_blocks.remove(&inst);
        self.stats.expressions_eliminated += 1;
    }

    fn translate_kind_for_predecessor(
        func: &Function,
        kind: &InstKind,
        target: BlockId,
        pred: BlockId,
        inst_blocks: &FxHashMap<InstId, BlockId>,
    ) -> Option<InstKind> {
        let mut translated = kind.clone();
        let mut ok = true;
        translated.visit_operands_mut(|value| {
            if let Some(translated) =
                Self::translate_value_for_predecessor(func, *value, target, pred, inst_blocks)
            {
                *value = translated;
            } else {
                ok = false;
            }
        });
        ok.then_some(translated)
    }

    fn translate_value_for_predecessor(
        func: &Function,
        value: ValueId,
        target: BlockId,
        pred: BlockId,
        inst_blocks: &FxHashMap<InstId, BlockId>,
    ) -> Option<ValueId> {
        match func.value(value) {
            Value::Inst(inst_id)
                if inst_blocks.get(inst_id).copied() == Some(target)
                    && matches!(func.instructions[*inst_id].kind, InstKind::Phi(_)) =>
            {
                let InstKind::Phi(incoming) = &func.instructions[*inst_id].kind else {
                    return None;
                };
                incoming.iter().find_map(|(incoming_pred, incoming_value)| {
                    (*incoming_pred == pred).then_some(*incoming_value)
                })
            }
            _ => Some(value),
        }
    }

    fn operands_available_at_end(
        func: &Function,
        kind: &InstKind,
        block: BlockId,
        inst_blocks: &FxHashMap<InstId, BlockId>,
        dominators: &DominatorTree,
    ) -> bool {
        kind.operands()
            .into_iter()
            .all(|value| Self::value_available_at_end(func, value, block, inst_blocks, dominators))
    }

    fn value_available_at_end(
        func: &Function,
        value: ValueId,
        block: BlockId,
        inst_blocks: &FxHashMap<InstId, BlockId>,
        dominators: &DominatorTree,
    ) -> bool {
        match func.value(value) {
            Value::Immediate(_) | Value::Arg { .. } | Value::Undef(_) | Value::Error(_) => true,
            Value::Inst(inst) => inst_blocks
                .get(inst)
                .is_some_and(|def_block| dominators.dominates(*def_block, block)),
        }
    }

    /// Finds the translated expression in `block` or any of its dominators; a
    /// def in a dominator is available at `block`'s end with no further
    /// checks.
    fn available_value_at_end(
        func: &Function,
        dominators: &DominatorTree,
        block: BlockId,
        key: &ExprKey,
        inst_results: &FxHashMap<InstId, ValueId>,
    ) -> Option<ValueId> {
        dominators
            .self_and_dominators(block)
            .into_iter()
            .find_map(|block| Self::available_value_in_block(func, block, key, inst_results))
    }

    fn available_value_in_block(
        func: &Function,
        block: BlockId,
        key: &ExprKey,
        inst_results: &FxHashMap<InstId, ValueId>,
    ) -> Option<ValueId> {
        func.blocks[block].instructions.iter().rev().find_map(|&inst| {
            let instruction = &func.instructions[inst];
            if !Self::is_pre_expression(&instruction.kind) {
                return None;
            }
            let candidate_key = Self::make_expr_key(func, &instruction.kind)?;
            (candidate_key == *key).then(|| inst_results.get(&inst).copied()).flatten()
        })
    }

    fn is_pre_expression(kind: &InstKind) -> bool {
        matches!(
            kind,
            InstKind::Add(_, _)
                | InstKind::Sub(_, _)
                | InstKind::Mul(_, _)
                | InstKind::Div(_, _)
                | InstKind::SDiv(_, _)
                | InstKind::Mod(_, _)
                | InstKind::SMod(_, _)
                | InstKind::Exp(_, _)
                | InstKind::AddMod(_, _, _)
                | InstKind::MulMod(_, _, _)
                | InstKind::And(_, _)
                | InstKind::Or(_, _)
                | InstKind::Xor(_, _)
                | InstKind::Not(_)
                | InstKind::Shl(_, _)
                | InstKind::Shr(_, _)
                | InstKind::Sar(_, _)
                | InstKind::Byte(_, _)
                | InstKind::Lt(_, _)
                | InstKind::Gt(_, _)
                | InstKind::SLt(_, _)
                | InstKind::SGt(_, _)
                | InstKind::Eq(_, _)
                | InstKind::IsZero(_)
                | InstKind::Select(_, _, _)
                | InstKind::SignExtend(_, _)
        )
    }

    fn make_expr_key(func: &Function, kind: &InstKind) -> Option<ExprKey> {
        let operand = |value| Self::operand_key(func, value);
        match kind {
            InstKind::Add(a, b) => {
                let (a, b) = Self::ordered_pair(operand(*a), operand(*b));
                Some(ExprKey::Add(a, b))
            }
            InstKind::Mul(a, b) => {
                let (a, b) = Self::ordered_pair(operand(*a), operand(*b));
                Some(ExprKey::Mul(a, b))
            }
            InstKind::And(a, b) => {
                let (a, b) = Self::ordered_pair(operand(*a), operand(*b));
                Some(ExprKey::And(a, b))
            }
            InstKind::Or(a, b) => {
                let (a, b) = Self::ordered_pair(operand(*a), operand(*b));
                Some(ExprKey::Or(a, b))
            }
            InstKind::Xor(a, b) => {
                let (a, b) = Self::ordered_pair(operand(*a), operand(*b));
                Some(ExprKey::Xor(a, b))
            }
            InstKind::Eq(a, b) => {
                let (a, b) = Self::ordered_pair(operand(*a), operand(*b));
                Some(ExprKey::Eq(a, b))
            }
            InstKind::AddMod(a, b, n) => {
                let (a, b) = Self::ordered_pair(operand(*a), operand(*b));
                Some(ExprKey::AddMod(a, b, operand(*n)))
            }
            InstKind::MulMod(a, b, n) => {
                let (a, b) = Self::ordered_pair(operand(*a), operand(*b));
                Some(ExprKey::MulMod(a, b, operand(*n)))
            }
            InstKind::Sub(a, b) => Some(ExprKey::Sub(operand(*a), operand(*b))),
            InstKind::Div(a, b) => Some(ExprKey::Div(operand(*a), operand(*b))),
            InstKind::SDiv(a, b) => Some(ExprKey::SDiv(operand(*a), operand(*b))),
            InstKind::Mod(a, b) => Some(ExprKey::Mod(operand(*a), operand(*b))),
            InstKind::SMod(a, b) => Some(ExprKey::SMod(operand(*a), operand(*b))),
            InstKind::Exp(a, b) => Some(ExprKey::Exp(operand(*a), operand(*b))),
            InstKind::Not(a) => Some(ExprKey::Not(operand(*a))),
            InstKind::Shl(a, b) => Some(ExprKey::Shl(operand(*a), operand(*b))),
            InstKind::Shr(a, b) => Some(ExprKey::Shr(operand(*a), operand(*b))),
            InstKind::Sar(a, b) => Some(ExprKey::Sar(operand(*a), operand(*b))),
            InstKind::Byte(a, b) => Some(ExprKey::Byte(operand(*a), operand(*b))),
            InstKind::Lt(a, b) => Some(ExprKey::Lt(operand(*a), operand(*b))),
            InstKind::Gt(a, b) => Some(ExprKey::Gt(operand(*a), operand(*b))),
            InstKind::SLt(a, b) => Some(ExprKey::SLt(operand(*a), operand(*b))),
            InstKind::SGt(a, b) => Some(ExprKey::SGt(operand(*a), operand(*b))),
            InstKind::IsZero(a) => Some(ExprKey::IsZero(operand(*a))),
            InstKind::Select(a, b, c) => {
                Some(ExprKey::Select(operand(*a), operand(*b), operand(*c)))
            }
            InstKind::SignExtend(a, b) => Some(ExprKey::SignExtend(operand(*a), operand(*b))),
            _ => None,
        }
    }

    fn operand_key(func: &Function, value: ValueId) -> OperandKey {
        match func.value(value) {
            Value::Immediate(imm) => OperandKey::Immediate(imm.clone()),
            _ => OperandKey::Value(value),
        }
    }

    fn ordered_pair(a: OperandKey, b: OperandKey) -> (OperandKey, OperandKey) {
        if Self::compare_operand_key(&a, &b) == Ordering::Greater { (b, a) } else { (a, b) }
    }

    fn compare_operand_key(a: &OperandKey, b: &OperandKey) -> Ordering {
        match (a, b) {
            (OperandKey::Value(a), OperandKey::Value(b)) => a.index().cmp(&b.index()),
            (OperandKey::Value(_), OperandKey::Immediate(_)) => Ordering::Less,
            (OperandKey::Immediate(_), OperandKey::Value(_)) => Ordering::Greater,
            (OperandKey::Immediate(a), OperandKey::Immediate(b)) => Self::compare_immediate(a, b),
        }
    }

    fn compare_immediate(a: &Immediate, b: &Immediate) -> Ordering {
        let rank = |imm: &Immediate| match imm {
            Immediate::Bool(_) => 0,
            Immediate::UInt(_, _) => 1,
            Immediate::Int(_, _) => 2,
        };
        rank(a).cmp(&rank(b)).then_with(|| match (a, b) {
            (Immediate::Bool(a), Immediate::Bool(b)) => a.cmp(b),
            (Immediate::UInt(a_value, a_bits), Immediate::UInt(b_value, b_bits))
            | (Immediate::Int(a_value, a_bits), Immediate::Int(b_value, b_bits)) => {
                a_bits.cmp(b_bits).then_with(|| a_value.cmp(b_value))
            }
            _ => Ordering::Equal,
        })
    }
}

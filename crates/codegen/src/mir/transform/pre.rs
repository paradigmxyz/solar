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
//! without inserting a duplicate computation. Commutative operands share a key;
//! swapped signed and unsigned comparisons retain their corresponding predicate.
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

use crate::mir::{
    BlockId, EffectKind, Function, Immediate, InstId, InstKind, Instruction, InstructionMetadata,
    MirType, Module, Terminator, Value, ValueId,
    analysis::{CfgInfo, DominatorTree},
    pass::{MirPass, run_function_pass},
    utils::split_edge,
};
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    map::{FxHashMap, FxHashSet},
};
use smallvec::SmallVec;

/// Function pass for pure expression PRE.
pub(crate) struct Pre;

impl MirPass for Pre {
    fn name(&self) -> &'static str {
        "pre"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| {
            PartialRedundancyEliminator::new().run(func).total() != 0
        })
    }
}

const MAX_INSERTIONS_PER_REWRITE: usize = 2;

/// Statistics for pure expression PRE.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PreStats {
    /// Number of join-block expressions replaced by PRE phis.
    expressions_eliminated: usize,
    /// Number of predecessor computations inserted.
    expressions_inserted: usize,
}

impl PreStats {
    /// Returns the total number of MIR edits made by this pass.
    const fn total(self) -> usize {
        self.expressions_eliminated + self.expressions_inserted
    }
}

/// Partial redundancy eliminator for pure expressions.
#[derive(Debug, Default)]
struct PartialRedundancyEliminator {
    stats: PreStats,
}

/// An expression with immediate operands replaced by placeholders, the
/// immediates in operand order, and the result type, which casts take from the
/// instruction rather than from their operands.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ExprKey {
    kind: InstKind,
    immediates: SmallVec<[Immediate; 2]>,
    ty: MirType,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
    fn new() -> Self {
        Self::default()
    }

    /// Runs PRE to a fixed point.
    fn run(&mut self, func: &mut Function) -> PreStats {
        self.stats = PreStats::default();

        // Candidates are expressions in blocks with several distinct predecessors.
        let has_candidate = func.blocks.iter().any(|block| {
            block.predecessors.len() >= 2
                && block.instructions.iter().any(|&inst| {
                    let instruction = func.inst(inst);
                    Self::is_pre_expression(instruction) && func.inst_result_value(inst).is_some()
                })
        });
        if !has_candidate {
            return self.stats;
        }

        let mut inst_blocks = func.inst_blocks();

        let mut eliminated_keys = FxHashSet::default();
        let mut inserted_insts = GrowableBitSet::with_capacity(func.num_insts());
        let rewrite_limit = func.num_insts().saturating_mul(2).max(64);
        let mut rewrites = 0usize;

        while rewrites < rewrite_limit {
            // Edge splitting grows the CFG between batches, so the dominator
            // tree is recomputed before each scan.
            let cfg = CfgInfo::new(func);
            let batch = self.collect_candidates(
                func,
                cfg.dominators(),
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
                    &mut inst_blocks,
                    &mut eliminated_keys,
                    &mut inserted_insts,
                );
            }
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
        inst_blocks: &FxHashMap<InstId, BlockId>,
        eliminated_keys: &FxHashSet<(ExprKey, BlockId)>,
        inserted_insts: &GrowableBitSet<InstId>,
        limit: usize,
    ) -> Vec<PreCandidate> {
        let mut batch = Vec::new();
        // Candidates whose analysis would be invalidated by an earlier
        // candidate in this batch are deferred to the next scan.
        let mut modified_blocks = DenseBitSet::new_empty(func.blocks.len());
        let mut eliminated_values = DenseBitSet::new_empty(func.num_values());

        'targets: for target in func.blocks.indices() {
            let predecessors = &func.blocks[target].predecessors;
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
                let instruction = func.inst(inst);
                if !Self::is_pre_expression(instruction) {
                    continue;
                }
                let Some(result_ty) = instruction.result_ty else {
                    continue;
                };
                let Some(result) = func.inst_result_value(inst) else {
                    continue;
                };

                let Some(candidate) = self.candidate_for_inst(
                    func,
                    target,
                    inst,
                    result,
                    result_ty,
                    instruction.metadata.clone(),
                    predecessors,
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
        inst_blocks: &FxHashMap<InstId, BlockId>,
        dominators: &DominatorTree,
        eliminated_keys: &FxHashSet<(ExprKey, BlockId)>,
    ) -> Option<PreCandidate> {
        let original = &func.inst(inst).kind;
        let mut incoming = Vec::with_capacity(predecessors.len());
        let mut insertions = Vec::new();
        let mut available = 0usize;

        for &pred in predecessors {
            let translated =
                Self::translate_kind_for_predecessor(func, original, target, pred, inst_blocks)?;
            if !Self::operands_available_at_end(func, &translated, pred, inst_blocks, dominators) {
                return None;
            }
            let key = Self::make_expr_key(func, &translated, result_ty)?;
            if let Some(value) = Self::available_value_at_end(func, dominators, pred, &key) {
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
        if insertions.len() > available || insertions.len() > MAX_INSERTIONS_PER_REWRITE {
            return None;
        }

        Some(PreCandidate { target, inst, result, result_ty, metadata, incoming, insertions })
    }

    fn apply_candidate(
        &mut self,
        func: &mut Function,
        candidate: PreCandidate,
        inst_blocks: &mut FxHashMap<InstId, BlockId>,
        eliminated_keys: &mut FxHashSet<(ExprKey, BlockId)>,
        inserted_insts: &mut GrowableBitSet<InstId>,
    ) {
        let PreCandidate { target, inst, result, result_ty, metadata, mut incoming, insertions } =
            candidate;

        if let Some(key) = Self::make_expr_key(func, &func.inst(inst).kind, result_ty) {
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
            let mut instruction = Instruction::new(kind, Some(result_ty));
            instruction.metadata = metadata.clone();
            let (new_inst, value) = func.alloc_value_inst(instruction);
            func.blocks[block].instructions.push(new_inst);
            incoming.push((block, value));
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
                let (phi_inst, phi_value) = func.alloc_value_inst(
                    Instruction::new(InstKind::Phi(incoming), Some(result_ty))
                        .with_debug_info_dropped(),
                );
                let phi_count = func.blocks[target]
                    .instructions
                    .iter()
                    .take_while(|&&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
                    .count();
                func.blocks[target].instructions.insert(phi_count, phi_inst);
                inst_blocks.insert(phi_inst, target);
                phi_value
            }
        };

        let replacements = FxHashMap::from_iter([(result, replacement)]);
        func.replace_uses(&replacements);
        func.blocks[target].instructions.retain(|&inst_id| inst_id != inst);
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
                    && matches!(func.inst(*inst_id).kind, InstKind::Phi(_)) =>
            {
                let InstKind::Phi(incoming) = &func.inst(*inst_id).kind else {
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
            Value::Immediate(_) | Value::Arg(_) | Value::Undef(_) | Value::Error(_) => true,
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
    ) -> Option<ValueId> {
        dominators
            .self_and_dominators(block)
            .into_iter()
            .find_map(|block| Self::available_value_in_block(func, block, key))
    }

    fn available_value_in_block(func: &Function, block: BlockId, key: &ExprKey) -> Option<ValueId> {
        func.blocks[block].instructions.iter().rev().find_map(|&inst| {
            let instruction = func.inst(inst);
            if !Self::is_pre_expression(instruction) {
                return None;
            }
            let candidate_key =
                Self::make_expr_key(func, &instruction.kind, instruction.result_ty?)?;
            (candidate_key == *key).then(|| func.inst_result_value(inst)).flatten()
        })
    }

    /// Returns whether `instruction` computes a word from its operands alone,
    /// so it can move to any point where its operands are available.
    fn is_pre_expression(instruction: &Instruction) -> bool {
        let kind = &instruction.kind;
        !matches!(kind, InstKind::Phi(_))
            && kind.effect_kind() == EffectKind::Pure
            && !kind.has_side_effects()
            && kind.any_operand(|_| true)
            && instruction.result_ty.is_some_and(MirType::is_word)
    }

    fn make_expr_key(func: &Function, kind: &InstKind, ty: MirType) -> Option<ExprKey> {
        let operand = |value| match func.value(value) {
            Value::Immediate(imm) => OperandKey::Immediate(imm.clone()),
            _ => OperandKey::Value(value),
        };
        let mut kind = match *kind {
            InstKind::Gt(a, b) => InstKind::Lt(b, a),
            InstKind::SGt(a, b) => InstKind::SLt(b, a),
            _ => kind.op().canonicalize_commutative_by_key(operand).into_kind()?,
        };
        let mut immediates = SmallVec::new();
        kind.visit_operands_mut(|value| {
            if let Value::Immediate(imm) = func.value(*value) {
                immediates.push(imm.clone());
                *value = ValueId::MAX;
            }
        });
        Some(ExprKey { kind, immediates, ty })
    }
}

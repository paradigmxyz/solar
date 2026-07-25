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
        BlockId, Function, Immediate, InstId, InstKind, Instruction, InstructionMetadata,
        MemoryObjectKind, MemoryObjectLayout, MirType, Module, Terminator, Value, ValueId,
        utils::split_edge,
    },
    pass::{MirPass, run_function_pass_no_analyses},
};
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet},
};
use std::cmp::Ordering;

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
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass_no_analyses(module, analyses, |func| {
            if !may_have_partial_redundancy(func) {
                return false;
            }
            PartialRedundancyEliminator::new().run(func).total() != 0
        })
    }
}

#[must_use]
fn may_have_partial_redundancy(func: &Function) -> bool {
    let has_join = func.blocks.iter().any(|block| {
        let Some(&first) = block.predecessors.first() else { return false };
        block.predecessors[1..].iter().any(|&predecessor| predecessor != first)
    });
    has_join
        && func
            .instructions()
            .any(|inst_id| PartialRedundancyEliminator::is_pre_expression(&func.inst(inst_id).kind))
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
    MemoryObjectData(OperandKey, MemoryObjectKind),
    MemoryObjectFieldAddr(OperandKey, MemoryObjectLayout, u64),
    MemoryObjectElementAddr(OperandKey, MemoryObjectLayout, OperandKey),
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

struct RewriteState {
    inst_results: IndexVec<InstId, Option<ValueId>>,
    inst_blocks: IndexVec<InstId, Option<BlockId>>,
    eliminated_keys: FxHashSet<(ExprKey, BlockId)>,
    inserted_insts: GrowableBitSet<InstId>,
}

struct RewriteBatch {
    replacements: FxHashMap<ValueId, ValueId>,
    dead: GrowableBitSet<InstId>,
    split_edges: FxHashMap<(BlockId, BlockId), BlockId>,
}

impl PartialRedundancyEliminator {
    /// Creates a new PRE pass.
    fn new() -> Self {
        Self::default()
    }

    /// Runs PRE to a fixed point.
    fn run(&mut self, func: &mut Function) -> PreStats {
        self.stats = PreStats::default();

        let mut state = RewriteState {
            inst_results: func.inst_results(),
            inst_blocks: func.inst_blocks(),
            eliminated_keys: FxHashSet::default(),
            inserted_insts: GrowableBitSet::with_capacity(func.num_insts()),
        };
        let rewrite_limit = func.num_insts().saturating_mul(2).max(64);
        let mut rewrites = 0usize;

        while rewrites < rewrite_limit {
            // Edge splitting grows the CFG between batches, so the dominator
            // tree is recomputed before each scan.
            let cfg = CfgInfo::new(func);
            let batch = self.collect_candidates(
                func,
                cfg.dominators(),
                &state.inst_results,
                &state.inst_blocks,
                &state.eliminated_keys,
                &state.inserted_insts,
                rewrite_limit - rewrites,
            );
            if batch.is_empty() {
                break;
            }
            rewrites += batch.len();
            let mut edits = RewriteBatch {
                replacements: FxHashMap::default(),
                dead: GrowableBitSet::with_capacity(func.num_insts()),
                split_edges: FxHashMap::default(),
            };
            for candidate in batch {
                self.apply_candidate(func, candidate, &mut state, &mut edits);
            }
            func.replace_uses_canonicalized(&edits.replacements);
            for block in &mut func.blocks {
                block.instructions.retain(|&inst| !edits.dead.contains(inst));
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
        inst_results: &IndexVec<InstId, Option<ValueId>>,
        inst_blocks: &IndexVec<InstId, Option<BlockId>>,
        eliminated_keys: &FxHashSet<(ExprKey, BlockId)>,
        inserted_insts: &GrowableBitSet<InstId>,
        limit: usize,
    ) -> Vec<PreCandidate> {
        let mut batch = Vec::new();
        // Candidates that consume a value eliminated by an earlier candidate
        // in this batch are deferred to the next scan.
        let mut eliminated_values = DenseBitSet::new_empty(func.values.len());
        let mut phi_incoming = FxHashMap::default();
        for inst in func.instructions() {
            if let InstKind::Phi(incoming) = &func.inst(inst).kind {
                for &(pred, value) in incoming {
                    phi_incoming.insert((inst, pred), value);
                }
            }
        }
        let mut available = index_vec![FxHashMap::default(); func.blocks.len()];
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst in &block.instructions {
                let instruction = func.inst(inst);
                if let Some(result) = inst_results[inst]
                    && let Some(key) = Self::make_expr_key(func, &instruction.kind)
                {
                    available[block_id].insert(key, result);
                }
            }
        }

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
                let instruction = func.inst(inst);
                if !Self::is_pre_expression(&instruction.kind) {
                    continue;
                }
                let Some(result_ty) = instruction.result_ty else {
                    continue;
                };
                let Some(result) = inst_results[inst] else {
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
                    inst_blocks,
                    &phi_incoming,
                    &available,
                    dominators,
                    eliminated_keys,
                ) else {
                    continue;
                };

                if Self::interferes_with_batch(&candidate, &eliminated_values) {
                    continue;
                }
                eliminated_values.insert(candidate.result);
                batch.push(candidate);
            }
        }

        batch
    }

    /// Returns true if this candidate references a value whose defining
    /// instruction an earlier candidate in the batch removes.
    fn interferes_with_batch(
        candidate: &PreCandidate,
        eliminated_values: &DenseBitSet<ValueId>,
    ) -> bool {
        candidate.incoming.iter().any(|&(_, value)| eliminated_values.contains(value))
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
        inst_blocks: &IndexVec<InstId, Option<BlockId>>,
        phi_incoming: &FxHashMap<(InstId, BlockId), ValueId>,
        available: &IndexVec<BlockId, FxHashMap<ExprKey, ValueId>>,
        dominators: &DominatorTree,
        eliminated_keys: &FxHashSet<(ExprKey, BlockId)>,
    ) -> Option<PreCandidate> {
        let original = &func.inst(inst).kind;
        let mut incoming = Vec::with_capacity(predecessors.len());
        let mut insertions = Vec::new();
        let mut available_paths = 0usize;

        for &pred in predecessors {
            let translated = Self::translate_kind_for_predecessor(
                func,
                original,
                target,
                pred,
                inst_blocks,
                phi_incoming,
            )?;
            if !Self::operands_available_at_end(func, &translated, pred, inst_blocks, dominators) {
                return None;
            }
            let key = Self::make_expr_key(func, &translated)?;
            if let Some(value) = Self::available_value_at_end(dominators, pred, &key, available) {
                available_paths += 1;
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
        if insertions.len() > available_paths || insertions.len() > MAX_INSERTIONS_PER_REWRITE {
            return None;
        }

        Some(PreCandidate { target, inst, result, result_ty, metadata, incoming, insertions })
    }

    fn apply_candidate(
        &mut self,
        func: &mut Function,
        candidate: PreCandidate,
        state: &mut RewriteState,
        edits: &mut RewriteBatch,
    ) {
        let PreCandidate { target, inst, result, result_ty, metadata, mut incoming, insertions } =
            candidate;

        if let Some(key) = Self::make_expr_key(func, &func.inst(inst).kind) {
            state.eliminated_keys.insert((key, target));
        }

        let fully_available = insertions.is_empty();
        for (pred, _) in &mut incoming {
            if let Some(&split) = edits.split_edges.get(&(*pred, target)) {
                *pred = split;
            }
        }
        for (pred, kind) in insertions {
            // A jump-terminated predecessor owns its single outgoing edge, so
            // the computation can go at its end. Any other terminator makes
            // the edge critical: split it so the computation runs only on the
            // edge into the join. The split block sits on that edge, so the
            // per-edge phi translation that held for `pred` holds for it too.
            let block = match edits.split_edges.get(&(pred, target)).copied() {
                Some(split) => split,
                None => match func.blocks[pred].terminator {
                    Some(Terminator::Jump(jump_target)) => {
                        debug_assert_eq!(jump_target, target);
                        pred
                    }
                    _ => {
                        let split = split_edge(func, pred, target);
                        edits.split_edges.insert((pred, target), split);
                        split
                    }
                },
            };
            let new_inst = func.alloc_inst(Instruction {
                kind,
                result_ty: Some(result_ty),
                metadata: metadata.clone(),
            });
            let value = func.alloc_value(Value::Inst(new_inst));
            func.blocks[block].instructions.push(new_inst);
            incoming.push((block, value));
            let result_inst = state.inst_results.push(Some(value));
            let block_inst = state.inst_blocks.push(Some(block));
            debug_assert_eq!(result_inst, new_inst);
            debug_assert_eq!(block_inst, new_inst);
            state.inserted_insts.insert(new_inst);
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
                    .take_while(|&&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
                    .count();
                func.blocks[target].instructions.insert(phi_count, phi_inst);
                let result_inst = state.inst_results.push(Some(phi_value));
                let block_inst = state.inst_blocks.push(Some(target));
                debug_assert_eq!(result_inst, phi_inst);
                debug_assert_eq!(block_inst, phi_inst);
                phi_value
            }
        };

        edits.replacements.insert(result, replacement);
        edits.dead.insert(inst);
        state.inst_results[inst] = None;
        state.inst_blocks[inst] = None;
        self.stats.expressions_eliminated += 1;
    }

    fn translate_kind_for_predecessor(
        func: &Function,
        kind: &InstKind,
        target: BlockId,
        pred: BlockId,
        inst_blocks: &IndexVec<InstId, Option<BlockId>>,
        phi_incoming: &FxHashMap<(InstId, BlockId), ValueId>,
    ) -> Option<InstKind> {
        let mut translated = kind.clone();
        let mut ok = true;
        translated.visit_operands_mut(|value| {
            if let Some(translated) = Self::translate_value_for_predecessor(
                func,
                *value,
                target,
                pred,
                inst_blocks,
                phi_incoming,
            ) {
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
        inst_blocks: &IndexVec<InstId, Option<BlockId>>,
        phi_incoming: &FxHashMap<(InstId, BlockId), ValueId>,
    ) -> Option<ValueId> {
        match func.value(value) {
            Value::Inst(inst_id)
                if inst_blocks[*inst_id] == Some(target)
                    && matches!(func.inst(*inst_id).kind, InstKind::Phi(_)) =>
            {
                phi_incoming.get(&(*inst_id, pred)).copied()
            }
            _ => Some(value),
        }
    }

    fn operands_available_at_end(
        func: &Function,
        kind: &InstKind,
        block: BlockId,
        inst_blocks: &IndexVec<InstId, Option<BlockId>>,
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
        inst_blocks: &IndexVec<InstId, Option<BlockId>>,
        dominators: &DominatorTree,
    ) -> bool {
        match func.value(value) {
            Value::Immediate(_) | Value::Arg { .. } | Value::Undef(_) | Value::Error(_) => true,
            Value::Inst(inst) => {
                inst_blocks[*inst].is_some_and(|def_block| dominators.dominates(def_block, block))
            }
        }
    }

    /// Finds the translated expression in `block` or any of its dominators; a
    /// def in a dominator is available at `block`'s end with no further
    /// checks.
    fn available_value_at_end(
        dominators: &DominatorTree,
        block: BlockId,
        key: &ExprKey,
        available: &IndexVec<BlockId, FxHashMap<ExprKey, ValueId>>,
    ) -> Option<ValueId> {
        dominators
            .self_and_dominators(block)
            .into_iter()
            .find_map(|block| available[block].get(key).copied())
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
                | InstKind::MemoryObjectData(_, _)
                | InstKind::MemoryObjectFieldAddr { .. }
                | InstKind::MemoryObjectElementAddr { .. }
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
            InstKind::MemoryObjectData(object, kind) => {
                Some(ExprKey::MemoryObjectData(operand(*object), *kind))
            }
            InstKind::MemoryObjectFieldAddr { object, layout, field } => {
                Some(ExprKey::MemoryObjectFieldAddr(operand(*object), *layout, *field))
            }
            InstKind::MemoryObjectElementAddr { object, layout, index } => {
                Some(ExprKey::MemoryObjectElementAddr(operand(*object), *layout, operand(*index)))
            }
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

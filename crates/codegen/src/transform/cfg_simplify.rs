//! CFG Simplification and Normalization passes.
//!
//! This module provides optimization passes to clean up the Control Flow Graph:
//!
//! ## Block Merging
//! If block A unconditionally jumps to B, and B has only A as predecessor,
//! merge A and B into a single block. This reduces jump instructions (8 gas each).
//!
//! ## Empty Block Elimination
//! Remove blocks that contain no instructions and only an unconditional jump,
//! redirecting predecessors to the target.
//!
//! ## Dead Function Elimination
//! Remove functions that are never called, starting from entry points
//! (public/external functions, constructor, fallback, receive).

use crate::{
    analysis::{CallGraphInfo, CfgInfo},
    mir::{
        AbiLayoutRef, AllocationKind, AllocationSemantics, BlockId, EffectKind, Function,
        FunctionId, Immediate, InstKind, MemoryObjectKind, MemoryObjectLayout, MemoryRegion,
        MirType, Module, StorageAlias, StorageLayoutRef, Terminator, Value, ValueId,
        utils::{repair_reachability_phis, retain_blocks},
    },
    pass::{MirPass, run_function_pass},
};
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet},
};

type SwitchTargetIndex = IndexVec<BlockId, Option<FxHashMap<BlockId, Vec<usize>>>>;

/// Function pass for CFG simplification.
pub(crate) struct CfgSimplify;

impl MirPass for CfgSimplify {
    fn name(&self) -> &'static str {
        "cfg-simplify"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| {
            CfgSimplifier::new().run_to_fixpoint(func).total() != 0
        })
    }
}

/// Module pass for dead internal function elimination.
pub(crate) struct FunctionDce;

impl MirPass for FunctionDce {
    fn name(&self) -> &'static str {
        "function-dce"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        DeadFunctionEliminator::new().run(module) != 0
    }
}

/// Alpha-equivalence key for a terminal block used by
/// [`CfgSimplifier::deduplicate_terminal_blocks`].
#[derive(Debug, PartialEq, Eq, Hash)]
struct CanonBlock {
    insts: Vec<CanonInst>,
    term_mnemonic: &'static str,
    term_payload: CanonTermPayload,
    term_operands: Vec<CanonOperand>,
}

/// Alpha-equivalence key for one instruction of a terminal block.
#[derive(Debug, PartialEq, Eq, Hash)]
struct CanonInst {
    mnemonic: &'static str,
    payload: CanonPayload,
    operands: Vec<CanonOperand>,
    result_ty: Option<MirType>,
    metadata: CanonMetadata,
}

/// Non-operand payload carried by an instruction kind.
#[derive(Debug, PartialEq, Eq, Hash)]
enum CanonPayload {
    None,
    Alloc(AllocationKind, AllocationSemantics),
    MemoryObjectKind(MemoryObjectKind),
    MemoryObjectField(MemoryObjectLayout, u64),
    MemoryObjectElement(MemoryObjectLayout),
    AbiEncode(AbiLayoutRef),
    StorageLayout(StorageLayoutRef),
    FrameAddr(u64),
    Immutable(u32),
    Call(FunctionId, usize),
}

/// Non-operand payload carried by a terminal instruction.
#[derive(Debug, PartialEq, Eq, Hash)]
enum CanonTermPayload {
    None,
    TailCall(FunctionId),
}

/// Optimization-relevant instruction metadata.
#[derive(Debug, PartialEq, Eq, Hash)]
struct CanonMetadata {
    storage_alias: Option<StorageAlias>,
    memory_region: Option<MemoryRegion>,
    effect: Option<EffectKind>,
    unchecked: bool,
    deferred_alloc: bool,
}

/// A canonicalized operand: block-local results compare by definition
/// position, immediates by value, and everything else by exact [`ValueId`].
#[derive(Debug, PartialEq, Eq, Hash)]
enum CanonOperand {
    Local(usize),
    Imm(Immediate),
    Outside(ValueId),
}

/// Statistics from CFG simplification.
#[derive(Debug, Default, Clone)]
struct CfgSimplifyStats {
    /// Number of blocks merged.
    blocks_merged: usize,
    /// Number of empty blocks eliminated.
    empty_blocks_eliminated: usize,
    /// Number of degenerate terminators simplified.
    terminators_simplified: usize,
    /// Number of trivial phi nodes replaced by their unique incoming value.
    trivial_phis_simplified: usize,
    /// Number of identical terminal blocks merged into one shared block.
    terminal_blocks_deduplicated: usize,
    /// Number of unreachable block tombstones removed.
    unreachable_blocks_removed: usize,
    /// Number of dead functions eliminated.
    dead_functions_eliminated: usize,
    /// Whether CFG backlinks or phi inputs were repaired.
    reachability_repaired: bool,
    /// Estimated gas saved (8 gas per eliminated jump).
    gas_saved: usize,
}

impl CfgSimplifyStats {
    /// Returns total optimizations performed.
    #[must_use]
    fn total(&self) -> usize {
        self.blocks_merged
            + self.empty_blocks_eliminated
            + self.terminators_simplified
            + self.trivial_phis_simplified
            + self.terminal_blocks_deduplicated
            + self.unreachable_blocks_removed
            + self.dead_functions_eliminated
            + self.reachability_repaired as usize
    }

    /// Combines stats from another run.
    fn combine(&mut self, other: &Self) {
        self.blocks_merged += other.blocks_merged;
        self.empty_blocks_eliminated += other.empty_blocks_eliminated;
        self.terminators_simplified += other.terminators_simplified;
        self.trivial_phis_simplified += other.trivial_phis_simplified;
        self.terminal_blocks_deduplicated += other.terminal_blocks_deduplicated;
        self.unreachable_blocks_removed += other.unreachable_blocks_removed;
        self.dead_functions_eliminated += other.dead_functions_eliminated;
        self.reachability_repaired |= other.reachability_repaired;
        self.gas_saved += other.gas_saved;
    }
}

/// CFG simplification pass for a single function.
#[derive(Debug, Default)]
struct CfgSimplifier {
    /// Statistics from the last run.
    stats: CfgSimplifyStats,
}

impl CfgSimplifier {
    /// Creates a new CFG simplifier.
    #[must_use]
    fn new() -> Self {
        Self::default()
    }

    /// Runs CFG simplification on a function.
    /// Returns the number of optimizations performed.
    fn run(&mut self, func: &mut Function) -> usize {
        self.stats = CfgSimplifyStats::default();

        self.simplify_degenerate_terminators(func);
        self.stats.unreachable_blocks_removed += self.remove_unreachable_blocks(func);
        self.merge_blocks(func);
        self.eliminate_empty_blocks(func);
        self.deduplicate_terminal_blocks(func);
        self.simplify_trivial_phis(func);

        self.stats.total()
    }

    /// Merges identical terminal blocks (no phis, terminator without
    /// successors, alpha-equivalent instructions) into one shared block and
    /// redirects all predecessor edges to it.
    ///
    /// Checked arithmetic materializes one panic block per check; this folds
    /// them to one block per panic code (and shared revert-string blocks) per
    /// function. The rewrite is phi-safe by construction: the kept block has
    /// no phis and a terminal block has no successors, so no phi inputs
    /// elsewhere can mention it.
    fn deduplicate_terminal_blocks(&mut self, func: &mut Function) {
        let mut kept = FxHashMap::default();
        let mut merges: Vec<(BlockId, BlockId)> = Vec::new();
        for block_id in func.blocks.indices() {
            if func.blocks[block_id].predecessors.is_empty() {
                continue;
            }
            let Some(canon) = Self::canonicalize_terminal_block(func, block_id) else {
                continue;
            };
            if let Some(&keep) = kept.get(&canon) {
                merges.push((block_id, keep));
            } else {
                kept.insert(canon, block_id);
            }
        }

        if merges.is_empty() {
            return;
        }

        let mut predecessor_edges = FxHashSet::default();
        for (block, basic_block) in func.blocks.iter_enumerated() {
            predecessor_edges.extend(basic_block.predecessors.iter().map(|&pred| (pred, block)));
        }
        for (dup, keep) in merges {
            let predecessors = func.unique_predecessors(dup);
            for pred in predecessors {
                self.redirect_terminator(func, pred, dup, keep);
                if predecessor_edges.insert((pred, keep)) {
                    func.blocks[keep].predecessors.push(pred);
                }
            }
            func.blocks[dup].instructions.clear();
            func.blocks[dup].terminator = Some(Terminator::Invalid);
            func.blocks[dup].predecessors.clear();
            self.stats.terminal_blocks_deduplicated += 1;
        }
    }

    /// Builds the alpha-equivalence key of a terminal block, or `None` if the
    /// block is not a dedup candidate.
    fn canonicalize_terminal_block(func: &Function, block_id: BlockId) -> Option<CanonBlock> {
        let block = &func.blocks[block_id];
        let term = block.terminator.as_ref()?;
        if matches!(term, Terminator::Invalid) || !term.successors().is_empty() {
            return None;
        }

        let mut local_defs: FxHashMap<ValueId, usize> = FxHashMap::default();
        for (position, &inst_id) in block.instructions.iter().enumerate() {
            if let Some(result) = func.inst_result_value(inst_id) {
                local_defs.insert(result, position);
            }
        }

        let canon_operand = |value: ValueId| {
            if let Some(&position) = local_defs.get(&value) {
                return CanonOperand::Local(position);
            }
            match &func.values[value] {
                Value::Immediate(imm) => CanonOperand::Imm(imm.clone()),
                _ => CanonOperand::Outside(value),
            }
        };

        let mut insts = Vec::with_capacity(block.instructions.len());
        for &inst_id in &block.instructions {
            let inst = func.inst(inst_id);
            let extra = match &inst.kind {
                InstKind::Phi(_) => return None,
                InstKind::Alloc { kind, semantics, .. } => CanonPayload::Alloc(*kind, *semantics),
                InstKind::MemoryObjectLen(_, kind)
                | InstKind::SetMemoryObjectLen(_, _, kind)
                | InstKind::MemoryObjectData(_, kind) => CanonPayload::MemoryObjectKind(*kind),
                InstKind::MemoryObjectFieldAddr { layout, field, .. } => {
                    CanonPayload::MemoryObjectField(*layout, *field)
                }
                InstKind::MemoryObjectElementAddr { layout, .. } => {
                    CanonPayload::MemoryObjectElement(*layout)
                }
                InstKind::AbiEncode { layout, .. } => CanonPayload::AbiEncode(layout.clone()),
                InstKind::StorageToMemory { layout, .. }
                | InstKind::MemoryToStorage { layout, .. }
                | InstKind::ClearStorage { layout, .. } => {
                    CanonPayload::StorageLayout(layout.clone())
                }
                InstKind::InternalFrameAddr(offset) => CanonPayload::FrameAddr(*offset),
                InstKind::LoadImmutable(offset) => CanonPayload::Immutable(*offset),
                InstKind::InternalCall { function, returns, .. } => {
                    CanonPayload::Call(*function, *returns as usize)
                }
                _ => CanonPayload::None,
            };
            let metadata = CanonMetadata {
                storage_alias: inst.metadata.storage_alias(),
                memory_region: inst.metadata.memory_region(),
                effect: inst.metadata.effect(),
                unchecked: inst.metadata.unchecked(),
                deferred_alloc: inst.metadata.deferred_alloc(),
            };
            insts.push(CanonInst {
                mnemonic: inst.kind.mnemonic(),
                payload: extra,
                operands: inst.kind.operands().into_iter().map(canon_operand).collect(),
                result_ty: inst.result_ty,
                metadata,
            });
        }

        let term_payload = match term {
            Terminator::TailCall { function, .. } => CanonTermPayload::TailCall(*function),
            _ => CanonTermPayload::None,
        };
        let term_operands = term.operands().into_iter().map(canon_operand).collect();
        Some(CanonBlock { insts, term_mnemonic: term.mnemonic(), term_payload, term_operands })
    }

    fn simplify_trivial_phis(&mut self, func: &mut Function) {
        let mut candidates = Vec::new();
        let mut raw = FxHashMap::default();

        for block_id in func.blocks.indices() {
            for &inst_id in &func.blocks[block_id].instructions {
                let InstKind::Phi(incoming) = &func.inst(inst_id).kind else {
                    continue;
                };
                let Some(phi_value) = func.inst_result_value(inst_id) else {
                    continue;
                };
                let Some(replacement) = Self::trivial_phi_replacement(incoming, phi_value) else {
                    continue;
                };
                candidates.push((inst_id, phi_value));
                raw.insert(phi_value, replacement);
            }
        }

        if raw.is_empty() {
            return;
        }

        // A trivial phi may be replaced by another phi deleted in the same
        // batch (`v81 -> v82 -> v80`); uses must be rewritten to the end of
        // the chain or they dangle once the intermediate phi is removed.
        // Mutually-trivial cycles have no outside source; keep those phis.
        let mut replacements = FxHashMap::default();
        let mut dead = DenseBitSet::new_empty(func.num_insts());
        let mut resolved = FxHashMap::default();
        let mut visiting = DenseBitSet::new_empty(func.values.len());
        let mut path = Vec::new();
        for &(inst_id, phi_value) in &candidates {
            if !resolved.contains_key(&phi_value) {
                path.clear();
                let mut current = phi_value;
                let target = loop {
                    if let Some(&target) = resolved.get(&current) {
                        break target;
                    }
                    let Some(&next) = raw.get(&current) else {
                        break Some(current);
                    };
                    if !visiting.insert(current) {
                        break None;
                    }
                    path.push(current);
                    current = next;
                };
                for &value in &path {
                    visiting.remove(value);
                    resolved.insert(value, target);
                }
            }

            if let Some(target) = resolved[&phi_value] {
                replacements.insert(phi_value, target);
                dead.insert(inst_id);
            }
        }

        if replacements.is_empty() {
            return;
        }

        func.replace_uses(&replacements);
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|&inst_id| !dead.contains(inst_id));
        }
        self.stats.trivial_phis_simplified += dead.count();
    }

    fn trivial_phi_replacement(
        incoming: &[(BlockId, ValueId)],
        phi_value: ValueId,
    ) -> Option<ValueId> {
        let mut incoming_values = incoming.iter().map(|(_, value)| *value);
        let first = incoming_values.find(|value| *value != phi_value)?;
        incoming_values.all(|value| value == phi_value || value == first).then_some(first)
    }

    fn simplify_degenerate_terminators(&mut self, func: &mut Function) {
        let mut changed = false;
        for block_id in func.blocks.indices() {
            if let Some(Terminator::Switch { default, cases, .. }) =
                func.blocks[block_id].terminator.as_mut()
            {
                let old_len = cases.len();
                while cases.last().is_some_and(|(_, target)| target == default) {
                    cases.pop();
                }
                if cases.len() != old_len {
                    self.stats.terminators_simplified += old_len - cases.len();
                    changed = true;
                }
            }

            let replacement = match func.blocks[block_id].terminator.as_ref() {
                Some(Terminator::Branch { condition, then_block, else_block }) => func
                    .value_u256(*condition)
                    .map(|condition| if condition.is_zero() { *else_block } else { *then_block })
                    .or((*then_block == *else_block).then_some(*then_block)),
                Some(Terminator::Switch { value, default, cases }) => func
                    .value_u256(*value)
                    .and_then(|value| {
                        for &(case, target) in cases {
                            let case = func.value_u256(case)?;
                            if case == value {
                                return Some(target);
                            }
                        }
                        Some(*default)
                    })
                    .or_else(|| cases.is_empty().then_some(*default)),
                _ => None,
            };
            if let Some(target) = replacement {
                func.blocks[block_id].terminator = Some(Terminator::Jump(target));
                self.stats.terminators_simplified += 1;
                self.stats.gas_saved += 10;
                changed = true;
            }
        }

        if changed {
            self.stats.reachability_repaired |= repair_reachability_phis(func);
        }
    }

    /// Runs CFG simplification iteratively until no more changes.
    fn run_to_fixpoint(&mut self, func: &mut Function) -> CfgSimplifyStats {
        let mut total_stats = CfgSimplifyStats::default();
        loop {
            let changed = self.run(func);
            if changed == 0 {
                break;
            }
            total_stats.combine(&self.stats);
        }
        total_stats
    }

    fn remove_unreachable_blocks(&self, func: &mut Function) -> usize {
        let cfg = CfgInfo::new(func);
        let order =
            func.blocks.indices().filter(|&block| cfg.is_reachable(block)).collect::<Vec<_>>();
        let removed = func.blocks.len() - order.len();
        if removed != 0 {
            retain_blocks(func, &order);
        }
        removed
    }

    /// Merges blocks where A unconditionally jumps to B and B has only A as predecessor.
    fn merge_blocks(&mut self, func: &mut Function) {
        let block_count = func.blocks.len();
        for block_id in (0..block_count).map(BlockId::from_usize) {
            while let Some(target) = self.can_merge(func, block_id) {
                self.do_merge(func, block_id, target);
                self.stats.blocks_merged += 1;
                self.stats.gas_saved += 8;
            }
        }
    }

    /// Checks if block_id can be merged with its successor.
    /// Returns the target block if merge is possible.
    fn can_merge(&self, func: &Function, block_id: BlockId) -> Option<BlockId> {
        let block = &func.blocks[block_id];

        let Terminator::Jump(target) = block.terminator.as_ref()? else {
            return None;
        };

        if *target == block_id {
            return None;
        }

        let target_block = &func.blocks[*target];
        if target_block.predecessors.len() != 1 {
            return None;
        }

        if target_block.predecessors[0] != block_id {
            return None;
        }

        for &inst_id in &target_block.instructions {
            let InstKind::Phi(incoming) = &func.inst(inst_id).kind else {
                continue;
            };
            if !incoming.iter().any(|(pred, _)| *pred == block_id) {
                return None;
            }
        }

        Some(*target)
    }

    /// Merges block_id with target, appending target's instructions and terminator to block_id.
    fn do_merge(&self, func: &mut Function, block_id: BlockId, target: BlockId) {
        let phi_replacements = self.fold_target_phis_for_merge(func, block_id, target);
        let target_instructions: Vec<_> = func.blocks[target]
            .instructions
            .iter()
            .copied()
            .filter(|&inst_id| !matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
            .collect();
        let target_terminator = func.blocks[target].terminator.take();
        let target_successors =
            target_terminator.as_ref().map(Terminator::successors).unwrap_or_default();

        func.blocks[block_id].instructions.extend(target_instructions);
        func.blocks[block_id].terminator = target_terminator;

        for &succ in &target_successors {
            self.redirect_target_phi_incoming(func, target, succ, &[block_id]);

            let succ_block = &mut func.blocks[succ];
            for pred in &mut succ_block.predecessors {
                if *pred == target {
                    *pred = block_id;
                }
            }
        }

        func.blocks[target].instructions.clear();
        func.blocks[target].terminator = Some(Terminator::Invalid);
        func.blocks[target].predecessors.clear();

        func.replace_uses(&phi_replacements);
    }

    fn fold_target_phis_for_merge(
        &self,
        func: &Function,
        pred: BlockId,
        target: BlockId,
    ) -> FxHashMap<ValueId, ValueId> {
        let mut replacements = FxHashMap::default();
        for &inst_id in &func.blocks[target].instructions {
            let InstKind::Phi(incoming) = &func.inst(inst_id).kind else {
                continue;
            };
            let Some(phi_value) = func.inst_result_value(inst_id) else {
                continue;
            };
            let Some((_, incoming_value)) =
                incoming.iter().find(|(incoming_pred, _)| *incoming_pred == pred)
            else {
                continue;
            };
            replacements.insert(phi_value, *incoming_value);
        }
        replacements
    }

    /// Eliminates empty blocks that only contain an unconditional jump.
    fn eliminate_empty_blocks(&mut self, func: &mut Function) {
        let cfg = CfgInfo::new(func);
        let mut loop_preheaders = DenseBitSet::new_empty(func.blocks.len());
        let mut switch_targets = index_vec![None; func.blocks.len()];
        // Defer tombstone removal so a group of forwarders into the same
        // target scans its predecessor list only once.
        let mut eliminated = DenseBitSet::new_empty(func.blocks.len());
        let mut dirty_predecessors = DenseBitSet::new_empty(func.blocks.len());
        for block_id in func.blocks.indices() {
            if self.is_loop_preheader_forwarder(func, &cfg, block_id) {
                loop_preheaders.insert(block_id);
            }
        }

        let block_count = func.blocks.len();
        for block_id in (0..block_count).map(BlockId::from_usize) {
            if dirty_predecessors.remove(block_id) {
                func.blocks[block_id].predecessors.retain(|pred| !eliminated.contains(*pred));
            }
            if func.blocks[block_id].predecessors.is_empty() && cfg.is_reachable(block_id) {
                continue;
            }

            if self.is_empty_forwarder(func, block_id)
                && !loop_preheaders.contains(block_id)
                && self.forwarder_elimination_preserves_phis(func, block_id)
            {
                let target = match &func.blocks[block_id].terminator {
                    Some(Terminator::Jump(target)) => *target,
                    _ => unreachable!(),
                };
                self.eliminate_forwarder(func, block_id, target, &mut switch_targets);
                eliminated.insert(block_id);
                dirty_predecessors.insert(target);
                self.stats.empty_blocks_eliminated += 1;
                self.stats.gas_saved += 8;
            }
        }
        for block_id in dirty_predecessors.iter() {
            func.blocks[block_id].predecessors.retain(|pred| !eliminated.contains(*pred));
        }
    }

    /// Checks if a block is an empty forwarder (no instructions, just a jump).
    fn is_empty_forwarder(&self, func: &Function, block_id: BlockId) -> bool {
        let block = &func.blocks[block_id];

        if !block.instructions.is_empty() {
            return false;
        }

        matches!(&block.terminator, Some(Terminator::Jump(target)) if *target != block_id)
    }

    fn is_loop_preheader_forwarder(
        &self,
        func: &Function,
        cfg: &CfgInfo,
        block_id: BlockId,
    ) -> bool {
        let Some(Terminator::Jump(target)) = func.blocks[block_id].terminator else {
            return false;
        };
        if !matches!(
            func.blocks[target].instructions.first(),
            Some(&inst) if matches!(func.inst(inst).kind, InstKind::Phi(_))
        ) {
            return false;
        }

        func.blocks[target]
            .predecessors
            .iter()
            .copied()
            .any(|pred| pred != block_id && cfg.dominators().dominates(target, pred))
    }

    /// Checks that redirecting the forwarder's predecessors into its target
    /// keeps the target's phis well formed: a predecessor must not end up with
    /// two incoming entries carrying different values (e.g. both arms of one
    /// branch being forwarders into the same join), since phi incoming lists
    /// are keyed per predecessor block, not per CFG edge.
    fn forwarder_elimination_preserves_phis(&self, func: &Function, block_id: BlockId) -> bool {
        let Some(Terminator::Jump(target)) = func.blocks[block_id].terminator else {
            return false;
        };
        let predecessors = &func.blocks[block_id].predecessors;
        for &inst_id in &func.blocks[target].instructions {
            let InstKind::Phi(incoming) = &func.inst(inst_id).kind else {
                continue;
            };
            let Some(&(_, forwarded)) = incoming.iter().find(|(pred, _)| *pred == block_id) else {
                continue;
            };
            for &pred in predecessors {
                if incoming.iter().any(|&(other, value)| other == pred && value != forwarded) {
                    return false;
                }
            }
        }
        true
    }

    /// Eliminates an empty forwarder block by redirecting its predecessors.
    fn eliminate_forwarder(
        &self,
        func: &mut Function,
        block_id: BlockId,
        target: BlockId,
        switch_targets: &mut SwitchTargetIndex,
    ) {
        let predecessors = func.unique_predecessors(block_id);
        self.redirect_target_phi_incoming(func, block_id, target, &predecessors);

        for pred_id in predecessors {
            self.redirect_terminator_indexed(func, pred_id, block_id, target, switch_targets);

            func.blocks[target].predecessors.push(pred_id);
        }

        func.blocks[block_id].instructions.clear();
        func.blocks[block_id].terminator = Some(Terminator::Invalid);
        func.blocks[block_id].predecessors.clear();
    }

    fn redirect_target_phi_incoming(
        &self,
        func: &mut Function,
        old_pred: BlockId,
        target: BlockId,
        new_preds: &[BlockId],
    ) {
        let instruction_count = func.blocks[target].instructions.len();
        let mut seen = DenseBitSet::new_empty(func.blocks.len());
        for index in 0..instruction_count {
            let inst_id = func.blocks[target].instructions[index];
            let InstKind::Phi(incoming) = &mut func.inst_mut(inst_id).kind else {
                continue;
            };

            let mut rewritten: Vec<(BlockId, ValueId)> =
                Vec::with_capacity(incoming.len() + new_preds.len());
            for &(pred, value) in incoming.iter() {
                if pred == old_pred {
                    rewritten.extend(new_preds.iter().map(|&new_pred| (new_pred, value)));
                } else {
                    rewritten.push((pred, value));
                }
            }
            // The safety check guarantees colliding entries carry equal values;
            // keep one entry per predecessor block.
            seen.clear();
            rewritten.retain(|&(pred, _)| seen.insert(pred));
            *incoming = rewritten;
        }
    }

    /// Redirects a terminator while indexing switch target positions lazily.
    fn redirect_terminator_indexed(
        &self,
        func: &mut Function,
        block_id: BlockId,
        old_target: BlockId,
        new_target: BlockId,
        switch_targets: &mut SwitchTargetIndex,
    ) {
        let Some(Terminator::Switch { default, cases, .. }) =
            func.blocks[block_id].terminator.as_mut()
        else {
            self.redirect_terminator(func, block_id, old_target, new_target);
            return;
        };

        let targets = switch_targets[block_id].get_or_insert_with(|| {
            let mut targets = FxHashMap::<_, Vec<_>>::default();
            targets.entry(*default).or_default().push(0);
            for (position, &(_, target)) in cases.iter().enumerate() {
                targets.entry(target).or_default().push(position + 1);
            }
            targets
        });
        let Some(positions) = targets.remove(&old_target) else {
            return;
        };
        for &position in &positions {
            if position == 0 {
                *default = new_target;
            } else {
                cases[position - 1].1 = new_target;
            }
        }
        targets.entry(new_target).or_default().extend(positions);
    }

    /// Redirects a terminator from old_target to new_target.
    fn redirect_terminator(
        &self,
        func: &mut Function,
        block_id: BlockId,
        old_target: BlockId,
        new_target: BlockId,
    ) {
        let block = &mut func.blocks[block_id];
        match &mut block.terminator {
            Some(Terminator::Jump(t)) if *t == old_target => {
                *t = new_target;
            }
            Some(Terminator::Branch { then_block, else_block, .. }) => {
                if *then_block == old_target {
                    *then_block = new_target;
                }
                if *else_block == old_target {
                    *else_block = new_target;
                }
            }
            Some(Terminator::Switch { default, cases, .. }) => {
                if *default == old_target {
                    *default = new_target;
                }
                for (_, target) in cases.iter_mut() {
                    if *target == old_target {
                        *target = new_target;
                    }
                }
            }
            _ => {}
        }
    }
}

/// Dead function elimination pass for a module.
#[derive(Debug, Default)]
struct DeadFunctionEliminator {
    /// Statistics from the last run.
    stats: CfgSimplifyStats,
}

impl DeadFunctionEliminator {
    /// Creates a new dead function eliminator.
    #[must_use]
    fn new() -> Self {
        Self::default()
    }

    /// Runs dead function elimination on a module.
    /// Returns the number of functions eliminated.
    fn run(&mut self, module: &mut Module) -> usize {
        self.stats = CfgSimplifyStats::default();

        let call_graph = CallGraphInfo::new(module);
        let reachable = call_graph.reachable_from_entries();
        if reachable.is_empty() {
            return 0;
        }

        self.stats.dead_functions_eliminated = module.functions.len() - reachable.count();
        if self.stats.dead_functions_eliminated == 0 {
            return 0;
        }

        let mut remap = vec![None; module.functions.len()];
        let mut old_functions: Vec<_> =
            std::mem::take(&mut module.functions).into_iter().map(Some).collect();
        let mut functions = IndexVec::with_capacity(reachable.count());
        for old_id in reachable {
            let function =
                old_functions[old_id.index()].take().expect("reachable function must exist");
            let new_id = functions.push(function);
            remap[old_id.index()] = Some(new_id);
        }
        module.functions = functions;

        for func in &mut module.functions {
            func.for_each_instruction_mut(|_, inst| {
                if let InstKind::InternalCall { function, .. } = &mut inst.kind {
                    *function = remap[function.index()]
                        .expect("reachable function cannot call an eliminated function");
                }
            });
            for block in &mut func.blocks {
                if let Some(Terminator::TailCall { function, .. }) = &mut block.terminator {
                    *function = remap[function.index()]
                        .expect("reachable function cannot tail-call an eliminated function");
                }
            }
        }

        self.stats.dead_functions_eliminated
    }
}

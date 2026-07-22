//! Loop canonicalization for MIR.
//!
//! This pass currently implements the first LoopSimplify building block: every
//! natural loop gets a dedicated preheader when its incoming edge does not
//! already come from a non-loop predecessor that jumps directly to the header.
//! Later loop passes can rely on that shape for LICM, storage promotion, and
//! ScalarEvolution-style reasoning.
//!
//! Safety contract:
//! - only split header edges whose terminators explicitly target the header
//! - preserve header phi semantics by moving outside incoming values through the inserted preheader
//! - repair reachability-dependent phis after CFG rewrites

use crate::{
    analysis::LoopAnalyzer,
    mir::{
        BlockId, Function, InstId, InstKind, Instruction, Module, Terminator, Value, ValueId,
        utils::repair_reachability_phis,
    },
    pass::{MirPass, run_function_pass},
};
use solar_data_structures::bit_set::DenseBitSet;

/// Function pass for loop canonicalization.
pub(crate) struct LoopCanonicalizePass;

impl MirPass for LoopCanonicalizePass {
    fn name(&self) -> &'static str {
        "loop-canonicalize"
    }

    fn run_pass(&self, _gcx: solar_sema::Gcx<'_>, module: &mut Module) -> bool {
        run_function_pass(module, |func| LoopCanonicalizer::new().run(func).total() != 0)
    }

    fn is_required(&self) -> bool {
        false
    }
}

/// Statistics from loop canonicalization.
#[derive(Clone, Debug, Default)]
pub(crate) struct LoopCanonicalizeStats {
    /// Number of preheader blocks inserted.
    pub preheaders_inserted: usize,
    /// Number of header phi nodes rewritten to use a preheader incoming value.
    pub header_phis_rewritten: usize,
    /// Number of new preheader phi nodes inserted.
    pub preheader_phis_inserted: usize,
}

impl LoopCanonicalizeStats {
    /// Returns total canonicalization changes performed.
    #[must_use]
    pub(crate) const fn total(&self) -> usize {
        self.preheaders_inserted + self.header_phis_rewritten + self.preheader_phis_inserted
    }
}

/// Canonicalizes natural loops into a form expected by loop optimizers.
#[derive(Debug, Default)]
pub(crate) struct LoopCanonicalizer {
    stats: LoopCanonicalizeStats,
}

impl LoopCanonicalizer {
    /// Creates a new loop canonicalizer.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Runs loop canonicalization to a fixed point.
    pub(crate) fn run(&mut self, func: &mut Function) -> &LoopCanonicalizeStats {
        self.stats = LoopCanonicalizeStats::default();

        loop {
            let mut analyzer = LoopAnalyzer::new();
            let loop_info = analyzer.analyze(func);
            let mut headers: Vec<_> = loop_info.loops.keys().copied().collect();
            headers.sort_by_key(|header| header.index());

            let Some(header) = headers.into_iter().find(|&header| {
                let loop_data = &loop_info.loops[&header];
                let outside_preds = self.outside_predecessors(func, loop_data);
                loop_data.preheader.is_none()
                    && !outside_preds.is_empty()
                    && self.can_split_preheader(func, header, &outside_preds)
            }) else {
                break;
            };

            let loop_data = &loop_info.loops[&header];
            let outside_preds = self.outside_predecessors(func, loop_data);
            self.insert_preheader(func, header, &outside_preds);
        }

        &self.stats
    }

    fn outside_predecessors(
        &self,
        func: &Function,
        loop_data: &crate::analysis::Loop,
    ) -> Vec<BlockId> {
        let mut seen = DenseBitSet::new_empty(func.blocks.len());
        func.blocks[loop_data.header]
            .predecessors
            .iter()
            .copied()
            .filter(|&pred| !loop_data.blocks.contains(pred))
            .filter(|pred| seen.insert(*pred))
            .collect()
    }

    fn can_split_preheader(
        &self,
        func: &Function,
        header: BlockId,
        outside_preds: &[BlockId],
    ) -> bool {
        for &pred in outside_preds {
            if !self.terminator_targets(func, pred, header) {
                return false;
            }
        }

        for &inst_id in &func.blocks[header].instructions {
            let InstKind::Phi(incoming) = &func.instructions[inst_id].kind else {
                continue;
            };
            if outside_preds
                .iter()
                .any(|pred| !incoming.iter().any(|(incoming_pred, _)| incoming_pred == pred))
            {
                return false;
            }
        }

        true
    }

    fn terminator_targets(&self, func: &Function, block: BlockId, target: BlockId) -> bool {
        func.blocks[block]
            .terminator
            .as_ref()
            .is_some_and(|term| term.successors().contains(&target))
    }

    fn insert_preheader(
        &mut self,
        func: &mut Function,
        header: BlockId,
        outside_preds: &[BlockId],
    ) {
        let preheader = func.alloc_block();
        func.blocks[preheader].terminator = Some(Terminator::Jump(header));

        for &pred in outside_preds {
            self.redirect_terminator(func, pred, header, preheader);
        }

        self.rewrite_header_phis(func, header, preheader, outside_preds);
        repair_reachability_phis(func);
        self.stats.preheaders_inserted += 1;
    }

    fn rewrite_header_phis(
        &mut self,
        func: &mut Function,
        header: BlockId,
        preheader: BlockId,
        outside_preds: &[BlockId],
    ) {
        let phi_insts: Vec<_> = func.blocks[header]
            .instructions
            .iter()
            .copied()
            .take_while(|&inst_id| matches!(func.instructions[inst_id].kind, InstKind::Phi(_)))
            .collect();

        let mut preheader_insert_pos = 0;
        for inst_id in phi_insts {
            let external_incoming = self.external_phi_incoming(func, inst_id, outside_preds);
            let preheader_value =
                self.canonical_preheader_value(func, inst_id, external_incoming, preheader);

            let InstKind::Phi(incoming) = &mut func.instructions[inst_id].kind else {
                continue;
            };
            incoming.retain(|(pred, _)| !outside_preds.contains(pred));
            incoming.push((preheader, preheader_value));
            self.stats.header_phis_rewritten += 1;

            if let Value::Inst(phi_inst) = func.values[preheader_value]
                && func.blocks[preheader].instructions.contains(&phi_inst)
                && let Some(pos) = func.blocks[preheader]
                    .instructions
                    .iter()
                    .position(|&existing| existing == phi_inst)
            {
                let phi_inst = func.blocks[preheader].instructions.remove(pos);
                func.blocks[preheader].instructions.insert(preheader_insert_pos, phi_inst);
                preheader_insert_pos += 1;
            }
        }
    }

    fn external_phi_incoming(
        &self,
        func: &Function,
        inst_id: InstId,
        outside_preds: &[BlockId],
    ) -> Vec<(BlockId, ValueId)> {
        let InstKind::Phi(incoming) = &func.instructions[inst_id].kind else {
            return Vec::new();
        };
        outside_preds
            .iter()
            .filter_map(|pred| {
                incoming
                    .iter()
                    .find(|(incoming_pred, _)| incoming_pred == pred)
                    .map(|(_, value)| (*pred, *value))
            })
            .collect()
    }

    fn canonical_preheader_value(
        &mut self,
        func: &mut Function,
        header_phi: InstId,
        incoming: Vec<(BlockId, ValueId)>,
        preheader: BlockId,
    ) -> ValueId {
        debug_assert!(!incoming.is_empty());
        let first_value = incoming[0].1;
        if incoming.iter().all(|(_, value)| *value == first_value) {
            return first_value;
        }

        let result_ty =
            func.instructions[header_phi].result_ty.expect("header phi should have result type");
        let preheader_phi =
            func.alloc_inst(Instruction::new(InstKind::Phi(incoming), Some(result_ty)));
        func.blocks[preheader].instructions.push(preheader_phi);
        self.stats.preheader_phis_inserted += 1;
        func.alloc_value(Value::Inst(preheader_phi))
    }

    fn redirect_terminator(
        &self,
        func: &mut Function,
        block_id: BlockId,
        old_target: BlockId,
        new_target: BlockId,
    ) {
        let Some(term) = &mut func.blocks[block_id].terminator else {
            return;
        };
        match term {
            Terminator::Jump(target) => {
                if *target == old_target {
                    *target = new_target;
                }
            }
            Terminator::Branch { then_block, else_block, .. } => {
                if *then_block == old_target {
                    *then_block = new_target;
                }
                if *else_block == old_target {
                    *else_block = new_target;
                }
            }
            Terminator::Switch { default, cases, .. } => {
                if *default == old_target {
                    *default = new_target;
                }
                for (_, target) in cases {
                    if *target == old_target {
                        *target = new_target;
                    }
                }
            }
            Terminator::TailCall { .. }
            | Terminator::Return { .. }
            | Terminator::Revert { .. }
            | Terminator::ReturnData { .. }
            | Terminator::Stop
            | Terminator::SelfDestruct { .. }
            | Terminator::Invalid => {}
        }
    }
}

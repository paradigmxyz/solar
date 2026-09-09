//! Bounded owner emission with exact rollback and call-protection cost selection.
//!
//! Ordinary emission runs first with unchanged layout, homes, alias facts and call ABI. Only an
//! owner with an actual backup around a known memory-clean ICall may emit one alternate body;
//! multi-result publication and nested Phi trials retain the ordinary path. Complete physical
//! blocks, including pre-call instructions and continuations, must keep their topology and
//! improve the existing bounded-literal cost query without increasing entry-adjusted stack peak.
//! The query remains conservative and does not prove final layout or literal reuse costs.
//!
//! Snapshots retain owner roots, appended continuation/edge bodies, metadata and shared switch
//! budgets. Rejection or alternate failure restores the already successful ordinary emission.
//! Tail-summary queries are disabled by the explicit tail-entry-scope gate, so their stable
//! MIR/layout cache cannot change during this trial. No whole-module clone or layout trial is
//! introduced. Metadata equality never controls selection; the selected emission keeps its
//! own metadata. Private root/continuation heights exclude the same opaque caller prefix and
//! account for removed saved words. Changed blocks without both heights decline; known blocks
//! must not read that opaque prefix. Call boundaries can lose saved words, so net effects may
//! differ.

use super::{Context, FunctionLayout, ir, lower_function};
use crate::backend::evm::switches::Planner;
use solar_config::OptimizationMode;
use solar_data_structures::map::FxHashMap;
use std::cell::Cell;

/// Owner-local physical state, shared by Phi fallback and the bounded call trial.
pub(super) struct Snapshot {
    blocks: Vec<(ir::BlockId, ir::Block)>,
    block_count: usize,
    switches: Planner,
    tail_entry_scope: Option<bool>,
}

impl Snapshot {
    pub(super) fn new(context: &Context<'_>, output: &ir::Module, switches: &Planner) -> Self {
        let blocks = context
            .layout
            .blocks
            .iter_enumerated()
            .filter(|&(id, _)| context.layout.cfg.is_reachable(id))
            .map(|(_, &id)| (id, output.blocks[id].clone()))
            .collect();
        Self {
            blocks,
            block_count: output.blocks.len(),
            switches: switches.clone(),
            tail_entry_scope: context.tail_entry_scope.get(),
        }
    }

    fn capture_after(
        &self,
        context: &Context<'_>,
        output: &ir::Module,
        switches: &Planner,
    ) -> Self {
        let mut captured = Self::new(context, output, switches);
        captured.blocks.extend(
            output
                .blocks
                .iter_enumerated()
                .skip(self.block_count)
                .map(|(id, block)| (id, block.clone())),
        );
        captured
    }

    pub(super) fn restore(
        self,
        output: &mut ir::Module,
        switches: &mut Planner,
        scope: &Cell<Option<bool>>,
    ) {
        // <original owner roots>; <original appended continuations and edges>
        // Discard speculative blocks or recreate a previously captured suffix.
        output.blocks.resize_with(self.block_count, ir::Block::default);
        for (id, block) in self.blocks {
            output.blocks[id] = block;
        }
        *switches = self.switches;
        scope.set(self.tail_entry_scope);
    }

    fn improves(
        &self,
        context: &Context<'_>,
        output: &ir::Module,
        old_heights: &FxHashMap<ir::BlockId, usize>,
        new_heights: &FxHashMap<ir::BlockId, usize>,
    ) -> bool {
        if output.blocks.len() != self.block_count {
            return false;
        }
        let mut changed = false;
        for (id, old) in &self.blocks {
            let new = &output.blocks[*id];
            if old.terminator != new.terminator
                || old.cold != new.cold
                || old.loop_header != new.loop_header
            {
                return false;
            }
            if old.insts == new.insts {
                continue;
            }
            let (Some(&old_entry), Some(&new_entry)) = (old_heights.get(id), new_heights.get(id))
            else {
                return false;
            };
            let (Some((old_need, _, old_peak)), Some((new_need, _, new_peak))) =
                (ir::scheduling_usage(&old.insts), ir::scheduling_usage(&new.insts))
            else {
                return false;
            };
            if old_need > old_entry as i64
                || new_need > new_entry as i64
                || new_entry as i64 + new_peak > old_entry as i64 + old_peak
                || !ir::scheduling_literal_costs_fit(context.version, &old.insts, &new.insts)
            {
                return false;
            }
            changed = true;
        }
        changed
    }
}

pub(super) fn lower(
    context: &Context<'_>,
    layouts: &FxHashMap<crate::mir::FunctionId, FunctionLayout>,
    output: &mut ir::Module,
    switches: &mut Planner,
) -> Result<(), String> {
    let trial = context.original.is_none()
        && (!context.layout.spills.homes.is_empty() || context.plan.max_dynamic_frame_size != 0)
        && matches!(context.optimization, OptimizationMode::Gas | OptimizationMode::Size)
        && context.tail_entry_scope.get() == Some(false)
        && context.function.instructions().any(|id| {
            matches!(context.function.inst(id).kind,
                crate::mir::InstKind::ICall { function, returns: 0..=1, .. }
                if context.plan.memory_clean_calls.contains(function))
        });
    let start = trial.then(|| Snapshot::new(context, output, switches));
    let mut old_heights = trial.then(FxHashMap::default);
    let eligible = lower_function(context, layouts, output, switches, false, old_heights.as_mut())?;
    if let (Some(start), Some(old_heights)) = (start, old_heights)
        && eligible
    {
        let original = start.capture_after(context, output, switches);
        // <same owner with certified clean-call backups omitted>
        start.restore(output, switches, context.tail_entry_scope);
        let mut new_heights = FxHashMap::default();
        if lower_function(context, layouts, output, switches, true, Some(&mut new_heights)).is_err()
            || !original.improves(context, output, &old_heights, &new_heights)
        {
            // <already successful ordinary owner, including its shared emission state>
            original.restore(output, switches, context.tail_entry_scope);
        }
    }
    Ok(())
}

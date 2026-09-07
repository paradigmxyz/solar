//! Selective Phi residence and simultaneous mixed stack/home transfers.
//!
//! The existing interval-ranked Phi proposal keeps its ordinary resident budget. If absent,
//! one bounded failure-directed no-Phi proposal may retire other optional homes instead.
//! Only that proposal permits existing last-use operand preparation to reorder retained values;
//! actual stack identities and canonical outgoing edges still account for every permutation.
//! The caller excludes construction, returning or hidden-prefix owners, dynamic frames and
//! recipes. Entry materialization, reserved homes and the spill protocol remain unchanged.
//! Actual function lowering checks the proposal; a checkpoint restores the complete original
//! output and shared switch budgets on failure. An actual MSTORE also rejects a proposal when
//! retiring live homes makes its complete protection cost worse in bytes or static gas. This
//! local guard includes unselected backups but excludes surrounding scheduling and later
//! outlining. This is a bounded trial, not a search for an optimal stack allocation.
//!
//! Mixed edges first capture old inputs for resident successors, then execute simultaneous
//! memory copies while retaining their stack sources, and finally reconcile successor values.
//! A captured input may still have a home, so edge-local snapshots bypass ordinary residency
//! liveness filtering. Memory cycles use the existing reserved scratch word. Explicit temporary
//! checks cover raw loads/stores; the physical scheduler checks permutations and duplication.
//! Unchanged all-homed edges retain their existing emission and copy-cost decisions.

use super::{
    Context, FunctionLayout, Slot, calls, edge_values, entry_order::OperandOrder, home_available,
    ir, load_value, op, parallel_copy, prefix, resident, schedule_error, store_spill, stored,
    writer,
};
use crate::{
    analysis::ModRef,
    backend::evm::{scheduler::Stack, spills::may_overlap, storage::FrameAddress},
    mir,
};
use solar_config::{EvmVersion, OptimizationMode};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
use std::cell::Cell;

/// The ordinary allocation bindings retained until the real emission trial succeeds.
pub(super) struct Original {
    pub(super) operand_order: OperandOrder,
    promoted: DenseBitSet<mir::ValueId>,
    homes: FxHashMap<mir::ValueId, usize>,
    entries: IndexVec<mir::BlockId, Vec<Slot>>,
    home_definitions: FxHashMap<mir::ValueId, (mir::BlockId, usize)>,
}

impl Original {
    /// Rejects a more expensive actual writer bank before candidate backups are emitted.
    pub(super) fn check_writer(
        &self,
        context: &Context<'_>,
        location: (mir::BlockId, usize),
        effects: &ModRef,
        addresses: &[FrameAddress],
        protection: Option<&writer::Protection>,
        live: impl Fn(mir::ValueId) -> bool,
    ) -> Result<(), String> {
        let mut original = None;
        for value in self.promoted.iter() {
            if home_available(context, &self.home_definitions, value, location) && live(value) {
                let home = *self.homes.get(&value).ok_or("promoted value has no original home")?;
                let address = context.storage.spill_address(home).map_err(str::to_owned)?;
                if !addresses.contains(&address)
                    && may_overlap(context.storage, context.plan.fixed_memory_end, effects, address)
                {
                    original.get_or_insert_with(|| addresses.to_vec()).push(address);
                }
            }
        }
        let Some(mut original) = original else { return Ok(()) };
        original.sort_unstable_by_key(|address| match address {
            FrameAddress::Absolute(address) => (0, *address),
            FrameAddress::Relative(address) => (1, *address),
        });
        original.dedup();
        let previous = writer::choose(&original, original.len(), context.version);
        let before = writer::protection_cost(context.version, &original, previous.as_ref())
            .ok_or("original Phi writer bank has unsupported addresses")?;
        let after = writer::protection_cost(context.version, addresses, protection)
            .ok_or("selected Phi writer bank has unsupported addresses")?;
        if after.0 > before.0 || after.1 > before.1 {
            return Err("selected Phi homes increase memory-writer protection cost".into());
        }
        Ok(())
    }

    pub(super) fn restore(self, layout: &mut FunctionLayout) {
        // <ordinary home bindings>; <ordinary canonical block entries>
        layout.spills.homes = self.homes;
        layout.entries = self.entries;
        layout.home_definitions = self.home_definitions;
    }
}

/// Selects after ordinary planning and recipes, without changing the entry preamble.
pub(super) fn select(
    function: &mir::Function,
    layout: &mut FunctionLayout,
    version: EvmVersion,
    optimization: OptimizationMode,
) -> Option<Original> {
    let (promoted, operand_order) = layout
        .spills
        .phi_residents(function, &layout.live, &layout.cfg, &layout.alias, version, |value| {
            stored(function, value, version, optimization)
        })
        .map(|promoted| (promoted, OperandOrder::Canonical))
        .or_else(|| {
            layout
                .spills
                .failure_residents(
                    function,
                    &layout.live,
                    &layout.cfg,
                    &layout.alias,
                    version,
                    |value| stored(function, value, version, optimization),
                )
                .map(|promoted| (promoted, OperandOrder::DeadOperands))
        })?;
    let mut entries = layout.entries.clone();
    // <ordinary residents>; <retired live-in and Phi identities in canonical order>
    for (block_id, block) in function.blocks.iter_enumerated() {
        let entry = &mut entries[block_id];
        entry.extend(
            layout
                .live
                .live_in(block_id)
                .iter()
                .filter(|&value| promoted.contains(value))
                .map(Slot::Value),
        );
        for &inst in &block.instructions {
            if matches!(function.inst(inst).kind, mir::InstKind::Phi(_))
                && let Some(value) = function.inst_result_value(inst)
                && promoted.contains(value)
            {
                entry.push(Slot::Value(value));
            }
        }
        entry.sort_unstable();
        entry.dedup();
    }
    if entries[mir::BlockId::ENTRY] != layout.entries[mir::BlockId::ENTRY] {
        return None;
    }
    let mut homes = layout.spills.homes.clone();
    homes.retain(|value, _| !promoted.contains(*value));
    let mut home_definitions = layout.home_definitions.clone();
    home_definitions.retain(|value, _| !promoted.contains(*value));
    Some(Original {
        operand_order,
        promoted,
        homes: std::mem::replace(&mut layout.spills.homes, homes),
        entries: std::mem::replace(&mut layout.entries, entries),
        home_definitions: std::mem::replace(&mut layout.home_definitions, home_definitions),
    })
}

/// Function-local emission state, including metadata and shared switch growth budgets.
pub(super) struct Checkpoint {
    blocks: Vec<(ir::BlockId, ir::Block)>,
    block_count: usize,
    switches: crate::backend::evm::switches::Planner,
    tail_entry_scope: Option<bool>,
}

impl Checkpoint {
    pub(super) fn new(
        context: &Context<'_>,
        output: &ir::Module,
        switches: &crate::backend::evm::switches::Planner,
    ) -> Self {
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

    pub(super) fn restore(
        self,
        output: &mut ir::Module,
        switches: &mut crate::backend::evm::switches::Planner,
        tail_entry_scope: &Cell<Option<bool>>,
    ) {
        // <original owner blocks>; discard speculative edge, call and switch blocks
        output.blocks.truncate(self.block_count);
        for (id, block) in self.blocks {
            output.blocks[id] = block;
        }
        *switches = self.switches;
        tail_entry_scope.set(self.tail_entry_scope);
    }
}

/// Reports edges which cannot use the original memory-only copy emitter.
pub(super) fn mixed(context: &Context<'_>, from: mir::BlockId, to: mir::BlockId) -> bool {
    context.function.blocks[to].instructions.iter().any(|&inst| {
        if let mir::InstKind::Phi(incoming) = &context.function.inst(inst).kind
            && let Some(result) = context.function.inst_result_value(inst)
        {
            !context.layout.spills.homes.contains_key(&result)
                || incoming.iter().any(|&(pred, source)| pred == from && resident(context, source))
        } else {
            false
        }
    })
}

/// Reads all old resident inputs before publishing any overlapping memory destinations.
pub(super) fn edge(
    context: &Context<'_>,
    from: mir::BlockId,
    to: mir::BlockId,
    incoming: &Stack<Slot>,
    output: &mut ir::Module,
) -> Result<ir::BlockId, String> {
    if prefix(context) != 0
        || !matches!(context.storage.base, super::FrameBase::Static(_))
        || incoming.values().iter().any(|slot| !matches!(slot, Slot::Value(_)))
    {
        return Err("mixed Phi edge has an unsupported activation prefix".into());
    }
    let desired = edge_values(context, from, to)?;
    let mut stack = incoming.clone();
    let mut insts = Vec::new();
    // <incoming residents>; <old home/literal inputs captured for resident Phis>
    for &slot in desired.iter().rev() {
        if !stack.values().contains(&slot) {
            let Slot::Value(value) = slot else {
                return Err("mixed Phi successor has an unsupported activation prefix".into());
            };
            capacity(&stack, 2)?;
            load_value(context, value, &mut insts)?;
            stack.push(slot);
        }
    }
    let mut transfers = Vec::new();
    for &inst in &context.function.blocks[to].instructions {
        if let mir::InstKind::Phi(incoming) = &context.function.inst(inst).kind
            && let Some(result) = context.function.inst_result_value(inst)
            && let Some(&home) = context.layout.spills.homes.get(&result)
        {
            let source = incoming
                .iter()
                .find(|(pred, _)| *pred == from)
                .ok_or("missing MIR phi predecessor")?
                .1;
            let source = context
                .layout
                .spills
                .homes
                .get(&source)
                .copied()
                .map_or(parallel_copy::Source::Value(source), parallel_copy::Source::Home);
            transfers.push((source, home));
        }
    }
    let ordered = parallel_copy::schedule(transfers, context.layout.spills.phi_scratch);
    for (position, &(source, home)) in ordered.iter().enumerate() {
        // <captured successor inputs>; <pending resident sources>; <one copy value>
        match source {
            // push <source home>; mload
            parallel_copy::Source::Home(home) => {
                capacity(&stack, 2)?;
                insts.extend(calls::address(
                    context.storage.spill_address(home).map_err(str::to_owned)?,
                ));
                insts.push(ir::InstKind::Op(op::MLOAD).into());
            }
            // <retained snapshots and pending sources>; <one duplicate or loaded value>
            parallel_copy::Source::Value(value) => {
                let slot = Slot::Value(value);
                if !stack.values().contains(&slot) {
                    capacity(&stack, 2)?;
                    load_value(context, value, &mut insts)?;
                    stack.push(slot);
                }
                // Keep homed snapshots too: their old contents may already be overwritten.
                insts.extend(stack.prepare(&[slot], 0, context.version, |slot| {
                    desired.contains(&slot)
                        || ordered[position + 1..].iter().any(|&(source, _)| {
                            matches!((source, slot), (parallel_copy::Source::Value(value), Slot::Value(held)) if value == held)
                        })
                }).map_err(schedule_error)?);
                capacity(&stack, 1)?;
            }
        }
        // <copy value>; push <destination home>; mstore
        store_spill(context, home, &mut insts)?;
        if matches!(source, parallel_copy::Source::Value(_)) {
            stack.truncate(stack.values().len() - 1);
        }
    }
    // <simultaneously selected resident inputs in canonical successor order>
    insts.extend(stack.reconcile(&desired, 0, context.version).map_err(schedule_error)?);
    // The explicit jump still pushes its destination above the final resident layout.
    capacity(&stack, 1)?;
    if insts.is_empty() {
        return Ok(context.layout.blocks[to]);
    }
    Ok(output.blocks.push(ir::Block {
        insts,
        terminator: ir::TerminatorKind::Jump(context.layout.blocks[to]).into(),
        ..Default::default()
    }))
}

fn capacity(stack: &Stack<Slot>, extra: usize) -> Result<(), String> {
    if stack.values().len() > 1024 - extra {
        Err("mixed Phi temporaries exceed the EVM stack limit".into())
    } else {
        Ok(())
    }
}

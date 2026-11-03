//! Gas observations that constrain elimination of dynamically priced accesses.
//!
//! A source `gas` read or an internal call whose summary may observe gas and then return
//! starts an observable interval. Forward propagation marks every block entry reachable
//! after such an instruction, using an OR join until a fixed point is reached.
//! This includes diamond arms and loop backedges that dominance alone misses.
//! Each marked block is visited once. Values used only to forward gas to legacy
//! external calls do not start an interval; all other uses remain conservative.
//!
//! A callee whose every gas read lies in a block that cannot reach a non-reverting exit
//! never hands control back after the read, so nothing the caller runs after the call
//! follows a read on any path, and the call starts no interval. Uncertainty stays
//! conservative: a return, a stop, a tail call, and an unknown callee all count as
//! returning. Such a call still observes gas for every consumer that keeps accesses
//! before it, since the read follows them on the reverting path and its revert data may
//! carry the gas, and [`GasObservations::any`] counts it. Only the consumers that remove
//! or forward accesses after the call use the intervals.
//!
//! These facts do not attempt to preserve exact gas for arbitrary pure rewrites.
//! They protect state accesses whose removal changes dynamic charges or warmness.

use super::{AliasAnalysis, CfgInfo};
use crate::mir::{BlockId, Function, InstId, InstKind};
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    map::FxHashMap,
};

#[derive(Debug)]
pub(crate) struct GasObservations {
    instructions: GrowableBitSet<InstId>,
    entries: DenseBitSet<BlockId>,
}

impl GasObservations {
    pub(crate) fn new(func: &Function, cfg: &CfgInfo, alias: &AliasAnalysis) -> Self {
        let forwarded = Self::classify_forwarded_call_gas(func);
        let mut instructions = GrowableBitSet::with_capacity(func.num_insts());
        let mut entries = DenseBitSet::new_empty(func.blocks.len());
        let mut worklist = Vec::new();
        for block in cfg.reachable().iter() {
            let mut observes = false;
            for &inst in &func.blocks[block].instructions {
                if Self::instruction_starts_interval(func, alias, &forwarded, inst) {
                    instructions.insert(inst);
                    observes = true;
                }
            }
            if observes {
                for &successor in cfg.successors(block) {
                    if entries.insert(successor) {
                        worklist.push(successor);
                    }
                }
            }
        }
        while let Some(block) = worklist.pop() {
            for &successor in cfg.successors(block) {
                if entries.insert(successor) {
                    worklist.push(successor);
                }
            }
        }
        Self { instructions, entries }
    }

    /// Whether this instruction may observe gas and continue after it. A snapshot remains
    /// conservative when a transform adds only pure instructions, which are absent from this set.
    pub(crate) fn observes(&self, inst: InstId) -> bool {
        self.instructions.contains(inst)
    }

    /// Whether an earlier observation can reach this block's entry.
    pub(crate) fn at_entry(&self, block: BlockId) -> bool {
        self.entries.contains(block)
    }

    /// Finds `gas` values used exclusively as the gas operand of legacy calls.
    pub(crate) fn classify_forwarded_call_gas(func: &Function) -> DenseBitSet<InstId> {
        let mut gas_values = FxHashMap::default();
        for inst_id in func.instructions() {
            if matches!(func.inst(inst_id).kind, InstKind::Gas)
                && let Some(value) = func.inst_result_value(inst_id)
            {
                gas_values.insert(value, inst_id);
            }
        }

        let mut forwarded = DenseBitSet::new_empty(func.num_insts());
        if gas_values.is_empty() {
            return forwarded;
        }
        let mut observed = DenseBitSet::new_empty(func.num_insts());
        for inst_id in func.instructions() {
            let kind = &func.inst(inst_id).kind;
            let call_gas = match kind {
                InstKind::Call { gas, .. }
                | InstKind::CallCode { gas, .. }
                | InstKind::StaticCall { gas, .. }
                | InstKind::DelegateCall { gas, .. } => Some(*gas),
                _ => None,
            };
            let mut accepted_call_gas = false;
            kind.visit_operands(|operand| {
                let Some(&gas_inst) = gas_values.get(&operand) else { return };
                if call_gas == Some(operand) && !accepted_call_gas {
                    forwarded.insert(gas_inst);
                    accepted_call_gas = true;
                } else {
                    observed.insert(gas_inst);
                }
            });
        }
        for block in func.blocks.iter() {
            if let Some(terminator) = &block.terminator {
                terminator.visit_operands(|operand| {
                    if let Some(&gas_inst) = gas_values.get(&operand) {
                        observed.insert(gas_inst);
                    }
                });
            }
        }
        for gas_inst in observed.iter() {
            forwarded.remove(gas_inst);
        }
        forwarded
    }

    /// Whether any reachable instruction observes gas, without constructing the propagation
    /// needed to answer block-entry queries.
    pub(crate) fn any(func: &Function, cfg: &CfgInfo, alias: &AliasAnalysis) -> bool {
        let forwarded = Self::classify_forwarded_call_gas(func);
        cfg.reachable().iter().any(|block| {
            func.blocks[block]
                .instructions
                .iter()
                .any(|&inst| Self::instruction_observes(func, alias, &forwarded, inst))
        })
    }

    fn instruction_observes(
        func: &Function,
        alias: &AliasAnalysis,
        forwarded: &DenseBitSet<InstId>,
        inst: InstId,
    ) -> bool {
        match func.inst(inst).kind {
            InstKind::Gas => !forwarded.contains(inst),
            InstKind::ICall { .. } => alias.instruction_mod_ref(func, inst).observes_gas(),
            _ => func.inst(inst).kind.observes_gas(),
        }
    }

    /// Whether the instruction may observe gas on a path that continues after it.
    fn instruction_starts_interval(
        func: &Function,
        alias: &AliasAnalysis,
        forwarded: &DenseBitSet<InstId>,
        inst: InstId,
    ) -> bool {
        match func.inst(inst).kind {
            InstKind::ICall { .. } => alias.instruction_mod_ref(func, inst).continues_after_gas(),
            _ => Self::instruction_observes(func, alias, forwarded, inst),
        }
    }
}

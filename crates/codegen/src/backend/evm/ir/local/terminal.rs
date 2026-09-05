//! Removal of unused incoming stack prefixes from acyclic terminal regions.
//!
//! A leading POP can be omitted when the entire remaining region executes using
//! only words produced within that region. Abstract stack-height execution proves
//! this over explicit edges, rejects cycles and dynamic control, and includes the
//! temporary label pushes required by assembly. The original maximum entry height
//! plus the region peak must fit the physical stack. Code-relative observations,
//! GAS, opaque effects and shifts requiring legacy legalization stop the proof. Analysis is capped
//! at 64 block/height states per candidate and runs after all local/structural transforms and
//! before final block placement. This preserves patterns used by earlier packing and sharing. At
//! most one region changes per invocation, so later candidates cannot use stale entry bounds.

use super::{
    super::{BlockId, EvmPass, InstKind, Module, TerminatorKind, verify},
    canonical, stack_usage,
};
use crate::backend::evm::op;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use solar_sema::Gcx;

pub(crate) struct TerminalPrefixes;

impl EvmPass for TerminalPrefixes {
    fn name(&self) -> &'static str {
        "terminal-prefixes"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let Ok(heights) = verify::stack_heights(module) else { return false };
        eliminate(module, &heights, gcx.sess.opts.evm_version.has_bitwise_shifting())
    }
}

fn eliminate(module: &mut Module, heights: &verify::StackHeights, native_shifts: bool) -> bool {
    for id in module.block_ids().collect::<Vec<_>>() {
        let count = module.blocks[id]
            .insts
            .iter()
            .take_while(|inst| canonical(inst) && inst.kind == InstKind::Op(op::POP))
            .count();
        if count == 0 {
            continue;
        }
        let Some((minimum, maximum)) = heights[id] else { continue };
        if minimum < count {
            continue;
        }
        let mut proof = Proof {
            module,
            native_shifts,
            active: DenseBitSet::new_empty(module.blocks.len()),
            peaks: FxHashMap::default(),
        };
        if let Some(peak) = proof.region(id, count, 0)
            && maximum.checked_add(peak).is_some_and(|peak| peak <= 1024)
        {
            // pop unused_prefix...; <region consuming only its own words>; terminate
            // -> <same region above the retained unused prefix>; terminate
            module.blocks[id].insts.drain(..count);
            return true;
        }
    }
    false
}

struct Proof<'a> {
    module: &'a Module,
    native_shifts: bool,
    active: DenseBitSet<BlockId>,
    peaks: FxHashMap<(BlockId, usize), Option<usize>>,
}

impl Proof<'_> {
    fn region(&mut self, id: BlockId, skip: usize, entry: usize) -> Option<usize> {
        if self.active.contains(id) || self.peaks.len() >= 64 {
            return None;
        }
        if skip == 0
            && let Some(&peak) = self.peaks.get(&(id, entry))
        {
            return peak;
        }
        self.peaks.insert((id, entry), None);
        self.active.insert(id);
        let result = self.block(id, skip, entry);
        self.active.remove(id);
        self.peaks.insert((id, entry), result);
        result
    }

    fn block(&mut self, id: BlockId, skip: usize, mut height: usize) -> Option<usize> {
        let block = &self.module.blocks[id];
        let insts = &block.insts[skip..];
        if insts.iter().any(|inst| {
            !canonical(inst)
                || (!self.native_shifts
                    && matches!(inst.kind, InstKind::Op(op::SHL | op::SHR | op::SAR)))
                || matches!(
                    inst.kind,
                    InstKind::Op(
                        op::PC
                            | op::CODESIZE
                            | op::CODECOPY
                            | op::GAS
                            | op::JUMP
                            | op::JUMPI
                            | op::JUMPDEST
                    )
                )
        }) {
            return None;
        }
        let (required, delta, extra_peak) = stack_usage(insts)?;
        if height < required as usize {
            return None;
        }
        let mut peak = height.checked_add(extra_peak as usize)?;
        height = height.checked_add_signed(delta as isize)?;
        let tail_peak = match block.terminator.kind {
            TerminatorKind::Jump(target) => {
                // push target; jump
                peak = peak.max(height + 1);
                self.region(target, 0, height)?
            }
            TerminatorKind::JumpI(yes, no) => {
                // push yes; jumpi; push no; jump
                peak = peak.max(height + 1);
                height = height.checked_sub(1)?;
                self.region(yes, 0, height)?.max(self.region(no, 0, height)?)
            }
            TerminatorKind::Stop | TerminatorKind::Invalid | TerminatorKind::Unreachable => 0,
            TerminatorKind::Return | TerminatorKind::Revert if height >= 2 => 0,
            TerminatorKind::SelfDestruct if height >= 1 => 0,
            _ => return None,
        };
        Some(peak.max(tail_peak))
    }
}

//! Unused stack-prefix removal and bounded full-word return compaction.
//!
//! A leading POP can be omitted when the entire remaining region executes using
//! only words produced within that region. Abstract stack-height execution proves
//! this over explicit edges, rejects cycles and dynamic control, and includes the
//! temporary label pushes required by assembly. The original maximum entry height
//! plus the region peak must fit the physical stack. Code-relative observations,
//! GAS, opaque effects and shifts requiring legacy legalization stop the proof. Analysis is capped
//! at 64 block/height states per candidate and runs after all local/structural transforms and
//! before final block placement. This preserves patterns used by earlier packing and sharing. Each
//! accepted region removes at least one POP. Recompute physical entry bounds before trying
//! another region, so cumulative retained words cannot invalidate the stack-capacity proof.
//!
//! An exact final `push A; mstore; push 32; push A; return` can use offset zero
//! instead: the return reads the entire word just written, with no intervening
//! observation. This preserves the stack and reduces or preserves memory expansion.
//! The initial scope is the compiler's low return/scratch range, 1 through 128;
//! larger or wrapping addresses retain their original expansion behavior. Canonical
//! effects and glue boundaries are required, and module-wide code observations or
//! unknown computed transfers prevent the rewrite. Earlier GAS and memory reads
//! remain unchanged because the rewritten tail exits immediately. Literal metadata
//! is retained. This runs before placement and can expose identical terminal tails;
//! downstream sharing and layout profitability still need whole-pipeline checks.

use super::{
    super::{BlockId, EvmPass, InstKind, Module, TerminatorKind, cfg, split_allowed, verify},
    canonical, stack_usage,
};
use crate::backend::evm::op;
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use solar_sema::Gcx;

pub(crate) struct TerminalPrefixes;

impl EvmPass for TerminalPrefixes {
    fn name(&self) -> &'static str {
        "terminal-prefixes"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let mut changed = false;
        while let Ok(heights) = verify::stack_heights(module) {
            if !eliminate(module, &heights, gcx.sess.opts.evm_version.has_bitwise_shifting()) {
                break;
            }
            changed = true;
        }
        changed |= compact_return_words(module);
        changed
    }
}

fn compact_return_words(module: &mut Module) -> bool {
    let candidates = module
        .block_ids()
        .filter(|&id| {
            let block = &module.blocks[id];
            if let [.., address, store, size, offset] = block.insts.as_slice()
                && block.terminator.kind == TerminatorKind::Return
                && !block.terminator.keep_with_next
                && block.terminator.stack_effect.is_none_or(|effect| effect == (2, 0))
                && let InstKind::Push(value) = address.kind
                && value > U256::ZERO
                && value <= U256::from(128)
                && store.kind == InstKind::Op(op::MSTORE)
                && size.kind == InstKind::Push(U256::from(32))
                && offset.kind == address.kind
                && [address, store, size, offset].into_iter().all(canonical)
                && split_allowed(&block.insts, block.insts.len() - 4)
            {
                true
            } else {
                false
            }
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() || cfg::sharing_observes_code(module) {
        return false;
    }
    for id in candidates {
        let insts = &mut module.blocks[id].insts;
        let len = insts.len();
        // push A; mstore; push 32; push A; return
        // -> push 0; mstore; push 32; push 0; return
        insts[len - 4].kind = InstKind::Push(U256::ZERO);
        insts[len - 1].kind = InstKind::Push(U256::ZERO);
    }
    true
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

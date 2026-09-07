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
//!
//! Two-word returns additionally scan backward within the same block until both
//! distinct final word stores at A and A+32 are found. Only canonical stack
//! operations, pure arithmetic and calldata reads may occur between those stores
//! and RETURN. Any other write, memory/storage read, gas observation or call stops
//! the proof. The stores can occur in either order; their literal addresses become
//! zero and 32, preserving stored values, stack heights and metadata.
//! Earlier effects are not crossed after full coverage is found. One-word returns
//! retain the original exact-adjacency rule. The scan is linear, uses two optional
//! instruction positions, and adds no range, CFG or stack-height analysis.

use super::{
    super::{
        Block, BlockId, EvmPass, InstKind, Module, TerminatorKind, cfg, split_allowed, verify,
    },
    canonical, pure, stack_usage,
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
        .filter_map(|id| return_word_addresses(&module.blocks[id]).map(|addresses| (id, addresses)))
        .collect::<Vec<_>>();
    if candidates.is_empty() || cfg::sharing_observes_code(module) {
        return false;
    }
    for (id, addresses) in candidates {
        let insts = &mut module.blocks[id].insts;
        // push A; mstore; <pure stack operations>; push A+32; mstore
        // push 64; push A; return
        // -> same values and order, storing/returning at offsets 0 and 32
        // The one-word case retains its adjacent store/return sequence.
        for (word, address) in addresses.into_iter().enumerate() {
            if let Some(index) = address {
                insts[index].kind = InstKind::Push(U256::from(word * 32));
            }
        }
        insts.last_mut().unwrap().kind = InstKind::Push(U256::ZERO);
    }
    true
}

/// Finds the final covering stores without crossing an observable memory effect.
fn return_word_addresses(block: &Block) -> Option<[Option<usize>; 2]> {
    let insts = &block.insts;
    if let [.., size, offset] = insts.as_slice()
        && block.terminator.kind == TerminatorKind::Return
        && !block.terminator.keep_with_next
        && block.terminator.stack_effect.is_none_or(|effect| effect == (2, 0))
        && let InstKind::Push(base) = offset.kind
        && base > U256::ZERO
        && base <= U256::from(128)
        && let InstKind::Push(bytes) = size.kind
        && (bytes == U256::from(32) || bytes == U256::from(64))
        && canonical(size)
        && canonical(offset)
    {
        let words = if bytes == U256::from(32) { 1 } else { 2 };
        let mut addresses = [None; 2];
        let mut cursor = insts.len() - 2;
        while cursor > 0 {
            let index = cursor - 1;
            let inst = &insts[index];
            if !canonical(inst) {
                return None;
            }
            if inst.kind == InstKind::Op(op::MSTORE) {
                if let Some(address) = insts[..index].last()
                    && canonical(address)
                    && split_allowed(insts, index - 1)
                    && let InstKind::Push(value) = address.kind
                {
                    let word = if value == base {
                        0
                    } else if words == 2 && value == base + U256::from(32) {
                        1
                    } else {
                        return None;
                    };
                    if addresses[word].replace(index - 1).is_some() {
                        return None;
                    }
                    if addresses[..words].iter().all(Option::is_some) {
                        return Some(addresses);
                    }
                    cursor = index - 1;
                    continue;
                }
                return None;
            }
            if words == 1
                || !match inst.kind {
                    InstKind::Push(_)
                    | InstKind::PushImmutable { .. }
                    | InstKind::Dup(_)
                    | InstKind::Swap(_)
                    | InstKind::Exchange(_, _) => true,
                    InstKind::Op(code) => {
                        pure(code)
                            || matches!(
                                code,
                                op::POP | op::PUSH0 | op::CALLDATALOAD | op::CALLDATASIZE
                            )
                    }
                    _ => false,
                }
            {
                return None;
            }
            cursor = index;
        }
    }
    None
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

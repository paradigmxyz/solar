//! Choose loop fallthroughs after structural sharing and stack cleanup.
//!
//! Ordinary block layout establishes traces, then gas mode moves a latch before its header when
//! doing so transfers a jump from an acyclic preheader into a fallthrough on every iteration.
//! Zero iterations can pay one extra jump, one breaks even, and further iterations save one.
//!
//! When the current fallthrough also comes from the loop, a bounded flow estimate distinguishes
//! the common iteration from rarer paths. Branches split integer probability mass equally;
//! returns to the header stop propagation and inner loops are limited to 32 rounds and 256
//! visited blocks. The replacement edge must receive more than twice the old edge's estimated
//! mass. This is a static frequency heuristic, not a guarantee for every input. Other existing
//! fallthroughs remain intact, and no instruction or condition is duplicated.
//!
//! All rewrites preserve explicit block identities, instructions, and stack effects. Changing
//! physical order never splits a block or alters its debug metadata. This pass runs last so
//! subsequent trace construction cannot undo the chosen loop fallthroughs.

use super::{
    EvmPass,
    block_layout::{BlockLayout, is_physical_terminal_boundary, layout_successor},
    utils::remap_block_order,
};
use crate::{
    backend::evm::{
        ir::{BlockId, Module, TerminatorKind},
        op,
    },
    target::Target,
};
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use solar_sema::Gcx;

pub(super) struct LoopLayout;

impl EvmPass for LoopLayout {
    fn name(&self) -> &'static str {
        "loop-layout"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        if !gcx.sess.opts.optimization.is_gas() {
            return false;
        }
        BlockLayout.run_pass(gcx, module) | place_loop_latches(gcx, module)
    }
}

/// Estimates one loop traversal with equiprobable branches. Header returns stop propagation;
/// bounded inner-loop rounds approximate their exit probability without an unbounded fixed point.
fn backedge_weights(module: &Module, header: BlockId) -> FxHashMap<BlockId, u64> {
    let mut frontier = FxHashMap::from_iter([(header, 1u64 << 32)]);
    let mut result = FxHashMap::default();
    let mut seen = DenseBitSet::new_empty(module.blocks.len());
    for _ in 0..32 {
        let mut current = frontier.drain().collect::<Vec<_>>();
        current.sort_unstable_by_key(|&(block, _)| block);
        for (id, mut weight) in current {
            seen.insert(id);
            if seen.count() > 256 {
                return result;
            }
            let mut send = |to, weight| {
                if weight == 0 {
                    return;
                }
                if to == header {
                    *result.entry(id).or_default() += weight;
                } else {
                    *frontier.entry(to).or_default() += weight;
                }
            };
            let block = &module.blocks[id];
            for pair in block.instructions.windows(2) {
                if pair[1].as_evm_opcode() == Some(op::JUMPI)
                    && let Some(to) = pair[0].pushed_block()
                {
                    weight /= 2;
                    send(to, weight);
                }
            }
            if let Some(term) = &block.terminator {
                match term.kind {
                    TerminatorKind::Jump(to) => send(to, weight),
                    TerminatorKind::JumpI { then_block, else_block } => {
                        send(then_block, weight / 2);
                        send(else_block, weight / 2);
                    }
                    _ => {}
                }
            }
        }
        if frontier.is_empty() {
            break;
        }
    }
    result
}

/// Gives a loop's latch the fallthrough currently owned by its one-time preheader.
/// An adjacent header/latch pair also qualifies when the header's physical
/// terminator leaves no fallthrough into the latch.
fn place_loop_latches(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let mut order = module.blocks.indices().collect::<Vec<_>>();
    let mut moved = DenseBitSet::new_empty(module.blocks.len());
    let mut reachable = DenseBitSet::new_empty(module.blocks.len());
    let mut pending = Vec::new();
    for latch in module.blocks.indices() {
        if let Some(TerminatorKind::Jump(header)) =
            module.blocks[latch].terminator.as_ref().map(|term| &term.kind)
            && *header != BlockId::ENTRY
            && !moved.contains(*header)
            && !module.blocks[latch].metadata.hotness.is_cold()
        {
            let h = order.iter().position(|block| block == header).unwrap();
            let l = order.iter().position(|&block| block == latch).unwrap();
            if h == 0 || l <= h {
                continue;
            }
            let preheader = order[h - 1];
            if layout_successor(&module.blocks[preheader]) != Some(*header)
                || !matches!(
                    module.blocks[preheader].terminator.as_ref().map(|term| &term.kind),
                    Some(TerminatorKind::Jump(_))
                )
                || !is_physical_terminal_boundary(&module.blocks[order[l - 1]], Some(latch))
            {
                continue;
            }
            reachable.clear();
            pending.clear();
            pending.push(*header);
            let mut visited = 0;
            while let Some(block) = pending.pop() {
                if !reachable.insert(block) {
                    continue;
                }
                visited += 1;
                if visited > 256 {
                    break;
                }
                let block = &module.blocks[block];
                if let Some(term) = &block.terminator {
                    term.kind.visit_targets(|target| pending.push(target));
                }
                for pair in block.instructions.windows(2) {
                    if pair[1].as_evm_opcode() == Some(op::JUMPI)
                        && let Some(target) = pair[0].pushed_block()
                    {
                        pending.push(target);
                    }
                }
            }
            let hotter = if reachable.contains(preheader) {
                let weights = backedge_weights(module, *header);
                let before = weights.get(&preheader).copied().unwrap_or(0);
                let after = weights.get(&latch).copied().unwrap_or(0);
                let price = u128::from(Target::new(gcx).opcode(op::JUMP).gas);
                after != 0 && u128::from(after) * price > u128::from(before) * price * 2
            } else {
                true
            };
            if visited <= 256 && reachable.contains(latch) && hotter {
                // preheader; header ... latch; jump header
                // -> preheader; jump header; latch; header ...
                order.remove(l);
                order.insert(h, latch);
                moved.insert(*header);
            }
        }
    }
    if moved.is_empty() {
        return false;
    }
    remap_block_order(module, &order);
    true
}

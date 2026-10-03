//! Choose loop fallthroughs after structural sharing and stack cleanup.
//!
//! Ordinary block layout establishes traces, then gas mode moves a latch before its header when
//! doing so transfers a jump from an acyclic preheader into a fallthrough on every iteration.
//! Zero iterations can pay one extra jump, one breaks even, and further iterations save one.
//!
//! When the current fallthrough also comes from the loop, a bounded flow estimate distinguishes
//! the common iteration from rarer paths. A branch into a cold block, such as an overflow panic,
//! is never taken. When the header dominates the latch, a branch in the natural loop of their
//! back edge that leaves the loop is taken once per `UNCOUNTED_LOOP_ITERATIONS` traversals, the
//! target's estimate for a loop without a computable trip count, and other branches split integer
//! probability mass equally. Returns to the header stop propagation and inner loops are limited to
//! 32 rounds and 256 visited blocks. The replacement edge must receive more than twice the old
//! edge's estimated mass. Without the loop odds, the inner loop of a nested pair looked no hotter
//! than its entry from the outer loop, which kept a jump on every iteration. This is a static
//! frequency heuristic, not a guarantee for every input. Other existing fallthroughs remain
//! intact, and no instruction or condition is duplicated.
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
use smallvec::SmallVec;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
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

/// The targets of each block's physical `PUSH target; JUMPI` pairs, in instruction order,
/// decoded on first use.
struct BranchTargets(IndexVec<BlockId, Option<SmallVec<[BlockId; 2]>>>);

impl BranchTargets {
    fn new(module: &Module) -> Self {
        Self(IndexVec::from_vec(vec![None; module.blocks.len()]))
    }

    fn get(&mut self, module: &Module, block: BlockId) -> &[BlockId] {
        self.0[block].get_or_insert_with(|| {
            module.blocks[block]
                .instructions
                .windows(2)
                .filter(|pair| pair[1].as_evm_opcode() == Some(op::JUMPI))
                .filter_map(|pair| pair[0].pushed_block())
                .collect()
        })
    }
}

/// Estimates one loop traversal, splitting each branch's mass with [`taken_share`]. Header returns
/// stop propagation; bounded inner-loop rounds approximate their exit probability without an
/// unbounded fixed point.
fn backedge_weights(
    module: &Module,
    branches: &mut BranchTargets,
    header: BlockId,
    body: &DenseBitSet<BlockId>,
) -> FxHashMap<BlockId, u64> {
    let mut frontier = FxHashMap::from_iter([(header, 1u64 << 32)]);
    let mut result = FxHashMap::default();
    let mut seen = DenseBitSet::new_empty(module.blocks.len());
    let mut seen_count = 0;
    for _ in 0..32 {
        let mut current = frontier.drain().collect::<Vec<_>>();
        current.sort_unstable_by_key(|&(block, _)| block);
        for (id, mut weight) in current {
            seen_count += usize::from(seen.insert(id));
            if seen_count > 256 {
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
            let terminator = module.blocks[id].terminator.as_ref().map(|term| &term.kind);
            let targets = branches.get(module, id);
            // Each branch otherwise continues to every later branch and the terminator.
            let mut rests = SmallVec::<[Continuation; 4]>::with_capacity(targets.len());
            let mut rest = terminator.map_or(Continuation::Outside, |kind| {
                Continuation::of_terminator(module, body, kind)
            });
            for &to in targets.iter().rev() {
                rests.push(rest);
                rest = rest.join(Continuation::of_target(module, body, to));
            }
            for (&to, rest) in targets.iter().zip(rests.into_iter().rev()) {
                let taken = taken_share(module, body, id, to, rest, weight);
                send(to, taken);
                weight -= taken;
            }
            match terminator {
                Some(&TerminatorKind::Jump(to)) => send(to, weight),
                Some(&TerminatorKind::JumpI { then_block, else_block }) => {
                    let rest = Continuation::of_target(module, body, else_block);
                    let taken = taken_share(module, body, id, then_block, rest, weight);
                    send(then_block, taken);
                    send(else_block, weight - taken);
                }
                _ => {}
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
    // Position of each block in `order`.
    let mut positions = (0..order.len()).collect::<IndexVec<BlockId, _>>();
    let mut moved = DenseBitSet::new_empty(module.blocks.len());
    let mut reachable = DenseBitSet::new_empty(module.blocks.len());
    let mut pending = Vec::new();
    let mut branches = BranchTargets::new(module);
    let mut predecessors = None;
    for latch in module.blocks.indices() {
        if let Some(TerminatorKind::Jump(header)) =
            module.blocks[latch].terminator.as_ref().map(|term| &term.kind)
            && *header != BlockId::ENTRY
            && !moved.contains(*header)
            && !module.blocks[latch].metadata.hotness.is_cold()
        {
            let (h, l) = (positions[*header], positions[latch]);
            if h == 0 || l <= h {
                continue;
            }
            let preheader = order[h - 1];
            if layout_successor(module, preheader) != Some(*header)
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
                if let Some(term) = &module.blocks[block].terminator {
                    term.kind.visit_targets(|target| pending.push(target));
                }
                pending.extend_from_slice(branches.get(module, block));
            }
            let hotter = if reachable.contains(preheader) {
                let predecessors = predecessors.get_or_insert_with(|| {
                    let mut lists = IndexVec::<BlockId, SmallVec<[BlockId; 2]>>::from_vec(vec![
                        SmallVec::new();
                        module.blocks.len()
                    ]);
                    for block in module.blocks.indices() {
                        if let Some(term) = &module.blocks[block].terminator {
                            term.kind.visit_targets(|target| lists[target].push(block));
                        }
                        for &target in branches.get(module, block) {
                            lists[target].push(block);
                        }
                    }
                    lists
                });
                // Without a natural loop, no branch counts as an exit.
                let body = natural_loop(predecessors, *header, latch)
                    .unwrap_or_else(|| DenseBitSet::new_empty(module.blocks.len()));
                let weights = backedge_weights(module, &mut branches, *header, &body);
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
                order[h..=l].rotate_right(1);
                for (offset, &block) in order[h..=l].iter().enumerate() {
                    positions[block] = h + offset;
                }
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

/// The natural loop of the back edge `latch -> header`: the header and every block that reaches
/// the latch without passing through the header. `None` when the edge is not a back edge, as some
/// path from the entry or from a block entered only through a computed jump, such as a call's
/// continuation, reaches the latch around the header.
fn natural_loop(
    predecessors: &IndexVec<BlockId, SmallVec<[BlockId; 2]>>,
    header: BlockId,
    latch: BlockId,
) -> Option<DenseBitSet<BlockId>> {
    let mut body = DenseBitSet::new_empty(predecessors.len());
    body.insert(header);
    let mut pending = vec![latch];
    while let Some(block) = pending.pop() {
        if body.insert(block) {
            if block == BlockId::ENTRY || predecessors[block].is_empty() {
                return None;
            }
            pending.extend_from_slice(&predecessors[block]);
        }
    }
    Some(body)
}

/// Where a block goes when it does not take a branch, relative to a loop.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Continuation {
    /// Into a cold block or an aborting terminator.
    Cold,
    /// Into the loop.
    Inside,
    /// Out of the loop, or out of the function.
    Outside,
    /// Into blocks on both sides.
    Mixed,
}

impl Continuation {
    fn of_target(module: &Module, body: &DenseBitSet<BlockId>, target: BlockId) -> Self {
        if module.blocks[target].metadata.hotness.is_cold() {
            Self::Cold
        } else if body.contains(target) {
            Self::Inside
        } else {
            Self::Outside
        }
    }

    fn join(self, other: Self) -> Self {
        if self == other { self } else { Self::Mixed }
    }

    fn of_terminator(module: &Module, body: &DenseBitSet<BlockId>, kind: &TerminatorKind) -> Self {
        match *kind {
            TerminatorKind::Op(op::REVERT | op::INVALID) => Self::Cold,
            TerminatorKind::Op(_) => Self::Outside,
            _ => {
                let mut joined = None;
                kind.visit_targets(|target| {
                    let next = Self::of_target(module, body, target);
                    joined = Some(joined.map_or(next, |previous: Self| previous.join(next)));
                });
                joined.unwrap_or(Self::Outside)
            }
        }
    }
}

/// The share of `weight` that a branch in `from` sends to `to` when the block otherwise continues
/// to `rest`: none into a cold block and all of it when `rest` is cold; inside the loop's body,
/// all but one part in `UNCOUNTED_LOOP_ITERATIONS` when only `to` stays in the loop and one part
/// when only `rest` does; half otherwise.
fn taken_share(
    module: &Module,
    body: &DenseBitSet<BlockId>,
    from: BlockId,
    to: BlockId,
    rest: Continuation,
    weight: u64,
) -> u64 {
    let exit = weight / Target::UNCOUNTED_LOOP_ITERATIONS;
    match (Continuation::of_target(module, body, to), rest) {
        (Continuation::Cold, _) => 0,
        (_, Continuation::Cold) => weight,
        (Continuation::Inside, Continuation::Outside) if body.contains(from) => weight - exit,
        (Continuation::Outside, Continuation::Inside) if body.contains(from) => exit,
        _ => weight / 2,
    }
}

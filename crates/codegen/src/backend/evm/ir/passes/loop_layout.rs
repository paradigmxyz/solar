//! Choose loop fallthroughs after structural sharing and stack cleanup.
//!
//! Ordinary block layout establishes traces, then gas mode moves a latch before its header when
//! doing so transfers a jump from an acyclic preheader into a fallthrough on every iteration.
//! Zero iterations can pay one extra jump, one breaks even, and further iterations save one.
//! A latch whose last test guards a cold block, such as a merged overflow check, jumps back on
//! the test's `iszero` instead, `iszero c; push header; jumpi; jump panic`. It moves the same
//! way, and its test turns around to `push panic; jumpi; jump header`: the jump to the header
//! becomes the fallthrough, and every iteration also saves the `iszero`. Only a latch that moves
//! turns around, since a turned test that keeps its jump to the header would pay that jump on
//! every iteration.
//!
//! Size mode skips the pass. A move trades the latch's jump for the preheader's, which is
//! byte-neutral, except that a turned test drops its `iszero`. Moving latches in size mode
//! measured 113 runtime bytes larger across the benchmark corpus, and moving only turned latches
//! 54 bytes larger. Running only the layout step shrank runtime code by 4,902 bytes but grew 19
//! contracts by 1,244 bytes in total, as the second layout moved small traces below the `PUSH1`
//! limit and pushed other labels past it; that needs a fix in block layout, not a price here.
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
//! The layouts that follow can place a branch's `PUSH; JUMPI` target next behind a `JUMP`, as
//! `jumpi-fallthrough` describes; the pass turns such branches around when it finishes.
//!
//! All rewrites preserve explicit block identities, instructions, and stack effects. Changing
//! physical order never splits a block or alters its debug metadata. This pass runs last so
//! subsequent trace construction cannot undo the chosen loop fallthroughs.

use super::{
    EvmPass,
    block_layout::{
        is_physical_terminal_boundary, jump_target, jumpi_targets, layout_successor, order_blocks,
        terminator_kind, visit_successors,
    },
    jumpi_fallthrough::turn_branches,
    utils::remap_block_order,
};
use crate::{
    backend::evm::{
        ir::{BlockId, Instruction, Module, TerminatorKind},
        op,
    },
    target::Target,
};
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::FxHashMap,
};
use solar_sema::Gcx;

pub(super) struct LoopLayout;

impl EvmPass for LoopLayout {
    fn name(&self) -> &'static str {
        "loop-layout"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        if !module.optimization.is_gas() {
            return false;
        }
        order_blocks(gcx, module) | place_loop_latches(module) | turn_branches(gcx, module)
    }
}

/// Gives a loop's latch the fallthrough currently owned by its one-time preheader.
/// An adjacent header/latch pair also qualifies when the header's physical
/// terminator leaves no fallthrough into the latch.
fn place_loop_latches(module: &mut Module) -> bool {
    let mut order = module.blocks.indices().collect::<Vec<_>>();
    // Position of each block in `order`.
    let mut positions = (0..order.len()).collect::<IndexVec<BlockId, _>>();
    let mut moved = DenseBitSet::new_empty(module.blocks.len());
    let mut flow = LoopFlow::new(module);
    for latch in module.blocks.indices() {
        let Some((header, inverted)) = latch_header(module, latch) else { continue };
        if header == BlockId::ENTRY
            || moved.contains(header)
            || module.blocks[latch].metadata.hotness.is_cold()
        {
            continue;
        }
        let (h, l) = (positions[header], positions[latch]);
        if h == 0 || l <= h {
            continue;
        }
        let preheader = order[h - 1];
        if layout_successor(module, preheader) != Some(header)
            || jump_target(&module.blocks[preheader]).is_none()
            || !is_physical_terminal_boundary(&module.blocks[order[l - 1]], Some(latch))
            || !flow.is_hotter_latch(module, header, latch, preheader)
        {
            continue;
        }
        if inverted {
            invert_latch(module, latch, header);
        }
        // preheader; header ... latch; jump header
        // -> preheader; jump header; latch; header ...
        order[h..=l].rotate_right(1);
        for (offset, &block) in order[h..=l].iter().enumerate() {
            positions[block] = h + offset;
        }
        moved.insert(header);
    }
    if moved.is_empty() {
        return false;
    }
    remap_block_order(module, &order);
    true
}

/// Scratch space for deciding whether to move latches, reused across loops.
struct LoopFlow {
    predecessors: Option<IndexVec<BlockId, SmallVec<[BlockId; 2]>>>,
    visited: DenseBitSet<BlockId>,
    pending: Vec<BlockId>,
    body: DenseBitSet<BlockId>,
    frontier: FxHashMap<BlockId, u64>,
    current: Vec<(BlockId, u64)>,
    /// The mass each block sends back to the header.
    weights: FxHashMap<BlockId, u64>,
}

impl LoopFlow {
    fn new(module: &Module) -> Self {
        let blocks = module.blocks.len();
        Self {
            predecessors: None,
            visited: DenseBitSet::new_empty(blocks),
            pending: Vec::new(),
            body: DenseBitSet::new_empty(blocks),
            frontier: FxHashMap::default(),
            current: Vec::new(),
            weights: FxHashMap::default(),
        }
    }

    /// Whether the header reaches the latch within a bounded search, and, when it reaches the
    /// preheader too, the latch's back edge carries more than twice the preheader's estimated
    /// mass.
    fn is_hotter_latch(
        &mut self,
        module: &Module,
        header: BlockId,
        latch: BlockId,
        preheader: BlockId,
    ) -> bool {
        self.visited.clear();
        self.pending.clear();
        self.pending.push(header);
        let mut count = 0;
        while let Some(block) = self.pending.pop() {
            if !self.visited.insert(block) {
                continue;
            }
            count += 1;
            if count > 256 {
                return false;
            }
            visit_successors(&module.blocks[block], |target| self.pending.push(target));
        }
        if !self.visited.contains(latch) {
            return false;
        }
        if !self.visited.contains(preheader) {
            return true;
        }
        let predecessors = self.predecessors.get_or_insert_with(|| {
            let mut lists = index_vec![SmallVec::new(); module.blocks.len()];
            for (block, data) in module.blocks.iter_enumerated() {
                visit_successors(data, |target| lists[target].push(block));
            }
            lists
        });
        natural_loop(predecessors, header, latch, &mut self.body, &mut self.pending);
        self.backedge_weights(module, header);
        let before = self.weights.get(&preheader).copied().unwrap_or_default();
        let after = self.weights.get(&latch).copied().unwrap_or_default();
        after != 0 && u128::from(after) > u128::from(before) * 2
    }

    /// Estimates one loop traversal into `weights`, splitting each branch's mass with
    /// [`taken_share`]. Header returns stop propagation; bounded inner-loop rounds approximate
    /// their exit probability without an unbounded fixed point.
    fn backedge_weights(&mut self, module: &Module, header: BlockId) {
        let Self { body, visited: seen, frontier, current, weights, .. } = self;
        frontier.clear();
        frontier.insert(header, 1u64 << 32);
        weights.clear();
        seen.clear();
        let mut seen_count = 0;
        'rounds: for _ in 0..32 {
            current.extend(frontier.drain());
            current.sort_unstable_by_key(|&(block, _)| block);
            for (id, mut weight) in current.drain(..) {
                seen_count += usize::from(seen.insert(id));
                if seen_count > 256 {
                    break 'rounds;
                }
                let mut send = |to, weight| {
                    if weight == 0 {
                        return;
                    }
                    if to == header {
                        *weights.entry(id).or_default() += weight;
                    } else {
                        *frontier.entry(to).or_default() += weight;
                    }
                };
                let terminator = terminator_kind(&module.blocks[id]);
                let targets = jumpi_targets(&module.blocks[id]).collect::<SmallVec<[_; 2]>>();
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
    }
}

/// The header a block jumps back to, and whether that jump is a conditional one to turn around:
/// the target of its terminator's jump, or, when that jump enters a cold block, the target of the
/// `iszero c; push header; jumpi` it ends with, as a loop body whose last test guards a panic
/// does.
fn latch_header(module: &Module, block: BlockId) -> Option<(BlockId, bool)> {
    let target = jump_target(&module.blocks[block])?;
    if module.blocks[target].metadata.hotness.is_cold()
        && let [.., test, push, jump] = module.blocks[block].instructions.as_slice()
        && test.as_evm_opcode() == Some(op::ISZERO)
        && jump.as_evm_opcode() == Some(op::JUMPI)
        && let Some(header) = push.pushed_block()
        && !module.blocks[header].metadata.hotness.is_cold()
    {
        return Some((header, true));
    }
    Some((target, false))
}

/// Turns a latch's conditional jump back to its header around, so the header can follow it.
fn invert_latch(module: &mut Module, latch: BlockId, header: BlockId) {
    let block = &mut module.blocks[latch];
    let terminator = block.terminator.as_mut().expect("latch has a terminator");
    let TerminatorKind::Jump(cold) = terminator.kind else { unreachable!("latch jumps") };
    // iszero c; push header; jumpi; jump cold
    // -> push cold; jumpi; jump header
    terminator.kind = TerminatorKind::Jump(header);
    let len = block.instructions.len();
    let test = block.instructions.remove(len - 3);
    let mut push = Instruction::push_block(cold);
    push.metadata = std::mem::take(&mut block.instructions[len - 3].metadata);
    block.instructions[len - 3] = push;
    block.instructions[len - 2].metadata.absorb_debug_info(&test.metadata);
}

/// Fills `body` with the natural loop of the back edge `latch -> header`: the header and every
/// block that reaches the latch without passing through the header. It stays empty when the edge
/// is not a back edge, as some path from the entry or from a block entered only through a
/// computed jump, such as a call's continuation, reaches the latch around the header.
fn natural_loop(
    predecessors: &IndexVec<BlockId, SmallVec<[BlockId; 2]>>,
    header: BlockId,
    latch: BlockId,
    body: &mut DenseBitSet<BlockId>,
    pending: &mut Vec<BlockId>,
) {
    body.clear();
    body.insert(header);
    pending.clear();
    pending.push(latch);
    while let Some(block) = pending.pop() {
        if body.insert(block) {
            if block == BlockId::ENTRY || predecessors[block].is_empty() {
                body.clear();
                return;
            }
            pending.extend_from_slice(&predecessors[block]);
        }
    }
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

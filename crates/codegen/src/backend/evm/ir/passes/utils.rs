//! Shared utilities for EVM IR transforms.
//!
//! Physical block reordering must preserve block identity from the perspective
//! of the rest of the IR. The helpers here rebuild block storage and remap every
//! entry, push, and terminator reference together.

use crate::backend::evm::{
    ir::{
        BlockId, Instruction, Module, PushValue, StackEffect, TerminatorKind,
        default_instruction_stack_effect, default_terminator_stack_effect,
    },
    op,
    stack::MAX_STACK_DEPTH,
};
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::FxHashSet,
};

type LabelSet = SmallVec<[BlockId; 1]>;

/// Allocates unused textual block labels without assuming labels are dense.
pub(super) struct FreshLabels {
    occupied: FxHashSet<u32>,
    next: u32,
}

impl FreshLabels {
    pub(super) fn new(module: &Module) -> Self {
        let occupied = module.blocks.iter().map(|block| block.label).collect::<FxHashSet<_>>();
        let next =
            occupied.iter().copied().max().and_then(|label| label.checked_add(1)).unwrap_or(0);
        Self { occupied, next }
    }

    /// Reserves `count` labels before a transform mutates the module.
    pub(super) fn take(&mut self, count: usize) -> Option<Vec<u32>> {
        (0..count).map(|_| self.next()).collect()
    }

    fn next(&mut self) -> Option<u32> {
        let start = self.next;
        loop {
            let label = self.next;
            self.next = self.next.wrapping_add(1);
            if self.occupied.insert(label) {
                return Some(label);
            }
            if self.next == start {
                return None;
            }
        }
    }
}

#[derive(Clone, Default)]
struct AbstractStack {
    /// Stack slots ordered from top to bottom. Each slot records block labels
    /// the word may contain; an empty set is an ordinary or unknown value.
    slots: Vec<LabelSet>,
}

struct BlockStackDepths {
    before: Vec<usize>,
    targets: Vec<(BlockId, AbstractStack)>,
    has_unknown_target: bool,
}

impl AbstractStack {
    fn depth(&self) -> usize {
        self.slots.len()
    }

    fn push_unknown(&mut self) -> Option<()> {
        (self.depth() < MAX_STACK_DEPTH).then(|| self.slots.insert(0, LabelSet::new()))
    }

    fn push_label(&mut self, target: BlockId) -> Option<()> {
        if self.depth() == MAX_STACK_DEPTH {
            return None;
        }
        self.slots.insert(0, smallvec::smallvec![target]);
        Some(())
    }

    fn pop(&mut self) -> Option<LabelSet> {
        (!self.slots.is_empty()).then(|| self.slots.remove(0))
    }

    fn apply_effect(&mut self, effect: StackEffect) -> Option<()> {
        for _ in 0..effect.inputs {
            self.pop()?;
        }
        for _ in 0..effect.outputs {
            self.push_unknown()?;
        }
        Some(())
    }

    fn merge(&mut self, other: &Self) -> bool {
        let mut changed = false;
        if other.depth() > self.depth() {
            self.slots.resize_with(other.depth(), LabelSet::new);
            changed = true;
        }
        for (known, incoming) in self.slots.iter_mut().zip(&other.slots) {
            for &target in incoming {
                if !known.contains(&target) {
                    known.push(target);
                    changed = true;
                }
            }
        }
        changed
    }
}

/// Maximum reachable stack depth immediately before every instruction.
///
/// EVM permits control-flow paths with different stack heights to join. The
/// largest incoming height is the one relevant to transforms that add a
/// temporary stack word, so the analysis propagates maxima to a fixed point.
pub(super) struct StackDepths {
    before: IndexVec<BlockId, Option<Vec<usize>>>,
    unbounded: DenseBitSet<BlockId>,
    has_unbounded_depth: bool,
    has_unknown_target: bool,
}

impl StackDepths {
    /// Computes depths through structural edges and statically recoverable physical jumps.
    pub(super) fn new(module: &Module) -> Option<Self> {
        if module.blocks.is_empty() {
            return None;
        }

        let mut entries = index_vec![None; module.blocks.len()];
        let mut before = index_vec![None; module.blocks.len()];
        let mut successors = IndexVec::from_vec(
            (0..module.blocks.len()).map(|_| DenseBitSet::new_empty(module.blocks.len())).collect(),
        );
        let mut unbounded = DenseBitSet::new_empty(module.blocks.len());
        let mut has_unbounded_depth = false;
        entries[BlockId::ENTRY] = Some(AbstractStack::default());
        let mut pending = vec![BlockId::ENTRY];
        let mut has_unknown_target = false;
        while let Some(block_id) = pending.pop() {
            if unbounded.contains(block_id) {
                continue;
            }
            let Some(entry) = entries[block_id].clone() else { continue };
            let depths = analyze_block(module, block_id, entry)?;
            has_unknown_target |= depths.has_unknown_target;
            before[block_id] = Some(depths.before);
            for (target, incoming) in depths.targets {
                if target.index() >= module.blocks.len() || unbounded.contains(target) {
                    continue;
                }
                successors[block_id].insert(target);
                let old_depth = entries[target].as_ref().map_or(0, AbstractStack::depth);
                let changed = if let Some(known) = entries[target].as_mut() {
                    known.merge(&incoming)
                } else {
                    entries[target] = Some(incoming);
                    true
                };
                if changed {
                    let new_depth = entries[target].as_ref().map_or(0, AbstractStack::depth);
                    if new_depth > old_depth
                        && reachable_blocks(target, &successors).contains(block_id)
                    {
                        has_unbounded_depth = true;
                        for affected in reachable_stack_blocks(target, &successors, &entries).iter()
                        {
                            before[affected] = None;
                            entries[affected] = None;
                            unbounded.insert(affected);
                        }
                        continue;
                    }
                    pending.push(target);
                }
            }
        }
        Some(Self { before, unbounded, has_unbounded_depth, has_unknown_target })
    }

    /// Returns whether an instruction has room for `growth` additional words.
    pub(super) fn has_headroom(&self, block: BlockId, index: usize, growth: usize) -> bool {
        if self.has_unknown_target || self.unbounded.contains(block) {
            return false;
        }
        let Some(depths) = self.before.get(block).and_then(Option::as_ref) else {
            // A block with no explicit incoming edge is unreachable in the EVM IR CFG. It may be
            // a standalone pass fixture or dead code. Treat it as safe only when no reachable
            // positive-depth cycle could transfer its unbounded prefix through a return label.
            return !self.has_unbounded_depth;
        };
        depths
            .get(index)
            .and_then(|depth| depth.checked_add(growth))
            .is_some_and(|depth| depth <= MAX_STACK_DEPTH)
    }
}

fn reachable_stack_blocks(
    start: BlockId,
    successors: &IndexVec<BlockId, DenseBitSet<BlockId>>,
    entries: &IndexVec<BlockId, Option<AbstractStack>>,
) -> DenseBitSet<BlockId> {
    let mut reachable = DenseBitSet::new_empty(successors.len());
    reachable.insert(start);
    let mut pending = vec![start];
    while let Some(block) = pending.pop() {
        for successor in successors[block].iter().chain(
            entries[block]
                .iter()
                .flat_map(|entry| &entry.slots)
                .flat_map(|slot| slot.iter().copied()),
        ) {
            if reachable.insert(successor) {
                pending.push(successor);
            }
        }
    }
    reachable
}

fn reachable_blocks(
    start: BlockId,
    successors: &IndexVec<BlockId, DenseBitSet<BlockId>>,
) -> DenseBitSet<BlockId> {
    let mut reachable = DenseBitSet::new_empty(successors.len());
    reachable.insert(start);
    let mut pending = vec![start];
    while let Some(block) = pending.pop() {
        for successor in successors[block].iter() {
            if reachable.insert(successor) {
                pending.push(successor);
            }
        }
    }
    reachable
}

fn analyze_block(
    module: &Module,
    block_id: BlockId,
    mut stack: AbstractStack,
) -> Option<BlockStackDepths> {
    let block = &module.blocks[block_id];
    let mut instruction_depths = Vec::with_capacity(block.instructions.len());
    let mut targets = Vec::new();
    let mut has_unknown_target = false;
    for inst in &block.instructions {
        instruction_depths.push(stack.depth());
        let is_jumpi = inst.opcode == op::JUMPI;
        let jump_targets =
            is_jumpi.then(|| stack.slots.first().cloned()).flatten().unwrap_or_default();
        has_unknown_target |= is_jumpi && jump_targets.is_empty();
        apply_instruction(&mut stack, inst)?;
        for target in jump_targets {
            targets.push((target, stack.clone()));
        }
    }

    let term = block.terminator.as_ref()?;
    let next = block_id
        .index()
        .checked_add(1)
        .filter(|&index| index < module.blocks.len())
        .map(BlockId::from_usize);
    let lowering_growth = match &term.kind {
        TerminatorKind::IndexedJump(_) => 3,
        TerminatorKind::Jump(target) => usize::from(Some(*target) != next),
        TerminatorKind::JumpI { .. } => 1,
        TerminatorKind::Op(_) => 0,
    };
    if stack.depth().checked_add(lowering_growth)? > MAX_STACK_DEPTH {
        return None;
    }
    let dynamic_targets = matches!(term.kind, TerminatorKind::Op(op::JUMP))
        .then(|| stack.slots.first().cloned())
        .flatten()
        .unwrap_or_default();
    has_unknown_target |=
        matches!(term.kind, TerminatorKind::Op(op::JUMP)) && dynamic_targets.is_empty();
    let effect = term.metadata.stack.or_else(|| default_terminator_stack_effect(&term.kind))?;
    stack.apply_effect(effect)?;
    for target in dynamic_targets {
        targets.push((target, stack.clone()));
    }
    term.kind.visit_targets(|target| targets.push((target, stack.clone())));
    Some(BlockStackDepths { before: instruction_depths, targets, has_unknown_target })
}

fn apply_instruction(stack: &mut AbstractStack, inst: &Instruction) -> Option<()> {
    if inst.is_encoded_push() {
        return if let Some(target) = inst.pushed_block() {
            stack.push_label(target)
        } else {
            stack.push_unknown()
        };
    }
    match inst.opcode {
        opcode if (op::DUP1..=op::DUP16).contains(&opcode) => {
            let reach = usize::from(opcode - op::DUP1) + 1;
            let value = stack.slots.get(reach - 1)?.clone();
            (stack.depth() < MAX_STACK_DEPTH).then(|| stack.slots.insert(0, value))
        }
        opcode if (op::SWAP1..=op::SWAP16).contains(&opcode) => {
            let reach = usize::from(opcode - op::SWAP1) + 2;
            if stack.depth() < reach {
                None
            } else {
                stack.slots.swap(0, reach - 1);
                Some(())
            }
        }
        op::POP => stack.pop().map(drop),
        _ => {
            let effect = inst.metadata.stack.or_else(|| default_instruction_stack_effect(inst))?;
            stack.apply_effect(effect)
        }
    }
}

/// Returns whether a terminator ends the current physical fallthrough trace.
pub(super) fn is_terminal_boundary(kind: &TerminatorKind) -> bool {
    matches!(kind, TerminatorKind::IndexedJump(_))
        || matches!(kind, TerminatorKind::Op(opcode) if op::is_terminal(*opcode))
}

pub(in crate::backend::evm::ir) fn remap_block_order(module: &mut Module, order: &[BlockId]) {
    debug_assert_eq!(order.len(), module.blocks.len());
    remap_blocks(module, order);
}

pub(super) fn retain_blocks(module: &mut Module, order: &[BlockId]) {
    debug_assert!(order.len() <= module.blocks.len());
    remap_blocks(module, order);
}

fn remap_blocks(module: &mut Module, order: &[BlockId]) {
    let mut remap = index_vec![None; module.blocks.len()];
    let mut old_blocks =
        std::mem::take(&mut module.blocks).into_iter().map(Some).collect::<IndexVec<BlockId, _>>();
    let mut blocks = IndexVec::with_capacity(order.len());
    for &old_block in order {
        let block =
            old_blocks[old_block].take().expect("block order must contain each block exactly once");
        let new_block = blocks.push(block);
        remap[old_block] = Some(new_block);
    }
    module.blocks = blocks;
    for block in &mut module.blocks {
        for inst in &mut block.instructions {
            if let Some(PushValue::Block(block)) = &mut inst.value {
                *block = remap[*block].expect("referenced block must be retained");
            }
        }
        if let Some(term) = &mut block.terminator {
            remap_terminator_blocks(&mut term.kind, &remap);
        }
    }
}

fn remap_terminator_blocks(kind: &mut TerminatorKind, remap: &IndexVec<BlockId, Option<BlockId>>) {
    kind.visit_targets_mut(|target| {
        *target = remap[*target].expect("terminator target must be retained");
    });
}

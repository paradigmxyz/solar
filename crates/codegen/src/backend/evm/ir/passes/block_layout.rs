//! EVM basic block trace layout.
//!
//! The IR keeps control-flow edges explicit and leaves physical fallthrough to
//! assembly. This pass follows unconditional jump successors to form linear
//! traces, making those successor blocks adjacent whenever possible. The
//! final lowering can then omit jumps whose target is the next emitted block
//! without encoding physical layout assumptions in the IR. Independent hot
//! traces are placed before cold terminal traces so unlikely exit paths do not
//! interrupt hot code. When an original fallthrough jump reaches the stack
//! limit, the pass keeps that edge adjacent: splitting it would require a
//! target push that cannot fit. These required fallthroughs form linear chains
//! that constrain the otherwise profitable trace order.

use super::{
    EvmPass,
    compact_pushes::selected_len,
    utils::{StackDepths, is_terminal_boundary, relative_stack_depths, remap_block_order},
};
use crate::backend::evm::{
    ir::{
        Block, BlockId, Instruction, Module, PushValue, TerminatorKind,
        assembly::{estimated_indexed_jump_terminator_size, indexed_jump_target_width_bound},
    },
    op,
};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec};
use solar_sema::Gcx;

pub(super) struct BlockLayout;

impl EvmPass for BlockLayout {
    fn name(&self) -> &'static str {
        "block-layout"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        layout_blocks(gcx, module)
    }
}

fn layout_blocks(gcx: Gcx<'_>, module: &mut Module) -> bool {
    if module.blocks.len() <= 1 {
        return false;
    }
    let mut state = RunState::default();
    state.reset(module.blocks.len());
    for block in &module.blocks {
        if let Some(target) = layout_successor(block)
            && target.index() < state.predecessor_counts.len()
        {
            state.predecessor_counts[target] += 1;
        }
    }

    append_layout_trace(module, BlockId::ENTRY, &mut state.placed, &mut state.order);
    for cold in [false, true] {
        for block in module.blocks.indices() {
            if state.predecessor_counts[block] == 0
                && is_cold_terminal_block(&module.blocks[block]) == cold
            {
                append_layout_trace(module, block, &mut state.placed, &mut state.order);
            }
        }
    }

    pack_hot_terminal_blocks(gcx, module, &mut state);
    for cold in [false, true] {
        for block in module.blocks.indices() {
            if is_cold_terminal_block(&module.blocks[block]) == cold {
                append_layout_trace(module, block, &mut state.placed, &mut state.order);
            }
        }
    }

    if state.order.iter().copied().eq(module.blocks.indices()) {
        return false;
    }
    if breaks_original_fallthrough(module, &state.order) {
        preserve_required_fallthroughs(module, &mut state.order)
    }
    if state.order.iter().copied().eq(module.blocks.indices()) {
        return false;
    }
    remap_block_order(module, &state.order);
    true
}

fn breaks_original_fallthrough(module: &Module, order: &[BlockId]) -> bool {
    let mut next = IndexVec::from_vec(vec![None; module.blocks.len()]);
    for blocks in order.windows(2) {
        next[blocks[0]] = Some(blocks[1]);
    }
    module.blocks.indices().any(|block| {
        matches!(
            module.blocks[block].terminator.as_ref().map(|term| &term.kind),
            Some(TerminatorKind::Jump(target))
                if module.next_block(block) == Some(*target) && next[block] != Some(*target)
        )
    })
}

fn preserve_required_fallthroughs(module: &Module, order: &mut Vec<BlockId>) {
    let mut depths = None;
    let mut predecessors = IndexVec::from_vec(vec![None; module.blocks.len()]);
    let mut successors = IndexVec::from_vec(vec![None; module.blocks.len()]);
    for block in module.blocks.indices() {
        let Some(TerminatorKind::Jump(target)) =
            module.blocks[block].terminator.as_ref().map(|term| &term.kind)
        else {
            continue;
        };
        let needs_fallthrough = !jump_has_local_headroom(&module.blocks[block])
            && !depths.get_or_insert_with(|| StackDepths::new(module)).as_ref().is_some_and(
                |depths| depths.has_headroom(block, module.blocks[block].instructions.len(), 1),
            );
        if module.next_block(block) != Some(*target) || !needs_fallthrough {
            continue;
        }
        predecessors[*target] = Some(block);
        successors[block] = Some(*target);
    }

    let mut repaired = Vec::with_capacity(order.len());
    let mut emitted = DenseBitSet::new_empty(module.blocks.len());
    for &block in order.iter() {
        let mut head = block;
        while let Some(predecessor) = predecessors[head] {
            head = predecessor;
        }
        while emitted.insert(head) {
            repaired.push(head);
            let Some(successor) = successors[head] else { break };
            head = successor;
        }
    }
    debug_assert_eq!(repaired.len(), order.len());
    debug_assert_eq!(repaired.first(), Some(&BlockId::ENTRY));
    *order = repaired;
}

fn jump_has_local_headroom(block: &Block) -> bool {
    relative_stack_depths(&block.instructions).is_some_and(|depths| {
        depths.last().is_some_and(|depth| depths.iter().any(|peak| peak > depth))
    })
}

struct RunState {
    predecessor_counts: IndexVec<BlockId, usize>,
    order: Vec<BlockId>,
    placed: DenseBitSet<BlockId>,
    references: IndexVec<BlockId, usize>,
    candidates: Vec<Candidate>,
    picked: DenseBitSet<BlockId>,
    picked_order: Vec<BlockId>,
}

impl Default for RunState {
    fn default() -> Self {
        Self {
            predecessor_counts: IndexVec::new(),
            order: Vec::new(),
            placed: DenseBitSet::new_empty(0),
            references: IndexVec::new(),
            candidates: Vec::new(),
            picked: DenseBitSet::new_empty(0),
            picked_order: Vec::new(),
        }
    }
}

impl RunState {
    fn reset(&mut self, blocks: usize) {
        self.predecessor_counts.clear();
        self.predecessor_counts.resize(blocks, 0);
        self.order.clear();
        if self.order.capacity() < blocks {
            self.order.reserve(blocks);
        }
        if self.placed.domain_size() == blocks {
            self.placed.clear();
            self.picked.clear();
        } else {
            self.placed = DenseBitSet::new_empty(blocks);
            self.picked = DenseBitSet::new_empty(blocks);
        }
        self.references.clear();
        self.references.resize(blocks, 0);
        self.candidates.clear();
        self.picked_order.clear();
    }
}

struct Candidate {
    block: BlockId,
    position: usize,
    size: usize,
    references: usize,
}

fn pack_hot_terminal_blocks(gcx: Gcx<'_>, module: &Module, state: &mut RunState) {
    let Some(first_terminal) = state.order.iter().enumerate().position(|(position, &block)| {
        is_physical_terminal_boundary(&module.blocks[block], state.order.get(position + 1).copied())
    }) else {
        return;
    };
    let insert_at = first_terminal + 1;
    block_reference_counts(module, &state.order, &mut state.references);
    let insert_offset: usize = state.order[..insert_at]
        .iter()
        .enumerate()
        .map(|(index, &block)| {
            estimated_block_size(
                gcx,
                &module.blocks[block],
                state.order.get(index + 1).copied(),
                state.references[block] != 0,
            )
        })
        .sum();
    if insert_offset >= 0xff {
        return;
    }

    for position in insert_at..state.order.len() {
        let block = state.order[position];
        if position == 0
            || !is_physical_terminal_boundary(
                &module.blocks[state.order[position - 1]],
                Some(block),
            )
            || !is_terminal_block(&module.blocks[block])
        {
            continue;
        }
        let size = estimated_block_size(
            gcx,
            &module.blocks[block],
            state.order.get(position + 1).copied(),
            state.references[block] != 0,
        );
        let count = state.references[block];
        if size <= 32 && count >= 2 {
            state.candidates.push(Candidate { block, position, size, references: count });
        }
    }
    state.candidates.sort_unstable_by(|a, b| {
        (b.references * a.size)
            .cmp(&(a.references * b.size))
            .then(b.references.cmp(&a.references))
            .then(a.position.cmp(&b.position))
    });
    let mut budget = 0xff_usize.saturating_sub(insert_offset);
    for candidate in &state.candidates {
        if candidate.size <= budget {
            budget -= candidate.size;
            state.picked.insert(candidate.block);
            state.picked_order.push(candidate.block);
        }
    }
    if state.picked_order.is_empty() {
        return;
    }
    state.order.retain(|block| !state.picked.contains(*block));
    state.order.splice(insert_at..insert_at, state.picked_order.drain(..));
}

fn block_reference_counts(
    module: &Module,
    order: &[BlockId],
    references: &mut IndexVec<BlockId, usize>,
) {
    for (position, &block_id) in order.iter().enumerate() {
        let block = &module.blocks[block_id];
        for inst in &block.instructions {
            if let Some(PushValue::Block(block)) = &inst.value {
                references[*block] += 1;
            }
        }
        if let Some(term) = &block.terminator {
            term.kind.visit_label_targets(order.get(position + 1).copied(), |target| {
                references[target] += 1;
            });
        }
    }
}

fn estimated_block_size(
    gcx: Gcx<'_>,
    block: &Block,
    next: Option<BlockId>,
    addressed: bool,
) -> usize {
    usize::from(addressed)
        + block.instructions.iter().map(|inst| estimated_instruction_size(gcx, inst)).sum::<usize>()
        + block
            .terminator
            .as_ref()
            .map_or(0, |term| estimated_terminator_size(gcx, &term.kind, next))
}

fn estimated_instruction_size(gcx: Gcx<'_>, inst: &Instruction) -> usize {
    if let Some(size) = inst.immutable_type_size() {
        1 + usize::from(size.bytes())
    } else if inst.deferred_push().is_some() {
        3
    } else if inst.is_encoded_push() {
        match &inst.value {
            Some(PushValue::Immediate(value)) => selected_len(gcx, *value),
            Some(PushValue::Block(_)) => 3,
            Some(PushValue::Data(_)) => 4,
            _ => 1,
        }
    } else if let Some(stack_op) = inst.as_stack_op() {
        stack_op
            .assembled_len(gcx.sess.opts.evm_version)
            .expect("block layout only runs on target-compatible stack operations")
    } else {
        1
    }
}

fn estimated_terminator_size(gcx: Gcx<'_>, kind: &TerminatorKind, next: Option<BlockId>) -> usize {
    match kind {
        TerminatorKind::Jump(target) => usize::from(Some(*target) != next) * 4,
        TerminatorKind::Op(op::STOP) => usize::from(next.is_some()),
        TerminatorKind::JumpI { then_block, else_block } => {
            if Some(*else_block) == next {
                4
            } else if Some(*then_block) == next {
                5
            } else {
                8
            }
        }
        TerminatorKind::IndexedJump(targets) => {
            // This pass does not know whether the module is runtime or initcode,
            // so use the larger bound. Final assembly resolves the exact width.
            let target_width = indexed_jump_target_width_bound(gcx.sess.opts.evm_version, true);
            estimated_indexed_jump_terminator_size(
                targets.len(),
                target_width as u8,
                gcx.sess.opts.evm_version,
                gcx.sess.opts.optimization.is_size(),
            )
        }
        TerminatorKind::Op(_) => 1,
    }
}

fn is_terminal_block(block: &Block) -> bool {
    block.terminator.as_ref().is_some_and(|term| is_terminal_boundary(&term.kind))
}

fn is_physical_terminal_boundary(block: &Block, next: Option<BlockId>) -> bool {
    block.terminator.as_ref().is_some_and(|term| {
        is_terminal_boundary(&term.kind)
            || matches!(term.kind, TerminatorKind::Jump(target) if Some(target) != next)
    })
}

fn append_layout_trace(
    module: &Module,
    mut block: BlockId,
    placed: &mut DenseBitSet<BlockId>,
    order: &mut Vec<BlockId>,
) {
    while block.index() < module.blocks.len() && placed.insert(block) {
        order.push(block);
        let Some(target) = layout_successor(&module.blocks[block]) else { return };
        block = target;
    }
}

fn layout_successor(block: &Block) -> Option<BlockId> {
    match &block.terminator.as_ref()?.kind {
        TerminatorKind::Jump(target) => Some(*target),
        _ => None,
    }
}

fn is_cold_terminal_block(block: &Block) -> bool {
    block.metadata.hotness.is_cold()
        && block.terminator.as_ref().is_some_and(|term| is_terminal_boundary(&term.kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_config::{CompileOpts, EvmVersion, OptimizationMode};
    use solar_interface::Session;
    use solar_sema::Compiler;

    fn opts(evm_version: EvmVersion, optimization: OptimizationMode) -> CompileOpts {
        CompileOpts { evm_version, optimization, ..Default::default() }
    }

    #[test]
    fn indexed_jump_estimate_includes_packed_table() {
        let one = TerminatorKind::IndexedJump(vec![BlockId::ENTRY].into_boxed_slice());
        let packed = TerminatorKind::IndexedJump(vec![BlockId::ENTRY; 2].into_boxed_slice());
        let many = TerminatorKind::IndexedJump(vec![BlockId::ENTRY; 33].into_boxed_slice());
        let compiler = Compiler::new(
            Session::builder().opts(opts(EvmVersion::Osaka, OptimizationMode::Size)).build(),
        );
        compiler.enter(|c| {
            assert_eq!(estimated_terminator_size(c.gcx(), &one, None), 8);
            assert_eq!(estimated_terminator_size(c.gcx(), &packed, None), 19);
            assert_eq!(estimated_terminator_size(c.gcx(), &many, None), 61);
        });

        let compiler = Compiler::new(
            Session::builder().opts(opts(EvmVersion::Byzantium, OptimizationMode::Size)).build(),
        );
        compiler.enter(|c| {
            assert_eq!(estimated_terminator_size(c.gcx(), &many, None), 9);
        });
    }
}

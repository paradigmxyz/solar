//! EVM basic block trace layout.
//!
//! The IR keeps control-flow edges explicit and leaves physical fallthrough to
//! assembly. This pass follows unconditional jump successors to form linear
//! traces, making those successor blocks adjacent whenever possible. For an acyclic branch
//! whose taken arm jumps to its other successor, it places the arm before that join. Cold arms
//! stay separate, and known loops keep their existing branch order. The
//! final lowering can then omit jumps whose target is the next emitted block
//! without encoding physical layout assumptions in the IR. Independent hot
//! traces are placed before cold terminal traces so unlikely exit paths do not
//! interrupt hot code. Small, independently movable traces ending in a terminal are packed
//! below the PUSH1 address limit by reference density. Moving the entire trace preserves its
//! fallthrough edges, including a call followed by a shared failure block. Packing also
//! reserves address space for one-byte indexed jump tables to avoid widening their lookups.
//! Size estimates guide placement; assembly still resolves the exact offsets and widths.

use super::{
    EvmPass,
    compact_pushes::selected_len,
    utils::{is_terminal_boundary, remap_block_order},
};
use crate::backend::{
    assembler::assembly::{
        estimated_indexed_jump_terminator_size, indexed_jump_target_width_bound,
    },
    evm::{
        ir::{Block, BlockId, Instruction, Module, PushValue, TerminatorKind},
        op,
    },
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
    for block in module.blocks.indices() {
        if let Some(target) = layout_successor(module, block)
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

    pack_terminal_traces(gcx, module, &mut state);
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
    // branch -> arm -> join; remaining traces
    remap_block_order(module, &state.order);
    true
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
        self.placed.clear_to(blocks);
        self.picked.clear_to(blocks);
        self.references.clear();
        self.references.resize(blocks, 0);
        self.candidates.clear();
        self.picked_order.clear();
    }
}

struct Candidate {
    position: usize,
    end: usize,
    size: usize,
    references: usize,
}

fn pack_terminal_traces(gcx: Gcx<'_>, module: &Module, state: &mut RunState) {
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

    let mut position = insert_at;
    let mut offset = insert_offset;
    while position < state.order.len() {
        let start = position;
        let mut size = 0;
        let mut references = 0;
        loop {
            let block = state.order[position];
            let next = state.order.get(position + 1).copied();
            let block_offset = offset + size;
            size += estimated_block_size(
                gcx,
                &module.blocks[block],
                next,
                state.references[block] != 0,
            );
            references += state.references[block];
            position += 1;
            if is_physical_terminal_boundary(&module.blocks[block], next) {
                if is_terminal_block(&module.blocks[block])
                    && size <= 32
                    && references >= 2
                    && (position == start + 1
                        || (block_offset > 0xff && state.references[block] >= 4))
                {
                    state.candidates.push(Candidate {
                        position: start,
                        end: position,
                        size,
                        references,
                    });
                }
                break;
            }
            if position == state.order.len() {
                break;
            }
        }
        offset += size;
    }
    state.candidates.sort_unstable_by(|a, b| {
        (b.references * a.size)
            .cmp(&(a.references * b.size))
            .then(b.references.cmp(&a.references))
            .then(a.position.cmp(&b.position))
    });
    let mut budget = terminal_packing_budget(gcx, module, state, insert_offset);
    for candidate in &state.candidates {
        if candidate.size <= budget {
            budget -= candidate.size;
            // trace_head; ...; terminal
            for &block in &state.order[candidate.position..candidate.end] {
                state.picked.insert(block);
                state.picked_order.push(block);
            }
        }
    }
    if state.picked_order.is_empty() {
        return;
    }
    // entry_trace; selected_traces; other_traces
    state.order.retain(|block| !state.picked.contains(*block));
    state.order.splice(insert_at..insert_at, state.picked_order.drain(..));
}

// Reserve space for tables whose absolute targets fit in one-byte entries. Widening
// such a table replaces BYTE with shifts and masking on every dispatch.
fn terminal_packing_budget(
    gcx: Gcx<'_>,
    module: &Module,
    state: &RunState,
    insert_offset: usize,
) -> usize {
    let mut budget = 0xff_usize.saturating_sub(insert_offset);
    if module.blocks.iter().any(|block| {
        matches!(
            block.terminator.as_ref().map(|term| &term.kind),
            Some(TerminatorKind::IndexedJump(_))
        )
    }) {
        let mut offsets = IndexVec::from_vec(vec![usize::MAX; module.blocks.len()]);
        let mut offset = 0;
        for (position, &block_id) in state.order.iter().enumerate() {
            offsets[block_id] = offset;
            let block = &module.blocks[block_id];
            let next = state.order.get(position + 1).copied();
            offset += estimated_block_size(gcx, block, next, state.references[block_id] != 0);
            if let Some(kind @ TerminatorKind::IndexedJump(targets)) =
                block.terminator.as_ref().map(|term| &term.kind)
                && targets.len() <= 32
            {
                offset -= estimated_terminator_size(gcx, kind, next);
                offset += estimated_indexed_jump_terminator_size(
                    targets.len(),
                    1,
                    gcx.sess.opts.evm_version,
                    gcx.sess.opts.optimization.is_size(),
                );
            }
        }
        for block in &module.blocks {
            if let Some(TerminatorKind::IndexedJump(targets)) =
                block.terminator.as_ref().map(|term| &term.kind)
                && targets.len() <= 32
                && let Some(last) = targets.iter().map(|&target| offsets[target]).max()
                && (insert_offset..0xff).contains(&last)
            {
                budget = budget.min(0xfe - last);
            }
        }
    }
    budget
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
        let Some(target) = layout_successor(module, block) else { return };
        block = target;
    }
}

fn layout_successor(module: &Module, block: BlockId) -> Option<BlockId> {
    match &module.blocks[block].terminator.as_ref()?.kind {
        TerminatorKind::Jump(target) => Some(*target),
        TerminatorKind::JumpI { then_block, else_block }
            if !module.blocks[block].metadata.in_loop
                && triangle_arm(module, *then_block, *else_block) =>
        {
            Some(*then_block)
        }
        _ => None,
    }
}

pub(super) fn triangle_arm(module: &Module, arm: BlockId, join: BlockId) -> bool {
    arm != join
        && module.blocks.get(arm).is_some_and(|block| {
            !block.metadata.hotness.is_cold()
                && !block.metadata.in_loop
                && matches!(block.terminator.as_ref().map(|term| &term.kind),
                    Some(TerminatorKind::Jump(target)) if *target == join)
        })
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

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
//! In size mode, shared indirect-return tails prefer the predecessor with strictly more incoming
//! references. This keeps the more frequently shared outlined entry adjacent to its return tail,
//! rather than making every call through it pay another jump. Reference counts estimate frequency;
//! they are not execution profiles. A predecessor that pushes a continuation counts as one call
//! site: its incoming branches do not measure uses of a shared stub entry.
//! When counting those references instead produces a smaller layout, use it: two candidate orders
//! are priced with their actual label PUSH widths, fallthroughs and landing instructions. Ties keep
//! the call-site weighting. Deferred data and indexed tables stay with the ordinary estimate, and
//! a bounded width calculation abandons the alternative if it does not converge.
//! Other trace edges and gas-mode placement keep their order.

use super::{
    EvmPass,
    utils::{is_split_point, is_terminal_boundary, remap_block_order},
};
use crate::{
    backend::{
        assembler::assembly::{
            TablePacking, estimated_indexed_jump_terminator_size, indexed_jump_target_width_bound,
        },
        evm::{
            ir::{Block, BlockId, Instruction, Module, ModuleTarget, PushValue, TerminatorKind},
            op,
        },
    },
    target::Target,
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
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

    let target = module.target(gcx);
    let preferred = if gcx.sess.opts.optimization.is_size() {
        shared_return_predecessors(module, &target, true)
    } else {
        FxHashMap::default()
    };
    build_layout_order(gcx, module, &preferred, &mut state);
    if gcx.sess.opts.optimization.is_size() {
        let alternate = shared_return_predecessors(module, &target, false);
        if alternate != preferred
            && let Some(size) = encoded_layout_size(module, &state.order, &target)
        {
            let mut other = RunState::default();
            other.reset(module.blocks.len());
            other.predecessor_counts.clone_from(&state.predecessor_counts);
            build_layout_order(gcx, module, &alternate, &mut other);
            if encoded_layout_size(module, &other.order, &target).is_some_and(|bytes| bytes < size)
            {
                state.order = other.order;
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

fn build_layout_order(
    gcx: Gcx<'_>,
    module: &Module,
    preferred: &FxHashMap<BlockId, BlockId>,
    state: &mut RunState,
) {
    append_layout_trace(module, BlockId::ENTRY, preferred, &mut state.placed, &mut state.order);
    for cold in [false, true] {
        for block in module.blocks.indices() {
            if state.predecessor_counts[block] == 0
                && is_cold_terminal_block(&module.blocks[block]) == cold
            {
                append_layout_trace(module, block, preferred, &mut state.placed, &mut state.order);
            }
        }
    }

    pack_terminal_traces(gcx, module, state);
    for cold in [false, true] {
        for block in module.blocks.indices() {
            if is_cold_terminal_block(&module.blocks[block]) == cold {
                append_layout_trace(module, block, preferred, &mut state.placed, &mut state.order);
            }
        }
    }
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
                module.code_follows,
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
            // Past the first 256 bytes, only whether a trace fits in 32 bytes matters.
            let limit = if offset > 0xff { 32usize.saturating_sub(size) } else { usize::MAX };
            size += estimated_block_size_up_to(
                gcx,
                &module.blocks[block],
                next,
                state.references[block] != 0,
                module.code_follows,
                limit,
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
// such a table replaces the BYTE pick with a larger stride table.
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
            // Offsets only grow, and targets from here on cannot fit in one-byte entries.
            if offset >= 0xff {
                break;
            }
            offsets[block_id] = offset;
            let block = &module.blocks[block_id];
            let next = state.order.get(position + 1).copied();
            offset += estimated_block_size(
                gcx,
                block,
                next,
                state.references[block_id] != 0,
                module.code_follows,
            );
            if let Some(kind @ TerminatorKind::IndexedJump(targets)) =
                block.terminator.as_ref().map(|term| &term.kind)
                && targets.len() <= 32
            {
                offset -= estimated_terminator_size(gcx, kind, next, module.code_follows);
                offset += estimated_indexed_jump_terminator_size(
                    targets.len(),
                    1,
                    gcx.sess.opts.evm_version,
                    TablePacking::new(&gcx.sess.opts),
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

pub(super) fn estimated_block_size(
    gcx: Gcx<'_>,
    block: &Block,
    next: Option<BlockId>,
    addressed: bool,
    code_follows: bool,
) -> usize {
    estimated_block_size_up_to(gcx, block, next, addressed, code_follows, usize::MAX)
}

/// Returns [`estimated_block_size`] when it is at most `limit`, and otherwise some larger size.
fn estimated_block_size_up_to(
    gcx: Gcx<'_>,
    block: &Block,
    next: Option<BlockId>,
    addressed: bool,
    code_follows: bool,
    limit: usize,
) -> usize {
    let mut size = usize::from(addressed)
        + block
            .terminator
            .as_ref()
            .map_or(0, |term| estimated_terminator_size(gcx, &term.kind, next, code_follows));
    for inst in &block.instructions {
        if size > limit {
            break;
        }
        size += estimated_instruction_size(gcx, inst);
    }
    size
}

fn estimated_instruction_size(gcx: Gcx<'_>, inst: &Instruction) -> usize {
    let target = Target::new(gcx);
    if let Some(size) = inst.immutable_type_size() {
        target.opcode(op::push(size.bytes())).bytes as usize
    } else if inst.is_encoded_push() {
        match &inst.value {
            Some(PushValue::Immediate(value)) => target.push(*value).bytes as usize,
            Some(PushValue::Library(_)) => target.opcode(op::PUSH20).bytes as usize,
            Some(PushValue::Block(_)) => target.label_push().bytes as usize,
            // Data follows the code, so assume its offsets need a third byte.
            Some(PushValue::Data(_)) => target.opcode(op::PUSH3).bytes as usize,
            Some(PushValue::DataSize(size)) => target.push(size.bound()).bytes as usize,
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

fn estimated_terminator_size(
    gcx: Gcx<'_>,
    kind: &TerminatorKind,
    next: Option<BlockId>,
    code_follows: bool,
) -> usize {
    let model = Target::new(gcx);
    let jump = (model.label_push() + model.opcode(op::JUMP)).bytes as usize;
    let branch = (model.label_push() + model.opcode(op::JUMPI)).bytes as usize;
    match kind {
        TerminatorKind::Jump(target) => usize::from(Some(*target) != next) * jump,
        TerminatorKind::Op(op::STOP) => usize::from(next.is_some() || code_follows),
        TerminatorKind::JumpI { then_block, else_block } => {
            if Some(*else_block) == next {
                branch
            } else if Some(*then_block) == next {
                model.opcode(op::ISZERO).bytes as usize + branch
            } else {
                branch + jump
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
                TablePacking::new(&gcx.sess.opts),
            )
        }
        TerminatorKind::Op(opcode) => model.opcode(*opcode).bytes as usize,
    }
}

fn is_terminal_block(block: &Block) -> bool {
    block.terminator.as_ref().is_some_and(|term| is_terminal_boundary(&term.kind))
}

pub(super) fn is_physical_terminal_boundary(block: &Block, next: Option<BlockId>) -> bool {
    block.terminator.as_ref().is_some_and(|term| {
        is_terminal_boundary(&term.kind)
            || matches!(term.kind, TerminatorKind::Jump(target) if Some(target) != next)
    })
}

fn append_layout_trace(
    module: &Module,
    mut block: BlockId,
    preferred: &FxHashMap<BlockId, BlockId>,
    placed: &mut DenseBitSet<BlockId>,
    order: &mut Vec<BlockId>,
) {
    while block.index() < module.blocks.len() && placed.insert(block) {
        order.push(block);
        let Some(target) = layout_successor(module, block) else { return };
        if preferred.get(&target).is_some_and(|&pred| pred != block && !placed.contains(pred))
            && is_split_point(
                &module.blocks[block].instructions,
                module.blocks[block].instructions.len(),
            )
        {
            return;
        }
        block = target;
    }
}

pub(super) fn layout_successor(module: &Module, block: BlockId) -> Option<BlockId> {
    match &module.blocks[block].terminator.as_ref()?.kind {
        TerminatorKind::Jump(target) => Some(*target),
        TerminatorKind::JumpI { then_block, else_block }
            if !module.blocks[block].metadata.in_loop
                && triangle_arm(module, *then_block, *else_block) =>
        {
            Some(*then_block)
        }
        TerminatorKind::JumpI { else_block, .. } => Some(*else_block),
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

/// Selects a shared return tail's unique highest-weight predecessor. Changing which jump falls
/// through keeps the number of jumps unchanged; price their estimated executions with the target.
fn shared_return_predecessors(
    module: &Module,
    target: &Target,
    cap_call_sites: bool,
) -> FxHashMap<BlockId, BlockId> {
    let mut incoming = IndexVec::from_vec(vec![0usize; module.blocks.len()]);
    let mut call_sites = DenseBitSet::new_empty(module.blocks.len());
    for (id, block) in module.blocks.iter_enumerated() {
        for inst in &block.instructions {
            if let Some(target) = inst.pushed_block() {
                incoming[target] += 1;
                if module.blocks[target].metadata.is_continuation {
                    call_sites.insert(id);
                }
            }
        }
        if let Some(term) = &block.terminator {
            term.kind.visit_targets(|target| incoming[target] += 1);
        }
    }
    let jump_gas = target.runtime_gas(target.jump()) as usize;
    let mut candidates = FxHashMap::<BlockId, (BlockId, usize, bool)>::default();
    for (block, data) in module.blocks.iter_enumerated() {
        let Some(TerminatorKind::Jump(tail)) = data.terminator.as_ref().map(|term| &term.kind)
        else {
            continue;
        };
        if !matches!(
            module.blocks[*tail].terminator.as_ref().map(|term| &term.kind),
            Some(TerminatorKind::Op(op::JUMP))
        ) {
            continue;
        }
        let references =
            if cap_call_sites && call_sites.contains(block) { 1 } else { incoming[block] };
        let weight = references.saturating_mul(jump_gas);
        candidates
            .entry(*tail)
            .and_modify(|best| {
                if weight > best.1 {
                    *best = (block, weight, true);
                } else if weight == best.1 {
                    best.2 = false;
                }
            })
            .or_insert((block, weight, true));
    }
    candidates
        .into_iter()
        .filter_map(|(tail, (block, weight, unique))| {
            (unique && weight > jump_gas).then_some((tail, block))
        })
        .collect()
}

/// Prices an order with the same label-width fixed point as ordinary assembly. The bounded search
/// only ranks layouts; unknown data, tables, or slow convergence keep the original choice.
fn encoded_layout_size(module: &Module, order: &[BlockId], target: &Target) -> Option<usize> {
    // Besides addressing data, its final fallthrough guard depends on the last emitted opcode.
    // Leave both that guard and data offsets with the assembler.
    if !module.data.is_empty() {
        return None;
    }
    let mut fixed = IndexVec::from_vec(vec![0usize; module.blocks.len()]);
    let mut labels = Vec::new();
    for (position, &block_id) in order.iter().enumerate() {
        let block = &module.blocks[block_id];
        for inst in &block.instructions {
            let size = if let Some(width) = inst.immutable_type_size() {
                target.opcode(op::push(width.bytes())).bytes
            } else if inst.is_encoded_push() {
                match &inst.value {
                    Some(PushValue::Immediate(value)) => target.encoded_push(*value).bytes,
                    Some(PushValue::Library(_)) => target.opcode(op::PUSH20).bytes,
                    Some(PushValue::Block(label)) => {
                        labels.push((block_id, *label));
                        continue;
                    }
                    _ => return None,
                }
            } else if let Some(op) = inst.as_stack_op() {
                target.stack_op(op)?.bytes
            } else {
                target.opcode(inst.opcode).bytes
            };
            fixed[block_id] += size as usize;
        }
        let next = order.get(position + 1).copied();
        let kind = &block.terminator.as_ref()?.kind;
        kind.visit_label_targets(next, |label| labels.push((block_id, label)));
        let tail = match kind {
            TerminatorKind::Jump(label) => {
                if Some(*label) == next {
                    0
                } else {
                    target.opcode(op::JUMP).bytes
                }
            }
            TerminatorKind::JumpI { then_block, else_block } => {
                target.opcode(op::JUMPI).bytes
                    + if next == Some(*else_block) {
                        0
                    } else if next == Some(*then_block) {
                        target.opcode(op::ISZERO).bytes
                    } else {
                        target.opcode(op::JUMP).bytes
                    }
            }
            TerminatorKind::Op(op::STOP) if next.is_none() && !module.code_follows => 0,
            TerminatorKind::Op(opcode) => target.opcode(*opcode).bytes,
            TerminatorKind::IndexedJump(_) => return None,
        };
        fixed[block_id] += tail as usize;
    }
    let mut addressed = DenseBitSet::new_empty(module.blocks.len());
    for &(_, label) in &labels {
        if addressed.insert(label) {
            fixed[label] += target.opcode(op::JUMPDEST).bytes as usize;
        }
    }
    let mut widths =
        IndexVec::from_vec(vec![target.encoded_push(U256::ZERO).bytes; module.blocks.len()]);
    let mut sizes = fixed.clone();
    // Most contracts converge in two or three rounds. Stop before pricing becomes an unbounded
    // compile-time cost on chains of labels just below a PUSH-width boundary.
    for _ in 0..16 {
        sizes.clone_from(&fixed);
        for &(block, label) in &labels {
            sizes[block] += widths[label] as usize;
        }
        let mut offset = 0;
        let mut changed = false;
        for &block in order {
            let width = target.encoded_push(U256::from(offset)).bytes;
            changed |= widths[block] != width;
            widths[block] = width;
            offset += sizes[block];
        }
        if !changed {
            return Some(offset);
        }
    }
    None
}

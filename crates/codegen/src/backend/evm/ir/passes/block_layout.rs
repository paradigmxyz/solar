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
//!
//! A branch's `PUSH; JUMPI` target is no trace edge, so a layout can place it next to the branch
//! behind a `JUMP`. The pass then turns such a branch around, as `jumpi-fallthrough` describes.

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
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::FxHashMap,
};
use solar_sema::Gcx;
use std::ops::Range;

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
    let reordered = order_blocks(gcx, module);
    let turned = super::jumpi_fallthrough::turn_branches(gcx, module);
    reordered | turned
}

pub(super) fn order_blocks(gcx: Gcx<'_>, module: &mut Module) -> bool {
    if module.blocks.len() <= 1 {
        return false;
    }
    let mut has_predecessor = DenseBitSet::new_empty(module.blocks.len());
    for block in module.blocks.indices() {
        if let Some(target) = layout_successor(module, block) {
            has_predecessor.insert(target);
        }
    }

    let est = SizeEstimator::new(gcx, module);
    let target = est.target.priced_for(module.code_follows);
    let [preferred, alternate] = if module.optimization.is_size() {
        shared_return_predecessors(module, &target)
    } else {
        Default::default()
    };
    let mut order = build_layout_order(est, module, &has_predecessor, &preferred);
    if alternate != preferred
        && let Some(size) = encoded_layout_size(module, &order, &target)
    {
        let other = build_layout_order(est, module, &has_predecessor, &alternate);
        if encoded_layout_size(module, &other, &target).is_some_and(|bytes| bytes < size) {
            order = other;
        }
    }

    if order.is_sorted() {
        return false;
    }
    // branch -> arm -> join; remaining traces
    remap_block_order(module, &order);
    true
}

fn build_layout_order(
    est: SizeEstimator,
    module: &Module,
    has_predecessor: &DenseBitSet<BlockId>,
    preferred: &FxHashMap<BlockId, BlockId>,
) -> Vec<BlockId> {
    let blocks = module.blocks.len();
    let mut layout = Layout {
        module,
        preferred,
        placed: DenseBitSet::new_empty(blocks),
        order: Vec::with_capacity(blocks),
    };
    layout.append_trace(BlockId::ENTRY);
    layout.append_roots(Some(has_predecessor));
    pack_terminal_traces(est, module, &mut layout.order);
    layout.append_roots(None);
    layout.order
}

struct Layout<'a> {
    module: &'a Module,
    preferred: &'a FxHashMap<BlockId, BlockId>,
    placed: DenseBitSet<BlockId>,
    order: Vec<BlockId>,
}

impl Layout<'_> {
    /// Places hot traces, then cold terminal ones, starting from the blocks that no trace
    /// reaches (all blocks when `has_predecessor` is `None`).
    fn append_roots(&mut self, has_predecessor: Option<&DenseBitSet<BlockId>>) {
        for cold in [false, true] {
            for block in self.module.blocks.indices() {
                if has_predecessor.is_none_or(|set| !set.contains(block))
                    && is_cold_terminal_block(&self.module.blocks[block]) == cold
                {
                    self.append_trace(block);
                }
            }
        }
    }

    fn append_trace(&mut self, mut block: BlockId) {
        while self.placed.insert(block) {
            self.order.push(block);
            let Some(target) = layout_successor(self.module, block) else { return };
            if self
                .preferred
                .get(&target)
                .is_some_and(|&pred| pred != block && !self.placed.contains(pred))
                && ends_at_split_point(&self.module.blocks[block])
            {
                return;
            }
            block = target;
        }
    }
}

struct Candidate {
    range: Range<usize>,
    size: usize,
    references: usize,
}

fn pack_terminal_traces(est: SizeEstimator, module: &Module, order: &mut Vec<BlockId>) {
    let Some(first_terminal) = (0..order.len()).find(|&position| {
        is_physical_terminal_boundary(
            &module.blocks[order[position]],
            order.get(position + 1).copied(),
        )
    }) else {
        return;
    };
    let insert_at = first_terminal + 1;
    let placement =
        Placement { est, module, order, references: block_reference_counts(module, order) };
    let insert_offset: usize =
        (0..insert_at).map(|position| placement.size(position, usize::MAX)).sum();
    if insert_offset >= 0xff {
        return;
    }

    let mut candidates = Vec::new();
    let mut position = insert_at;
    let mut offset = insert_offset;
    while position < order.len() {
        let start = position;
        let mut size = 0;
        let mut references = 0;
        loop {
            let block = order[position];
            let block_offset = offset + size;
            // Past the first 256 bytes, only whether a trace fits in 32 bytes matters.
            let limit = if offset > 0xff { 32usize.saturating_sub(size) } else { usize::MAX };
            size += placement.size(position, limit);
            references += placement.references[block];
            position += 1;
            if is_physical_terminal_boundary(&module.blocks[block], order.get(position).copied()) {
                if is_terminal_block(&module.blocks[block])
                    && size <= 32
                    && references >= 2
                    && (position == start + 1
                        || (block_offset > 0xff && placement.references[block] >= 4))
                {
                    candidates.push(Candidate { range: start..position, size, references });
                }
                break;
            }
            if position == order.len() {
                break;
            }
        }
        offset += size;
    }
    candidates.sort_unstable_by(|a, b| {
        (b.references * a.size)
            .cmp(&(a.references * b.size))
            .then(b.references.cmp(&a.references))
            .then(a.range.start.cmp(&b.range.start))
    });
    let mut budget = terminal_packing_budget(&placement, insert_offset);
    let mut picked = DenseBitSet::new_empty(module.blocks.len());
    let mut picked_order = Vec::new();
    for candidate in candidates {
        if candidate.size <= budget {
            budget -= candidate.size;
            // trace_head; ...; terminal
            for &block in &order[candidate.range] {
                picked.insert(block);
                picked_order.push(block);
            }
        }
    }
    if picked_order.is_empty() {
        return;
    }
    // entry_trace; selected_traces; other_traces
    order.retain(|&block| !picked.contains(block));
    order.splice(insert_at..insert_at, picked_order);
}

/// A partial layout with the references that decide which of its blocks need a label.
struct Placement<'a> {
    est: SizeEstimator,
    module: &'a Module,
    order: &'a [BlockId],
    references: IndexVec<BlockId, usize>,
}

impl Placement<'_> {
    /// Returns [`SizeEstimator::block`] for the block at `position`.
    fn size(&self, position: usize, limit: usize) -> usize {
        let block = self.order[position];
        self.est.block(
            &self.module.blocks[block],
            self.order.get(position + 1).copied(),
            self.references[block] != 0,
            limit,
        )
    }
}

// Reserve space for tables whose absolute targets fit in one-byte entries. Widening
// such a table replaces the BYTE pick with a larger stride table.
fn terminal_packing_budget(placement: &Placement<'_>, insert_offset: usize) -> usize {
    let Placement { est, module, order, .. } = *placement;
    let budget = 0xff_usize.saturating_sub(insert_offset);
    if !module.blocks.iter().any(|block| one_byte_table(block).is_some()) {
        return budget;
    }
    let mut offsets = index_vec![usize::MAX; module.blocks.len()];
    let mut offset = 0;
    for (position, &block_id) in order.iter().enumerate() {
        // Offsets only grow, and targets from here on cannot fit in one-byte entries.
        if offset >= 0xff {
            break;
        }
        offsets[block_id] = offset;
        offset += placement.size(position, usize::MAX);
        if let Some(targets) = one_byte_table(&module.blocks[block_id]) {
            offset = offset - est.indexed_jump(targets.len(), false)
                + est.indexed_jump(targets.len(), true);
        }
    }
    module
        .blocks
        .iter()
        .filter_map(|block| {
            let last =
                one_byte_table(block)?.iter().map(|&destination| offsets[destination]).max()?;
            (insert_offset..0xff).contains(&last).then(|| 0xfe - last)
        })
        .fold(budget, usize::min)
}

fn one_byte_table(block: &Block) -> Option<&[BlockId]> {
    match terminator_kind(block) {
        Some(TerminatorKind::IndexedJump(targets)) if targets.len() <= 32 => Some(targets),
        _ => None,
    }
}

fn block_reference_counts(module: &Module, order: &[BlockId]) -> IndexVec<BlockId, usize> {
    let mut references = index_vec![0usize; module.blocks.len()];
    for (position, &block_id) in order.iter().enumerate() {
        let block = &module.blocks[block_id];
        for inst in &block.instructions {
            if let Some(target) = inst.pushed_block() {
                references[target] += 1;
            }
        }
        if let Some(term) = &block.terminator {
            term.kind.visit_label_targets(order.get(position + 1).copied(), |target| {
                references[target] += 1;
            });
        }
    }
    references
}

/// Estimates block sizes for layout decisions with a runtime cost model.
#[derive(Clone, Copy)]
pub(super) struct SizeEstimator {
    pub(super) target: Target,
    packing: TablePacking,
    code_follows: bool,
}

impl SizeEstimator {
    pub(super) fn new(gcx: Gcx<'_>, module: &Module) -> Self {
        Self {
            target: module.runtime_target(gcx),
            packing: TablePacking::new(gcx, module.optimization),
            code_follows: module.code_follows,
        }
    }

    /// Returns the estimated size of `block` when it is at most `limit`, and otherwise some
    /// larger size.
    pub(super) fn block(
        self,
        block: &Block,
        next: Option<BlockId>,
        addressed: bool,
        limit: usize,
    ) -> usize {
        let mut size = usize::from(addressed)
            + terminator_kind(block).map_or(0, |kind| self.terminator(kind, next));
        for inst in &block.instructions {
            if size > limit {
                break;
            }
            size += self.instruction(inst);
        }
        size
    }

    fn instruction(self, inst: &Instruction) -> usize {
        let target = self.target;
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
                .assembled_len(target.evm_version())
                .expect("block layout only runs on target-compatible stack operations")
        } else {
            1
        }
    }

    fn terminator(self, kind: &TerminatorKind, next: Option<BlockId>) -> usize {
        let target = self.target;
        let transfer = |opcode| (target.label_push() + target.opcode(opcode)).bytes as usize;
        let (jump, branch) = (transfer(op::JUMP), transfer(op::JUMPI));
        match kind {
            TerminatorKind::Jump(destination) => usize::from(Some(*destination) != next) * jump,
            TerminatorKind::Op(op::STOP) => usize::from(next.is_some() || self.code_follows),
            TerminatorKind::JumpI { then_block, else_block } => {
                if Some(*else_block) == next {
                    branch
                } else if Some(*then_block) == next {
                    target.opcode(op::ISZERO).bytes as usize + branch
                } else {
                    branch + jump
                }
            }
            TerminatorKind::IndexedJump(targets) => self.indexed_jump(targets.len(), false),
            TerminatorKind::Op(opcode) => target.opcode(*opcode).bytes as usize,
        }
    }

    /// The size of a table of `targets` entries, with one-byte entries or, as this pass does not
    /// know whether the module is runtime or initcode, the larger bound. Final assembly resolves
    /// the exact width.
    fn indexed_jump(self, targets: usize, one_byte: bool) -> usize {
        let evm_version = self.target.evm_version();
        let width = if one_byte { 1 } else { indexed_jump_target_width_bound(evm_version, true) };
        estimated_indexed_jump_terminator_size(targets, width as u8, evm_version, self.packing)
    }
}

pub(super) fn terminator_kind(block: &Block) -> Option<&TerminatorKind> {
    block.terminator.as_ref().map(|term| &term.kind)
}

/// The target of a block ending in an unconditional jump.
pub(super) fn jump_target(block: &Block) -> Option<BlockId> {
    match terminator_kind(block) {
        Some(&TerminatorKind::Jump(target)) => Some(target),
        _ => None,
    }
}

/// Visits the block's terminator targets and the targets of its physical `PUSH target; JUMPI`
/// pairs.
pub(super) fn visit_successors(block: &Block, mut visit: impl FnMut(BlockId)) {
    if let Some(kind) = terminator_kind(block) {
        kind.visit_targets(&mut visit);
    }
    jumpi_targets(block).for_each(visit);
}

/// Whether the end of the block is a legal place to split it from its successor.
pub(super) fn ends_at_split_point(block: &Block) -> bool {
    is_split_point(&block.instructions, block.instructions.len())
}

/// The targets of the block's physical `PUSH target; JUMPI` pairs, in instruction order.
pub(super) fn jumpi_targets(block: &Block) -> impl Iterator<Item = BlockId> + '_ {
    block
        .instructions
        .windows(2)
        .filter(|pair| pair[1].as_evm_opcode() == Some(op::JUMPI))
        .filter_map(|pair| pair[0].pushed_block())
}

fn is_terminal_block(block: &Block) -> bool {
    terminator_kind(block).is_some_and(is_terminal_boundary)
}

pub(super) fn is_physical_terminal_boundary(block: &Block, next: Option<BlockId>) -> bool {
    terminator_kind(block).is_some_and(|kind| {
        is_terminal_boundary(kind)
            || matches!(kind, TerminatorKind::Jump(target) if Some(*target) != next)
    })
}

pub(super) fn layout_successor(module: &Module, block: BlockId) -> Option<BlockId> {
    match terminator_kind(&module.blocks[block])? {
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
    let block = &module.blocks[arm];
    arm != join
        && !block.metadata.hotness.is_cold()
        && !block.metadata.in_loop
        && jump_target(block) == Some(join)
}

fn is_cold_terminal_block(block: &Block) -> bool {
    block.metadata.hotness.is_cold() && is_terminal_block(block)
}

/// Selects each shared return tail's unique highest-weight predecessor, once with call sites
/// capped at one reference and once counting all references. Changing which jump falls through
/// keeps the number of jumps unchanged; price their estimated executions with the target.
fn shared_return_predecessors(
    module: &Module,
    target: &Target,
) -> [FxHashMap<BlockId, BlockId>; 2] {
    let mut incoming = index_vec![0usize; module.blocks.len()];
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
    let mut candidates = <[FxHashMap<_, (BlockId, usize, bool)>; 2]>::default();
    for (block, data) in module.blocks.iter_enumerated() {
        if let Some(tail) = jump_target(data)
            && matches!(terminator_kind(&module.blocks[tail]), Some(TerminatorKind::Op(op::JUMP)))
        {
            for (candidates, cap_call_sites) in candidates.iter_mut().zip([true, false]) {
                let references =
                    if cap_call_sites && call_sites.contains(block) { 1 } else { incoming[block] };
                let weight = references.saturating_mul(jump_gas);
                candidates
                    .entry(tail)
                    .and_modify(|best| {
                        if weight > best.1 {
                            *best = (block, weight, true);
                        } else if weight == best.1 {
                            best.2 = false;
                        }
                    })
                    .or_insert((block, weight, true));
            }
        }
    }
    candidates.map(|candidates| {
        candidates
            .into_iter()
            .filter_map(|(tail, (block, weight, unique))| {
                (unique && weight > jump_gas).then_some((tail, block))
            })
            .collect()
    })
}

/// Prices an order with the same label-width fixed point as ordinary assembly. The bounded search
/// only ranks layouts; unknown data, tables, or slow convergence keep the original choice.
fn encoded_layout_size(module: &Module, order: &[BlockId], target: &Target) -> Option<usize> {
    // Besides addressing data, its final fallthrough guard depends on the last emitted opcode.
    // Leave both that guard and data offsets with the assembler.
    if !module.data.is_empty() {
        return None;
    }
    let mut fixed = index_vec![0usize; module.blocks.len()];
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
    let mut widths = index_vec![target.encoded_push(U256::ZERO).bytes; module.blocks.len()];
    let mut sizes = IndexVec::new();
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

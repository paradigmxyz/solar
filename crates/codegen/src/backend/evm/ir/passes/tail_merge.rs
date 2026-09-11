//! Merge profitable suffixes of machine-level terminal blocks.
//!
//! The pass groups blocks by their machine terminator and indexes representative
//! tails in reverse. This finds each block's longest shared suffix without
//! comparing it with every earlier block. It then splits profitable suffixes
//! into shared tail blocks until no new merges remain. Each candidate includes
//! the cost of its new jumps and labels, and the pass keeps address-taken or
//! otherwise incompatible entries separate. Debug metadata never participates
//! in equivalence: path-specific function activations stay on the original
//! blocks or the jumps that replace their instruction suffixes. Those jumps also
//! retain the suffix's entry location before its origins are merged, so single-origin
//! source maps do not lose both callers' locations on a shared body.
//! In gas mode, a hot shared suffix must also repay its added transfer over the
//! requested optimizer run count with deposited-byte savings. Cold suffixes
//! retain the static size decision.
//!
//! A shared tail starts at a block boundary, so both the merged block and the representative may
//! only be cut where `keep_with_next` allows a split. That keeps sequences whose intervening gas
//! is observable, such as a pre-EIP-150 call's `GAS`-relative gas reserve, in one block.
//!
//! Splitting between a pushed label and its branch must preserve the label's control-only
//! identity. Once it finds a profitable merge, the pass records opaque label uses and marks safe
//! branch continuations before separating the push from its consumer. Subsequent CFG cleanup can
//! still redirect those addresses through jump thunks, while numerically observed labels remain
//! distinct.
//!
//! Gas mode keeps a short word loop's branch in its original block. Such a branch targets a
//! latch of at most 24 pure word/stack instructions, with a three- or four-word input, that jumps
//! straight back to the branch's block. A shared suffix must begin after its conditional branch:
//! the continuing path avoids an extra jump, while the exiting path may still share the terminal
//! suffix. This bounded frequency heuristic is independent of optional loop markers. Simpler
//! layouts, larger loops, stack-only latches and memory/call bodies retain the existing sharing
//! policy; broadly preventing their tail merges increased corpus bytecode size. Size mode may
//! share across the branch as before.

use super::{
    EvmPass,
    cfg_simplify::is_direct_jump_label,
    utils::{
        FreshLabels, MachineInstKey, instruction_size_lower_bound, is_split_point,
        is_terminal_boundary,
    },
};
use crate::{
    backend::evm::{
        ir::{Block, BlockId, Hotness, Instruction, Metadata, Module, Terminator, TerminatorKind},
        op::{self, StackOp, push_len},
    },
    target::Target,
};
use smallvec::SmallVec;
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_sema::Gcx;

pub(super) struct TailMerge;

impl EvmPass for TailMerge {
    fn name(&self) -> &'static str {
        "tail-merge"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        merge_tails(gcx, module)
    }
}

fn merge_tails(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let mut state = RunState::default();
    state.plan_merges(gcx, module);
    if state.merges.is_empty() {
        return false;
    }
    let mut labels = FreshLabels::new(module);
    let mut changed = false;
    loop {
        if !state.apply_merges(module, &mut labels) {
            return changed;
        }
        changed = true;
        state.plan_merges(gcx, module);
        if state.merges.is_empty() {
            return true;
        }
    }
}

#[derive(Default)]
struct RunState {
    opaque_labels: FxHashSet<BlockId>,
    merges: Vec<Merge>,
    group_indices: FxHashMap<BlockId, usize>,
    groups: Vec<MergeGroup>,
    commons: Vec<usize>,
    tails: Vec<(usize, BlockId)>,
    tail_roots: FxHashMap<TerminatorKind, usize>,
    tail_nodes: Vec<TailNode>,
    tail_node_pool: Vec<TailNode>,
}

impl RunState {
    fn plan_merges(&mut self, gcx: Gcx<'_>, module: &Module) {
        self.merges.clear();
        self.opaque_labels.clear();
        self.tail_roots.clear();
        self.tail_node_pool.append(&mut self.tail_nodes);
        for (block_id, block) in module.blocks.iter_enumerated() {
            if !is_candidate(block) {
                continue;
            }
            // A shared tail is reached by a jump every time it runs. In gas mode a loop block
            // keeps its own copy: the bytes saved never pay back a jump per iteration.
            if gcx.sess.opts.optimization.is_gas() && block.metadata.in_loop {
                continue;
            }

            let keep_branches =
                gcx.sess.opts.optimization.is_gas() && has_short_word_backedge(module, block_id);
            let matched = self.longest_common_tail(block, keep_branches);

            let target = Target::new(gcx);
            let transfer_bytes = (target.opcode(op::PUSH2).bytes
                + target.opcode(op::JUMP).bytes
                + target.opcode(op::JUMPDEST).bytes) as usize;
            let transfer_gas = target.opcode_gas(op::PUSH2)
                + target.opcode_gas(op::JUMP)
                + target.opcode_gas(op::JUMPDEST);
            if let Some((representative, common)) = matched
                && common > 0
                && {
                    let hot = !block.metadata.hotness.is_cold()
                        || !module.blocks[representative].metadata.hotness.is_cold();
                    let suffix_size = suffix_size(gcx, module, block_id, common);
                    let saved_bytes = suffix_size.saturating_sub(transfer_bytes);
                    suffix_size > transfer_bytes
                        && (!gcx.sess.opts.optimization.is_gas()
                            || !hot
                            || tail_merge_improves_lifetime(
                                saved_bytes,
                                transfer_gas,
                                target.expected_executions(),
                            ))
                }
            {
                self.merges.push(Merge { representative, block: block_id, common });
            } else {
                self.insert_tail(block_id, block, keep_branches);
            }
        }
        if !self.merges.is_empty() {
            for block in &module.blocks {
                for (at, inst) in block.instructions.iter().enumerate() {
                    if let Some(target) = inst.pushed_block()
                        && !module.blocks[target].metadata.is_continuation
                        && !is_direct_jump_label(block, at)
                    {
                        self.opaque_labels.insert(target);
                    }
                }
            }
        }
    }

    fn longest_common_tail(&self, block: &Block, keep_branches: bool) -> Option<(BlockId, usize)> {
        let terminator = &block.terminator.as_ref()?.kind;
        let mut node = *self.tail_roots.get(terminator)?;
        let mut matched = None;
        let len = block.instructions.len();
        for (common, inst) in block.instructions.iter().rev().enumerate() {
            if keep_branches && inst.as_evm_opcode() == Some(op::JUMPI) {
                break;
            }
            let key = MachineInstKey::new(inst);
            let Some(child) = self.tail_nodes[node]
                .children
                .iter()
                .find_map(|&(known, child)| (known == key).then_some(child))
            else {
                break;
            };
            node = child;
            // Splitting the tail off leaves a jump at this boundary, so only offer tails that
            // start at a legal split point. A longer tail may still start at one.
            if !is_split_point(&block.instructions, len - common - 1) {
                continue;
            }
            if let Some(representative) = self.tail_nodes[node].representative {
                matched = Some((representative, common + 1));
            }
        }
        matched
    }

    fn insert_tail(&mut self, block_id: BlockId, block: &Block, keep_branches: bool) {
        let terminator = &block.terminator.as_ref().expect("candidate must have a terminator").kind;
        let mut node = self.tail_root(terminator);
        let len = block.instructions.len();
        // The representative is truncated at the shared tail too, so it only represents tails
        // whose start is a legal split point in its own instruction list.
        for common in 0..=len {
            if common > 0 {
                if keep_branches
                    && block.instructions[len - common].as_evm_opcode() == Some(op::JUMPI)
                {
                    break;
                }
                node =
                    self.tail_child(node, MachineInstKey::new(&block.instructions[len - common]));
            }
            if is_split_point(&block.instructions, len - common) {
                self.tail_nodes[node].representative.get_or_insert(block_id);
            }
        }
    }

    fn tail_root(&mut self, terminator: &TerminatorKind) -> usize {
        if let Some(&root) = self.tail_roots.get(terminator) {
            return root;
        }
        let root = self.new_tail_node();
        self.tail_roots.insert(terminator.clone(), root);
        root
    }

    fn tail_child(&mut self, node: usize, key: MachineInstKey) -> usize {
        if let Some(child) = self.tail_nodes[node]
            .children
            .iter()
            .find_map(|&(known, child)| (known == key).then_some(child))
        {
            return child;
        }
        let child = self.new_tail_node();
        self.tail_nodes[node].children.push((key, child));
        child
    }

    fn new_tail_node(&mut self) -> usize {
        let node = self.tail_nodes.len();
        let mut tail = self.tail_node_pool.pop().unwrap_or_default();
        tail.clear();
        self.tail_nodes.push(tail);
        node
    }

    fn apply_merges(&mut self, module: &mut Module, labels: &mut FreshLabels) -> bool {
        let track_debug_info = module.debug_info_is_tracked();
        self.group_indices.clear();
        let mut group_count = 0;
        for &merge in &self.merges {
            let index = if let Some(&index) = self.group_indices.get(&merge.representative) {
                index
            } else {
                let index = group_count;
                group_count += 1;
                if let Some(group) = self.groups.get_mut(index) {
                    group.representative = merge.representative;
                    group.sites.clear();
                } else {
                    self.groups.push(MergeGroup {
                        representative: merge.representative,
                        sites: Vec::new(),
                    });
                }
                self.group_indices.insert(merge.representative, index);
                index
            };
            self.groups[index].sites.push((merge.block, merge.common));
        }

        let Self { groups, commons, tails, opaque_labels, .. } = self;
        let mut label_count = 0;
        for group in groups.iter().take(group_count) {
            commons.clear();
            commons.extend(group.sites.iter().map(|&(_, common)| common));
            commons.sort_unstable();
            commons.dedup();
            label_count += commons.len();
        }
        let Some(labels) = labels.take(label_count) else { return false };
        let mut labels = labels.into_iter();
        for group in groups.iter().take(group_count) {
            let representative = &module.blocks[group.representative];
            let instructions = representative.instructions.clone();
            let terminator = representative.terminator.clone();
            let metadata = representative.metadata;
            let max_hot_common = group
                .sites
                .iter()
                .filter(|&&(block, _)| !module.blocks[block].metadata.hotness.is_cold())
                .map(|&(_, common)| common)
                .max();
            commons.clear();
            commons.extend(group.sites.iter().map(|&(_, common)| common));
            commons.sort_unstable();
            commons.dedup();

            tails.clear();
            let mut previous_common = 0;
            let mut previous_tail = None;
            for &common in commons.iter() {
                preserve_split_control_target(
                    module,
                    group.representative,
                    instructions.len() - common,
                    opaque_labels,
                );
                let mut tail = Block::new(labels.next().expect("reserved one label per tail"));
                tail.metadata.hotness = metadata.hotness;
                tail.metadata.in_loop = metadata.in_loop
                    || group.sites.iter().any(|&(site, site_common)| {
                        site_common >= common && module.blocks[site].metadata.in_loop
                    });
                if !metadata.hotness.is_cold()
                    || max_hot_common.is_some_and(|hot_common| common <= hot_common)
                {
                    tail.metadata.hotness = Hotness::Hot;
                }
                tail.instructions = instructions
                    [instructions.len() - common..instructions.len() - previous_common]
                    .to_vec();
                if track_debug_info {
                    for instruction in &mut tail.instructions {
                        instruction.metadata.take_function_invoke();
                    }
                    for &(site, site_common) in &group.sites {
                        if site_common < common {
                            continue;
                        }
                        let site_instructions = &module.blocks[site].instructions;
                        let site_segment = &site_instructions[site_instructions.len() - common
                            ..site_instructions.len() - previous_common];
                        for (instruction, site_instruction) in
                            tail.instructions.iter_mut().zip(site_segment)
                        {
                            instruction.metadata.merge_source_spans(&site_instruction.metadata);
                        }
                    }
                }
                tail.terminator = previous_tail.map_or_else(
                    || terminator.clone(),
                    |target| {
                        Some(
                            Terminator::new(TerminatorKind::Jump(target)).with_debug_info_dropped(),
                        )
                    },
                );
                if track_debug_info
                    && previous_tail.is_none()
                    && let Some(tail_terminator) = &mut tail.terminator
                {
                    for &(site, site_common) in &group.sites {
                        if site_common >= common
                            && let Some(site_terminator) = &module.blocks[site].terminator
                        {
                            tail_terminator.metadata.merge_source_spans(&site_terminator.metadata);
                        }
                    }
                }
                let tail = module.add_block(tail);
                tails.push((common, tail));
                previous_common = common;
                previous_tail = Some(tail);
            }

            let &(max_common, max_tail) = tails.last().expect("merge group must have a tail");
            let representative_debug = track_debug_info
                .then(|| suffix_debug_info(&module.blocks[group.representative], max_common));
            // prefix; suffix !metadata(origin) => prefix; jump tail !metadata(origin)
            module.blocks[group.representative]
                .instructions
                .truncate(instructions.len() - max_common);
            let mut terminator =
                Terminator::new(TerminatorKind::Jump(max_tail)).with_debug_info_dropped();
            if let Some(representative_debug) = representative_debug {
                terminator.metadata.copy_debug_info_from(&representative_debug);
            }
            module.blocks[group.representative].terminator = Some(terminator);
            for &(block, common) in &group.sites {
                let tail = tails
                    .binary_search_by_key(&common, |&(known, _)| known)
                    .map(|index| tails[index].1)
                    .expect("tail must exist for every merge site");
                let len = module.blocks[block].instructions.len();
                preserve_split_control_target(module, block, len - common, opaque_labels);
                let debug_info =
                    track_debug_info.then(|| suffix_debug_info(&module.blocks[block], common));
                // prefix; suffix !metadata(origin) => prefix; jump tail !metadata(origin)
                module.blocks[block].instructions.truncate(len - common);
                let mut terminator =
                    Terminator::new(TerminatorKind::Jump(tail)).with_debug_info_dropped();
                if let Some(debug_info) = debug_info {
                    terminator.metadata.copy_debug_info_from(&debug_info);
                }
                module.blocks[block].terminator = Some(terminator);
            }
        }
        debug_assert!(labels.next().is_none());
        true
    }
}

/// Whether one hot transfer into a shared tail repays its deposited-byte saving.
const fn tail_merge_improves_lifetime(
    saved_bytes: usize,
    transfer_gas: u32,
    expected_executions: u64,
) -> bool {
    saved_bytes as u128 * Target::CODE_DEPOSIT_GAS_PER_BYTE as u128
        > transfer_gas as u128 * expected_executions as u128
}

/// Preserve an address's control-only identity when its consumer moves into a shared tail.
fn preserve_split_control_target(
    module: &mut Module,
    block: BlockId,
    split: usize,
    opaque_labels: &FxHashSet<BlockId>,
) {
    if let Some(previous) = split.checked_sub(1)
        && is_direct_jump_label(&module.blocks[block], previous)
        && let Some(target) = module.blocks[block].instructions[previous].pushed_block()
        && !opaque_labels.contains(&target)
    {
        // push target; jumpi -> push target; jump shared; shared: jumpi
        module.blocks[target].metadata.is_continuation = true;
    }
}

/// Recognizes a small recurring word computation whose branch must stay on the local path.
fn has_short_word_backedge(module: &Module, header: BlockId) -> bool {
    module.blocks[header].instructions.windows(2).any(|pair| {
        pair[1].as_evm_opcode() == Some(op::JUMPI)
            && pair[0].pushed_block().is_some_and(|target| {
                let latch = &module.blocks[target];
                matches!(latch.terminator.as_ref().map(|term| &term.kind),
                    Some(TerminatorKind::Jump(back)) if *back == header)
                    && latch.instructions.len() <= 24
                    && word_loop_input_width(&latch.instructions)
                        .is_some_and(|width| (3..=4).contains(&width))
            })
    })
}

/// Computes the required input prefix for a pure latch containing a word computation.
fn word_loop_input_width(instructions: &[Instruction]) -> Option<isize> {
    let mut depth = 0;
    let mut required = 0;
    let mut computes_word = false;
    for inst in instructions {
        if !inst.has_canonical_stack_effect() {
            return None;
        }
        let (inputs, growth) = if let Some(stack) = inst.as_stack_op() {
            (stack.required_depth() as isize, stack.net_growth())
        } else if inst.is_encoded_push() {
            (0, 1)
        } else if inst.as_evm_opcode().is_some_and(op::is_pure) {
            computes_word = true;
            let effect = inst.effective_stack_effect()?;
            (isize::from(effect.inputs), isize::from(effect.outputs) - isize::from(effect.inputs))
        } else {
            return None;
        };
        required = required.max(inputs - depth);
        depth += growth;
    }
    computes_word.then_some(required)
}

fn suffix_debug_info(block: &Block, len: usize) -> Metadata {
    let suffix = &block.instructions[block.instructions.len() - len..];
    let mut metadata = Metadata::default();
    metadata.mark_debug_info_dropped();
    if let Some(origin) = suffix
        .iter()
        .map(|inst| &inst.metadata)
        .chain(block.terminator.iter().map(|term| &term.metadata))
        // Already-shared suffixes cannot provide a path-specific jump origin.
        // Prefer a unique origin from this site; otherwise leave it unmapped.
        .find(|metadata| metadata.source_spans().len() == 1)
    {
        metadata.copy_source_debug_from(origin);
    }
    let mut functions =
        suffix.iter().filter_map(|instruction| instruction.metadata.function_invoke());
    let function = functions.next();
    debug_assert!(functions.all(|other| Some(other) == function));
    if let Some(function) = function {
        metadata.set_function_invoke(function);
    }
    metadata
}

fn is_candidate(block: &Block) -> bool {
    block.terminator.as_ref().is_some_and(|term| {
        is_terminal_boundary(&term.kind) || matches!(term.kind, TerminatorKind::Jump(_))
    })
}

fn suffix_size(gcx: Gcx<'_>, module: &Module, block_id: BlockId, common: usize) -> usize {
    let block = &module.blocks[block_id];
    let terminator = &block.terminator.as_ref().expect("candidate must have a terminator").kind;
    terminator_lower_bound(gcx, module, block_id, terminator)
        + block.instructions[block.instructions.len() - common..]
            .iter()
            .map(|inst| match inst.as_stack_op() {
                Some(StackOp::Exchange(_, ..=16)) => 3,
                _ => instruction_size_lower_bound(gcx, inst),
            })
            .sum::<usize>()
}

fn terminator_lower_bound(
    gcx: Gcx<'_>,
    module: &Module,
    block_id: BlockId,
    kind: &TerminatorKind,
) -> usize {
    let TerminatorKind::Jump(target) = kind else { return 1 };
    let next = block_id
        .index()
        .checked_add(1)
        .filter(|&index| index < module.blocks.len())
        .map(BlockId::from_usize);
    if Some(*target) == next {
        0
    } else {
        push_len(gcx.sess.opts.evm_version, alloy_primitives::U256::ZERO) + 1
    }
}

#[derive(Clone, Copy)]
struct Merge {
    representative: BlockId,
    block: BlockId,
    common: usize,
}

struct MergeGroup {
    representative: BlockId,
    sites: Vec<(BlockId, usize)>,
}

#[derive(Default)]
struct TailNode {
    children: SmallVec<[(MachineInstKey, usize); 1]>,
    representative: Option<BlockId>,
}

impl TailNode {
    fn clear(&mut self) {
        self.children.clear();
        self.representative = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hot_tail_lifetime_profitability() {
        assert!(tail_merge_improves_lifetime(20, 12, 200));
        assert!(!tail_merge_improves_lifetime(12, 12, 200));
        assert!(!tail_merge_improves_lifetime(20, 12, 1_000_000));
    }
}

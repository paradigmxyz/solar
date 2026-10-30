//! Merge profitable suffixes of machine-level terminal blocks.
//!
//! The pass groups blocks by their machine terminator and indexes representative
//! tails in reverse. This finds each block's longest shared suffix without
//! comparing it with every earlier block. A single edge map indexes `(node, instruction)` pairs,
//! so linear tails do not allocate a hash table for each instruction. It then splits profitable
//! suffixes into shared tail blocks until no new merges remain. Each candidate includes
//! the cost of its new jumps and labels, and the pass keeps address-taken or
//! otherwise incompatible entries separate. Debug metadata never participates
//! in equivalence or profitability. Path-specific function activations stay on the original
//! blocks or replacement jumps where representable; shared entries drop ambiguous events.
//! A terminal suffix covering the representative's whole body reuses its block and label when
//! there are no nested shared tails. Nested tails keep their placement
//! to preserve fallthrough paths. Other address-taken entries keep their jump stubs.
//! Gas mode indexes non-loop tails first, so loop paths can reuse them regardless of block order,
//! and loop-only paths do not create sharing groups.
//! Replacement jumps retain the suffix's entry location before its origins are merged, so
//! single-origin source maps do not lose both callers' locations on a shared body.
//! In gas mode, hot sharing must repay its transfer over the optimizer run count.
//! Size mode prices the jump that a shared suffix removes with at least a `PUSH1` label unless it
//! targets the entry block, the only block that can start at offset 0.
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
//! policy; broadly preventing their tail merges increased corpus bytecode size.
//!
//! Size mode keeps the branch of every block marked `in_loop` in its original block. A loop
//! header stays `cond; push body; jumpi; jump exit_tail` and is never `cond; push body; push
//! shared; jump` into a shared `jumpi`. evm2's JIT turns that shared block into one switch on a
//! jump target it reloads from the in-memory EVM stack. LLVM cannot remove that store and load, so
//! every loop that uses the block goes through the switch and keeps its 256-bit values in memory.
//! On evm2 against the better of solc 0.8.38 via-IR and legacy at one optimizer run, the
//! seeded-words and verified-words benchmark cases were +51% and +54% in the JIT with the shared
//! block and are -6% and -5% without it. The interpreter gains too, from -14% and -31% to -22% and
//! -38%. The benchmark's runtime bytes grow by 0.08% and its execution gas falls by 0.15%. Exit
//! tails after the branch are still shared, and blocks outside loops share across their branches.

use super::{
    EvmPass,
    cfg_simplify::is_direct_jump_label,
    utils::{
        FreshLabels, MachineInstKey, instruction_size_lower_bound, is_split_point,
        is_terminal_boundary,
    },
};
use crate::backend::evm::{
    ir::{
        Block, BlockId, BlockTarget, Hotness, Instruction, Metadata, Module, ModuleTarget,
        Terminator, TerminatorKind,
    },
    op::{self, StackOp, push_len},
};
use solar_data_structures::{
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
    newtype_index,
};
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
    while !state.merges.is_empty() && state.apply_merges(module, &mut labels) {
        changed = true;
        state.plan_merges(gcx, module);
    }
    changed
}

#[derive(Default)]
struct RunState {
    opaque_labels: FxHashSet<BlockId>,
    merges: Vec<Merge>,
    group_indices: FxHashMap<BlockId, usize>,
    commons: Vec<usize>,
    tails: Vec<BlockId>,
    tail_roots: FxHashMap<TerminatorKind, TailNode>,
    tail_edges: FxHashMap<(TailNode, MachineInstKey), TailNode>,
    tail_representatives: IndexVec<TailNode, Option<BlockId>>,
    /// The single inserted block continuing below a node whose chain is not built yet.
    tail_lazy: IndexVec<TailNode, Option<LazyTail>>,
}

newtype_index! {
    /// A node of the reversed-suffix trie.
    struct TailNode;
}

/// An unmaterialized trie chain: only `block` continues below the node, down to depth `limit`.
///
/// The chain's nodes are built only when another block shares a suffix with it; until then,
/// queries compare against `block` directly. Every node on the chain has `block` as its
/// representative exactly where `block` can be split, as if it had been inserted eagerly.
#[derive(Clone, Copy)]
struct LazyTail {
    block: BlockId,
    limit: usize,
}

impl RunState {
    fn plan_merges(&mut self, gcx: Gcx<'_>, module: &Module) {
        self.merges.clear();
        self.opaque_labels.clear();
        self.tail_roots.clear();
        self.tail_edges.clear();
        self.tail_representatives.clear();
        self.tail_lazy.clear();
        let gas = module.optimization.is_gas();
        let in_gas_loop = |block: &Block| gas && block.metadata.in_loop;
        let blocks = module.blocks.iter_enumerated();
        for (block_id, block) in blocks
            .clone()
            .filter(|(_, block)| !in_gas_loop(block))
            .chain(blocks.filter(|(_, block)| in_gas_loop(block)))
        {
            if !is_candidate(block) {
                continue;
            }
            let keep_branches = if gas {
                has_short_word_backedge(module, block_id)
            } else {
                block.metadata.in_loop
            };
            if let Some((representative, common)) =
                self.longest_common_tail(module, block, keep_branches)
                && is_profitable(gcx, module, block_id, representative, common)
            {
                self.merges.push(Merge { representative, block: block_id, common });
            } else if !in_gas_loop(block) {
                // Do not use loop bodies to seed sharing groups. They may reuse a tail
                // from a non-loop path to preserve common loop entries.
                self.insert_tail(module, block_id, block, keep_branches);
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

    fn longest_common_tail(
        &self,
        module: &Module,
        block: &Block,
        keep_branches: bool,
    ) -> Option<(BlockId, usize)> {
        let terminator = &block.terminator.as_ref()?.kind;
        let mut node = *self.tail_roots.get(terminator)?;
        let mut lazy = None;
        let mut matched = None;
        let len = block.instructions.len();
        for (common, inst) in block.instructions.iter().rev().enumerate() {
            if keep_branches && inst.as_evm_opcode() == Some(op::JUMPI) {
                break;
            }
            let representative = if lazy.is_none()
                && let Some(&child) = self.tail_edges.get(&(node, MachineInstKey::new(inst)))
            {
                node = child;
                self.tail_representatives[node]
            } else if let Some(tail) = lazy.or(self.tail_lazy[node]) {
                // Below a lazy node, only its owner's own instructions continue the chain.
                lazy = Some(tail);
                if common >= tail.limit {
                    break;
                }
                let owner = &module.blocks[tail.block].instructions;
                let at = owner.len() - common - 1;
                if MachineInstKey::new(&owner[at]) != MachineInstKey::new(inst) {
                    break;
                }
                is_split_point(owner, at).then_some(tail.block)
            } else {
                break;
            };
            // Splitting the tail off leaves a jump at this boundary, so only offer tails that
            // start at a legal split point. A longer tail may still start at one.
            if !is_split_point(&block.instructions, len - common - 1) {
                continue;
            }
            if let Some(representative) = representative {
                matched = Some((representative, common + 1));
            }
        }
        matched
    }

    fn insert_tail(
        &mut self,
        module: &Module,
        block_id: BlockId,
        block: &Block,
        keep_branches: bool,
    ) {
        let terminator = &block.terminator.as_ref().expect("candidate must have a terminator").kind;
        let mut node = self.tail_root(terminator);
        let len = block.instructions.len();
        let limit = if keep_branches {
            block
                .instructions
                .iter()
                .rev()
                .position(|inst| inst.as_evm_opcode() == Some(op::JUMPI))
                .unwrap_or(len)
        } else {
            len
        };
        // The representative is truncated at the shared tail too, so it only represents tails
        // whose start is a legal split point in its own instruction list.
        if is_split_point(&block.instructions, len) {
            self.tail_representatives[node].get_or_insert(block_id);
        }
        for common in 0..limit {
            self.expand_lazy_tail(module, node, common);
            let key = MachineInstKey::new(&block.instructions[len - common - 1]);
            let split = is_split_point(&block.instructions, len - common - 1);
            if let Some(&child) = self.tail_edges.get(&(node, key)) {
                node = child;
                if split {
                    self.tail_representatives[node].get_or_insert(block_id);
                }
                continue;
            }
            // No other block shares this suffix: leave the rest of the chain lazy.
            let child = self.new_tail_node();
            self.tail_edges.insert((node, key), child);
            self.tail_representatives[child] = split.then_some(block_id);
            self.tail_lazy[child] =
                (common + 1 < limit).then_some(LazyTail { block: block_id, limit });
            return;
        }
    }

    /// Builds the next node of a lazy chain at `node`, which is at depth `depth`.
    fn expand_lazy_tail(&mut self, module: &Module, node: TailNode, depth: usize) {
        let Some(tail) = self.tail_lazy[node].take() else { return };
        let owner = &module.blocks[tail.block].instructions;
        let at = owner.len() - depth - 1;
        let child = self.new_tail_node();
        self.tail_edges.insert((node, MachineInstKey::new(&owner[at])), child);
        self.tail_representatives[child] = is_split_point(owner, at).then_some(tail.block);
        self.tail_lazy[child] = (depth + 1 < tail.limit).then_some(tail);
    }

    fn tail_root(&mut self, terminator: &TerminatorKind) -> TailNode {
        if let Some(&root) = self.tail_roots.get(terminator) {
            return root;
        }
        let root = self.new_tail_node();
        self.tail_roots.insert(terminator.clone(), root);
        root
    }

    fn new_tail_node(&mut self) -> TailNode {
        self.tail_lazy.push(None);
        self.tail_representatives.push(None)
    }

    fn apply_merges(&mut self, module: &mut Module, labels: &mut FreshLabels) -> bool {
        let track_debug_info = module.debug_info_is_tracked();
        // Group the sites of each representative, in order of first appearance.
        self.group_indices.clear();
        for merge in &self.merges {
            let next = self.group_indices.len();
            self.group_indices.entry(merge.representative).or_insert(next);
        }
        let Self { merges, group_indices, commons, tails, opaque_labels, .. } = self;
        merges.sort_by_key(|merge| group_indices[&merge.representative]);
        let label_count = merges
            .chunk_by(|a, b| a.representative == b.representative)
            .map(|sites| {
                collect_commons(sites, commons);
                commons.len()
            })
            .sum();
        let Some(labels) = labels.take(label_count) else { return false };
        let mut labels = labels.into_iter();
        for sites in merges.chunk_by(|a, b| a.representative == b.representative) {
            let representative_id = sites[0].representative;
            collect_commons(sites, commons);
            let &max_common = commons.last().expect("merge group must have a site");
            let representative = &module.blocks[representative_id];
            let len = representative.instructions.len();
            let suffix = representative.instructions[len - max_common..].to_vec();
            let terminator = representative.terminator.clone();
            let metadata = representative.metadata;
            let max_hot_common = sites
                .iter()
                .filter(|site| !module.blocks[site.block].metadata.hotness.is_cold())
                .map(|site| site.common)
                .max();
            let any_site = |module: &Module, common: usize, flag: fn(&Block) -> bool| {
                sites.iter().any(|site| site.common >= common && flag(&module.blocks[site.block]))
            };

            tails.clear();
            let mut previous_common = 0;
            for &common in commons.iter() {
                preserve_split_control_target(
                    module,
                    representative_id,
                    len - common,
                    opaque_labels,
                );
                let mut tail = Block::new(labels.next().expect("reserved one label per tail"));
                tail.metadata.hotness = metadata.hotness;
                tail.metadata.in_loop =
                    metadata.in_loop || any_site(module, common, |block| block.metadata.in_loop);
                tail.metadata.repeats =
                    metadata.repeats || any_site(module, common, |block| block.metadata.repeats);
                if !metadata.hotness.is_cold()
                    || max_hot_common.is_some_and(|hot_common| common <= hot_common)
                {
                    tail.metadata.hotness = Hotness::Hot;
                }
                tail.instructions =
                    suffix[max_common - common..max_common - previous_common].to_vec();
                if track_debug_info {
                    for instruction in &mut tail.instructions {
                        instruction.metadata.take_function_invoke();
                    }
                    for site in sites.iter().filter(|site| site.common >= common) {
                        let site_instructions = &module.blocks[site.block].instructions;
                        let site_segment = &site_instructions[site_instructions.len() - common
                            ..site_instructions.len() - previous_common];
                        for (instruction, site_instruction) in
                            tail.instructions.iter_mut().zip(site_segment)
                        {
                            instruction.metadata.merge_source_spans(&site_instruction.metadata);
                        }
                    }
                }
                tail.terminator = tails.last().map_or_else(
                    || terminator.clone(),
                    |&target| {
                        Some(
                            Terminator::new(TerminatorKind::Jump(target)).with_debug_info_dropped(),
                        )
                    },
                );
                if track_debug_info
                    && tails.is_empty()
                    && let Some(tail_terminator) = &mut tail.terminator
                {
                    tail_terminator.metadata.take_function_invoke();
                    for site in sites.iter().filter(|site| site.common >= common) {
                        if let Some(site_terminator) = &module.blocks[site.block].terminator {
                            tail_terminator.metadata.merge_source_spans(&site_terminator.metadata);
                        }
                    }
                }
                // whole_body: suffix => whole_body: shared_suffix
                // Shared entries drop path-specific invocation events.
                let tail = if commons.len() == 1
                    && common == len
                    && terminator.as_ref().is_some_and(|term| is_terminal_boundary(&term.kind))
                {
                    tail.label = module.blocks[representative_id].label;
                    module.blocks[representative_id] = tail;
                    representative_id
                } else {
                    module.add_block(tail)
                };
                tails.push(tail);
                previous_common = common;
            }

            let &max_tail = tails.last().expect("merge group must have a tail");
            if max_tail != representative_id {
                let representative_debug =
                    suffix_debug_info(&module.blocks[representative_id], max_common);
                // prefix; suffix !metadata(origin) => prefix; jump tail !metadata(origin)
                module.blocks[representative_id].instructions.truncate(len - max_common);
                let mut terminator =
                    Terminator::new(TerminatorKind::Jump(max_tail)).with_debug_info_dropped();
                terminator.metadata.copy_debug_info_from(&representative_debug);
                module.blocks[representative_id].terminator = Some(terminator);
            }
            for site in sites {
                let tail = tails[commons
                    .binary_search(&site.common)
                    .expect("tail must exist for every merge site")];
                let len = module.blocks[site.block].instructions.len();
                preserve_split_control_target(module, site.block, len - site.common, opaque_labels);
                let debug_info = track_debug_info
                    .then(|| suffix_debug_info(&module.blocks[site.block], site.common));
                // prefix; suffix !metadata(origin) => prefix; jump tail !metadata(origin)
                module.blocks[site.block].instructions.truncate(len - site.common);
                let mut terminator =
                    Terminator::new(TerminatorKind::Jump(tail)).with_debug_info_dropped();
                if let Some(debug_info) = debug_info {
                    terminator.metadata.copy_debug_info_from(&debug_info);
                }
                module.blocks[site.block].terminator = Some(terminator);
            }
        }
        debug_assert!(labels.next().is_none());
        true
    }
}

/// Collects the distinct suffix lengths of a group's sites in ascending order.
fn collect_commons(sites: &[Merge], commons: &mut Vec<usize>) {
    commons.clear();
    commons.extend(sites.iter().map(|site| site.common));
    commons.sort_unstable();
    commons.dedup();
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
        let (inputs, growth) = if let Some(stack) = inst.as_stack_op() {
            (stack.required_depth() as isize, stack.net_growth())
        } else if inst.is_encoded_push() {
            (0, 1)
        } else if inst.as_evm_opcode().is_some_and(op::is_pure) {
            computes_word = true;
            let effect = inst.stack_effect();
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
    let all = suffix
        .iter()
        .map(|inst| &inst.metadata)
        .chain(block.terminator.iter().map(|term| &term.metadata));
    // Already-shared suffixes cannot provide a path-specific jump origin.
    // Prefer a unique origin from this site; otherwise leave it unmapped.
    if let Some(origin) = all.clone().find(|metadata| metadata.source_spans().len() == 1) {
        metadata.copy_source_debug_from(origin);
    }
    let mut functions = all.filter_map(Metadata::function_invoke);
    if let Some(function) = functions.next()
        && functions.all(|other| other == function)
    {
        metadata.set_function_invoke(function);
    }
    metadata
}

/// Whether sharing the `common`-instruction suffix of `block_id` with `representative` repays the
/// jump and label it adds.
fn is_profitable(
    gcx: Gcx<'_>,
    module: &Module,
    block_id: BlockId,
    representative: BlockId,
    common: usize,
) -> bool {
    let block = &module.blocks[block_id];
    let representative = &module.blocks[representative];
    // A share also runs the representative's code more than once when it may.
    let creation = module.code_follows && representative.runs_once();
    let target = block.target(gcx, module.optimization, creation);
    let transfer = target.opcode(op::PUSH2) + target.opcode(op::JUMP) + target.opcode(op::JUMPDEST);
    let transfer_bytes = transfer.bytes as usize;
    let suffix_size = suffix_size(gcx, module, block_id, block, common);
    suffix_size > transfer_bytes
        && (!module.optimization.is_gas()
            || (block.metadata.hotness.is_cold() && representative.metadata.hotness.is_cold())
            || target.deposit_repays_gas(
                (suffix_size - transfer_bytes) as u64,
                u64::from(target.runtime_gas(transfer)),
            ))
}

fn is_candidate(block: &Block) -> bool {
    block.terminator.as_ref().is_some_and(|term| {
        is_terminal_boundary(&term.kind) || matches!(term.kind, TerminatorKind::Jump(_))
    })
}

fn suffix_size(
    gcx: Gcx<'_>,
    module: &Module,
    block_id: BlockId,
    block: &Block,
    common: usize,
) -> usize {
    let terminator = &block.terminator.as_ref().expect("candidate must have a terminator").kind;
    let terminator_size = match *terminator {
        TerminatorKind::Jump(target) if target.index() == block_id.index() + 1 => 0,
        TerminatorKind::Jump(target) => label_push_lower_bound(gcx, module, target) + 1,
        _ => 1,
    };
    terminator_size
        + block.instructions[block.instructions.len() - common..]
            .iter()
            .map(|inst| match inst.as_stack_op() {
                Some(StackOp::Exchange(_, ..=16)) => 3,
                _ => instruction_size_lower_bound(module.runtime_target(gcx), inst),
            })
            .sum::<usize>()
}

/// Lower bound on the bytes of a push of `target`'s label. Layout keeps the entry block first, so
/// once it holds an instruction every other block starts past offset 0 and needs `PUSH1`. Gas mode
/// keeps the one-byte bound, leaving its lifetime pricing of hot tails unchanged.
fn label_push_lower_bound(gcx: Gcx<'_>, module: &Module, target: BlockId) -> usize {
    let entry = BlockId::from_usize(0);
    let past_entry = !module.optimization.is_gas()
        && target != entry
        && !module.blocks[entry].instructions.is_empty();
    push_len(gcx.sess.opts.evm_version, alloy_primitives::U256::from(u8::from(past_entry)))
}

#[derive(Clone, Copy)]
struct Merge {
    representative: BlockId,
    block: BlockId,
    common: usize,
}

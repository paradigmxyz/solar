//! Outline repeated closed computations and large immediate pushes.

use super::{
    EvmPass,
    utils::{FreshLabels, StackDepths, instruction_size_lower_bound},
};
use crate::backend::evm::{
    ir::{Block, BlockId, Instruction, Module, PushValue, Terminator, TerminatorKind},
    op, push_len,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::IndexVec,
    map::{FxHashMap, FxHasher},
};
use solar_sema::Gcx;
use std::hash::{Hash, Hasher};

pub(super) struct Outline;

impl EvmPass for Outline {
    fn name(&self) -> &'static str {
        "outline"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        outline(gcx, module)
    }
}

const MIN_MACHINE_RUN: usize = 4;

type BlockEdits = SmallVec<[(usize, usize, BlockId, u16); 1]>;
type OutlineEdits = FxHashMap<BlockId, BlockEdits>;

fn outline(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let mut state = RunState::default();
    outline_machine_runs(gcx, module, &mut state) | outline_repeated_pushes(gcx, module, &mut state)
}

fn outline_machine_runs(gcx: Gcx<'_>, module: &mut Module, state: &mut RunState) -> bool {
    // Enumerating every contiguous run is quadratic in a block's length, and
    // hashing each run's instructions made it cubic. Two exact filters keep it
    // in hand without changing which groups are found:
    //
    // - A run can only be outlined if it occurs at least twice, so every instruction in it occurs
    //   at least twice module-wide. Runs are cut at any instruction that does not, which ends them
    //   at the unique pushes that separate most straight-line code.
    // - A run's hash comes from a per-block prefix table, so it costs the same whatever the run's
    //   length. Equality still compares instructions, so the grouping is exactly as before.
    let hashes = InstHashes::new(module);

    let mut candidates = FxHashMap::<MachineInstSlice<'_>, SmallVec<[Site; 2]>>::default();
    for (block_id, block) in module.blocks.iter_enumerated() {
        for start in 0..block.instructions.len() {
            if !hashes.repeats(block_id, start) {
                continue;
            }
            let mut delta = 0i32;
            let mut inputs = 0i32;
            let mut peak = 0i32;
            let mut run_size = 0usize;
            for end in start..block.instructions.len() {
                let inst = &block.instructions[end];
                if !hashes.repeats(block_id, end) {
                    break;
                }
                let Some((reads, pops, pushes)) = whitelisted_effect(inst) else { break };
                run_size += instruction_size_lower_bound(gcx, inst);
                inputs = inputs.max(i32::from(reads) - delta);
                delta = delta - i32::from(pops) + i32::from(pushes);
                peak = peak.max(delta);
                let outputs = inputs + delta;
                if inputs != 0 && !gcx.sess.opts.optimization.is_size() {
                    break;
                }
                let len = end + 1 - start;
                let closed = inputs == 0 && matches!(outputs, 0 | 1);
                let open_size_run = gcx.sess.opts.optimization.is_size()
                    && (0..=16).contains(&inputs)
                    && (0..=16).contains(&outputs);
                // An outlined site costs at least seven bytes plus one stack shuffle per input,
                // before the shared stub's fixed overhead. A run no larger than that site can
                // never save bytes regardless of its occurrence count, so do not intern it.
                let can_amortize = run_size > 7 + inputs as usize;
                let added_peak = 2usize.max(1 + peak as usize);
                if len >= MIN_MACHINE_RUN && can_amortize && (closed || open_size_run) {
                    let key = MachineInstSlice {
                        hash: hashes.range(block_id, start, end),
                        insts: &block.instructions[start..=end],
                    };
                    candidates.entry(key).or_default().push(Site {
                        block: block_id,
                        start,
                        len,
                        inputs: inputs as u16,
                        outputs: outputs as u16,
                        added_peak,
                    });
                }
            }
        }
    }

    let mut groups: Vec<_> = candidates.into_iter().filter(|(_, sites)| sites.len() >= 2).collect();
    if groups.is_empty() {
        return false;
    }
    let Some(depths) = StackDepths::new(module) else { return false };
    groups.sort_unstable_by_key(|(key, sites)| {
        let first = sites[0];
        (std::cmp::Reverse(key.insts.len()), first.block.index(), first.start)
    });
    let mut claimed = FxHashMap::<BlockId, DenseBitSet<usize>>::default();
    let mut chosen = Vec::new();
    for (_, sites) in groups {
        let mut free = SmallVec::<[Site; 2]>::new();
        for site in sites {
            if !depths.has_headroom(site.block, site.start, site.added_peak) {
                continue;
            }
            let end = site.start + site.len;
            let instruction_count = module.blocks[site.block].instructions.len();
            let claimed = claimed
                .entry(site.block)
                .or_insert_with(|| DenseBitSet::new_empty(instruction_count));
            if claimed.contains_any(site.start..end) {
                continue;
            }
            // `claimed` only carries sites fixed by earlier groups; this group's sites are marked
            // below, after the profitability check, so it cannot catch two occurrences of the same
            // run that overlap each other. Periodic runs produce exactly that — window `[0, L)` and
            // window `[p, p + L)` are byte-identical when the period `p < L` divides the pattern,
            // so both land here. Two overlapping occurrences cannot both be replaced,
            // and the descending-order edit application would split the block at a
            // stale index (panicking in `split_off`), so keep every chosen site
            // pairwise disjoint.
            if free
                .iter()
                .any(|f| f.block == site.block && f.start < end && site.start < f.start + f.len)
            {
                continue;
            }
            free.push(site);
        }
        if free.len() < 2 {
            continue;
        }
        let first = free[0];
        let body =
            module.blocks[first.block].instructions[first.start..first.start + first.len].to_vec();
        let run_size = lower_bound(gcx, &body);
        let stub_size = 1 + run_size + usize::from(first.outputs) + 1;
        let site_size = (if free.len() >= 4 { 7 } else { 8 }) + usize::from(first.inputs);
        if free.len() * run_size < free.len() * site_size + stub_size + 2 {
            continue;
        }
        for site in &free {
            claimed
                .get_mut(&site.block)
                .expect("candidate block has a claimed-site set")
                .insert_range(site.start..site.start + site.len);
        }
        chosen.push(ChosenGroup {
            body,
            sites: free,
            inputs: first.inputs,
            outputs: first.outputs,
        });
    }
    if chosen.is_empty() {
        return false;
    }

    let outlined_sites = chosen.iter().map(|group| group.sites.len()).sum::<usize>();
    let Some(labels) = state.labels(module, chosen.len() + outlined_sites) else { return false };
    let mut labels = labels.into_iter();
    let removed_body_instructions =
        chosen.iter().map(|group| (group.sites.len() - 1) * group.body.len()).sum::<usize>();
    tracing::debug!(
        target: "solar::codegen::evm_ir::outline",
        outlined_groups = chosen.len(),
        outlined_sites,
        removed_body_instructions,
        "outlined repeated machine instruction runs"
    );

    let mut stubs = Vec::with_capacity(chosen.len());
    for group in &chosen {
        let mut stub = Block::new(labels.next().expect("reserved one label per outline stub"));
        stub.instructions = group.body.clone();
        for depth in 1..=group.outputs {
            stub.instructions.push(Instruction::opcode(op::swap(depth as u8)));
        }
        stub.terminator = Some(Terminator::new(TerminatorKind::Op(op::JUMP)));
        stubs.push(module.add_block(stub));
    }
    let mut edits = OutlineEdits::default();
    for (group, stub) in chosen.into_iter().zip(stubs) {
        for site in group.sites {
            edits.entry(site.block).or_default().push((site.start, site.len, stub, group.inputs));
        }
    }
    apply_outline_edits(module, edits, &mut labels);
    debug_assert!(labels.next().is_none());
    true
}

fn outline_repeated_pushes(gcx: Gcx<'_>, module: &mut Module, state: &mut RunState) -> bool {
    let mut sites = FxHashMap::<U256, SmallVec<[(BlockId, usize); 2]>>::default();
    for (block_id, block) in module.blocks.iter_enumerated() {
        for (index, inst) in block.instructions.iter().enumerate() {
            if inst.is_encoded_push()
                && inst.deferred_push().is_none()
                && inst.immutable_push().is_none()
                && let Some(PushValue::Immediate(value)) = &inst.value
            {
                sites.entry(*value).or_default().push((block_id, index));
            }
        }
    }

    const SITE_BYTES: usize = 8;
    const MIN_SAVING: usize = 8;
    let mut values: Vec<_> = sites
        .iter()
        .filter_map(|(&value, occurrences)| {
            let push_size = push_len(gcx.sess.opts.evm_version, value);
            let inline = occurrences.len() * push_size;
            let outlined = occurrences.len() * SITE_BYTES + push_size + 3;
            (occurrences.len() >= 2 && inline >= outlined + MIN_SAVING).then_some(value)
        })
        .collect();
    if values.is_empty() {
        return false;
    }
    let Some(depths) = StackDepths::new(module) else { return false };
    for occurrences in sites.values_mut() {
        occurrences.retain(|site| depths.has_headroom(site.0, site.1, 2));
    }
    values.retain(|value| {
        let occurrences = &sites[value];
        let push_size = push_len(gcx.sess.opts.evm_version, *value);
        let inline = occurrences.len() * push_size;
        let outlined = occurrences.len() * SITE_BYTES + push_size + 3;
        occurrences.len() >= 2 && inline >= outlined + MIN_SAVING
    });
    if values.is_empty() {
        return false;
    }
    values.sort_unstable();
    let site_count = values.iter().map(|value| sites[value].len()).sum::<usize>();
    let Some(labels) = state.labels(module, values.len() + site_count) else { return false };
    let mut labels = labels.into_iter();

    let mut edits = OutlineEdits::default();
    for value in values {
        let mut stub = Block::new(labels.next().expect("reserved one label per push stub"));
        stub.instructions.push(Instruction::push_value(value));
        stub.instructions.push(Instruction::opcode(op::SWAP1));
        stub.terminator = Some(Terminator::new(TerminatorKind::Op(op::JUMP)));
        let stub = module.add_block(stub);
        for &(block, index) in &sites[&value] {
            edits.entry(block).or_default().push((index, 1, stub, 0));
        }
    }
    apply_outline_edits(module, edits, &mut labels);
    debug_assert!(labels.next().is_none());
    true
}

fn apply_outline_edits(
    module: &mut Module,
    mut edits: OutlineEdits,
    labels: &mut impl Iterator<Item = u32>,
) {
    let mut blocks: Vec<_> = edits.keys().copied().collect();
    blocks.sort_unstable();
    for block in blocks {
        let block_edits = edits.get_mut(&block).expect("edit block came from the map");
        block_edits.sort_unstable_by_key(|(start, ..)| std::cmp::Reverse(*start));
        let mut next_start = module.blocks[block].instructions.len();
        for &(start, len, ..) in block_edits.iter() {
            let end = start.checked_add(len).expect("outline edit range overflow");
            assert!(len != 0 && end <= next_start, "outline edits overlap or exceed their block");
            next_start = start;
        }
        for &(start, len, stub, inputs) in block_edits.iter() {
            let label = labels.next().expect("reserved one label per outline site");
            split_outline_site(module, block, start, len, stub, inputs, label);
        }
    }
}

fn split_outline_site(
    module: &mut Module,
    block: BlockId,
    start: usize,
    len: usize,
    stub: BlockId,
    inputs: u16,
    continuation_label: u32,
) {
    let mut continuation = Block::new(continuation_label);
    continuation.metadata = module.blocks[block].metadata;
    continuation.instructions = module.blocks[block].instructions.split_off(start + len);
    module.blocks[block].instructions.truncate(start);
    continuation.terminator = module.blocks[block].terminator.take();
    let continuation = module.add_block(continuation);
    module.blocks[block].instructions.push(Instruction::push_block(continuation));
    for depth in (1..=inputs).rev() {
        module.blocks[block].instructions.push(Instruction::opcode(op::swap(depth as u8)));
    }
    module.blocks[block].terminator = Some(Terminator::new(TerminatorKind::Jump(stub)));
}

fn whitelisted_effect(inst: &Instruction) -> Option<(u16, u16, u16)> {
    if inst.is_encoded_push() {
        return Some((0, 0, 1));
    }
    Some(match inst.opcode {
        op::CALLDATASIZE | op::PUSH0 | op::RETURNDATASIZE | op::MSIZE | op::CALLVALUE => (0, 0, 1),
        op::ISZERO | op::NOT | op::CALLDATALOAD | op::MLOAD => (1, 1, 1),
        op::ADD
        | op::SUB
        | op::MUL
        | op::AND
        | op::OR
        | op::XOR
        | op::SHL
        | op::SHR
        | op::LT
        | op::GT
        | op::SLT
        | op::SGT
        | op::EQ
        | op::DIV => (2, 2, 1),
        op::MSTORE => (2, 2, 0),
        op::POP => (1, 1, 0),
        dup if (op::DUP1..=op::DUP16).contains(&dup) => (u16::from(dup - op::DUP1) + 1, 0, 1),
        swap if (op::SWAP1..=op::SWAP16).contains(&swap) => (u16::from(swap - op::SWAP1) + 2, 0, 0),
        _ => return None,
    })
}

fn lower_bound(gcx: Gcx<'_>, instructions: &[Instruction]) -> usize {
    instructions.iter().map(|inst| instruction_size_lower_bound(gcx, inst)).sum()
}

#[derive(Clone, Copy, Debug)]
struct Site {
    block: BlockId,
    start: usize,
    len: usize,
    inputs: u16,
    outputs: u16,
    added_peak: usize,
}

struct ChosenGroup {
    body: Vec<Instruction>,
    sites: SmallVec<[Site; 2]>,
    inputs: u16,
    outputs: u16,
}

/// The identity of an instruction for outlining: what a run must match on.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct InstKey(u8, u8, Option<PushValue>);

impl InstKey {
    fn new(inst: &Instruction) -> Self {
        Self(inst.opcode, inst.encoding, inst.value)
    }
}

/// Per-block instruction tables: whether each instruction occurs more than
/// once module-wide, and prefix hashes so any run hashes in constant time.
///
/// `prefix[i + 1] = prefix[i] * BASE + hash(inst[i])`, so the run `[start, end]`
/// hashes to `prefix[end + 1] - prefix[start] * BASE^len`. Equal instruction
/// sequences always produce equal hashes, which is all the map needs: equality
/// still compares the instructions themselves.
struct InstHashes {
    prefixes: IndexVec<BlockId, Vec<u64>>,
    repeats: IndexVec<BlockId, DenseBitSet<usize>>,
    powers: Vec<u64>,
}

impl InstHashes {
    const BASE: u64 = 0x100_0000_01b3;

    fn new(module: &Module) -> Self {
        let mut counts = FxHashMap::<InstKey, u32>::default();
        for block in module.blocks.iter() {
            for inst in &block.instructions {
                *counts.entry(InstKey::new(inst)).or_default() += 1;
            }
        }

        let mut longest = 0;
        let mut prefixes = IndexVec::with_capacity(module.blocks.len());
        let mut repeats = IndexVec::with_capacity(module.blocks.len());
        for block in module.blocks.iter() {
            longest = longest.max(block.instructions.len());
            let mut prefix = Vec::with_capacity(block.instructions.len() + 1);
            let mut repeated = DenseBitSet::new_empty(block.instructions.len());
            prefix.push(0u64);
            for (index, inst) in block.instructions.iter().enumerate() {
                let key = InstKey::new(inst);
                if counts.get(&key).copied().unwrap_or(0) >= 2 {
                    repeated.insert(index);
                }
                let mut hasher = FxHasher::default();
                key.hash(&mut hasher);
                let last = *prefix.last().expect("prefix starts with the empty run");
                prefix.push(last.wrapping_mul(Self::BASE).wrapping_add(hasher.finish()));
            }
            prefixes.push(prefix);
            repeats.push(repeated);
        }

        let mut powers = Vec::with_capacity(longest + 1);
        powers.push(1u64);
        for index in 0..longest {
            powers.push(powers[index].wrapping_mul(Self::BASE));
        }

        Self { prefixes, repeats, powers }
    }

    /// Whether this instruction occurs more than once in the module. A run that
    /// contains an instruction occurring exactly once can never occur twice, so
    /// it can never be outlined.
    fn repeats(&self, block: BlockId, index: usize) -> bool {
        self.repeats[block].contains(index)
    }

    fn range(&self, block: BlockId, start: usize, end: usize) -> u64 {
        let prefix = &self.prefixes[block];
        prefix[end + 1].wrapping_sub(prefix[start].wrapping_mul(self.powers[end + 1 - start]))
    }
}

#[derive(Clone, Copy)]
struct MachineInstSlice<'a> {
    hash: u64,
    insts: &'a [Instruction],
}

impl PartialEq for MachineInstSlice<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.insts.len() == other.insts.len()
            && self.insts.iter().zip(other.insts).all(|(a, b)| {
                a.opcode == b.opcode && a.encoding == b.encoding && a.value == b.value
            })
    }
}

impl Eq for MachineInstSlice<'_> {}

impl Hash for MachineInstSlice<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

#[derive(Default)]
struct RunState {
    labels: Option<FreshLabels>,
}

impl RunState {
    fn labels(&mut self, module: &Module, count: usize) -> Option<Vec<u32>> {
        let labels = self.labels.get_or_insert_with(|| FreshLabels::new(module));
        labels.take(count)
    }
}

//! Outline repeated closed computations and large immediate pushes.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Block, BlockId, Instruction, Module, PushValue, Terminator, TerminatorKind},
    op,
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

const MIN_CLOSED_RUN: usize = 4;

type BlockEdits = SmallVec<[(usize, usize, BlockId); 1]>;
type OutlineEdits = FxHashMap<BlockId, BlockEdits>;

fn outline(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let mut state = RunState::default();
    outline_closed_computations(module, &mut state)
        | outline_repeated_pushes(gcx, module, &mut state)
}

fn outline_closed_computations(module: &mut Module, state: &mut RunState) -> bool {
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
            let mut height = 0i32;
            for end in start..block.instructions.len() {
                let inst = &block.instructions[end];
                if !hashes.repeats(block_id, end) {
                    break;
                }
                let Some((reads, pops, pushes)) = whitelisted_effect(inst) else { break };
                if height < i32::from(reads) {
                    break;
                }
                height = height - i32::from(pops) + i32::from(pushes);
                let len = end + 1 - start;
                if len >= MIN_CLOSED_RUN && matches!(height, 0 | 1) {
                    let key = MachineInstSlice {
                        hash: hashes.range(block_id, start, end),
                        insts: &block.instructions[start..=end],
                    };
                    candidates.entry(key).or_default().push(Site {
                        block: block_id,
                        start,
                        len,
                        height: height as u16,
                    });
                }
            }
        }
    }

    let mut groups: Vec<_> = candidates.into_iter().filter(|(_, sites)| sites.len() >= 2).collect();
    groups.sort_unstable_by_key(|(key, sites)| {
        let first = sites[0];
        (std::cmp::Reverse(key.insts.len()), first.block.index(), first.start)
    });
    let mut claimed = FxHashMap::<BlockId, DenseBitSet<usize>>::default();
    let mut chosen = Vec::new();
    for (_, sites) in groups {
        let mut free = SmallVec::<[Site; 2]>::new();
        for site in sites {
            let instruction_count = module.blocks[site.block].instructions.len();
            let claimed = claimed
                .entry(site.block)
                .or_insert_with(|| DenseBitSet::new_empty(instruction_count));
            if !claimed.contains_any(site.start..site.start + site.len) {
                free.push(site);
            }
        }
        if free.len() < 2 {
            continue;
        }
        let first = free[0];
        let body =
            module.blocks[first.block].instructions[first.start..first.start + first.len].to_vec();
        let run_size = lower_bound(&body);
        let stub_size = 1 + run_size + usize::from(first.height) + 1;
        let site_size = if free.len() >= 4 { 7 } else { 8 };
        if free.len() * run_size < free.len() * site_size + stub_size + 2 {
            continue;
        }
        for site in &free {
            claimed
                .get_mut(&site.block)
                .expect("candidate block has a claimed-site set")
                .insert_range(site.start..site.start + site.len);
        }
        chosen.push(ChosenGroup { body, sites: free, height: first.height });
    }
    if chosen.is_empty() {
        return false;
    }

    let mut stubs = Vec::with_capacity(chosen.len());
    for group in &chosen {
        let mut stub = Block::new(state.next_label(module));
        stub.instructions = group.body.clone();
        if group.height == 1 {
            stub.instructions.push(Instruction::opcode(op::SWAP1));
        }
        stub.terminator = Some(Terminator::new(TerminatorKind::Op(op::JUMP)));
        stubs.push(module.add_block(stub));
    }
    let mut edits = OutlineEdits::default();
    for (group, stub) in chosen.into_iter().zip(stubs) {
        for site in group.sites {
            edits.entry(site.block).or_default().push((site.start, site.len, stub));
        }
    }
    apply_outline_edits(module, edits, state);
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
            let push_len = push_len(gcx, value);
            let inline = occurrences.len() * push_len;
            let outlined = occurrences.len() * SITE_BYTES + push_len + 3;
            (occurrences.len() >= 2 && inline >= outlined + MIN_SAVING).then_some(value)
        })
        .collect();
    if values.is_empty() {
        return false;
    }
    values.sort_unstable();

    let mut edits = OutlineEdits::default();
    for value in values {
        let mut stub = Block::new(state.next_label(module));
        stub.instructions.push(Instruction::push_value(value));
        stub.instructions.push(Instruction::opcode(op::SWAP1));
        stub.terminator = Some(Terminator::new(TerminatorKind::Op(op::JUMP)));
        let stub = module.add_block(stub);
        for &(block, index) in &sites[&value] {
            edits.entry(block).or_default().push((index, 1, stub));
        }
    }
    apply_outline_edits(module, edits, state);
    true
}

fn apply_outline_edits(module: &mut Module, mut edits: OutlineEdits, state: &mut RunState) {
    let mut blocks: Vec<_> = edits.keys().copied().collect();
    blocks.sort_unstable();
    for block in blocks {
        let block_edits = edits.get_mut(&block).expect("edit block came from the map");
        block_edits.sort_unstable_by_key(|(start, _, _)| std::cmp::Reverse(*start));
        for &(start, len, stub) in block_edits.iter() {
            split_outline_site(module, block, start, len, stub, state);
        }
    }
}

fn split_outline_site(
    module: &mut Module,
    block: BlockId,
    start: usize,
    len: usize,
    stub: BlockId,
    state: &mut RunState,
) {
    let mut continuation = Block::new(state.next_label(module));
    continuation.metadata = module.blocks[block].metadata;
    continuation.instructions = module.blocks[block].instructions.split_off(start + len);
    module.blocks[block].instructions.truncate(start);
    continuation.terminator = module.blocks[block].terminator.take();
    let continuation = module.add_block(continuation);
    module.blocks[block].instructions.push(Instruction::push_block(continuation));
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

fn lower_bound(instructions: &[Instruction]) -> usize {
    instructions.iter().map(|inst| if inst.is_encoded_push() { 2 } else { 1 }).sum()
}

fn push_len(gcx: Gcx<'_>, value: U256) -> usize {
    let width = value.byte_len();
    if width == 0 && !gcx.sess.opts.evm_version.has_push0() { 2 } else { width + 1 }
}

#[derive(Clone, Copy, Debug)]
struct Site {
    block: BlockId,
    start: usize,
    len: usize,
    height: u16,
}

struct ChosenGroup {
    body: Vec<Instruction>,
    sites: SmallVec<[Site; 2]>,
    height: u16,
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
    next_label: Option<u32>,
}

impl RunState {
    fn next_label(&mut self, module: &Module) -> u32 {
        let label = *self.next_label.get_or_insert_with(|| {
            module
                .blocks
                .iter()
                .map(|block| block.label)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .expect("EVM IR block label overflow")
        });
        self.next_label = Some(label.checked_add(1).expect("EVM IR block label overflow"));
        label
    }
}

//! Fuses a run of adjacent single-byte memory reads into one word read.
//!
//! Source that packs consecutive bytes out of a `bytes` value reads each byte
//! separately, extracts it, shifts it into place and ORs the parts together.
//! Every one of those reads already pulls a whole 32-byte word out of memory,
//! so after memory lowering the run's value is simply a prefix field of the
//! word at the lowest address:
//!
//! ```text
//! (mem[p] << 16) | (mem[p + 1] << 8) | mem[p + 2]  ==  mload(p) >> 232
//! ```
//!
//! The pass matches the OR tree at its root. Every leaf must be one byte of
//! one word read, `byte(0, mload(a))` or `mload(a) & 0xff`, the addresses must
//! share one base and cover a consecutive constant range, and each leaf's
//! accumulated shift must be the one its position in that range implies. The
//! root is then rewritten to a single shift of the lowest address's word and
//! the interior nodes become dead.
//!
//! # Safety
//!
//! Addresses are compared after flattening the wrapping `add` tree into its
//! non-constant addends and their constant sum, so `b + i`, `b + (i + 1)` and
//! `b + (i + 2)` are recognized as one base at offsets zero, one and two
//! without assuming anything about `b` or `i`.
//!
//! Every matched instruction must lie in the root's block with no
//! side-effecting instruction between the first read and the root, so no store
//! can fall between the reads the fused read replaces. The fused read is taken
//! at the lowest address of the run, and the original read at that same
//! address is reused rather than a new one being introduced, so the pass never
//! touches a byte of memory the original code did not already touch and never
//! grows memory further than it already grew. The result is a pure value
//! computation; the reads it removes observed no state the remaining read does
//! not observe.
//!
//! A run is at most one word long and every interior node must be used only by
//! the run, so the rewrite is unconditionally fewer instructions and smaller
//! code. It is not gated on the optimization objective.
//!
//! # Sharing a read between neighbours
//!
//! A run that is not packed into one value still reads the same word many
//! times: a decoder taking four bytes through a lookup table each iteration
//! loads four words to use four bytes of the first one. After the packing
//! rewrite above, every remaining group of byte extractions over one base
//! whose offsets fit in a word is pointed at the group's lowest read, and each
//! extraction takes the byte its offset names. The lowest read must come first
//! in the block, so the value it produces is available where the others stood.
//!
//! # Limitations
//!
//! A run must lie in one basic block. Reads whose bounds checks were not
//! eliminated sit in separate blocks and are left alone, so the pass fires on
//! loops and guarded bodies where the checks already folded, which is where the
//! packing it targets occurs. Extending it to a dominating chain of blocks
//! needs a store-free check over every path between them, not the single
//! backward scan a block gives.

use crate::mir::{
    EffectKind, Function, Immediate, InstId, InstKind, Module, Value, ValueId,
    pass::{MirPass, ModuleAnalyses, run_function_pass},
};
use alloy_primitives::U256;
use solar_data_structures::{index::IndexVec, map::FxHashSet};
use solar_sema::Gcx;

/// Bytes one word read can cover, so the longest run the pass will fuse.
const MAX_RUN: usize = 32;
/// Bound on the addends flattened out of one address expression.
const MAX_ADDENDS: usize = 8;
/// Bound on the OR-tree nodes visited while decomposing one root.
const MAX_NODES: usize = 96;

pub(crate) struct ByteRunLoads;

impl MirPass for ByteRunLoads {
    fn name(&self) -> &'static str {
        "byte-run-loads"
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        run_function_pass(module, analyses, |func, _| run_function(func))
    }
}

/// One leaf of a matched OR tree: a single byte of one word read.
struct Leaf {
    /// Bit positions the leaf is shifted left by before being ORed in.
    shift: u64,
    /// Non-constant addends of the read's address.
    base: Vec<ValueId>,
    /// Constant part of the read's address.
    offset: u64,
    /// Result of the `mload` the byte came from.
    word: ValueId,
}

fn run_function(func: &mut Function) -> bool {
    // A dead use of an extracted byte would hide the run behind a use count the
    // shape does not really have, so drop unused pure instructions first.
    sweep_dead(func);
    let uses = super::egraph::use_counts(func);
    let mut changed = false;
    for block in func.blocks.indices().collect::<Vec<_>>() {
        // A side-effecting instruction may store into the range being read, so
        // only instructions since the last one can take part in a run.
        let mut window = FxHashSet::default();
        for index in 0..func.blocks[block].instructions.len() {
            let inst = func.blocks[block].instructions[index];
            if func.inst(inst).kind.has_side_effects() {
                window.clear();
                continue;
            }
            if matches!(func.inst(inst).kind, InstKind::Or(..)) && fuse(func, inst, &window, &uses)
            {
                changed = true;
            }
            window.insert(inst);
        }
    }
    changed |= share_reads(func);
    if changed {
        sweep_dead(func);
    }
    changed
}

/// Rewrites one OR root to a single shifted word read, if it is a byte run.
fn fuse(
    func: &mut Function,
    root: InstId,
    window: &FxHashSet<InstId>,
    uses: &IndexVec<ValueId, u32>,
) -> bool {
    let Some(result) = func.inst_result_value(root) else { return false };
    let mut leaves = Vec::new();
    let mut nodes = 0;
    if !collect(func, result, 0, window, uses, true, &mut leaves, &mut nodes) {
        return false;
    }
    let run = leaves.len();
    if !(2..=MAX_RUN).contains(&run) {
        return false;
    }
    leaves.sort_unstable_by_key(|leaf| leaf.offset);
    let first = &leaves[0];
    for (position, leaf) in leaves.iter().enumerate() {
        let position = position as u64;
        // The byte at `offset + k` contributes at bit `8 * (run - 1 - k)`.
        if leaf.base != first.base
            || leaf.offset != first.offset + position
            || leaf.shift != 8 * (run as u64 - 1 - position)
        {
            return false;
        }
    }

    // %result = shr(256 - 8 * run, %lowest_word)
    let amount = 256 - 8 * run as u64;
    let shift = func.alloc_value(Value::Immediate(Immediate::I256(U256::from(amount))));
    let word = leaves[0].word;
    func.inst_mut(root).replace_kind(InstKind::Shr(shift, word));
    true
}

/// Decomposes one OR-tree value into the byte reads it combines.
#[allow(clippy::too_many_arguments)]
fn collect(
    func: &Function,
    value: ValueId,
    shift: u64,
    window: &FxHashSet<InstId>,
    uses: &IndexVec<ValueId, u32>,
    is_root: bool,
    leaves: &mut Vec<Leaf>,
    nodes: &mut usize,
) -> bool {
    *nodes += 1;
    if *nodes > MAX_NODES || leaves.len() > MAX_RUN || shift >= 256 {
        return false;
    }
    let Value::Inst(inst) = *func.value(value) else { return false };
    // The root is the instruction being visited; its operands must be in the
    // store-free window below it.
    if !is_root && !window.contains(&inst) {
        return false;
    }
    // An `or` or `shl` that only shapes the run is consumed by the rewrite, so
    // it must have no other user. A byte extraction and its word read may be
    // shared: they are left in place and the rewrite still removes the tree.
    let structural = !is_root && uses.get(value) != Some(&1);
    match func.inst(inst).kind {
        InstKind::Or(first, second) if !structural => {
            collect(func, first, shift, window, uses, false, leaves, nodes)
                && collect(func, second, shift, window, uses, false, leaves, nodes)
        }
        InstKind::Shl(amount, inner) if !structural => {
            let Some(amount) = func.value_u64(amount) else { return false };
            if amount % 8 != 0 {
                return false;
            }
            collect(func, inner, shift + amount, window, uses, false, leaves, nodes)
        }
        InstKind::Byte(index, word) if func.value_u64(index) == Some(0) => {
            push_leaf(func, word, shift, window, leaves)
        }
        InstKind::And(first, second) => {
            let (word, mask) = match func.value_u256(second) {
                Some(mask) => (first, mask),
                None => match func.value_u256(first) {
                    Some(mask) => (second, mask),
                    None => return false,
                },
            };
            mask == U256::from(0xffu64) && push_leaf(func, word, shift, window, leaves)
        }
        _ => false,
    }
}

/// Records one `mload` whose low byte the run combines.
fn push_leaf(
    func: &Function,
    word: ValueId,
    shift: u64,
    window: &FxHashSet<InstId>,
    leaves: &mut Vec<Leaf>,
) -> bool {
    let Value::Inst(inst) = *func.value(word) else { return false };
    let InstKind::MLoad(address) = func.inst(inst).kind else { return false };
    // The read must be reachable from the root with no store in between. A
    // read may be shared with other users; only the interior nodes are
    // consumed, and the lowest address's read is kept.
    if !window.contains(&inst) {
        return false;
    }
    let Some((base, offset)) = address_key(func, address) else { return false };
    leaves.push(Leaf { shift, base, offset, word });
    true
}

/// Flattens an address into its non-constant addends and their constant sum.
///
/// `add` wraps, so the split is exact for any operands: two addresses with
/// equal addend lists differ by exactly the difference of their constants.
fn address_key(func: &Function, address: ValueId) -> Option<(Vec<ValueId>, u64)> {
    let mut base = Vec::new();
    let mut offset = 0u64;
    let mut pending = vec![address];
    while let Some(value) = pending.pop() {
        if let Some(constant) = func.value_u64(value) {
            offset = offset.checked_add(constant)?;
            continue;
        }
        if let Value::Inst(inst) = *func.value(value)
            && let InstKind::Add(first, second) = func.inst(inst).kind
        {
            if pending.len() >= MAX_ADDENDS {
                return None;
            }
            pending.push(first);
            pending.push(second);
            continue;
        }
        if base.len() >= MAX_ADDENDS {
            return None;
        }
        base.push(value);
    }
    base.sort_unstable();
    Some((base, offset))
}

/// Points every group of neighbouring byte extractions at one word read.
fn share_reads(func: &mut Function) -> bool {
    let mut changed = false;
    for block in func.blocks.indices().collect::<Vec<_>>() {
        let mut groups: Vec<Vec<(u64, InstId, ValueId)>> = Vec::new();
        let mut bases: Vec<Vec<ValueId>> = Vec::new();
        let mut start = 0;
        let instructions = func.blocks[block].instructions.clone();
        for (position, &inst) in instructions.iter().enumerate() {
            // A store may change what a later read sees, so a group cannot
            // span one.
            if func.inst(inst).kind.has_side_effects() {
                changed |= rewrite_groups(func, &groups);
                groups.clear();
                bases.clear();
                start = position + 1;
                continue;
            }
            let _ = start;
            let Some((base, offset, word)) = byte_read(func, inst) else { continue };
            match bases.iter().position(|other| *other == base) {
                Some(index) => groups[index].push((offset, inst, word)),
                None => {
                    bases.push(base);
                    groups.push(vec![(offset, inst, word)]);
                }
            }
        }
        changed |= rewrite_groups(func, &groups);
    }
    changed
}

/// Rewrites each group whose first read is also its lowest and whose offsets
/// fit one word.
fn rewrite_groups(func: &mut Function, groups: &[Vec<(u64, InstId, ValueId)>]) -> bool {
    let mut changed = false;
    for group in groups {
        let [(first_offset, _, first_word), rest @ ..] = group.as_slice() else { continue };
        if rest.is_empty() {
            continue;
        }
        for &(offset, inst, _) in rest {
            let Some(index) = offset.checked_sub(*first_offset) else { continue };
            if index >= MAX_RUN as u64 || index == 0 {
                continue;
            }
            // %byte = byte(offset - first, %first_word)
            let index = func.alloc_value(Value::Immediate(Immediate::I256(U256::from(index))));
            func.inst_mut(inst).replace_kind(InstKind::Byte(index, *first_word));
            changed = true;
        }
    }
    changed
}

/// Recognizes `byte(0, mload(base + constant))` as a single byte of memory.
fn byte_read(func: &Function, inst: InstId) -> Option<(Vec<ValueId>, u64, ValueId)> {
    let InstKind::Byte(index, word) = func.inst(inst).kind else { return None };
    if func.value_u64(index) != Some(0) {
        return None;
    }
    let Value::Inst(load) = *func.value(word) else { return None };
    let InstKind::MLoad(address) = func.inst(load).kind else { return None };
    let (base, offset) = address_key(func, address)?;
    Some((base, offset, word))
}

/// Removes instructions left without users by the rewrites above.
///
/// A memory read expands the EVM memory high-water mark even when its value is
/// discarded, and a later `msize` observes that. Functions containing one keep
/// their reads, matching the rule dead code elimination applies.
fn sweep_dead(func: &mut Function) {
    let observes_msize =
        func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::MSize));
    loop {
        let uses = super::egraph::use_counts(func);
        let mut dead = FxHashSet::default();
        for inst in func.instructions() {
            let inst_ref = func.inst(inst);
            if inst_ref.kind.has_side_effects()
                || (observes_msize && inst_ref.kind.effect_kind() == EffectKind::MemoryRead)
            {
                continue;
            }
            if let Some(result) = func.inst_result_value(inst)
                && uses.get(result) == Some(&0)
            {
                dead.insert(inst);
            }
        }
        if dead.is_empty() {
            return;
        }
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|inst| !dead.contains(inst));
        }
    }
}

//! Replaces a branchy search over the highest set bit of a word with the `CLZ` opcode.
//!
//! [`bit_scan`](super::bit_scan) proves the scans of one block. Some libraries spell a scan with
//! branches that if-conversion keeps, because each arm updates two values. The initial estimate
//! of PRBMath's and OpenZeppelin's `sqrt` halves the word and the estimate together:
//!
//! ```text
//! if (x >= 2**128) { x >>= 128; r <<= 64; }
//! if (x >= 2**64) { x >>= 64; r <<= 32; }
//! ...
//! if (x >= 4) { r <<= 1; }
//! ```
//!
//! which leaves `r = 2**(m / 2)` for the index `m` of the highest set bit: seven branches and
//! six joins with two phis each, where `1 << ((255 - clz(x | 1)) >> 1)` takes six operations.
//!
//! # Recognition
//!
//! A test of the upper half of a word `x` (a comparison with `2**128` or `2**128 - 1`, or a
//! right shift by 128 bits) starts a region at its block. The pass runs the region from there on
//! one input per class of inputs, zero and `2**m` for every `m`, and follows its branches until
//! it reaches an instruction it cannot run: one with an effect, a checked operation whose result
//! it does not know, or a branch whose condition it does not know.
//!
//! A run tracks three kinds of value. `x` shifted right by a known amount is one: a right
//! shift by a known amount shifts it further, and a select or a phi passes it on. A comparison
//! of such a value with a constant `c` is known when it tests a threshold of a power of two:
//! `< c` with `c` zero or a power of two, `> c` with `c + 1` one, and `==` and `!=` with zero.
//! Every such test of `x >> s` asks whether `m >= s + k` for some `k`, so it has the same result
//! for every input with the same highest set bit as long as `s` does. Pure operations on known
//! values are known; any other operation that reads `x` makes an unknown value, and so does
//! anything that reads one. By induction every known value and every branch a run takes depends
//! only on the highest set bit of `x`, so the run on `2**m` gives what every input of its class
//! gets, and the runs on the 257 classes prove a value for every input.
//!
//! # Rewrite
//!
//! All runs must stop in one block, the region's last, and the blocks they pass must form a
//! region its first block enters: the predecessors of every other block lie in it, the last
//! block leads back into none of them, and no run loops. A value of the last block that every
//! run computed before stopping is a function of `m`, given by a table of 257 entries, and it is
//! replaced when the table is one of
//!
//! ```text
//! index >> k        (255 - clz(x | 1)) >> k, or with clz(x)
//! index >> k << k   (255 - clz(x | 1)) & (255 >> k << k), or with clz(x)
//! leading zeros     clz(x | 1), or clz(x)
//! 2**(index / 2)    1 << ((255 - clz(x | 1)) >> 1), or with clz(x)
//! ```
//!
//! with the form its value for zero picks. Solady rounds the index down to a byte this way when
//! it stops its scan at the byte that holds the highest set bit. A proved value that only the
//! removed code and other proved values read goes with them rather than being rewritten, so a
//! scan that rounds its index in steps keeps only its last step. Nothing else the region
//! computes may be read past it: the first block then jumps straight to the last one, and the
//! other blocks, the phis of the last block and the instructions only the region read go. The
//! rewrite is taken when the target prices the new operations below the removed instructions
//! and branches.
//!
//! # Limits and placement
//!
//! The target must have `CLZ` (Osaka). The region has no loop and, before its last block, no
//! instruction with an effect and no checked operation that some run does not know succeeds. The
//! runs of one function pass at most [`WORK`] instructions.
//! The pass runs after `bit-scan`, which proves the straight-line scans first.
//!
//! # Cost
//!
//! A run passes every instruction up to the one it stops at, but most of them read neither the
//! input nor a constant, and their value is unknown in every run. Before the runs of a region,
//! the pass follows the readers of the input once and marks what a run may see as shifted or
//! known; a run then evaluates only those instructions, finds where it stops from the positions
//! of the instructions with an effect, and charges [`WORK`] for every instruction it passes, so
//! the budget runs out where it would if the run evaluated them all. A region of a long
//! straight-line block then costs what reads its input, not the rest of the block.

use super::word_sequence::is_pure;
use crate::{
    mir::{
        BlockId, Function, Immediate, InstId, InstKind, Instruction, MirType, Module, Terminator,
        Value, ValueId,
        pass::{MirPass, ModuleAnalyses, run_function_pass},
        utils::{eval::eval_typed_inst, invalidate_unreachable_block, replace_terminator},
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::{
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use solar_interface::Ident;
use solar_sema::Gcx;

/// The instructions the runs of one function may evaluate.
const WORK: u32 = 1 << 18;
/// The regions one function may try.
const MAX_REGIONS: usize = 64;
/// The classes tried first, which reject most regions.
const PROBES: [u32; 5] = [0, 128, 255, 64, 1];
/// A run may see the value as the input shifted right.
const SHIFTED: u8 = 1;
/// A run may see the value as known.
const KNOWN: u8 = 2;

pub(crate) struct ScanRegion;

impl MirPass for ScanRegion {
    fn name(&self) -> &'static str {
        "scan-region"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        if !gcx.sess.opts.evm_version.has_clz() {
            return false;
        }
        run_function_pass(module, analyses, |func, analyses| {
            run_function(func, &analyses.target(gcx))
        })
    }
}

/// A value of one run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Word {
    /// The same number for every input of the class.
    Known(U256),
    /// The input shifted right by this many bits, fewer than 256.
    Shifted(u32),
    /// Anything else.
    Unknown,
}

/// Where a run stopped, and the known values it computed.
struct Run {
    stop_block: BlockId,
    /// The index of the instruction the run could not run, or the block's length for its
    /// terminator.
    stop_at: usize,
    known: Known,
    blocks: Vec<BlockId>,
}

/// The known values of a run, sorted by value.
struct Known(Vec<(ValueId, U256)>);

impl Known {
    fn get(&self, value: &ValueId) -> Option<&U256> {
        let at = self.0.binary_search_by_key(value, |&(value, _)| value).ok()?;
        Some(&self.0[at].1)
    }

    fn contains_key(&self, value: &ValueId) -> bool {
        self.get(value).is_some()
    }
}

impl std::ops::Index<&ValueId> for Known {
    type Output = U256;

    fn index(&self, value: &ValueId) -> &U256 {
        self.get(value).expect("a known value")
    }
}

/// The values of the run in progress, kept across runs so that a run allocates no map.
#[derive(Default)]
struct Env {
    words: IndexVec<ValueId, Word>,
    /// The values a run set, to read its known values and to clear before the next run.
    set: Vec<ValueId>,
}

impl Env {
    /// Clears the values of the last run, for a function of `len` values.
    fn clear(&mut self, len: usize) {
        for value in self.set.drain(..) {
            self.words[value] = Word::Unknown;
        }
        if self.words.len() < len {
            self.words.resize(len, Word::Unknown);
        }
    }

    fn insert(&mut self, value: ValueId, word: Word) {
        self.words[value] = word;
        self.set.push(value);
    }

    /// The known values of the run, or only those among `keep`, which is sorted.
    fn known(&self, keep: Option<&[ValueId]>) -> Known {
        let known_word = |&value: &ValueId| match self.words[value] {
            Word::Known(c) => Some((value, c)),
            _ => None,
        };
        if let Some(keep) = keep {
            return Known(keep.iter().filter_map(known_word).collect());
        }
        let mut known: Vec<(ValueId, U256)> = self.set.iter().filter_map(known_word).collect();
        known.sort_unstable_by_key(|&(value, _)| value);
        known.dedup_by_key(|&mut (value, _)| value);
        Known(known)
    }
}

/// What a proved value computes from the index `m` of the highest set bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    /// `m >> k`.
    Index(u32),
    /// `(m >> k) << k`, for `k` from 1 to 7.
    Masked(u32),
    /// `255 - m`.
    LeadingZeros,
    /// `2**(m >> 1)`.
    HalfPower,
}

impl Shape {
    /// Every shape [`shape`] can pick; [`hopeless`] relies on the list being whole.
    fn all() -> impl Iterator<Item = Self> {
        // Keep the list below in step with the variants.
        let _ = |shape: Self| match shape {
            Self::Index(_) | Self::Masked(_) | Self::LeadingZeros | Self::HalfPower => {}
        };
        (0..8)
            .map(Self::Index)
            .chain((1..8).map(Self::Masked))
            .chain([Self::LeadingZeros, Self::HalfPower])
    }
}

fn run_function(func: &mut Function, target: &Target) -> bool {
    if candidates(func).is_empty() {
        return false;
    }
    let mut changed = false;
    let mut work = WORK;
    let mut tried = FxHashSet::default();
    let mut facts = Facts::new(func);
    let mut env = Env::default();
    for _ in 0..MAX_REGIONS {
        let Some(&(entry, input)) =
            facts.candidates.iter().find(|&&candidate| tried.insert(candidate))
        else {
            break;
        };
        if rewrite_region(func, &facts, target, &mut env, &mut work, entry, input) {
            changed = true;
            facts = Facts::new(func);
        }
        if work == 0 {
            break;
        }
    }
    changed
}

/// The blocks that test the upper half of a word, and the word, in block order.
fn candidates(func: &Function) -> Vec<(BlockId, ValueId)> {
    let mut candidates = Vec::new();
    let mut seen = FxHashSet::default();
    for (block, data) in func.blocks.iter_enumerated() {
        if matches!(data.terminator, None | Some(Terminator::Invalid)) {
            continue;
        }
        for &inst in &data.instructions {
            if let Some(input) = half_test(func, inst)
                && seen.insert((block, input))
            {
                candidates.push((block, input));
            }
        }
    }
    candidates
}

/// What the runs of one function read, built again after each rewrite.
struct Facts {
    /// The block and index of every instruction in a block.
    place: IndexVec<InstId, Option<(BlockId, u32)>>,
    /// The instructions that read each value.
    users: IndexVec<ValueId, SmallVec<[InstId; 2]>>,
    /// The indices of the instructions a run stops at in each block: those with an effect, and
    /// phis.
    stops: IndexVec<BlockId, Vec<u32>>,
    /// What a run may see as each value through constants alone, without reading the input.
    base: IndexVec<ValueId, u8>,
    /// The indices of the instructions of each block that a run evaluates for every input.
    base_insts: IndexVec<BlockId, Vec<u32>>,
    /// The reads of each value, by instructions in blocks and by terminators.
    uses: IndexVec<ValueId, u32>,
    /// The pure instructions of each block whose result nothing reads.
    unused: IndexVec<BlockId, Vec<InstId>>,
    candidates: Vec<(BlockId, ValueId)>,
}

impl Facts {
    fn new(func: &Function) -> Self {
        let mut place = IndexVec::from_vec(vec![None; func.num_insts()]);
        let mut users = IndexVec::from_vec(vec![SmallVec::new(); func.num_values()]);
        let mut stops = IndexVec::with_capacity(func.blocks.len());
        for (block, data) in func.blocks.iter_enumerated() {
            let mut block_stops = Vec::new();
            for (index, &inst) in data.instructions.iter().enumerate() {
                let index = index as u32;
                place[inst] = Some((block, index));
                let instruction = func.inst(inst);
                instruction.kind.visit_operands(|operand| users[operand].push(inst));
                if !evaluates(instruction) {
                    block_stops.push(index);
                }
            }
            stops.push(block_stops);
        }

        // What constants alone make known, to a fixed point through phis.
        let mut base = IndexVec::from_vec(vec![0u8; func.num_values()]);
        let mut pending: Vec<(InstId, Option<ValueId>)> =
            func.instructions().map(|inst| (inst, None)).collect();
        while let Some((inst, changed)) = pending.pop() {
            let Some(value) = func.inst_result_value(inst) else { continue };
            let new =
                reach(func, inst, changed, |operand| constant_or(func, operand, base[operand]));
            if new | base[value] != base[value] {
                base[value] |= new;
                pending.extend(users[value].iter().map(|&user| (user, Some(value))));
            }
        }
        let base_insts = func
            .blocks
            .iter()
            .map(|data| {
                (0..data.instructions.len() as u32)
                    .filter(|&index| {
                        let inst = data.instructions[index as usize];
                        evaluates(func.inst(inst))
                            && func.inst_result_value(inst).is_some_and(|value| base[value] != 0)
                    })
                    .collect()
            })
            .collect();
        let mut uses: IndexVec<ValueId, u32> =
            users.iter().map(|users| users.len() as u32).collect();
        for data in func.blocks.iter() {
            if let Some(terminator) = &data.terminator {
                terminator.visit_operands(|operand| uses[operand] += 1);
            }
        }
        let unused = func
            .blocks
            .iter()
            .map(|data| {
                data.instructions
                    .iter()
                    .copied()
                    .filter(|&inst| {
                        let instruction = func.inst(inst);
                        evaluates(instruction)
                            && instruction.result().is_some_and(|value| uses[value] == 0)
                    })
                    .collect()
            })
            .collect();
        Self { place, users, stops, base, base_insts, uses, unused, candidates: candidates(func) }
    }

    /// The indices of the instructions of each block that a run of `input` may see as other than
    /// unknown, for the blocks where they differ from `base_insts`.
    fn slice(&self, func: &Function, input: ValueId) -> FxHashMap<BlockId, Vec<u32>> {
        let mut flags: FxHashMap<ValueId, u8> = FxHashMap::default();
        let flag = |flags: &FxHashMap<ValueId, u8>, value: ValueId| {
            if value == input {
                SHIFTED
            } else {
                constant_or(func, value, flags.get(&value).copied().unwrap_or(self.base[value]))
            }
        };
        let mut marked = Vec::new();
        let mut pending: Vec<(InstId, Option<ValueId>)> =
            self.users[input].iter().map(|&user| (user, Some(input))).collect();
        while let Some((inst, changed)) = pending.pop() {
            let Some(value) = func.inst_result_value(inst) else { continue };
            if value == input {
                continue;
            }
            let new = reach(func, inst, changed, |operand| flag(&flags, operand));
            let old = flag(&flags, value);
            if new | old != old {
                if flags.insert(value, new | old).is_none() {
                    marked.push(inst);
                }
                pending.extend(self.users[value].iter().map(|&user| (user, Some(value))));
            }
        }
        let mut slice: FxHashMap<BlockId, Vec<u32>> = FxHashMap::default();
        for inst in marked {
            if let Some((block, index)) = self.place[inst]
                && evaluates(func.inst(inst))
            {
                slice.entry(block).or_insert_with(|| self.base_insts[block].clone()).push(index);
            }
        }
        for list in slice.values_mut() {
            list.sort_unstable();
            list.dedup();
        }
        slice
    }
}

/// Whether a run evaluates the instruction when it passes it, rather than stopping there. A
/// checked operation, which reverts for some operands, stops it unless the run knows its result.
fn evaluates(instruction: &Instruction) -> bool {
    is_pure(instruction)
        && !instruction.kind.effects().control.any()
        && !matches!(instruction.kind, InstKind::Phi(_))
}

/// `KNOWN` for a constant, and `flags` otherwise.
fn constant_or(func: &Function, value: ValueId, flags: u8) -> u8 {
    if func.value_u256(value).is_some() { KNOWN } else { flags }
}

/// What a run may see as the result of `inst`, given what it may see as each operand, following
/// [`eval`] and the phis of [`run`]. The flags only grow, so when only the operand `changed`
/// grew since a phi was last reached, the phi gains just what that operand may be.
fn reach(
    func: &Function,
    inst: InstId,
    changed: Option<ValueId>,
    flag: impl Fn(ValueId) -> u8,
) -> u8 {
    let instruction = func.inst(inst);
    if let InstKind::Phi(incoming) = &instruction.kind {
        return match changed {
            Some(value) => flag(value),
            None => incoming.iter().fold(0, |flags, &(_, value)| flags | flag(value)),
        };
    }
    if !is_pure(instruction) {
        return 0;
    }
    let known = |value| flag(value) & KNOWN != 0;
    let shifted = |value| flag(value) & SHIFTED != 0;
    let mut operands_known = true;
    instruction.kind.visit_operands(|operand| operands_known &= known(operand));
    let mut flags = if operands_known { KNOWN } else { 0 };
    match instruction.kind {
        InstKind::Select(condition, a, b) => {
            return if known(condition) { flag(a) | flag(b) } else { 0 };
        }
        InstKind::Shr(shift, value) if known(shift) && shifted(value) => flags |= SHIFTED | KNOWN,
        InstKind::Lt(a, b) | InstKind::Gt(a, b) | InstKind::Eq(a, b) | InstKind::Ne(a, b)
            if (shifted(a) && known(b)) || (known(a) && shifted(b)) =>
        {
            flags |= KNOWN;
        }
        _ => {}
    }
    flags
}

/// The word whose upper half `inst` tests.
fn half_test(func: &Function, inst: InstId) -> Option<ValueId> {
    let half = U256::ONE << 128;
    let is_half = |value| {
        func.value_u256(value).is_some_and(|limit| limit == half || limit == half - U256::ONE)
    };
    let input = match func.inst(inst).kind {
        InstKind::Lt(a, b) | InstKind::Gt(a, b) if is_half(b) => a,
        InstKind::Lt(a, b) | InstKind::Gt(a, b) if is_half(a) => b,
        InstKind::Shr(shift, word) if func.value_u256(shift) == Some(U256::from(128)) => word,
        _ => return None,
    };
    (func.value_u256(input).is_none() && func.value_ty(input) == Some(MirType::I256))
        .then_some(input)
}

/// Whether `x < c` asks only for a bound on the highest set bit of `x`.
fn is_threshold(c: U256) -> bool {
    c.is_zero() || c.is_power_of_two()
}

/// Whether a comparison of `x >> s` with the constant `c` is decided by the highest set bit
/// of `x`, where `shifted_first` tells which operand `x >> s` is.
fn decides(kind: &InstKind, shifted_first: bool, c: U256) -> bool {
    match (kind, shifted_first) {
        // x < c, c > x
        (InstKind::Lt(..), true) | (InstKind::Gt(..), false) => is_threshold(c),
        // x > c, c < x
        (InstKind::Gt(..), true) | (InstKind::Lt(..), false) => {
            is_threshold(c.wrapping_add(U256::ONE))
        }
        (InstKind::Eq(..) | InstKind::Ne(..), _) => c.is_zero(),
        _ => false,
    }
}

/// The value of `value` in a run.
fn word(func: &Function, env: &Env, input: ValueId, value: ValueId) -> Word {
    if value == input {
        Word::Shifted(0)
    } else if let Some(c) = func.value_u256(value) {
        Word::Known(c)
    } else {
        env.words[value]
    }
}

/// Runs the region of `input` from `entry` on `probe`, or returns `None` when it loops or
/// the work runs out. It evaluates only the instructions of `slice`, or of `Facts::base_insts`
/// for a block `slice` leaves out: the others are unknown in every run. The run keeps the known
/// values among `keep`, or all of them.
#[allow(clippy::too_many_arguments)]
fn run(
    func: &Function,
    facts: &Facts,
    slice: &FxHashMap<BlockId, Vec<u32>>,
    entry: BlockId,
    start: usize,
    input: ValueId,
    probe: U256,
    keep: Option<&[ValueId]>,
    env: &mut Env,
    work: &mut u32,
) -> Option<Run> {
    env.clear(func.num_values());
    let mut blocks = vec![entry];
    let mut block = entry;
    let mut pred = None;
    loop {
        let data = &func.blocks[block];
        let mut index = if block == entry { start } else { 0 };
        // Phis read the edge the run came along, all at once.
        let mut phis = Vec::new();
        while let Some(&inst) = data.instructions.get(index) {
            let InstKind::Phi(incoming) = &func.inst(inst).kind else { break };
            let value = func.inst_result_value(inst)?;
            let word = pred
                .and_then(|pred| incoming.iter().find(|&&(from, _)| from == pred))
                .map_or(Word::Unknown, |&(_, value)| word(func, env, input, value));
            phis.push((value, word));
            index += 1;
        }
        for (value, word) in phis {
            env.insert(value, word);
        }
        let list = slice.get(&block).unwrap_or(&facts.base_insts[block]);
        loop {
            // Every instruction up to the first one the run cannot run costs work, that one too.
            let stops = &facts.stops[block];
            let stop = stops.get(stops.partition_point(|&at| (at as usize) < index));
            let end = stop.map_or(data.instructions.len(), |&at| at as usize);
            let passed =
                u32::try_from(end - index + usize::from(stop.is_some())).unwrap_or(u32::MAX);
            let Some(left) = work.checked_sub(passed) else {
                *work = 0;
                return None;
            };
            *work = left;
            let from = list.partition_point(|&at| (at as usize) < index);
            for &at in &list[from..] {
                if at as usize >= end {
                    break;
                }
                let instruction = func.inst(data.instructions[at as usize]);
                if let Some(value) = instruction.result() {
                    let result = eval(func, &instruction.kind, env, input, probe);
                    env.insert(value, result);
                }
            }
            index = end;
            if stop.is_none() {
                break;
            }
            // A checked operation on known values succeeds for every input of the class, as
            // its operands depend on the class alone, so the run goes on past it.
            let instruction = func.inst(data.instructions[index]);
            if is_pure(instruction)
                && let Some(value) = instruction.result()
                && let result @ Word::Known(_) = eval(func, &instruction.kind, env, input, probe)
            {
                env.insert(value, result);
                index += 1;
                continue;
            }
            return Some(Run { stop_block: block, stop_at: index, known: env.known(keep), blocks });
        }
        let next = match &data.terminator {
            Some(Terminator::Jump(next)) => Some(*next),
            Some(Terminator::Branch { condition, then_block, else_block }) => {
                match word(func, env, input, *condition) {
                    Word::Known(c) => Some(if c.is_zero() { *else_block } else { *then_block }),
                    _ => None,
                }
            }
            _ => None,
        };
        let Some(next) = next else {
            return Some(Run { stop_block: block, stop_at: index, known: env.known(keep), blocks });
        };
        // A loop, or a return to the entry, is not a region the rewrite takes.
        if blocks.contains(&next) {
            return None;
        }
        blocks.push(next);
        pred = Some(block);
        block = next;
    }
}

/// Evaluates one pure instruction of a run on `probe`.
fn eval(func: &Function, kind: &InstKind, env: &Env, input: ValueId, probe: U256) -> Word {
    let get = |value| word(func, env, input, value);
    match *kind {
        InstKind::Shr(shift, value) => {
            if let (Word::Known(shift), Word::Shifted(s)) = (get(shift), get(value)) {
                let total = U256::from(s).saturating_add(shift);
                return if total >= U256::from(256) {
                    Word::Known(U256::ZERO)
                } else {
                    Word::Shifted(total.to::<u32>())
                };
            }
        }
        InstKind::Lt(a, b) | InstKind::Gt(a, b) | InstKind::Eq(a, b) | InstKind::Ne(a, b) => {
            let (shifted, c, shifted_first) = match (get(a), get(b)) {
                (Word::Shifted(s), Word::Known(c)) => (s, c, true),
                (Word::Known(c), Word::Shifted(s)) => (s, c, false),
                (Word::Shifted(_), _) | (_, Word::Shifted(_)) => return Word::Unknown,
                _ => (0, U256::ZERO, false),
            };
            if matches!(get(a), Word::Shifted(_)) || matches!(get(b), Word::Shifted(_)) {
                if !decides(kind, shifted_first, c) {
                    return Word::Unknown;
                }
                let x = probe >> shifted as usize;
                let (a, b) = if shifted_first { (x, c) } else { (c, x) };
                let result = match kind {
                    InstKind::Lt(..) => a < b,
                    InstKind::Gt(..) => a > b,
                    InstKind::Eq(..) => a == b,
                    _ => a != b,
                };
                return Word::Known(U256::from(result));
            }
        }
        InstKind::Select(condition, a, b) => {
            return match get(condition) {
                Word::Known(c) => get(if c.is_zero() { b } else { a }),
                _ => Word::Unknown,
            };
        }
        _ => {}
    }
    let mut operands_known = true;
    kind.visit_operands(|operand| operands_known &= matches!(get(operand), Word::Known(_)));
    if !operands_known {
        return Word::Unknown;
    }
    let result = eval_typed_inst(func, kind, |operand| match get(operand) {
        Word::Known(c) => Ok(c),
        _ => Err(()),
    });
    match result {
        Ok(Some(c)) => Word::Known(c),
        _ => Word::Unknown,
    }
}

/// The shape of a table of results by highest set bit, and whether its value for zero needs
/// `x | 1` rather than `x`.
fn shape(table: &[U256], zero: U256) -> Option<(Shape, bool)> {
    let fits = |f: &dyn Fn(u32) -> U256| (0..256u32).all(|m| table[m as usize] == f(m));
    for k in 0..8u32 {
        if fits(&|m| U256::from(m >> k)) {
            return if zero.is_zero() {
                Some((Shape::Index(k), true))
            } else if zero == U256::MAX >> k as usize {
                Some((Shape::Index(k), false))
            } else {
                None
            };
        }
    }
    for k in 1..8u32 {
        if fits(&|m| U256::from(m >> k << k)) {
            let mask = U256::from(0xffu32 >> k << k);
            return if zero.is_zero() {
                Some((Shape::Masked(k), true))
            } else if zero == mask {
                Some((Shape::Masked(k), false))
            } else {
                None
            };
        }
    }
    if fits(&|m| U256::from(255 - m)) {
        return if zero == U256::from(255) {
            Some((Shape::LeadingZeros, true))
        } else if zero == U256::from(256) {
            Some((Shape::LeadingZeros, false))
        } else {
            None
        };
    }
    if fits(&|m| U256::ONE << (m >> 1) as usize) {
        return if zero == U256::ONE {
            Some((Shape::HalfPower, true))
        } else if zero.is_zero() {
            Some((Shape::HalfPower, false))
        } else {
            None
        };
    }
    None
}

/// Whether a result for the class `m` fits one of the shapes.
fn may_fit(m: u32, result: U256) -> bool {
    (0..8u32).any(|k| result == U256::from(m >> k) || result == U256::from(m >> k << k))
        || result == U256::from(255 - m)
        || result == U256::ONE << (m >> 1) as usize
}

fn immediate(func: &mut Function, value: u64) -> ValueId {
    func.alloc_value(Value::Immediate(Immediate::I256(U256::from(value))))
}

/// Emits the `CLZ` form of `shape` and returns its last operation.
fn clz_form(
    func: &mut Function,
    shape: Shape,
    odd: bool,
    input: ValueId,
    emit: &mut dyn FnMut(&mut Function, InstKind) -> ValueId,
) -> InstKind {
    let one = immediate(func, 1);
    let all = immediate(func, 255);
    // %word = or %input, 1
    let word = if odd { emit(func, InstKind::Or(input, one)) } else { input };
    if shape == Shape::LeadingZeros {
        // %zeros = clz %word
        return InstKind::Clz(word);
    }
    // %zeros = clz %word
    // %index = sub 255, %zeros
    let zeros = emit(func, InstKind::Clz(word));
    let index = InstKind::Sub(all, zeros);
    match shape {
        Shape::Index(0) => index,
        Shape::Index(k) => {
            // %result = shr %index, k
            let index = emit(func, index);
            let k = immediate(func, u64::from(k));
            InstKind::Shr(k, index)
        }
        Shape::Masked(k) => {
            // %result = and %index, 255 >> k << k
            let index = emit(func, index);
            let mask = immediate(func, u64::from(0xffu32 >> k << k));
            InstKind::And(index, mask)
        }
        _ => {
            // %half = shr %index, 1
            // %result = shl 1, %half
            let index = emit(func, index);
            let half = emit(func, InstKind::Shr(one, index));
            InstKind::Shl(half, one)
        }
    }
}

fn phi_count(func: &Function, block: BlockId) -> usize {
    func.blocks[block]
        .instructions
        .iter()
        .take_while(|&&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)))
        .count()
}

/// The values of `last` that every run computed, before the first instruction one stopped at.
/// Only the leading phis and the instructions of `list`, the ones a run evaluates in `last`, can
/// be known.
fn computed(
    func: &Function,
    last: BlockId,
    first: usize,
    runs: &[(u32, Run)],
    list: &[u32],
) -> Vec<(InstId, ValueId)> {
    let limit = runs.iter().map(|(_, run)| run.stop_at).min().unwrap_or(0);
    let instructions = &func.blocks[last].instructions;
    let range = first.min(limit)..limit.min(instructions.len());
    let phis = instructions[range.clone()]
        .iter()
        .take_while(|&&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)));
    let rest = list
        .iter()
        .map(|&at| at as usize)
        .filter(|at| range.contains(at))
        .map(|at| &instructions[at]);
    phis.chain(rest)
        .filter_map(|&inst| {
            let value = func.inst_result_value(inst)?;
            (func.value_ty(value) == Some(MirType::I256)
                && runs.iter().all(|(_, run)| run.known.contains_key(&value)))
            .then_some((inst, value))
        })
        .collect()
}

/// Proves and rewrites the region of `input` that starts at `entry`.
fn rewrite_region(
    func: &mut Function,
    facts: &Facts,
    target: &Target,
    env: &mut Env,
    work: &mut u32,
    entry: BlockId,
    input: ValueId,
) -> bool {
    // The runs start after the input when the entry defines it, and after the phis otherwise.
    let start = match func.value(input) {
        Value::Inst(def) => func.blocks[entry]
            .instructions
            .iter()
            .position(|inst| inst == def)
            .map_or_else(|| phi_count(func, entry), |place| place + 1),
        _ => phi_count(func, entry),
    };
    let first = |last: BlockId| if last == entry { start } else { 0 };
    let slice = facts.slice(func, input);
    let list = |last: BlockId| slice.get(&last).unwrap_or(&facts.base_insts[last]).as_slice();

    // The zero input and a few classes first; they reject most regions.
    let mut runs: Vec<(u32, Run)> = Vec::new();
    let Some(zero) = run(func, facts, &slice, entry, start, input, U256::ZERO, None, env, work)
    else {
        return false;
    };
    let last = zero.stop_block;
    for m in PROBES {
        let probe = U256::ONE << m as usize;
        let Some(run) = run(func, facts, &slice, entry, start, input, probe, None, env, work)
        else {
            return false;
        };
        if run.stop_block != last {
            return false;
        }
        runs.push((m, run));
    }
    let plausible: Vec<(InstId, ValueId)> = computed(func, last, first(last), &runs, list(last))
        .into_iter()
        .filter(|(_, value)| {
            zero.known.contains_key(value)
                && runs.iter().all(|(m, run)| may_fit(*m, run.known[value]))
        })
        .collect();
    if plausible.is_empty() {
        return false;
    }

    // Runs that stay in the entry and stop at an instruction, or at a terminator that is not a
    // branch, all stop at the same place, so the region and what the rewrite can remove are
    // known now. When no value can take a form cheaper than what replacing them all removes,
    // the other classes cannot change the answer: charge the work they would take and stop.
    let stays = last == entry
        && (zero.stop_at < func.blocks[entry].instructions.len()
            || !matches!(func.blocks[entry].terminator, Some(Terminator::Branch { .. })));
    if stays && hopeless(func, facts, target, entry, input, &plausible, &zero, &runs) {
        let instructions = &func.blocks[entry].instructions;
        let from = start
            + instructions[start..]
                .iter()
                .take_while(|&&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)))
                .count();
        let passed = zero.stop_at - from + usize::from(zero.stop_at < instructions.len());
        let classes = 256 - PROBES.len();
        let total = (passed as u64).saturating_mul(classes as u64);
        *work = u32::try_from(u64::from(*work).saturating_sub(total)).unwrap_or(0);
        return false;
    }

    // Every class now. Only a plausible value can fit a shape, so the runs keep only those.
    let mut keep: Vec<ValueId> = plausible.iter().map(|&(_, value)| value).collect();
    keep.sort_unstable();
    for m in 0..256u32 {
        if PROBES.contains(&m) {
            continue;
        }
        let probe = U256::ONE << m as usize;
        let Some(run) =
            run(func, facts, &slice, entry, start, input, probe, Some(&keep), env, work)
        else {
            return false;
        };
        if run.stop_block != last {
            return false;
        }
        runs.push((m, run));
    }
    runs.sort_by_key(|&(m, _)| m);
    let mut proved = Vec::new();
    for (inst, value) in computed(func, last, first(last), &runs, list(last)) {
        let Some(&at_zero) = zero.known.get(&value) else { continue };
        if zero.stop_at < func.blocks[last].instructions.len()
            && func.blocks[last].instructions[..zero.stop_at].iter().all(|&i| i != inst)
        {
            continue;
        }
        let table: Vec<U256> = runs.iter().map(|(_, run)| run.known[&value]).collect();
        if let Some((shape, odd)) = shape(&table, at_zero) {
            proved.push((inst, value, shape, odd));
        }
    }
    if proved.is_empty() {
        return false;
    }

    // The region: the blocks the runs passed, entered only through the entry.
    let mut region: FxHashSet<BlockId> = zero.blocks.iter().copied().collect();
    for (_, run) in &runs {
        region.extend(run.blocks.iter().copied());
    }
    for &block in &region {
        let preds = &func.blocks[block].predecessors;
        let closed = if block == entry {
            preds.iter().all(|pred| !region.contains(pred))
        } else {
            preds.iter().all(|&pred| pred != last && region.contains(&pred))
        };
        if !closed {
            return false;
        }
    }
    let mut middle: Vec<BlockId> =
        region.iter().copied().filter(|&block| block != entry && block != last).collect();
    middle.sort_unstable();

    let mut replaced: FxHashSet<InstId> = proved.iter().map(|&(inst, ..)| inst).collect();
    let (mut removed, uses, branches) =
        removal(func, facts, target, entry, last, &middle, &replaced, input);
    // A proved value that only the removed code or other proved values read goes with them.
    proved.retain(|&(inst, value, ..)| {
        let dead = uses[value] == 0;
        if dead {
            replaced.remove(&inst);
            removed.insert(inst);
        }
        !dead
    });
    if proved.is_empty() {
        return false;
    }
    // Nothing that stays may read what the removed code computes. A replaced phi hands its
    // readers to its new value.
    for &inst in &removed {
        if let Some(value) = func.inst_result_value(inst)
            && uses[value] != 0
            && !replaced.contains(&inst)
        {
            return false;
        }
    }

    // Price the rewrite.
    let old = most_removed(func, target, &removed, &replaced) + branches;
    let mut new = if last != entry { target.jump() } else { Cost::ZERO };
    for &(_, _, shape, odd) in &proved {
        let last_kind = clz_form(func, shape, odd, input, &mut |func, kind| {
            new += target.kind_estimate(func, &kind);
            input
        });
        new += target.kind_estimate(func, &last_kind);
    }
    if !target.cmp(new, old).is_lt() {
        return false;
    }

    // Rewrite.
    let mut replacements = FxHashMap::default();
    let mut front = Vec::new();
    let mut before: FxHashMap<InstId, Vec<InstId>> = FxHashMap::default();
    for &(inst, value, shape, odd) in &proved {
        let metadata = func.inst(inst).metadata.debug_context();
        let mut inserted = Vec::new();
        let last_kind = clz_form(func, shape, odd, input, &mut |func, kind| {
            let mut new = Instruction::new(kind, Some(MirType::I256));
            new.metadata = metadata.clone();
            let (new, value) = func.alloc_value_inst(new);
            inserted.push(new);
            value
        });
        if matches!(func.inst(inst).kind, InstKind::Phi(_)) {
            let mut new = Instruction::new(last_kind, Some(MirType::I256));
            new.metadata = metadata.clone();
            let (new, new_value) = func.alloc_value_inst(new);
            inserted.push(new);
            front.extend(inserted);
            replacements.insert(value, new_value);
        } else {
            func.replace_inst_kind(inst, last_kind);
            before.insert(inst, inserted);
        }
    }
    let old_instructions = std::mem::take(&mut func.blocks[last].instructions);
    let mut instructions = front;
    for inst in old_instructions {
        let is_phi = matches!(func.inst(inst).kind, InstKind::Phi(_));
        if (removed.contains(&inst) && !replaced.contains(&inst)) || (is_phi && last != entry) {
            continue;
        }
        instructions.extend(before.remove(&inst).unwrap_or_default());
        instructions.push(inst);
    }
    func.blocks[last].instructions = instructions;
    if last != entry {
        func.blocks[entry].instructions.retain(|inst| !removed.contains(inst));
        for &block in &middle {
            let _ = invalidate_unreachable_block(func, block);
        }
        replace_terminator(func, entry, Terminator::Jump(last));
    }
    if !replacements.is_empty() {
        func.replace_uses(&replacements);
    }
    true
}

/// What replacing `replaced` removes: the middle blocks, the phis of the last block, and then
/// every pure instruction of the entry and the last block that nothing reads any more. The
/// replacements read `input`, so it stays. Returns the removed instructions, the uses left, and
/// the branches the region loses.
#[allow(clippy::too_many_arguments)]
fn removal(
    func: &Function,
    facts: &Facts,
    target: &Target,
    entry: BlockId,
    last: BlockId,
    middle: &[BlockId],
    replaced: &FxHashSet<InstId>,
    input: ValueId,
) -> (FxHashSet<InstId>, IndexVec<ValueId, u32>, Cost) {
    let mut uses = facts.uses.clone();
    uses[input] += 1;
    let mut removed: FxHashSet<InstId> = FxHashSet::default();
    let mut branches = Cost::ZERO;
    // The instructions whose result nothing may read any more, to sweep.
    let mut dead: Vec<InstId> = facts.unused[entry].clone();
    if last != entry {
        dead.extend(facts.unused[last].iter().copied());
    }
    let drop_use = |uses: &mut IndexVec<ValueId, u32>, dead: &mut Vec<InstId>, value| {
        uses[value] -= 1;
        if uses[value] == 0
            && let Value::Inst(def) = *func.value(value)
        {
            dead.push(def);
        }
    };
    let mut pending = Vec::new();
    if last != entry {
        for &block in middle.iter().chain([&entry]) {
            let terminator = func.blocks[block].terminator.as_ref().expect("a run passed it");
            terminator.visit_operands(|operand| drop_use(&mut uses, &mut dead, operand));
            branches += match terminator {
                Terminator::Jump(_) => target.jump(),
                _ => target.branch() + target.branch(),
            };
        }
        for &block in middle {
            pending.extend(func.blocks[block].instructions.iter().copied());
        }
        pending.extend(func.blocks[last].instructions.iter().copied().take(phi_count(func, last)));
    }
    for &inst in replaced {
        func.inst(inst).kind.visit_operands(|operand| drop_use(&mut uses, &mut dead, operand));
    }
    for &inst in &pending {
        if removed.insert(inst) && !replaced.contains(&inst) {
            func.inst(inst).kind.visit_operands(|operand| drop_use(&mut uses, &mut dead, operand));
        }
    }
    // Then every pure instruction of the entry and the last block that nothing reads any more,
    // to a fixed point.
    while let Some(inst) = dead.pop() {
        let instruction = func.inst(inst);
        let sweeps = facts.place[inst].is_some_and(|(block, _)| block == entry || block == last)
            && !replaced.contains(&inst)
            && evaluates(instruction)
            && instruction.result().is_some_and(|value| uses[value] == 0);
        if sweeps && removed.insert(inst) {
            instruction.kind.visit_operands(|operand| drop_use(&mut uses, &mut dead, operand));
        }
    }
    (removed, uses, branches)
}

/// The cost of the removed and replaced instructions.
fn most_removed(
    func: &Function,
    target: &Target,
    removed: &FxHashSet<InstId>,
    replaced: &FxHashSet<InstId>,
) -> Cost {
    removed
        .iter()
        .chain(replaced.iter().filter(|inst| !removed.contains(inst)))
        .map(|&inst| target.instruction_estimate(func, inst))
        .sum::<Cost>()
}

/// Whether no rewrite of a region of one block can win, from the probe runs alone.
///
/// Every value the rewrite takes is among `plausible` and has one of the forms its probe
/// results and its result for zero fit, so the new code costs at least the cheapest such form
/// of one of them, in every dimension. Replacing more values removes no less, so what the
/// rewrite removes costs at most what replacing all of `plausible` removes. A proved value
/// that only removed code reads moves from the replaced to the removed instructions, which
/// changes neither. The objective order grows with every dimension, so when the cheapest form
/// of every value costs no less than that, no rewrite of the region is cheaper.
#[allow(clippy::too_many_arguments)]
fn hopeless(
    func: &Function,
    facts: &Facts,
    target: &Target,
    entry: BlockId,
    input: ValueId,
    plausible: &[(InstId, ValueId)],
    zero: &Run,
    runs: &[(u32, Run)],
) -> bool {
    let replaced: FxHashSet<InstId> = plausible.iter().map(|&(inst, _)| inst).collect();
    let (removed, _, branches) = removal(func, facts, target, entry, entry, &[], &replaced, input);
    let most = most_removed(func, target, &removed, &replaced) + branches;
    let forms = form_costs(target);
    plausible.iter().all(|(_, value)| {
        let at_zero = zero.known[value];
        let cheapest = forms
            .iter()
            .filter(|&&(shape, odd, _)| {
                zero_form(shape, at_zero) == Some(odd)
                    && runs.iter().all(|(m, run)| run.known[value] == shape_at(shape, *m))
            })
            .map(|&(.., cost)| cost)
            .reduce(|a, b| Cost {
                gas: a.gas.min(b.gas),
                bytes: a.bytes.min(b.bytes),
                steps: a.steps.min(b.steps),
            });
        cheapest.is_none_or(|cheapest| !target.cmp(cheapest, most).is_lt())
    })
}

/// The value of `shape` for the class `m`.
fn shape_at(shape: Shape, m: u32) -> U256 {
    match shape {
        Shape::Index(k) => U256::from(m >> k),
        Shape::Masked(k) => U256::from(m >> k << k),
        Shape::LeadingZeros => U256::from(255 - m),
        Shape::HalfPower => U256::ONE << (m >> 1) as usize,
    }
}

/// Whether `shape` with the value `zero` for zero needs `x | 1`, or `None` when neither
/// form gives it, as [`shape`] picks.
fn zero_form(shape: Shape, zero: U256) -> Option<bool> {
    let (odd, even) = match shape {
        Shape::Index(k) => (U256::ZERO, U256::MAX >> k as usize),
        Shape::Masked(k) => (U256::ZERO, U256::from(0xffu32 >> k << k)),
        Shape::LeadingZeros => (U256::from(255), U256::from(256)),
        Shape::HalfPower => (U256::ONE, U256::ZERO),
    };
    if zero == odd {
        Some(true)
    } else if zero == even {
        Some(false)
    } else {
        None
    }
}

/// The cost of every `CLZ` form, priced as the rewrite prices it, on a scratch function so that
/// pricing allocates nothing in the function rewritten.
fn form_costs(target: &Target) -> Vec<(Shape, bool, Cost)> {
    let mut scratch = Function::new(Ident::DUMMY);
    let input = scratch.alloc_value(Value::Undef(MirType::I256));
    let mut forms = Vec::new();
    for shape in Shape::all() {
        for odd in [true, false] {
            let mut cost = Cost::ZERO;
            let last = clz_form(&mut scratch, shape, odd, input, &mut |func, kind| {
                cost += target.kind_estimate(func, &kind);
                input
            });
            cost += target.kind_estimate(&scratch, &last);
            forms.push((shape, odd, cost));
        }
    }
    forms
}

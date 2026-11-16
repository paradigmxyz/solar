//! Replaces a search for the highest or the lowest set bit of a word with the `CLZ` opcode.
//!
//! Before Osaka the EVM had no bit scan, so libraries find the index of the highest set bit of a
//! word by halving: test the upper half, shift it down when it holds a bit, add the half's width
//! to the result, and repeat with half the width, often ending in a lookup table for the last
//! byte or nibble:
//!
//! ```text
//! r = (x > 2**128 - 1) << 7
//! r = r | ((x >> r > 2**64 - 1) << 6)
//! ...
//! r = r | byte(x >> r, table)
//! ```
//!
//! PRBMath's `msb`, Solady's `log2`, `fls` and `lnWad`, OpenZeppelin's `Math.log2` and Uniswap's
//! `BitMath` and `TickMath` all spell it differently: chained shifts or an accumulated shift, `or`
//! or `add`, branches that if-conversion turned into selects, a De Bruijn multiplication or a byte
//! table at the end. The initial estimate of PRBMath's and OpenZeppelin's `sqrt` keeps its
//! branches, because each arm halves the word and the estimate together:
//!
//! ```text
//! if (x >= 2**128) { x >>= 128; r <<= 64; }
//! if (x >= 2**64) { x >>= 64; r <<= 32; }
//! ...
//! if (x >= 4) { r <<= 1; }
//! ```
//!
//! which leaves `r = 2**(m / 2)` for the index `m` of the highest set bit. A scan takes 25 to 45
//! operations or seven branches; `255 - clz(x | 1)` computes the same index in three.
//!
//! # Recognition
//!
//! The pass does not match those shapes. It proves what a value computes. A test of the upper
//! half of a word `x` (a comparison with `2**128` or `2**128 - 1`, or a right shift by 128 bits)
//! starts a region at its block, and so does the lowest set bit of a word, `y & (0 - y)` or
//! `y & (~y + 1)`, as Solady's `ffs` and Uniswap's `BitMath.leastSignificantBit` isolate it
//! before a De Bruijn multiplication. The pass runs the region from there on inputs that stand
//! for classes of inputs, zero and the inputs whose highest set bit is `m` for every `m`, and
//! follows its branches until it reaches an instruction it cannot run: one with an effect, a
//! checked operation whose result it does not know, or a branch whose condition it does not know.
//!
//! A run tracks four kinds of value: known, unknown, `x` shifted right by a known amount, and
//! numbers fixed by the class and the bits the run fixed below its highest set bit. A right
//! shift by a known amount shifts a shifted `x` further, and a select or a phi passes it on. A
//! comparison of such a value with a constant `c` is known when it tests a threshold of a power of
//! two: `< c` with `c` zero or a power of two, `> c` with `c + 1` one, and `==` and `!=` with zero.
//! Every such test of `x >> s` asks whether `m >= s + k` for some `k`, so it has the same result
//! for every input with the same highest set bit as long as `s` does. Any other operation reads
//! `x >> s` as a number only when the class fixes it: when `s` is above `m`, so no bit is left,
//! or when the run also fixes the `w` bits below `m` and `s` is at least `m - w`. A run that
//! needs such bits asks for them, up to seven, and its class then runs once for each value of
//! them, each run going on from the instruction that asked: this is how a table indexed by the
//! top byte or nibble of `x` is proved. No branch or checked operation goes on with such a
//! number, so the runs of a region stop where the classes alone take them. Pure operations
//! on known values are known; anything else that reads `x` is unknown, and so is anything that
//! reads such a value. By induction every known value and every branch a run takes depends only
//! on the highest set bit of `x` and the bits the run fixed below it, so the runs of the 257
//! classes prove a value for every input. A lowest set bit takes only the values zero and `2**m`,
//! so a run on `2**m` knows every value it computes from it.
//!
//! # Rewrite
//!
//! All runs must stop in one block, the region's last, and the blocks they pass must form a
//! region its first block enters: the predecessors of every other block lie in it, the last
//! block leads back into none of them, and no run loops. A region first runs within its first
//! block alone, which proves the scans of straight-line code, and then along its branches. A
//! value of the last block that every run computed before stopping is a function of `m`, given
//! by a table of 257 entries, and it is replaced when the table is one of
//!
//! ```text
//! index >> k        (255 - clz(x | 1)) >> k, or with clz(x)
//! index >> k << k   (255 - clz(x | 1)) & (255 >> k << k), or with clz(x)
//! leading zeros     clz(x | 1), or clz(x)
//! 2**(index / 2)    1 << ((255 - clz(x | 1)) >> 1), or with clz(x)
//! ```
//!
//! with the form its value for zero picks. An index that is 256 for zero, as Solady's `fls`
//! returns, takes `(255 - clz(x | 1)) | ((x == 0) << 8)`, or `256 - clz(b - 1)` for a lowest set
//! bit `b`, whose `b - 1` has a one for each zero below the bit and every bit set for zero. The
//! index is a subtraction rather than an exclusive or so that a subtraction from it cancels and
//! range analysis bounds it by the count. Solady rounds the index down to a byte when it stops
//! its scan at the byte that holds the highest set bit. A proved value that only the removed
//! code and other proved values read goes with them rather than being rewritten, so a scan that
//! rounds its index in steps, or takes a count from an index, keeps only its last step. Nothing
//! else the region computes may be read past it: the first block then jumps straight to the last
//! one, and the other blocks, the phis of the last block and the instructions only the region
//! read go. The rewrite is taken when the target prices the new operations below the removed
//! instructions and branches. The new instructions take the source location of the value they
//! replace.
//!
//! # Limits and placement
//!
//! The target must have `CLZ` (Osaka). The region has no loop and, before its last block, no
//! instruction with an effect and no checked operation that some run does not know succeeds. The
//! runs of one function pass at most [`WORK`] instructions, and copying the values a run knows
//! counts against the same budget. A scan that narrows the word to more
//! than eight bits before reading it as a number keeps its code. The pass runs early in the
//! semantic pipeline, after the first e-graph run has put comparisons in canonical form, so that
//! the inliner prices a scan helper as the few operations it becomes. It runs again after
//! if-conversion, where scans that use checked additions and branches have become straight-line
//! code, and once more after ABI lowering, when a size build has inlined the helper calls of a
//! scan.
//!
//! The zero input and a few classes run first and reject most regions, and a region of one block
//! whose rewrite cannot pay with any form those runs leave stops there, charging the work the
//! other classes would take without forking.

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
/// The most bits of a class, its highest set bit and those below, that a run may fix.
const WINDOW: u32 = 8;

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
    /// The same number for every input of the class and the bits a run fixed below its highest
    /// set bit, or computed from the input as a number. A value proof reads it, but no branch
    /// or checked operation goes on with it, so every run of a region stops where the classes
    /// alone take it.
    Fixed(U256),
    /// Anything else.
    Unknown,
}

/// The inputs one run stands for: those that share the highest set bit of `probe` and the
/// `window` bits below it, or zero. The input of a lowest set bit is `probe` alone.
#[derive(Clone, Copy)]
struct Class {
    probe: U256,
    window: u32,
    one_bit: bool,
}

impl Class {
    /// The input shifted right by `s` bits as a number, or the window it needs, if any does.
    fn number(self, s: u32) -> Result<U256, Option<u32>> {
        let len = self.probe.bit_len() as u32;
        if s >= len {
            return Ok(U256::ZERO);
        }
        let depth = len - 1 - s;
        if depth <= self.window {
            Ok(self.probe >> s as usize)
        } else {
            Err((depth < WINDOW).then_some(depth))
        }
    }
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
        self.truncate(0);
        if self.words.len() < len {
            self.words.resize(len, Word::Unknown);
        }
    }

    fn insert(&mut self, value: ValueId, word: Word) {
        self.words[value] = word;
        self.set.push(value);
    }

    /// Clears the values set after the first `len`.
    fn truncate(&mut self, len: usize) {
        for value in self.set.drain(len..) {
            self.words[value] = Word::Unknown;
        }
    }

    /// The known values of the run, or only those among `keep`, which is sorted.
    fn known(&self, keep: Option<&[ValueId]>) -> Known {
        let known_word = |&value: &ValueId| match self.words[value] {
            Word::Known(c) | Word::Fixed(c) => Some((value, c)),
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
    /// Every shape a proved value can take; [`hopeless`] relies on the list being whole.
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

/// The word a `CLZ` form counts the leading zeros of, which sets its value for zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Form {
    /// The input itself.
    Plain,
    /// `x | 1`, which has the highest bit of `x` and counts 255 zeros for zero.
    Odd,
    /// `x | 1` for an index, with 256 put back for zero.
    Flagged,
    /// `b - 1` for a lowest set bit `b`: `256 - clz(b - 1)` is the index of the bit and 256 for
    /// zero.
    Below,
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
        if [true, false].into_iter().any(|confined| {
            rewrite_region(func, &facts, target, &mut env, &mut work, entry, input, confined)
        }) {
            changed = true;
            facts = Facts::new(func);
        }
        if work == 0 {
            break;
        }
    }
    changed
}

/// The blocks that test the upper half of a word or isolate its lowest set bit, and the word
/// or the bit, in block order.
fn candidates(func: &Function) -> Vec<(BlockId, ValueId)> {
    let mut candidates = Vec::new();
    let mut seen = FxHashSet::default();
    for (block, data) in func.blocks.iter_enumerated() {
        if matches!(data.terminator, None | Some(Terminator::Invalid)) {
            continue;
        }
        for &inst in &data.instructions {
            let input = half_test(func, inst).or_else(|| {
                func.inst_result_value(inst).filter(|&value| is_lowest_bit(func, value))
            });
            if let Some(input) = input
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
    /// The block of every instruction in a block.
    place: IndexVec<InstId, Option<BlockId>>,
    /// The reads of each value, by instructions in blocks and by terminators.
    uses: IndexVec<ValueId, u32>,
    /// The pure instructions of each block whose result nothing reads.
    unused: IndexVec<BlockId, Vec<InstId>>,
    candidates: Vec<(BlockId, ValueId)>,
}

impl Facts {
    fn new(func: &Function) -> Self {
        let mut uses = IndexVec::from_vec(vec![0u32; func.num_values()]);
        for data in func.blocks.iter() {
            for &inst in &data.instructions {
                func.inst(inst).kind.visit_operands(|operand| uses[operand] += 1);
            }
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
        Self { place: func.inst_block_table(), uses, unused, candidates: candidates(func) }
    }
}

/// Whether a run evaluates the instruction when it passes it, rather than stopping there. A
/// checked operation, which reverts for some operands, stops it unless the run knows its result.
fn evaluates(instruction: &Instruction) -> bool {
    is_pure(instruction)
        && !instruction.kind.effects().control.any()
        && !matches!(instruction.kind, InstKind::Phi(_))
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

/// Whether `value` is the lowest set bit of a word, `y & -y`, which has at most one bit set.
fn is_lowest_bit(func: &Function, value: ValueId) -> bool {
    let Some(&InstKind::And(a, b)) = func.value_inst_kind(value) else { return false };
    let negates = |negated, word| match func.value_inst_kind(negated) {
        // 0 - y
        Some(&InstKind::Sub(zero, y)) => y == word && func.value_u256(zero) == Some(U256::ZERO),
        // ~y + 1
        Some(&InstKind::Add(a, b)) => [(a, b), (b, a)].into_iter().any(|(not, one)| {
            func.value_u256(one) == Some(U256::ONE)
                && matches!(func.value_inst_kind(not), Some(&InstKind::Not(y)) if y == word)
        }),
        _ => false,
    };
    func.value_ty(value) == Some(MirType::I256) && (negates(a, b) || negates(b, a))
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

/// The value of `value` in a run of `class`.
fn word(func: &Function, env: &Env, input: ValueId, class: Class, value: ValueId) -> Word {
    if value == input {
        if class.one_bit { Word::Known(class.probe) } else { Word::Shifted(0) }
    } else if let Some(c) = func.value_u256(value) {
        Word::Known(c)
    } else {
        env.words[value]
    }
}

/// Where a run is: the next instruction to evaluate, the edge the run came along, and the blocks
/// it passed.
#[derive(Clone)]
struct Cursor {
    block: BlockId,
    index: usize,
    pred: Option<BlockId>,
    blocks: Vec<BlockId>,
}

/// What a run reached.
enum Reached {
    /// The instruction or terminator it could not run.
    Stop(Run),
    /// An instruction that reads `depth` bits below the highest set bit, more than the class
    /// fixes.
    Fork { depth: u32, cursor: Cursor },
}

/// Runs the region of `input` from `cursor` on `class`, or returns `None` when it loops or the
/// work runs out. A `confined` run stops at the end of its first block. The run keeps the known
/// values among `keep`, or all of them.
#[allow(clippy::too_many_arguments)]
fn run(
    func: &Function,
    input: ValueId,
    class: Class,
    confined: bool,
    keep: Option<&[ValueId]>,
    env: &mut Env,
    work: &mut u32,
    mut cursor: Cursor,
) -> Option<Reached> {
    loop {
        let data = &func.blocks[cursor.block];
        while let Some(&inst) = data.instructions.get(cursor.index) {
            charge(work, 1)?;
            let instruction = func.inst(inst);
            if is_pure(instruction)
                && let Some(value) = instruction.result()
            {
                // A checked operation on known values succeeds for every input of the class, as
                // its operands depend on the class alone, so the run goes on past it.
                match eval(func, &instruction.kind, env, input, class) {
                    Ok(word) if evaluates(instruction) || matches!(word, Word::Known(_)) => {
                        env.insert(value, word)
                    }
                    Ok(_) => return Some(Reached::Stop(stopped(env, keep, work, cursor)?)),
                    Err(depth) => return Some(Reached::Fork { depth, cursor }),
                }
            } else if !evaluates(instruction) {
                return Some(Reached::Stop(stopped(env, keep, work, cursor)?));
            }
            cursor.index += 1;
        }
        let next = match &data.terminator {
            _ if confined => None,
            Some(Terminator::Jump(next)) => Some(*next),
            Some(Terminator::Branch { condition, then_block, else_block }) => {
                match word(func, env, input, class, *condition) {
                    Word::Known(c) => Some(if c.is_zero() { *else_block } else { *then_block }),
                    _ => None,
                }
            }
            _ => None,
        };
        let Some(next) = next else {
            return Some(Reached::Stop(stopped(env, keep, work, cursor)?));
        };
        // A loop, or a return to the entry, is not a region the rewrite takes.
        if cursor.blocks.contains(&next) {
            return None;
        }
        cursor.blocks.push(next);
        cursor.pred = Some(cursor.block);
        cursor.block = next;
        cursor.index = 0;
        enter(func, env, input, class, &mut cursor)?;
    }
}

/// Sets the phis at `cursor`, which read the edge the run came along, all at once.
fn enter(
    func: &Function,
    env: &mut Env,
    input: ValueId,
    class: Class,
    cursor: &mut Cursor,
) -> Option<()> {
    let instructions = &func.blocks[cursor.block].instructions;
    let mut phis = SmallVec::<[(ValueId, Word); 4]>::new();
    while let Some(&inst) = instructions.get(cursor.index) {
        let InstKind::Phi(incoming) = &func.inst(inst).kind else { break };
        let value = func.inst_result_value(inst)?;
        let word = cursor
            .pred
            .and_then(|pred| incoming.iter().find(|&&(from, _)| from == pred))
            .map_or(Word::Unknown, |&(_, value)| word(func, env, input, class, value));
        phis.push((value, word));
        cursor.index += 1;
    }
    for (value, word) in phis {
        env.insert(value, word);
    }
    Some(())
}

/// Charges `n` instructions against `work`, or empties it and returns `None` when it runs out.
fn charge(work: &mut u32, n: usize) -> Option<()> {
    let left = u32::try_from(n).ok().and_then(|n| work.checked_sub(n));
    *work = left.unwrap_or(0);
    left.map(drop)
}

/// The run that stopped at `cursor`. Copying its known values costs work too: the runs of a
/// forked class each copy the values set before the fork, which they did not pay for.
fn stopped(env: &Env, keep: Option<&[ValueId]>, work: &mut u32, cursor: Cursor) -> Option<Run> {
    charge(work, keep.map_or(env.set.len(), <[_]>::len))?;
    Some(Run {
        stop_block: cursor.block,
        stop_at: cursor.index,
        known: env.known(keep),
        blocks: cursor.blocks,
    })
}

/// Evaluates one pure instruction of a run of `class`, or returns how many bits below the
/// highest set bit it reads when the class fixes fewer.
fn eval(
    func: &Function,
    kind: &InstKind,
    env: &Env,
    input: ValueId,
    class: Class,
) -> Result<Word, u32> {
    let get = |value| word(func, env, input, class, value);
    match *kind {
        InstKind::Shr(shift, value) => {
            if let (Word::Known(shift), Word::Shifted(s)) = (get(shift), get(value)) {
                let total = U256::from(s).saturating_add(shift);
                return Ok(if total >= U256::from(256) {
                    Word::Known(U256::ZERO)
                } else {
                    Word::Shifted(total.to::<u32>())
                });
            }
        }
        InstKind::Lt(a, b) | InstKind::Gt(a, b) | InstKind::Eq(a, b) | InstKind::Ne(a, b) => {
            let decided = match (get(a), get(b)) {
                (Word::Shifted(s), Word::Known(c)) => Some((s, c, true)),
                (Word::Known(c), Word::Shifted(s)) => Some((s, c, false)),
                _ => None,
            };
            if let Some((s, c, shifted_first)) = decided
                && decides(kind, shifted_first, c)
            {
                let x = class.probe >> s as usize;
                let (a, b) = if shifted_first { (x, c) } else { (c, x) };
                let result = match kind {
                    InstKind::Lt(..) => a < b,
                    InstKind::Gt(..) => a > b,
                    InstKind::Eq(..) => a == b,
                    _ => a != b,
                };
                return Ok(Word::Known(U256::from(result)));
            }
        }
        InstKind::Select(condition, a, b) => {
            return Ok(match get(condition) {
                Word::Known(c) => get(if c.is_zero() { b } else { a }),
                Word::Fixed(c) => match get(if c.is_zero() { b } else { a }) {
                    Word::Known(c) | Word::Fixed(c) => Word::Fixed(c),
                    _ => Word::Unknown,
                },
                _ => Word::Unknown,
            });
        }
        _ => {}
    }
    // Every other operation reads numbers.
    let mut operands = SmallVec::<[(ValueId, U256); 3]>::new();
    let mut needed = None;
    let mut known = true;
    let mut fixed = false;
    kind.visit_operands(|operand| match get(operand) {
        Word::Known(c) => operands.push((operand, c)),
        Word::Fixed(c) => {
            fixed = true;
            operands.push((operand, c));
        }
        Word::Shifted(s) => match class.number(s) {
            Ok(c) => {
                fixed = true;
                operands.push((operand, c));
            }
            Err(Some(depth)) => needed = needed.max(Some(depth)),
            Err(None) => known = false,
        },
        Word::Unknown => known = false,
    });
    if !known {
        return Ok(Word::Unknown);
    }
    if let Some(depth) = needed {
        return Err(depth);
    }
    let result = eval_typed_inst(func, kind, |operand| {
        operands.iter().find(|&&(value, _)| value == operand).map(|&(_, c)| c).ok_or(())
    });
    Ok(match result {
        Ok(Some(c)) if fixed => Word::Fixed(c),
        Ok(Some(c)) => Word::Known(c),
        _ => Word::Unknown,
    })
}

/// The form of `shape` for a value with the results `runs` and `zero`, if the shape fits it.
fn form_of(
    runs: &[(u32, Run)],
    value: ValueId,
    zero: U256,
    one_bit: bool,
    shape: Shape,
) -> Option<Form> {
    let form = zero_form(shape, zero, one_bit)?;
    runs.iter().all(|(m, run)| run.known[&value] == shape_at(shape, *m)).then_some(form)
}

/// The shape and form of a value with the results `runs` and `zero`, if it has one.
fn fit(runs: &[(u32, Run)], value: ValueId, zero: U256, one_bit: bool) -> Option<(Shape, Form)> {
    Shape::all().find_map(|shape| Some((shape, form_of(runs, value, zero, one_bit, shape)?)))
}

fn immediate(func: &mut Function, value: u64) -> ValueId {
    func.alloc_value(Value::Immediate(Immediate::I256(U256::from(value))))
}

/// Emits the `CLZ` form of `shape` through `emit`, which returns the result of each
/// instruction, and returns its last operation unemitted.
fn clz_form(
    func: &mut Function,
    shape: Shape,
    form: Form,
    input: ValueId,
    emit: &mut dyn FnMut(&mut Function, InstKind, MirType) -> ValueId,
) -> InstKind {
    let one = immediate(func, 1);
    if form == Form::Below {
        // %below = sub %input, 1
        // %zeros = clz %below
        // %index = sub 256, %zeros
        let below = emit(func, InstKind::Sub(input, one), MirType::I256);
        let zeros = emit(func, InstKind::Clz(below), MirType::I256);
        let bits = immediate(func, 256);
        return InstKind::Sub(bits, zeros);
    }
    let all = immediate(func, 255);
    // %word = or %input, 1
    let word = if form == Form::Plain {
        input
    } else {
        emit(func, InstKind::Or(input, one), MirType::I256)
    };
    if shape == Shape::LeadingZeros {
        // %zeros = clz %word
        return InstKind::Clz(word);
    }
    // %zeros = clz %word
    // %index = sub 255, %zeros
    let zeros = emit(func, InstKind::Clz(word), MirType::I256);
    let index = InstKind::Sub(all, zeros);
    match shape {
        Shape::Index(0) if form == Form::Flagged => {
            // %index = sub 255, %zeros
            // %is_zero = eq %input, 0
            // %flag = zext i1 %is_zero to i256
            // %none = shl %flag, 8
            // %result = or %index, %none
            let index = emit(func, index, MirType::I256);
            let zero = immediate(func, 0);
            let is_zero = emit(func, InstKind::Eq(input, zero), MirType::I1);
            let flag = emit(func, InstKind::Zext(is_zero), MirType::I256);
            let eight = immediate(func, 8);
            let none = emit(func, InstKind::Shl(eight, flag), MirType::I256);
            InstKind::Or(index, none)
        }
        Shape::Index(0) => index,
        Shape::Index(k) => {
            // %result = shr %index, k
            let index = emit(func, index, MirType::I256);
            let k = immediate(func, u64::from(k));
            InstKind::Shr(k, index)
        }
        Shape::Masked(k) => {
            // %result = and %index, 255 >> k << k
            let index = emit(func, index, MirType::I256);
            let mask = immediate(func, u64::from(0xffu32 >> k << k));
            InstKind::And(index, mask)
        }
        _ => {
            // %half = shr %index, 1
            // %result = shl 1, %half
            let index = emit(func, index, MirType::I256);
            let half = emit(func, InstKind::Shr(one, index), MirType::I256);
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
fn computed(
    func: &Function,
    last: BlockId,
    first: usize,
    runs: &[(u32, Run)],
) -> Vec<(InstId, ValueId)> {
    let limit = runs.iter().map(|(_, run)| run.stop_at).min().unwrap_or(0);
    let instructions = &func.blocks[last].instructions;
    let range = first.min(limit)..limit.min(instructions.len());
    instructions[range]
        .iter()
        .filter_map(|&inst| {
            let value = func.inst_result_value(inst)?;
            (func.value_ty(value) == Some(MirType::I256)
                && runs.iter().all(|(_, run)| run.known.contains_key(&value)))
            .then_some((inst, value))
        })
        .collect()
}

/// Proves and rewrites the region of `input` that starts at `entry`, within `entry` alone when
/// `confined`.
#[allow(clippy::too_many_arguments)]
fn rewrite_region(
    func: &mut Function,
    facts: &Facts,
    target: &Target,
    env: &mut Env,
    work: &mut u32,
    entry: BlockId,
    input: ValueId,
    confined: bool,
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
    let one_bit = is_lowest_bit(func, input);
    // The runs of the class of zero or of the highest set bit `m`.
    let class = |m: Option<u32>, keep: Option<&[ValueId]>, env: &mut Env, work: &mut u32| {
        env.clear(func.num_values());
        let probe = m.map_or(U256::ZERO, |m| U256::ONE << m as usize);
        let class = Class { probe, window: 0, one_bit };
        let mut cursor = Cursor { block: entry, index: start, pred: None, blocks: vec![entry] };
        enter(func, env, input, class, &mut cursor)?;
        let m = m.unwrap_or(0);
        let mut runs = Vec::new();
        // Where an instruction reads more bits below the highest set bit than the class fixes,
        // the class runs once for each value of those bits, each going on from that instruction.
        let mut pending = vec![(class, cursor, env.set.len())];
        while let Some((class, cursor, set)) = pending.pop() {
            env.truncate(set);
            match run(func, input, class, confined, keep, env, work, cursor)? {
                Reached::Stop(run) => runs.push((m, run)),
                Reached::Fork { depth, cursor } => {
                    let fixed = class.probe >> (m - class.window) as usize;
                    let extra = depth - class.window;
                    for low in (0..1u32 << extra).rev() {
                        let top = (fixed << extra as usize) | U256::from(low);
                        let probe = top << (m - depth) as usize;
                        let class = Class { probe, window: depth, ..class };
                        pending.push((class, cursor.clone(), env.set.len()));
                    }
                }
            }
        }
        Some(runs)
    };

    // The zero input and a few classes first; they reject most regions. A region that stays in
    // the entry is the one its confined runs saw.
    let mut runs: Vec<(u32, Run)> = Vec::new();
    let Some(zero) = class(None, None, env, work).and_then(|mut runs| runs.pop()) else {
        return false;
    };
    let zero = zero.1;
    let last = zero.stop_block;
    if !confined && last == entry {
        return false;
    }
    for m in PROBES {
        let Some(class) = class(Some(m), None, env, work) else {
            return false;
        };
        if class.iter().any(|(_, run)| run.stop_block != last) {
            return false;
        }
        runs.extend(class);
    }
    let plausible: Vec<(InstId, ValueId)> = computed(func, last, first(last), &runs)
        .into_iter()
        .filter(|(_, value)| {
            zero.known
                .get(value)
                .is_some_and(|&at_zero| fit(&runs, *value, at_zero, one_bit).is_some())
        })
        .collect();
    if plausible.is_empty() {
        return false;
    }

    // Confined runs all stop at the same place, so the region and what the rewrite can remove
    // are known now. When no value can take a form cheaper than what replacing them all removes,
    // the other classes cannot change the answer: charge the work they would take without
    // forking and stop.
    if confined && hopeless(func, facts, target, entry, input, one_bit, &plausible, &zero, &runs) {
        let instructions = &func.blocks[entry].instructions;
        let from = start
            + instructions[start..]
                .iter()
                .take_while(|&&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)))
                .count();
        let passed = zero.stop_at - from + usize::from(zero.stop_at < instructions.len());
        let classes = 256 - PROBES.len();
        let total = ((passed + plausible.len()) as u64).saturating_mul(classes as u64);
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
        let Some(class) = class(Some(m), Some(&keep), env, work) else {
            return false;
        };
        if class.iter().any(|(_, run)| run.stop_block != last) {
            return false;
        }
        runs.extend(class);
    }
    let mut proved = Vec::new();
    for (inst, value) in computed(func, last, first(last), &runs) {
        let Some(&at_zero) = zero.known.get(&value) else { continue };
        if zero.stop_at < func.blocks[last].instructions.len()
            && func.blocks[last].instructions[..zero.stop_at].iter().all(|&i| i != inst)
        {
            continue;
        }
        if let Some((shape, form)) = fit(&runs, value, at_zero, one_bit) {
            proved.push((inst, value, shape, form));
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
    if last != entry {
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
    let forms = form_costs(target);
    for &(_, _, shape, form) in &proved {
        new += forms.iter().find(|&&(s, f, _)| (s, f) == (shape, form)).expect("every form").2;
    }
    if !target.cmp(new, old).is_lt() {
        return false;
    }

    // Rewrite.
    let mut replacements = FxHashMap::default();
    let mut front = Vec::new();
    let mut before: FxHashMap<InstId, Vec<InstId>> = FxHashMap::default();
    for &(inst, value, shape, form) in &proved {
        let metadata = func.inst(inst).metadata.debug_context();
        let mut inserted = Vec::new();
        let last_kind = clz_form(func, shape, form, input, &mut |func, kind, ty| {
            let mut new = Instruction::new(kind, Some(ty));
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
        let sweeps = facts.place[inst].is_some_and(|block| block == entry || block == last)
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
    one_bit: bool,
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
            .filter(|&&(shape, form, _)| {
                form_of(runs, *value, at_zero, one_bit, shape) == Some(form)
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

/// The form of `shape` whose value for zero is `zero`, if one has it.
fn zero_form(shape: Shape, zero: U256, one_bit: bool) -> Option<Form> {
    let (odd, plain) = match shape {
        Shape::Index(k) => (U256::ZERO, U256::MAX >> k as usize),
        Shape::Masked(k) => (U256::ZERO, U256::from(0xffu32 >> k << k)),
        Shape::LeadingZeros => (U256::from(255), U256::from(256)),
        Shape::HalfPower => (U256::ONE, U256::ZERO),
    };
    if zero == odd {
        Some(Form::Odd)
    } else if zero == plain {
        Some(Form::Plain)
    } else if shape == Shape::Index(0) && zero == U256::from(256) {
        Some(if one_bit { Form::Below } else { Form::Flagged })
    } else {
        None
    }
}

/// The cost of every `CLZ` form, priced as the rewrite prices it, on a scratch function so that
/// pricing allocates nothing in the function rewritten.
fn form_costs(target: &Target) -> Vec<(Shape, Form, Cost)> {
    let mut scratch = Function::new(Ident::DUMMY);
    let input = scratch.alloc_value(Value::Undef(MirType::I256));
    let mut forms = Vec::new();
    for shape in Shape::all() {
        let wide: &[Form] =
            if shape == Shape::Index(0) { &[Form::Flagged, Form::Below] } else { &[] };
        for &form in [Form::Odd, Form::Plain].iter().chain(wide) {
            let mut cost = Cost::ZERO;
            let last = clz_form(&mut scratch, shape, form, input, &mut |func, kind, _| {
                cost += target.kind_estimate(func, &kind);
                input
            });
            cost += target.kind_estimate(&scratch, &last);
            forms.push((shape, form, cost));
        }
    }
    forms
}

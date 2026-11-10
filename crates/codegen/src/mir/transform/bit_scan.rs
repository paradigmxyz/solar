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
//! PRBMath's `msb`, Solady's `log2`, `fls`, `clz` and `lnWad`, OpenZeppelin's `Math.log2` and
//! Uniswap's `BitMath.mostSignificantBit` and `TickMath` all spell it differently: chained
//! shifts or an accumulated shift, `or` or `add`, branches that if-conversion turned into
//! selects, a De Bruijn multiplication or a byte table at the end. The scan takes 25 to 45
//! operations; `255 - clz(x | 1)` computes the same index in five.
//!
//! # Recognition
//!
//! The pass does not match those shapes. It proves what a value computes. A test of the upper
//! half of a word `x`, a comparison with `2**128` or `2**128 - 1` or a right shift by 128 bits,
//! marks `x` as the possible input of a scan. The pure instructions of that block that cannot
//! revert and whose operands are `x`, constants, and earlier such instructions are the cone of
//! `x`: every value in it is a function of `x` alone, and computing it does nothing else. A
//! checked operation is not one of them: the proof does not evaluate the operand a select
//! leaves unchosen, so it could not tell that an operation there never reverts.
//!
//! A few concrete inputs first discard the values of the cone that are neither the index `m` of
//! the highest set bit nor `255 - m`, the number of leading zeros. Each remaining value is then
//! proved for every input, by evaluating the instructions it depends on over classes of inputs
//! rather than over inputs. A class fixes the highest set bit and up to seven bits below it and
//! leaves the lower bits free. In a class the input is not a number: it is "`x` shifted right by
//! `s` bits", and only three kinds of operation accept it. A right shift by a known amount
//! shifts further, and yields a number once no free bit is left. A comparison with a known
//! number is decided when the number lies outside the range the class leaves for the shifted
//! input. A select with a known condition and a widening cast pass it on. Every other operation
//! on it is not evaluated, and neither is anything that reads such a result. Operations on known
//! numbers go through the same evaluator constant folding uses. When free bits keep an operation
//! from its result, the class is split on as many of them as let the operation go on, down to
//! eight known bits, which is as far as a scan narrows the word before its table.
//!
//! The classes of one highest bit partition the inputs with that bit, so a value that evaluates to
//! `m` in every class of every `m` is the index of the highest set bit for every nonzero input,
//! whatever the free bits are; no instruction that produced it could read them. Zero is one more
//! class. The proof rests on the evaluation rules alone, not on the shape of the source, so a
//! table with one wrong entry, a threshold off by one, or a scan that stops early fails it and
//! keeps its code.
//!
//! # Lowest set bit
//!
//! Libraries find the lowest set bit by isolating it, `x & -x`, and mapping the result to its
//! index with a De Bruijn multiplication, as Solady's `ffs` and Uniswap's
//! `BitMath.leastSignificantBit` do. The isolated bit is the input of that scan, and its index is
//! the index of its highest set bit. It has at most one bit set, so it takes 256 nonzero values:
//! the values of its cone are evaluated on each of them, which is the whole proof, and no class
//! is needed. `x & (0 - x)` and `x & (~x + 1)` are the isolations the pass knows.
//!
//! # Rewrite
//!
//! A proved value is replaced where it stands by the form that matches what it returns for zero:
//!
//! ```text
//! index, 0 for zero          255 - clz(x | 1)
//! index, 256 for zero        (255 - clz(x | 1)) | ((x == 0) << 8)
//! index, 2**256 - 1 for zero 255 - clz(x)
//! leading zeros, 256         clz(x)
//! leading zeros, 255         clz(x | 1)
//! ```
//!
//! Any other result for zero keeps the code. The instructions only the scan used are removed with
//! it, and the rewrite is taken when the target prices the new operations below the old ones.
//! The new instructions take the source location of the value they replace. The last proved
//! value of a block is replaced first: where code takes a count from an index, or gives zero its
//! own result after the search, the outer value is a scan as well, and the search goes with it.
//! The index is a subtraction rather than an exclusive or so that a subtraction from it cancels
//! and range analysis bounds it by the count.
//!
//! # Limits and placement
//!
//! The target must have `CLZ` (Osaka). The cone lies in one block: a scan written with branches
//! is found once if-conversion has made it straight-line. A scan that narrows the word to more
//! than eight bits before a table would need more classes than the pass tries. The instructions
//! evaluated for one input are bounded, at several times what the proofs of a library's scans
//! take, so that code with hundreds of values that pass the probes cannot make the pass slow:
//! the values left when the bound is reached keep their code.
//!
//! The pass runs early in the semantic pipeline, after the first e-graph run has put comparisons
//! in canonical form, so that the inliner prices a scan helper as the few operations it becomes.
//! It runs again after arithmetic lowering and if-conversion, where the scans that use checked
//! additions and branches have become straight-line code, and once more after ABI lowering, when
//! a size build has inlined the helper calls of a scan.

use super::word_sequence::is_pure;
use crate::{
    mir::{
        BlockId, Function, Immediate, InstId, InstKind, Instruction, MirType, Module, Semantics,
        Value, ValueId,
        pass::{MirPass, ModuleAnalyses, run_function_pass},
        utils::eval::eval_typed_inst,
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::{
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use solar_sema::Gcx;
use std::{cell::Cell, cmp::Reverse, collections::BinaryHeap};

/// Bound on the instructions of one cone.
const MAX_CONE: usize = 256;
/// Bound on the instructions one scan may depend on.
const MAX_SCAN: usize = 96;
/// The leading bits a class may fix: a scan narrows its word to a byte before a table.
const WINDOW: u32 = 8;
/// Bound on the instructions evaluated for one input, over all its probes and proofs. The
/// proof of a library's scan evaluates 10 to 50 thousand, and an input has a scan or two.
const MAX_WORK: u32 = 1 << 18;

pub(crate) struct BitScan;

impl MirPass for BitScan {
    fn name(&self) -> &'static str {
        "bit-scan"
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

/// A word of the evaluation of one class of inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Word {
    /// The same number for every input of the class.
    Known(U256),
    /// The input shifted right by this many bits, fewer than the class leaves free.
    Shifted(u32),
}

/// The inputs that start with the bits of `lead`, whose highest bit is set, followed by
/// `free` bits of any value. A `lead` of zero is the class of the zero input.
#[derive(Clone, Copy, Debug)]
struct Class {
    lead: u32,
    free: u32,
}

impl Class {
    const ZERO: Self = Self { lead: 0, free: 0 };

    /// Every input whose highest set bit is `msb`.
    const fn with_msb(msb: u32) -> Self {
        Self { lead: 1, free: msb }
    }

    /// The classes that fix the next `bits` free bits, or as many as the window has room for; none
    /// when it has room for no bit or no bit is free.
    fn reveal(self, bits: u32) -> impl Iterator<Item = Self> {
        let room = WINDOW - (32 - self.lead.leading_zeros());
        let bits = bits.min(room).min(self.free);
        let count = if bits == 0 { 0 } else { 1 << bits };
        (0..count).map(move |low| Self { lead: self.lead << bits | low, free: self.free - bits })
    }

    /// The input shifted right by `shift` bits.
    fn shifted(self, shift: u32) -> Word {
        if shift < self.free {
            Word::Shifted(shift)
        } else {
            Word::Known(U256::from(self.lead.checked_shr(shift - self.free).unwrap_or(0)))
        }
    }

    /// The least and the greatest value of the input shifted right by `shift` bits, fewer than
    /// the free ones.
    fn bounds(self, shift: u32) -> (U256, U256) {
        let free = self.free - shift;
        let low = U256::from(self.lead) << free;
        (low, low | (U256::MAX >> (256 - free)))
    }
}

/// Where an instruction of the cone finds an operand.
#[derive(Clone, Copy)]
enum Slot {
    Input,
    Step(u32),
    Constant(U256),
}

/// One instruction of the cone.
struct Step {
    inst: InstId,
    operands: SmallVec<[(ValueId, Slot); 3]>,
}

/// What a proved scan computes.
#[derive(Clone, Copy)]
struct Scan {
    /// Whether the value counts leading zeros, `255 - m`, instead of the index `m`.
    leading_zeros: bool,
    /// The value for a zero input.
    zero: U256,
}

/// The pure instructions of one block that compute functions of `input` alone, in block order.
struct Cone<'a> {
    input: ValueId,
    steps: Vec<Step>,
    /// The instructions the pass may still evaluate for this input.
    work: &'a Cell<u32>,
}

fn run_function(func: &mut Function, target: &Target) -> bool {
    let mut inputs = Vec::new();
    let mut found = FxHashSet::default();
    for (block, data) in func.blocks.iter_enumerated() {
        for &inst in &data.instructions {
            let input = half_test(func, inst).or_else(|| {
                func.inst_result_value(inst).filter(|&value| is_lowest_bit(func, value))
            });
            if let Some(input) = input
                && found.insert((block, input))
            {
                inputs.push((block, input));
            }
        }
    }
    let mut index = Index::default();
    let mut changed = false;
    for (block, input) in inputs {
        changed |= rewrite_scans(func, target, &mut index, block, input);
    }
    index.apply(func);
    changed
}

/// The blocks the pass works in, as its rewrites leave them. A function may test the upper half
/// of thousands of words in one block, so the cone of an input is found from the readers of its
/// values, and a rewrite is recorded here and reaches the block once, when the pass is done
/// with the function: neither walks the block.
#[derive(Default)]
struct Index {
    blocks: FxHashMap<BlockId, BlockIndex>,
    /// How many operands read each value, counted at the first rewrite.
    uses: Option<IndexVec<ValueId, u32>>,
}

#[derive(Default)]
struct BlockIndex {
    /// Where each instruction stands in the block. The instructions of a rewrite stand between
    /// the value they replace and the instruction before it.
    places: FxHashMap<InstId, (u32, u32)>,
    /// The instructions of the block that read each value.
    readers: FxHashMap<ValueId, SmallVec<[InstId; 2]>>,
    /// The instructions that rewrites removed.
    removed: FxHashSet<InstId>,
    /// The instructions that rewrites put before an instruction.
    inserted: FxHashMap<InstId, Vec<InstId>>,
}

impl Index {
    fn block(&mut self, func: &Function, block: BlockId) -> &mut BlockIndex {
        self.blocks.entry(block).or_insert_with(|| {
            let mut index = BlockIndex::default();
            for (place, &inst) in func.blocks[block].instructions.iter().enumerate() {
                index.places.insert(inst, (place as u32 + 1, 0));
                index.read(func, inst);
            }
            index
        })
    }

    /// Makes the blocks what the rewrites left of them.
    fn apply(self, func: &mut Function) {
        for (block, index) in self.blocks {
            if index.removed.is_empty() && index.inserted.is_empty() {
                continue;
            }
            let old = std::mem::take(&mut func.blocks[block].instructions);
            let instructions = &mut func.blocks[block].instructions;
            for inst in old {
                let before = index.inserted.get(&inst).into_iter().flatten();
                for inst in before.chain([&inst]) {
                    if !index.removed.contains(inst) {
                        instructions.push(*inst);
                    }
                }
            }
        }
    }
}

impl BlockIndex {
    /// Records `inst` as a reader of its operands.
    fn read(&mut self, func: &Function, inst: InstId) {
        func.inst(inst).kind.visit_operands(|operand| {
            let readers = self.readers.entry(operand).or_default();
            if readers.last() != Some(&inst) {
                readers.push(inst);
            }
        });
    }
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

/// The word that `inst` tests for a bit in its upper half, the first step of a scan: by comparing
/// it with `2**128`, or by shifting its lower half out, which is how a comparison with a wide
/// constant is spelled when code size is what counts.
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

/// Rewrites every scan of `input` in `block`.
fn rewrite_scans(
    func: &mut Function,
    target: &Target,
    index: &mut Index,
    block: BlockId,
    input: ValueId,
) -> bool {
    let mut changed = false;
    // Each instruction is tried once: a rewrite changes the cone, which is then built again.
    let mut tried = FxHashSet::default();
    let one_bit = is_lowest_bit(func, input);
    let work = &Cell::new(MAX_WORK);
    loop {
        // The values left untried when the work runs out keep their code.
        if work.get() == 0 {
            return changed;
        }
        let cone = Cone::new(func, index.block(func, block), input, work);
        let mut rewritten = false;
        // The last value first: where a count is taken from an index, or zero is given its own
        // result after the search, the outer value is a scan too, and the inner one goes with it.
        for (root, leading_zeros) in cone.candidates(func, one_bit).into_iter().rev() {
            if !tried.insert(cone.steps[root].inst) {
                continue;
            }
            let needed = cone.needed(root);
            if needed.len() > MAX_SCAN {
                continue;
            }
            let Some(scan) = cone.prove(func, &needed, root, leading_zeros, one_bit) else {
                continue;
            };
            if let Some(inserted) = rewrite(func, target, index, block, &cone, root, scan) {
                tried.extend(inserted);
                rewritten = true;
                break;
            }
        }
        if !rewritten {
            return changed;
        }
        changed = true;
    }
}

impl<'a> Cone<'a> {
    fn new(func: &Function, index: &BlockIndex, input: ValueId, work: &'a Cell<u32>) -> Self {
        let mut steps = Vec::<Step>::new();
        let mut step_of = FxHashMap::default();
        // The readers of the input and of the steps, in block order: an instruction is reached
        // after every step it reads, since each of them stands before it.
        let mut reached = FxHashSet::default();
        let mut pending = BinaryHeap::new();
        let mut reach = |value, pending: &mut BinaryHeap<_>| {
            for &reader in index.readers.get(&value).into_iter().flatten() {
                if let Some(&place) = index.places.get(&reader)
                    && reached.insert(reader)
                {
                    pending.push(Reverse((place, reader)));
                }
            }
        };
        reach(input, &mut pending);
        while let Some(Reverse((_, inst))) = pending.pop() {
            if steps.len() == MAX_CONE {
                break;
            }
            let data = func.inst(inst);
            // A value of the cone goes with the scan that alone reads it, so it must leave nothing
            // but its result: a checked operation, which reverts for some operands, stays out.
            if !is_pure(data)
                || data.kind.effects().control.any()
                || matches!(data.kind, InstKind::Phi(..))
            {
                continue;
            }
            let Some(result) = func.inst_result_value(inst) else { continue };
            let mut operands = SmallVec::new();
            let mut reads_input = false;
            for value in data.kind.operands() {
                let slot = if value == input {
                    Slot::Input
                } else if let Some(&step) = step_of.get(&value) {
                    Slot::Step(step)
                } else if let Some(constant) = func.value_u256(value) {
                    Slot::Constant(constant)
                } else {
                    break;
                };
                reads_input |= !matches!(slot, Slot::Constant(_));
                operands.push((value, slot));
            }
            if reads_input && operands.len() == data.kind.operands().len() {
                step_of.insert(result, steps.len() as u32);
                steps.push(Step { inst, operands });
                reach(result, &mut pending);
            }
        }
        Self { input, steps, work }
    }

    /// The word-sized values of the cone that agree with a scan on a few inputs, with whether
    /// they count leading zeros. An input of at most one bit has 256 nonzero values, and the
    /// candidates agree with a scan on every one.
    fn candidates(&self, func: &Function, one_bit: bool) -> Vec<(usize, bool)> {
        let mut candidates = Vec::new();
        let mut words = vec![None; self.steps.len()];
        let all = (0..self.steps.len() as u32).collect::<Vec<_>>();
        let every_bit = (0..256).map(|bit| (1, bit));
        let probes = PROBES.iter().copied().take_while(|_| !one_bit);
        for (probe, (lead, shift)) in probes.chain(every_bit.take_while(|_| one_bit)).enumerate() {
            let input = U256::from(lead) << shift;
            let msb = U256::from(255 - input.leading_zeros());
            words.fill(None);
            self.evaluate(func, &all, Word::Known(input), Class::ZERO, &mut words);
            let agrees = |step: usize, leading_zeros: bool| {
                let expected = if leading_zeros { U256::from(255) - msb } else { msb };
                words[step] == Some(Word::Known(expected))
            };
            if probe == 0 {
                for (step, data) in self.steps.iter().enumerate() {
                    if func.inst(data.inst).result_ty != Some(MirType::I256) {
                        continue;
                    }
                    candidates.extend(
                        [false, true]
                            .into_iter()
                            .filter(|&kind| agrees(step, kind))
                            .map(|kind| (step, kind)),
                    );
                }
            } else {
                candidates.retain(|&(step, leading_zeros)| agrees(step, leading_zeros));
            }
            if candidates.is_empty() {
                break;
            }
        }
        candidates
    }

    /// The steps that `root` depends on, itself included, in block order.
    fn needed(&self, root: usize) -> Vec<u32> {
        let mut needed = vec![root as u32];
        let mut seen = FxHashSet::default();
        let mut next = 0;
        while let Some(&step) = needed.get(next) {
            next += 1;
            for &(_, slot) in &self.steps[step as usize].operands {
                if let Slot::Step(operand) = slot
                    && seen.insert(operand)
                {
                    needed.push(operand);
                }
            }
        }
        needed.sort_unstable();
        needed
    }

    /// Proves that `root`, a candidate, is the index of the highest set bit of every nonzero
    /// input, or the number of zeros above it, and returns what it is for zero.
    fn prove(
        &self,
        func: &Function,
        needed: &[u32],
        root: usize,
        leading_zeros: bool,
        one_bit: bool,
    ) -> Option<Scan> {
        let mut scratch = vec![vec![None; root + 1]; WINDOW as usize];
        self.evaluate(func, needed, Word::Known(U256::ZERO), Class::ZERO, &mut scratch[0]);
        let Some(Word::Known(zero)) = scratch[0][root] else { return None };
        // The candidates of an input of one bit have been tried on each of its values.
        if one_bit {
            return Some(Scan { leading_zeros, zero });
        }
        // The widest classes first: a value the classes cannot settle fails on them at once.
        for msb in (0..256).rev() {
            let expected = U256::from(if leading_zeros { 255 - msb } else { msb });
            let class = Class::with_msb(msb);
            if !self.prove_class(func, needed, root, class, expected, &mut scratch, 0) {
                return None;
            }
        }
        Some(Scan { leading_zeros, zero })
    }

    /// Whether `root` is `expected` for every input of `class`. The words of the enclosing class
    /// are in `scratch[depth - 1]`; the numbers among them hold in this class too.
    #[allow(clippy::too_many_arguments)]
    fn prove_class(
        &self,
        func: &Function,
        needed: &[u32],
        root: usize,
        class: Class,
        expected: U256,
        scratch: &mut [Vec<Option<Word>>],
        depth: usize,
    ) -> bool {
        let (outer, inner) = scratch.split_at_mut(depth);
        let words = &mut inner[0];
        match outer.last() {
            // A word of the enclosing class holds here too: a number as it is, and the shifted
            // input as this class knows it.
            Some(outer) => {
                for (word, &outer) in words.iter_mut().zip(outer) {
                    *word = outer.map(|word| match word {
                        Word::Known(_) => word,
                        Word::Shifted(by) => class.shifted(by),
                    });
                }
            }
            None => words.fill(None),
        }
        let blocked = self.evaluate(func, needed, class.shifted(0), class, words);
        match (words[root], blocked) {
            (Some(Word::Known(value)), _) => value == expected,
            // Free bits kept an operation from its result: fix as many as let it go on.
            (_, Some(bits)) => {
                let mut classes = class.reveal(bits).peekable();
                classes.peek().is_some()
                    && classes.all(|class| {
                        self.prove_class(func, needed, root, class, expected, scratch, depth + 1)
                    })
            }
            (_, None) => false,
        }
    }

    /// Evaluates the steps of `needed` that have no word yet, for the inputs of `class`, which
    /// `input` stands for. Returns the fewest free bits that, once fixed, let an operation that
    /// the free bits blocked go on, if they blocked any.
    fn evaluate(
        &self,
        func: &Function,
        needed: &[u32],
        input: Word,
        class: Class,
        words: &mut [Option<Word>],
    ) -> Option<u32> {
        let mut blocked = None::<u32>;
        for &step in needed {
            if words[step as usize].is_none() {
                // Out of work: the step has no word, and so nothing that reads it is proved.
                let Some(work) = self.work.get().checked_sub(1) else { break };
                self.work.set(work);
                let mut bits = None;
                words[step as usize] =
                    self.evaluate_step(func, step as usize, input, class, words, &mut bits);
                if let Some(bits) = bits {
                    blocked = Some(blocked.map_or(bits, |blocked| blocked.min(bits)));
                }
            }
        }
        blocked
    }

    /// Evaluates one step. When free bits of the input keep it from a result, `blocked` is the
    /// number of them to fix before it can have one.
    fn evaluate_step(
        &self,
        func: &Function,
        step: usize,
        input: Word,
        class: Class,
        words: &[Option<Word>],
        blocked: &mut Option<u32>,
    ) -> Option<Word> {
        let step = &self.steps[step];
        let word = |slot| match slot {
            Slot::Input => Some(input),
            Slot::Step(step) => words[step as usize],
            Slot::Constant(constant) => Some(Word::Known(constant)),
        };
        let kind = &func.inst(step.inst).kind;
        // A select reads its condition and the operand it chooses.
        if let InstKind::Select(..) = kind
            && let Some(Word::Known(condition)) = word(step.operands[0].1)
        {
            return word(step.operands[if condition.is_zero() { 2 } else { 1 }].1);
        }
        let mut operands = SmallVec::<[Word; 3]>::new();
        for &(_, slot) in &step.operands {
            operands.push(word(slot)?);
        }
        if operands.iter().all(|word| matches!(word, Word::Known(_))) {
            let value = eval_typed_inst(func, kind, |value| {
                let &(_, slot) =
                    step.operands.iter().find(|&&(operand, _)| operand == value).ok_or(())?;
                match word(slot) {
                    Some(Word::Known(value)) => Ok(value),
                    _ => Err(()),
                }
            });
            return value.ok().flatten().map(Word::Known);
        }
        // The operations that take the input while bits of it are free.
        let decided = |holds: bool| Some(Word::Known(U256::from(holds)));
        let result = match (kind, &operands[..]) {
            // x >> s >> k => x >> (s + k)
            (InstKind::Shr(..), &[Word::Known(shift), Word::Shifted(by)]) => {
                Some(match u32::try_from(shift) {
                    Ok(shift) if shift < 256 => class.shifted(by + shift),
                    _ => Word::Known(U256::ZERO),
                })
            }
            (InstKind::Lt(..), &[Word::Shifted(by), Word::Known(limit)])
            | (InstKind::Gt(..), &[Word::Known(limit), Word::Shifted(by)]) => {
                let (low, high) = class.bounds(by);
                if high < limit {
                    decided(true)
                } else if low >= limit {
                    decided(false)
                } else {
                    None
                }
            }
            (InstKind::Gt(..), &[Word::Shifted(by), Word::Known(limit)])
            | (InstKind::Lt(..), &[Word::Known(limit), Word::Shifted(by)]) => {
                let (low, high) = class.bounds(by);
                if low > limit {
                    decided(true)
                } else if high <= limit {
                    decided(false)
                } else {
                    None
                }
            }
            (
                InstKind::Eq(..) | InstKind::Ne(..),
                &[Word::Shifted(by), Word::Known(other)] | &[Word::Known(other), Word::Shifted(by)],
            ) => {
                let (low, high) = class.bounds(by);
                (other < low || other > high)
                    .then_some(matches!(kind, InstKind::Ne(..)))
                    .and_then(decided)
            }
            (_, &[word]) if matches!(kind.semantics(), Some(Semantics::Word(_))) => Some(word),
            // Any other operation needs its operands as numbers: the one closest to being one
            // becomes one when the free bits left in it are fixed.
            _ => {
                *blocked = operands
                    .iter()
                    .filter_map(|word| match word {
                        Word::Shifted(by) => Some(class.free - by),
                        Word::Known(_) => None,
                    })
                    .min();
                return None;
            }
        };
        // A comparison the class leaves open is decided once more bits are fixed.
        if result.is_none() {
            *blocked = Some(1);
        }
        result
    }
}

/// The inputs that sort the values of a cone into scans and others, as leading bits and the
/// number of zero bits below them: every stage of the search decides at least one of them each
/// way, and the low bytes cover a table's entries.
const PROBES: &[(u64, usize)] = &[
    (1, 0),
    (1, 255),
    (0xb, 0),
    (0xd3, 0),
    (0x1ff, 3),
    (0xa5a5, 16),
    (0x9234_5678, 0),
    (0xffff_ffff_ffff_ffff, 0),
    (1, 64),
    (0xc0de, 112),
    (0x7, 126),
    (0xf00d, 170),
    (0xffff_ffff_ffff_ffff, 192),
];

/// The constants of the `CLZ` forms.
struct Constants {
    zero: ValueId,
    one: ValueId,
    eight: ValueId,
    all: ValueId,
}

/// Builds the `CLZ` form of `scan` through `emit`, which returns the result of each instruction,
/// and returns the last instruction unemitted: it takes the place of the scan.
fn clz_form(
    scan: Scan,
    odd: bool,
    input: ValueId,
    constants: &Constants,
    emit: &mut dyn FnMut(InstKind, MirType) -> ValueId,
) -> InstKind {
    // %odd = or %input, 1
    let word = if odd { emit(InstKind::Or(input, constants.one), MirType::I256) } else { input };
    if scan.leading_zeros {
        // %zeros = clz %word
        return InstKind::Clz(word);
    }
    // %zeros = clz %word
    let zeros = emit(InstKind::Clz(word), MirType::I256);
    if !odd || scan.zero.is_zero() {
        // %index = sub 255, %zeros
        return InstKind::Sub(constants.all, zeros);
    }
    // %index = sub 255, %zeros
    // %is_zero = eq %input, 0
    // %flag = zext i1 %is_zero to i256
    // %none = shl %flag, 8
    // %result = or %index, %none
    let index = emit(InstKind::Sub(constants.all, zeros), MirType::I256);
    let is_zero = emit(InstKind::Eq(input, constants.zero), MirType::I1);
    let flag = emit(InstKind::Zext(is_zero), MirType::I256);
    let none = emit(InstKind::Shl(constants.eight, flag), MirType::I256);
    InstKind::Or(index, none)
}

/// Replaces the scan at `root` with its `CLZ` form, if that is cheaper, and returns the
/// instructions it added.
fn rewrite(
    func: &mut Function,
    target: &Target,
    index: &mut Index,
    block: BlockId,
    cone: &Cone<'_>,
    root: usize,
    scan: Scan,
) -> Option<Vec<InstId>> {
    let input = cone.input;
    let root_inst = cone.steps[root].inst;
    // `x | 1` has the highest bit of `x` and counts 255 zeros for zero.
    let odd = match (scan.leading_zeros, scan.zero) {
        (false, zero) if zero.is_zero() || zero == U256::from(256) => true,
        (true, zero) if zero == U256::from(255) => true,
        (false, U256::MAX) => false,
        (true, zero) if zero == U256::from(256) => false,
        _ => return None,
    };

    // The instructions that only the scan reads go with it.
    let uses = index.uses.get_or_insert_with(|| super::egraph::use_counts(func));
    let mut left = FxHashMap::default();
    let mut removed = FxHashSet::default();
    let mut pending = vec![root as u32];
    while let Some(step) = pending.pop() {
        for &(value, slot) in &cone.steps[step as usize].operands {
            let Slot::Step(operand) = slot else { continue };
            let left = left.entry(operand).or_insert_with(|| uses[value]);
            *left -= 1;
            if *left == 0 {
                removed.insert(cone.steps[operand as usize].inst);
                pending.push(operand);
            }
        }
    }
    let old = removed
        .iter()
        .chain([&root_inst])
        .map(|&inst| target.instruction_estimate(func, inst))
        .sum::<Cost>();

    let mut immediate =
        |value: u64| func.alloc_value(Value::Immediate(Immediate::I256(U256::from(value))));
    let constants = Constants {
        zero: immediate(0),
        one: immediate(1),
        eight: immediate(8),
        all: immediate(255),
    };
    let mut new = Cost::ZERO;
    let last = clz_form(scan, odd, input, &constants, &mut |kind, _| {
        new += target.kind_estimate(func, &kind);
        input
    });
    new += target.kind_estimate(func, &last);
    if !target.cmp(new, old).is_lt() {
        return None;
    }

    let metadata = func.inst(root_inst).metadata.debug_context();
    let mut inserted = Vec::new();
    let last = clz_form(scan, odd, input, &constants, &mut |kind, ty| {
        let mut inst = Instruction::new(kind, Some(ty));
        inst.metadata = metadata.clone();
        let (inst, value) = func.alloc_value_inst(inst);
        inserted.push(inst);
        value
    });
    // The block and the counts as the rewrite leaves them.
    uses.raw.resize(func.num_values(), 0);
    let mut count = |kind: &InstKind, add: bool| {
        kind.visit_operands(|operand| {
            if add {
                uses[operand] += 1;
            } else {
                uses[operand] -= 1;
            }
        });
    };
    for &inst in removed.iter().chain([&root_inst]) {
        count(&func.inst(inst).kind, false);
    }
    for &inst in &inserted {
        count(&func.inst(inst).kind, true);
    }
    count(&last, true);
    func.replace_inst_kind(root_inst, last);
    let index = index.block(func, block);
    let (place, _) = index.places[&root_inst];
    for (nth, &inst) in inserted.iter().enumerate() {
        index.places.insert(inst, (place - 1, nth as u32 + 1));
        index.read(func, inst);
    }
    index.read(func, root_inst);
    for inst in &removed {
        index.places.remove(inst);
    }
    index.removed.extend(removed);
    index.inserted.entry(root_inst).or_default().extend(inserted.iter().copied());
    Some(inserted)
}

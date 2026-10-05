//! Search-based rescheduling of the stack operations around the computations of hot
//! straight-line code.
//!
//! The stack scheduler places `DUP`, `SWAP` and `POP` greedily, one operation at a time, in the
//! order MIR left the computations. Inside a loop's straight-line code that order can cost
//! words. A counter that both feeds a sum and steps, `t += i; i += 1`, is cheaper with the step
//! computed first from a copy of `i` and the sum consuming `i` itself, which leaves the new
//! counter in the old one's slot. A dead word is cheaper to pop before an operation on the word
//! above it than to swap around that operation and pop it afterwards.
//!
//! This pass takes each maximal run of a block's instructions that only computes: stack
//! operations, immediate and label pushes, pure word operations, calldata reads, and memory and
//! storage accesses. It executes the run symbolically from the deepest stack word the run reads,
//! which yields the operations it performs, their operand words, and the stack it leaves. A
//! best-first search over the stack's words and the set of performed operations then looks for a
//! cheaper sequence of `DUP`, `SWAP`, `POP` and pushes around the same operations that leaves the
//! same stack. Operations run in any order their operands allow, except that memory and storage
//! accesses keep their original relative order; a commutative operation takes its operands in
//! either order, and a comparison may become its mirror, `gt` for `lt` with swapped operands.
//! Moves are priced by the target cost model under the objective, so when optimizing for gas a
//! schedule is taken when it saves gas, or the same gas in fewer bytes, and when optimizing for
//! size when it saves bytes, or the same bytes in less gas.
//!
//! A run continues through a branch to a block that aborts without reading the stack, the
//! `PUSH target; JUMPI` of an overflow check or a guard: the pair takes only its condition, so it
//! joins the run as one more ordered operation, and work may move across it, as only the path
//! that continues matters. A block that returns or stops is a normal exit and ends the run.
//! Spanning the checks gives the search more freedom, but a longer run can exhaust its budget
//! where the runs between its checks would not, so those are scheduled separately as well and
//! the cheaper result is kept. A run with more operations than one search takes is cut into
//! windows after its checks.
//!
//! The remaining cost of a state is bounded below by one copy for every missing use of a word
//! and one pop for every surplus copy, and by one swap more when those counted moves cannot bring
//! the operands of any ready operation to the top, or, once every operation ran, the stack into
//! its exit order: every path then pays a swap, or a copy or pop the bound does not count, which
//! costs at least as much. So does a state whose lowest word that differs from the exit's at its
//! level can only change through a swap, because no word at or below that level can be built
//! again from the words below it, constants and results still to come. The search checks that
//! only for the states it takes from its queue, and puts a state that needs the swap back in its
//! raised order. It orders states by their cost plus three times the bound, which reaches a
//! cheaper schedule after far fewer states than an exact search, and then keeps going below each
//! schedule's cost with the exact bound. Swaps only bring up a word that a ready operation reads
//! or a surplus copy, until every operation ran and the exit order is all that is left, and only
//! right after an operation or a pop, while a surplus copy on top is popped or consumed before
//! anything else. These rules leave out some schedules, rarely a cheaper one, and stop the search
//! from trying the many orders of the same words its bound cannot tell apart, which took most of
//! its states and kept it from the cheap schedules for thousands of states in small loops. The
//! bound changes with the few words a move touches, so it is updated per move rather than
//! recomputed, and the queue keeps one small heap per value of a priority's first part, popping
//! in the order a single heap would.
//!
//! The search is exponential in the run's operations and stack height, and dominated the compile
//! time of small contracts with a hot loop, so its budgets are tight. A run longer than its
//! budget's operations is searched in windows cut after its checks, or skipped, the stack may grow
//! at most [`STACK_SLACK`] words above the run's entry and exit heights, and a run that yields no
//! cheaper schedule within the first part of its [`Limits`] keeps its code, while one that does
//! keeps refining for the rest, or until no cheaper schedule turned up for its stall part, as later
//! improvements are rare and small. Runs in hot loop blocks get [`LOOP_BUDGET`] when optimizing for
//! gas, which takes longer runs and looks longer near the bound, as their code runs many times per
//! call; all others get [`BLOCK_BUDGET`]. A run whose bound already reaches its cost in the
//! objective's first part is not searched, as only the second part could improve, and one at most a
//! swap above it gets its budget's smaller near limits. Equal runs share their result, as unrolled
//! copies and repeated checks repeat them, and so do runs that push other immediates of the same
//! widths: a schedule depends only on which pushes repeat a value and how wide each is.
//!
//! Safety: the replacement performs exactly the original operations on the same operand words,
//! keeps memory and storage accesses in order, and leaves the same words in the same stack
//! slots, so the code after the run observes no difference; a schedule whose replay does not
//! reproduce the run is discarded. A check keeps its order with every access and its pushed
//! target right before its `JUMPI`. Runs end at every other instruction, including `GAS`, calls,
//! other branches and an instruction glued to the next one. Performed operations and the original
//! pushes keep their debug metadata, function events included, while the new stack operations
//! carry none, which is marked as intentionally dropped, and the origins and events of the
//! replaced stack operations move to the run's last instruction. Debug metadata decides nothing,
//! so requesting it leaves the code unchanged.
//!
//! The pass runs after the late push compaction, so it sees the final pushes, and before the
//! last stack cleanup and loop layout.

use super::{EvmPass, utils::is_split_point};
use crate::{
    backend::evm::{
        ir::{Block, Instruction, Module, TerminatorKind},
        op::{self, OpcodeTraits, StackOp},
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::map::FxHashMap;
use solar_sema::Gcx;
use std::collections::{BinaryHeap, hash_map::Entry};

/// Words the search may grow the stack by above the run's entry and exit heights.
const STACK_SLACK: usize = 3;
/// How large a run the search takes and how many states it may expand for it.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Budget {
    /// Operations a searched run may perform.
    operations: usize,
    /// The states a run may expand.
    states: Limits,
    /// The states a run whose first objective is at most one swap above its bound may expand:
    /// the search can save little there, and keeps going long when it saves nothing.
    near: Limits,
}

/// How many states one search may expand.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Limits {
    /// States to expand before some cheaper schedule turns up, after which the run keeps its
    /// code.
    first: usize,
    /// States to expand in all.
    refine: usize,
    /// States to expand after the last cheaper schedule before giving up on finding another.
    stall: usize,
}

/// The budget of a run in a hot loop block, when optimizing for gas: longer runs, and a closer
/// look near the bound, as every swap saved there is saved on every iteration.
const LOOP_BUDGET: Budget = Budget {
    operations: 16,
    states: Limits { first: 200, refine: 600, stall: 100 },
    near: Limits { first: 100, refine: 200, stall: 50 },
};
/// The budget of any other run, whose code runs at most a few times per call.
const BLOCK_BUDGET: Budget = Budget {
    operations: 12,
    states: Limits { first: 200, refine: 600, stall: 100 },
    near: Limits { first: 50, refine: 100, stall: 25 },
};
/// How many times its bound the search adds to a state's cost to order it: greedier than an
/// exact search, it reaches cheaper schedules after far fewer states.
const BOUND_WEIGHT: u64 = 3;
/// An arena entry whose next operation was not yet checked for a swap.
const UNKNOWN: u8 = 0;
/// An arena entry whose next operation needs no swap the bound leaves out.
const NO_SWAP: u8 = 1;
/// An arena entry whose next operation needs a swap the bound leaves out.
const SWAP: u8 = 2;
/// An arena entry that a cheaper entry of its state replaced.
const SUPERSEDED: u8 = 3;
/// Deepest stack word a `DUP` or `SWAP` reaches.
const REACH: usize = 16;
/// Bits of a word's index in a packed stack.
const WORD_BITS: u32 = 6;
/// Words a packed stack holds.
const MAX_HEIGHT: usize = 21;

pub(super) struct StackReschedule;

impl EvmPass for StackReschedule {
    fn name(&self) -> &'static str {
        "stack-reschedule"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let target = Target::new(gcx);
        let halts = module.blocks.iter().map(aborts_without_stack).collect::<Vec<_>>();
        let mut scratch = Scratch::default();
        let mut changed = false;
        for block in &mut module.blocks {
            let hot_loop = target.optimization().is_gas()
                && block.metadata.in_loop
                && !block.metadata.hotness.is_cold();
            let budget = if hot_loop { LOOP_BUDGET } else { BLOCK_BUDGET };
            changed |=
                reschedule_block(&mut block.instructions, target, budget, &halts, &mut scratch);
        }
        changed
    }
}

/// A stack word of a run: one found on entry, by depth, an operation's result, or a constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Word {
    Entry(u8),
    Result(u8),
    Constant(u8),
}

/// An operation a run performs.
struct Operation {
    /// Position of the instruction in the run.
    position: usize,
    /// Operand words in pop order.
    operands: SmallVec<[Word; 3]>,
    /// Whether the operation leaves a result word.
    produces: bool,
    commutative: bool,
    /// The comparison that computes the same result from swapped operands.
    mirror: Option<u8>,
    /// The memory or storage access that has to run before this one.
    after: Option<u8>,
    /// Whether this is a `PUSH target; JUMPI` pair into a block that aborts without reading the
    /// stack, which only takes the condition.
    branch: bool,
}

/// The symbolic summary of a run.
struct Run {
    operations: Vec<Operation>,
    /// Position of each constant's first push in the run.
    constants: SmallVec<[usize; 8]>,
    /// Price of pushing each constant.
    constant_costs: SmallVec<[u64; 8]>,
    /// Words on entry the run reads.
    entry: usize,
    /// Words the run leaves, top first.
    exit: SmallVec<[Word; 24]>,
    /// Price of the run's stack operations and pushes.
    cost: Cost,
    /// Highest stack the run builds above the words below its entry.
    peak: usize,
}

/// A step of a schedule.
#[derive(Clone, Copy, Debug)]
enum Move {
    Perform { operation: u8, mirrored: bool },
    Push(u8),
    Stack(StackOp),
}

/// A search state: the stack's dense word indices packed six bits each, top lowest, and the
/// performed operations. Eight-byte alignment shrinks the search's arena entries from 64 to 48
/// bytes and its table entries from 48 to 32.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(Rust, packed(8))]
struct State {
    stack: u128,
    len: u8,
    done: u16,
}

impl State {
    fn word(self, depth: usize) -> usize {
        ((self.stack >> (WORD_BITS as usize * depth)) & 63) as usize
    }

    /// The stack's words, top first.
    fn words(self) -> [u8; MAX_HEIGHT] {
        let mut words = [0; MAX_HEIGHT];
        let mut stack = self.stack;
        for word in &mut words[..usize::from(self.len)] {
            *word = (stack & 63) as u8;
            stack >>= WORD_BITS;
        }
        words
    }

    fn push(self, word: usize) -> Self {
        Self { stack: (self.stack << WORD_BITS) | word as u128, len: self.len + 1, ..self }
    }

    fn drop(self, count: usize) -> Self {
        Self {
            stack: self.stack >> (WORD_BITS as usize * count),
            len: self.len - count as u8,
            ..self
        }
    }

    fn swap(self, depth: usize) -> Self {
        let difference = (self.word(0) ^ self.word(depth)) as u128;
        Self {
            stack: self.stack ^ difference ^ (difference << (WORD_BITS as usize * depth)),
            ..self
        }
    }
}

fn reschedule_block(
    instructions: &mut Vec<Instruction>,
    target: Target,
    budget: Budget,
    halts: &[bool],
    scratch: &mut Scratch,
) -> bool {
    let mut replacements = Vec::new();
    let mut start = 0;
    while start < instructions.len() {
        // A run continues through checks that branch to a block aborting without the stack.
        let mut end = start;
        let mut cuts = SmallVec::<[usize; 8]>::new();
        loop {
            if is_branch_pair(instructions, end, halts) {
                end += 2;
                cuts.push(end);
            } else if end < instructions.len()
                && schedulable(&instructions[end])
                && is_split_point(instructions, end)
                && !instructions[end].keeps_with_next()
            {
                end += 1;
            } else {
                break;
            }
        }
        if end == start {
            start += 1;
            continue;
        }
        // Search windows of at most the budget's operations, cut after the checks.
        let mut window_start = start;
        while window_start < end {
            let mut window_end = end;
            if operation_count(&instructions[window_start..end]) > budget.operations {
                window_end = cuts
                    .iter()
                    .copied()
                    .rev()
                    .find(|&cut| {
                        cut > window_start
                            && operation_count(&instructions[window_start..cut])
                                <= budget.operations
                    })
                    .unwrap_or_else(|| {
                        cuts.iter().copied().find(|&cut| cut > window_start).unwrap_or(end)
                    });
            }
            let window = &instructions[window_start..window_end];
            // Spanning the checks gives the search more freedom, but a longer run can exhaust
            // its budget where the runs between the checks would not: keep the cheaper one.
            let mut best = None;
            let mut best_price = price(window, target);
            let candidates = [
                schedule(window, target, budget, halts, scratch),
                split_schedule(window, target, budget, halts, scratch),
            ];
            for candidate in candidates.into_iter().flatten() {
                let candidate_price = price(&candidate, target);
                if candidate_price < best_price {
                    best_price = candidate_price;
                    best = Some(candidate);
                }
            }
            if let Some(replacement) = best {
                replacements.push((window_start, window_end, replacement));
            }
            window_start = window_end;
        }
        start = end;
    }
    if replacements.is_empty() {
        return false;
    }
    let mut rebuilt = Vec::with_capacity(instructions.len());
    let mut cursor = 0;
    let mut old = std::mem::take(instructions).into_iter();
    for (start, end, replacement) in replacements {
        rebuilt.extend(old.by_ref().take(start - cursor));
        old.by_ref().take(end - start).for_each(drop);
        rebuilt.extend(replacement);
        cursor = end;
    }
    rebuilt.extend(old);
    *instructions = rebuilt;
    true
}

/// The schedules found for runs, by their instructions and search budget.
type Schedules = FxHashMap<(SmallVec<[RunKey; 24]>, Budget), Option<Vec<Move>>>;

/// What the pass keeps across runs: the schedules found and the search's buffers.
#[derive(Default)]
struct Scratch {
    schedules: Schedules,
    buffers: Buffers,
}

/// The search's state storage, cleared and reused for every run.
#[derive(Default)]
struct Buffers {
    /// Visited states: the state, its parent, the move into it, its cost and its bound.
    arena: Vec<(State, u32, Option<Move>, u64, u64)>,
    /// The cheapest arena entry of each state.
    best: FxHashMap<State, u32>,
    /// Arena entries by priority.
    open: Queue,
    /// For each arena entry, whether its next operation needs a swap the bound leaves out:
    /// [`UNKNOWN`], [`NO_SWAP`] or [`SWAP`], or [`SUPERSEDED`].
    refined: Vec<u8>,
}

/// Arena entries ordered by priority, then by larger cost, then by later entry. The first part
/// of a priority spans a few hundred values, so each gets a heap of its own, whose keys pack the
/// second part, the cost and the entry into one number: the order is one heap's, and every heap
/// stays small.
#[derive(Default)]
struct Queue {
    buckets: Vec<BinaryHeap<u128>>,
    lowest: usize,
}

impl Queue {
    fn clear(&mut self) {
        self.buckets.iter_mut().for_each(BinaryHeap::clear);
        self.lowest = usize::MAX;
    }

    fn push(&mut self, priority: u64, cost: u64, node: u32) {
        let bucket = (priority >> 32) as usize;
        if bucket >= self.buckets.len() {
            self.buckets.resize_with(bucket + 1, BinaryHeap::new);
        }
        // The largest first: the lowest second part, then the largest cost and entry.
        let key =
            (u128::from(!(priority as u32)) << 96) | (u128::from(cost) << 32) | u128::from(node);
        self.buckets[bucket].push(key);
        self.lowest = self.lowest.min(bucket);
    }

    fn pop(&mut self) -> Option<(u64, u32)> {
        while let Some(bucket) = self.buckets.get_mut(self.lowest) {
            if let Some(key) = bucket.pop() {
                return Some(((key >> 32) as u64, key as u32));
            }
            self.lowest += 1;
        }
        None
    }
}

/// What a run's schedule depends on in one of its instructions.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum RunKey {
    Stack(StackOp),
    Block(u32),
    /// An immediate by its first position among the run's immediates and its width.
    Immediate(u8, u8),
    Operation(u8),
}

/// The key of an instruction, given the distinct immediates the run pushed before it, which
/// collects the one it pushes. A schedule depends only on which pushes repeat a value and how
/// wide each is.
fn run_key(inst: &Instruction, immediates: &mut SmallVec<[U256; 8]>) -> RunKey {
    if let Some(stack_op) = inst.as_stack_op() {
        return RunKey::Stack(stack_op);
    }
    match constant_key(inst) {
        Some((0, value)) => {
            let index = immediates.iter().position(|&known| known == value).unwrap_or_else(|| {
                immediates.push(value);
                immediates.len() - 1
            });
            RunKey::Immediate(index as u8, value.byte_len() as u8)
        }
        Some((_, block)) => RunKey::Block(block.to()),
        None => RunKey::Operation(inst.opcode),
    }
}

/// Whether an instruction may be part of a searched run.
fn schedulable(inst: &Instruction) -> bool {
    if let Some(stack_op) = inst.as_stack_op() {
        return !matches!(stack_op, StackOp::Exchange(..));
    }
    if inst.is_encoded_push() {
        return constant_key(inst).is_some();
    }
    let Some(definition) = inst.definition() else { return false };
    matches!(definition.stack_io, Some((_, outputs)) if outputs <= 1)
        && (movable(definition.opcode, definition.traits) || ordered(definition.opcode))
}

/// Operations that read nothing a run's other operations can change.
fn movable(opcode: u8, traits: OpcodeTraits) -> bool {
    traits.contains(OpcodeTraits::PURE) || matches!(opcode, op::CALLDATALOAD | op::CALLDATASIZE)
}

/// Memory and storage accesses, which keep their relative order.
fn ordered(opcode: u8) -> bool {
    matches!(
        opcode,
        op::MLOAD
            | op::MSTORE
            | op::MSTORE8
            | op::SLOAD
            | op::SSTORE
            | op::TLOAD
            | op::TSTORE
            | op::KECCAK256
    )
}

/// What identifies a push's constant, when pushing it again is free of side conditions.
fn constant_key(inst: &Instruction) -> Option<(u8, U256)> {
    if inst.deferred_push().is_some() || inst.immutable_push().is_some() {
        return None;
    }
    if let Some(value) = inst.concrete_immediate() {
        return Some((0, value));
    }
    inst.pushed_block().map(|block| (1, U256::from(block.index())))
}

/// Price of a push as assembly encodes it: the narrowest `PUSH` of its value.
fn push_cost(inst: &Instruction, target: Target) -> Cost {
    let width = match inst.concrete_immediate() {
        Some(value) if value.is_zero() && target.evm_version().has_push0() => {
            return target.opcode(op::PUSH0);
        }
        Some(value) => value.byte_len().max(1),
        // Labels resolve to two-byte pushes in all but the largest contracts.
        None => 2,
    };
    target.opcode(op::PUSH1 + (width as u8 - 1))
}

fn stack_op_cost(stack_op: StackOp, target: Target) -> Cost {
    stack_op
        .single_byte_evm_opcode()
        .map_or(Cost::new(u32::MAX, u32::MAX), |opcode| target.opcode(opcode))
}

/// Orders costs under the objective as one number: gas first, then bytes.
fn scalar(cost: Cost, target: Target) -> u64 {
    let [first, second] = target.objective_key(cost);
    (u64::from(first) << 32) | u64::from(second)
}

/// Executes a run symbolically.
fn summarize(
    run: &[Instruction],
    target: Target,
    max_operations: usize,
    halts: &[bool],
) -> Option<Run> {
    let mut stack = SmallVec::<[Word; 24]>::new();
    let mut entry = 0usize;
    let mut operations = Vec::new();
    let mut constants = SmallVec::<[usize; 8]>::new();
    let mut constant_costs = SmallVec::<[u64; 8]>::new();
    let mut keys = SmallVec::<[(u8, U256); 8]>::new();
    let mut cost = Cost::ZERO;
    let mut last_ordered = None;
    let mut peak = 0usize;
    let deepen = |stack: &mut SmallVec<[Word; 24]>, entry: &mut usize, depth: usize| {
        while stack.len() < depth {
            stack.push(Word::Entry(u8::try_from(*entry).ok()?));
            *entry += 1;
        }
        Some(())
    };
    let mut position = 0;
    while position < run.len() {
        let inst = &run[position];
        if is_branch_pair(run, position, halts) {
            // push target; jumpi -> branch(condition)
            deepen(&mut stack, &mut entry, 1)?;
            let condition = stack.remove(0);
            let index = u8::try_from(operations.len()).ok()?;
            operations.push(Operation {
                position,
                operands: SmallVec::from_slice(&[condition]),
                produces: false,
                commutative: false,
                mirror: None,
                after: last_ordered,
                branch: true,
            });
            last_ordered = Some(index);
            position += 2;
            continue;
        }
        if let Some(stack_op) = inst.as_stack_op() {
            deepen(&mut stack, &mut entry, stack_op.required_depth())?;
            match stack_op {
                StackOp::Dup(depth) => stack.insert(0, stack[usize::from(depth) - 1]),
                StackOp::Swap(depth) => stack.swap(0, usize::from(depth)),
                StackOp::Pop => {
                    stack.remove(0);
                }
                StackOp::Exchange(..) => return None,
            }
            cost = cost.plus(stack_op_cost(stack_op, target));
        } else if inst.is_encoded_push() {
            let key = constant_key(inst)?;
            let index = match keys.iter().position(|&existing| existing == key) {
                Some(index) => index,
                None => {
                    keys.push(key);
                    constants.push(position);
                    constant_costs.push(scalar(push_cost(inst, target), target));
                    keys.len() - 1
                }
            };
            stack.insert(0, Word::Constant(u8::try_from(index).ok()?));
            cost = cost.plus(push_cost(inst, target));
        } else {
            let definition = inst.definition()?;
            let (inputs, outputs) = definition.stack_io?;
            deepen(&mut stack, &mut entry, usize::from(inputs))?;
            let operands = stack.drain(..usize::from(inputs)).collect::<SmallVec<[Word; 3]>>();
            let index = u8::try_from(operations.len()).ok()?;
            let is_ordered = !movable(definition.opcode, definition.traits);
            operations.push(Operation {
                position,
                operands,
                produces: outputs == 1,
                commutative: definition.traits.contains(OpcodeTraits::COMMUTATIVE),
                mirror: match definition.opcode {
                    op::LT => Some(op::GT),
                    op::GT => Some(op::LT),
                    op::SLT => Some(op::SGT),
                    op::SGT => Some(op::SLT),
                    _ => None,
                },
                after: if is_ordered { last_ordered } else { None },
                branch: false,
            });
            if is_ordered {
                last_ordered = Some(index);
            }
            if outputs == 1 {
                stack.insert(0, Word::Result(index));
            }
        }
        peak = peak.max(stack.len());
        position += 1;
    }
    (operations.len() >= 2
        && operations.len() <= max_operations
        && entry <= REACH
        && keys.len() <= 8)
        .then_some(Run { operations, constants, constant_costs, entry, exit: stack, cost, peak })
}

/// Dense indices of a run's words: entry words, then results, then constants.
struct Words {
    entry: usize,
    operations: usize,
}

impl Words {
    fn index(&self, word: Word) -> usize {
        match word {
            Word::Entry(depth) => usize::from(depth),
            Word::Result(operation) => self.entry + usize::from(operation),
            Word::Constant(constant) => self.entry + self.operations + usize::from(constant),
        }
    }
}

/// Searches for the cheapest schedule of a run's operations that leaves its stack, when one
/// beats the run's own stack operations.
fn search(run: &Run, target: Target, limits: Budget, buffers: &mut Buffers) -> Option<Vec<Move>> {
    let operation_count = run.operations.len();
    let words = Words { entry: run.entry, operations: operation_count };
    let word_count = run.entry + operation_count + run.constants.len();
    let height = run.peak.max(run.entry.max(run.exit.len()) + STACK_SLACK);
    if word_count > 64 || height > MAX_HEIGHT {
        return None;
    }
    let all_done = ((1u32 << operation_count) - 1) as u16;
    let operand_words = run
        .operations
        .iter()
        .map(|operation| {
            operation
                .operands
                .iter()
                .map(|&word| words.index(word))
                .collect::<SmallVec<[usize; 3]>>()
        })
        .collect::<Vec<_>>();
    let operands = operand_words.iter().map(SmallVec::as_slice).collect::<Vec<_>>();
    // The operations each one waits for: the producers of its operands and its ordered access.
    let needs = run
        .operations
        .iter()
        .map(|operation| {
            let mut mask = operation.after.map_or(0u16, |after| 1 << after);
            for &operand in &operation.operands {
                if let Word::Result(producer) = operand {
                    mask |= 1 << producer;
                }
            }
            mask
        })
        .collect::<Vec<_>>();
    let mut exit_uses = [0u8; 64];
    for &word in &run.exit {
        exit_uses[words.index(word)] += 1;
    }
    let exit =
        run.exit.iter().rev().fold(State { stack: 0, len: 0, done: all_done }, |state, &word| {
            state.push(words.index(word))
        });
    let exit_len = usize::from(exit.len);
    let exit_words = exit.words();
    // The uses left of each word: by the exit and by the operations not yet performed.
    let demand = |done: u16| -> [u8; 64] {
        let mut need = exit_uses;
        for (index, &operation_operands) in operands.iter().enumerate() {
            if done & (1 << index) == 0 {
                for &operand in operation_operands {
                    need[operand] += 1;
                }
            }
        }
        need
    };
    let result_of = |word: usize| -> Option<usize> {
        (run.entry..run.entry + operation_count).contains(&word).then(|| word - run.entry)
    };
    let constant_of = |word: usize| -> Option<usize> {
        (word >= run.entry + operation_count).then(|| word - run.entry - operation_count)
    };
    let dup = scalar(target.dup(), target);
    let swap = scalar(target.opcode(op::SWAP1), target);
    let pop = scalar(target.opcode(op::POP), target);
    let counts_of = |stack: &[u8]| -> [u8; 64] {
        let mut counts = [0u8; 64];
        for &word in stack {
            counts[usize::from(word)] += 1;
        }
        counts
    };
    // Every missing copy of a word needs a `DUP` or a push, and every surplus copy a `POP`.
    let term = |word: usize, count: u8, need: u8, done: u16| -> u64 {
        if count > need {
            return u64::from(count - need) * pop;
        }
        let pending = result_of(word).is_some_and(|operation| done & (1 << operation) == 0);
        let have = count + u8::from(pending);
        if need <= have {
            return 0;
        }
        let copy = constant_of(word).map_or(dup, |constant| dup.min(run.constant_costs[constant]));
        u64::from(need - have) * copy
    };
    let estimate = |state: State| -> u64 {
        let counts = counts_of(&state.words()[..usize::from(state.len)]);
        let need = demand(state.done);
        (0..word_count).map(|word| term(word, counts[word], need[word], state.done)).sum()
    };
    let mut budget = scalar(run.cost, target);
    let start = (0..run.entry)
        .rev()
        .fold(State { stack: 0, len: 0, done: 0 }, |state, depth| state.push(depth));
    // When the bound reaches the run's cost in the objective's first part, only the second part
    // could improve, which measured not worth a search; one swap above it, little can improve.
    let gap = (budget >> 32).saturating_sub(estimate(start) >> 32);
    if gap == 0 {
        return None;
    }
    let limits = if gap <= swap >> 32 { limits.near } else { limits.states };

    // Whether the moves the bound counts, popping surplus words and copying or pushing missing
    // ones, cannot bring the operands of a ready operation to the top, or, once every operation
    // ran, the stack into its exit order. Every path then pays a swap the bound leaves out, or
    // a copy or pop it does not count, which costs at least as much.
    let needs_swap =
        |state: State, stack: &[u8], ready: u16, counts: &[u8; 64], need: &[u8; 64]| {
            let len = stack.len();
            let mut counts = *counts;
            // The bound counts `copies` more copies of the word, which it can copy or push now.
            let owed = |counts: &[u8; 64], word: usize, copies: u8| {
                let pending =
                    result_of(word).is_some_and(|operation| state.done & (1 << operation) == 0);
                (constant_of(word).is_some() || counts[word] > 0)
                    && need[word] >= counts[word] + u8::from(pending) + copies
            };
            // The lowest word that differs from the exit's word at its level changes only through
            // a swap that reaches it, or by emptying the stack down to it and building it again
            // from the words below, constants and results still to come. When no word at or
            // below that level can be built again that way, a swap is unavoidable.
            let matching = (0..len.min(exit_len))
                .take_while(|&level| stack[len - 1 - level] == exit_words[exit_len - 1 - level])
                .count();
            if matching < len.min(exit_len) {
                let rebuilt = |level: usize| {
                    let word = exit_words[exit_len - 1 - level];
                    constant_of(usize::from(word)).is_some()
                        || result_of(usize::from(word))
                            .is_some_and(|operation| state.done & (1 << operation) == 0)
                        || exit_words[exit_len - level..exit_len].contains(&word)
                };
                if !(0..=matching).any(rebuilt) {
                    return true;
                }
            }
            for depth in 0..=len {
                let top = |offset: usize| stack.get(depth + offset).map(|&word| usize::from(word));
                if state.done == all_done {
                    // exit = copies ++ the stack below the popped words
                    let rest = len - depth;
                    if rest <= exit_len && stack[depth..] == exit_words[exit_len - rest..exit_len] {
                        let mut fresh = [0u8; 64];
                        for &word in &exit_words[..exit_len - rest] {
                            fresh[usize::from(word)] += 1;
                        }
                        if (0..word_count)
                            .all(|word| fresh[word] == 0 || owed(&counts, word, fresh[word]))
                        {
                            return false;
                        }
                    }
                } else {
                    for (index, operation) in run.operations.iter().enumerate() {
                        if ready & (1 << index) == 0 {
                            continue;
                        }
                        // `a` on top and `b` below it, by the counted moves alone.
                        let reaches = |a: usize, b: usize| {
                            (top(0) == Some(a) && top(1) == Some(b))
                                || (top(0) == Some(b) && owed(&counts, a, 1))
                                || if a == b {
                                    owed(&counts, a, 2)
                                } else {
                                    owed(&counts, a, 1) && owed(&counts, b, 1)
                                }
                        };
                        let reachable = match operands[index] {
                            [] => true,
                            &[a] => top(0) == Some(a) || owed(&counts, a, 1),
                            &[a, b] => {
                                reaches(a, b)
                                    || ((operation.commutative || operation.mirror.is_some())
                                        && reaches(b, a))
                            }
                            _ => true,
                        };
                        if reachable {
                            return false;
                        }
                    }
                }
                // Pop the top word when it is a surplus copy.
                match top(0) {
                    Some(word) if counts[word] > need[word] => counts[word] -= 1,
                    _ => break,
                }
            }
            true
        };

    // state, parent, move, cost, bound on the remaining cost
    let Buffers { arena, best, open, refined } = buffers;
    arena.clear();
    best.clear();
    open.clear();
    refined.clear();
    arena.push((start, u32::MAX, None, 0, estimate(start)));
    refined.push(UNKNOWN);
    best.insert(start, 0);
    open.push(BOUND_WEIGHT * estimate(start), 0, 0);
    let mut expansions = 0;
    let mut found = None;
    let mut found_at = 0;
    let mut successors = SmallVec::<[(State, Move, u64, u64); 48]>::new();
    while let Some((cost, node)) = open.pop() {
        let (state, _, _, _, remaining) = arena[node as usize];
        let raised = match refined[node as usize] {
            SUPERSEDED => continue,
            SWAP => swap,
            _ => 0,
        };
        if cost + remaining + raised >= budget {
            continue;
        }
        if state == exit {
            // A cheaper complete schedule: keep searching below its cost.
            budget = cost;
            found = Some(node);
            found_at = expansions;
            continue;
        }
        let len = usize::from(state.len);
        let words = state.words();
        let stack = &words[..len];
        let counts = counts_of(stack);
        let need = demand(state.done);
        let ready = (0..operation_count).fold(0u16, |ready, index| {
            let ready_now =
                state.done & (1 << index) == 0 && state.done & needs[index] == needs[index];
            ready | (u16::from(ready_now) << index)
        });
        // Raise the bound of a state that needs a swap before its next operation, and order
        // it again, before expanding it.
        if refined[node as usize] == UNKNOWN {
            if needs_swap(state, stack, ready, &counts, &need) {
                refined[node as usize] = SWAP;
                if cost + remaining + swap < budget {
                    open.push(cost + BOUND_WEIGHT * (remaining + swap), cost, node);
                }
                continue;
            }
            refined[node as usize] = NO_SWAP;
        }
        expansions += 1;
        let stalled = found.is_some() && expansions > found_at + limits.stall;
        if expansions > limits.refine || (found.is_none() && expansions > limits.first) || stalled {
            break;
        }
        // The bound stored with the state.
        let here = remaining;
        // The bound after one word's count moves by `delta`; swaps keep every count.
        let shifted = |word: usize, delta: i8| -> u64 {
            let count = counts[word].wrapping_add_signed(delta);
            here - term(word, counts[word], need[word], state.done)
                + term(word, count, need[word], state.done)
        };
        let finishing = state.done == all_done;
        // Words some ready operation reads.
        let mut wanted = 0u64;
        for (index, &operation_operands) in operands.iter().enumerate() {
            if ready & (1 << index) != 0 {
                for &operand in operation_operands {
                    wanted |= 1 << operand;
                }
            }
        }
        let at = |depth: usize| usize::from(stack[depth]);
        // A surplus copy on top is popped or consumed before anything else, and until every
        // operation ran a swap follows only an operation or a pop: swapping again, or right after
        // a copy or push, mostly reorders words the bound cannot tell apart.
        let surplus_top = len > 0 && counts[at(0)] > need[at(0)];
        let swaps = !surplus_top
            && (finishing
                || !matches!(
                    arena[node as usize].2,
                    Some(Move::Push(_) | Move::Stack(StackOp::Dup(_) | StackOp::Swap(_)))
                ));
        successors.clear();
        // perform a ready operation
        for (index, operation) in run.operations.iter().enumerate() {
            let arity = operands[index].len();
            if ready & (1 << index) == 0
                || len < arity
                || (surplus_top && !operands[index].contains(&at(0)))
            {
                continue;
            }
            let in_order = (0..arity).all(|depth| at(depth) == operands[index][depth]);
            let swapped = arity == 2 && at(0) == operands[index][1] && at(1) == operands[index][0];
            let mirrored = !in_order && swapped && !operation.commutative;
            if !(in_order || (swapped && (operation.commutative || operation.mirror.is_some()))) {
                continue;
            }
            let mut next = state.drop(arity);
            if operation.produces {
                next = next.push(run.entry + index);
            }
            next.done |= 1 << index;
            // Performing it changes only the terms of the words it reads, each read `uses`
            // times less, and of its result.
            let mut touched = SmallVec::<[(usize, u8); 4]>::new();
            for &operand in operands[index] {
                match touched.iter_mut().find(|(word, _)| *word == operand) {
                    Some((_, uses)) => *uses += 1,
                    None => touched.push((operand, 1)),
                }
            }
            let mut bound = here;
            for &(word, uses) in &touched {
                bound = bound - term(word, counts[word], need[word], state.done)
                    + term(word, counts[word] - uses, need[word] - uses, next.done);
            }
            if operation.produces {
                let result = run.entry + index;
                bound = bound - term(result, counts[result], need[result], state.done)
                    + term(result, counts[result] + 1, need[result], next.done);
            }
            successors.push((next, Move::Perform { operation: index as u8, mirrored }, 0, bound));
        }
        if len < height && !surplus_top {
            // push a constant a ready operation, or the finished run, still needs
            for (constant, &constant_cost) in run.constant_costs.iter().enumerate() {
                let word = run.entry + operation_count + constant;
                if (wanted & (1 << word) != 0 || (finishing && exit_uses[word] != 0))
                    && counts[word] < need[word]
                {
                    successors.push((
                        state.push(word),
                        Move::Push(constant as u8),
                        constant_cost,
                        shifted(word, 1),
                    ));
                }
            }
            // copy a word that has more uses than copies
            let mut seen = 0u64;
            for depth in 1..=len.min(REACH) {
                let word = at(depth - 1);
                if seen & (1 << word) != 0 {
                    continue;
                }
                seen |= 1 << word;
                if counts[word] < need[word] {
                    successors.push((
                        state.push(word),
                        Move::Stack(StackOp::Dup(depth as u8)),
                        dup,
                        shifted(word, 1),
                    ));
                }
            }
        }
        if swaps {
            // swap up a word a ready operation reads, a surplus copy, or any word once all
            // operations ran
            for depth in 1..len.min(REACH + 1) {
                let word = at(depth);
                if word == at(0)
                    || !(finishing || wanted & (1 << word) != 0 || counts[word] > need[word])
                {
                    continue;
                }
                successors.push((
                    state.swap(depth),
                    Move::Stack(StackOp::Swap(depth as u8)),
                    swap,
                    here,
                ));
            }
        }
        if surplus_top {
            successors.push((state.drop(1), Move::Stack(StackOp::Pop), pop, shifted(at(0), -1)));
        }
        for &(successor, step, step_cost, remaining) in &successors {
            let next_cost = cost + step_cost;
            if next_cost + remaining >= budget {
                continue;
            }
            let id = arena.len() as u32;
            match best.entry(successor) {
                Entry::Occupied(mut entry) => {
                    let known = *entry.get();
                    if arena[known as usize].3 <= next_cost {
                        continue;
                    }
                    refined[known as usize] = SUPERSEDED;
                    entry.insert(id);
                }
                Entry::Vacant(entry) => {
                    entry.insert(id);
                }
            }
            arena.push((successor, node, Some(step), next_cost, remaining));
            refined.push(UNKNOWN);
            open.push(next_cost + BOUND_WEIGHT * remaining, next_cost, id);
        }
    }
    let mut cursor = found?;
    let mut moves = Vec::new();
    while let Some(step) = arena[cursor as usize].2 {
        moves.push(step);
        cursor = arena[cursor as usize].1;
    }
    moves.reverse();
    Some(moves)
}

/// Builds the instructions of a schedule, after replaying it against the run's summary.
fn emit(run: &[Instruction], summary: &Run, moves: &[Move]) -> Option<Vec<Instruction>> {
    let mut stack: SmallVec<[Word; 24]> =
        (0..summary.entry).map(|depth| Word::Entry(depth as u8)).collect();
    let mut performed = 0u32;
    let mut instructions = Vec::with_capacity(moves.len());
    // The original pushes of each constant, whose metadata the emitted pushes take in order.
    let mut pushes = vec![SmallVec::<[usize; 4]>::new(); summary.constants.len()];
    for (position, inst) in run.iter().enumerate() {
        if inst.is_encoded_push()
            && let Some(constant) = summary
                .constants
                .iter()
                .position(|&first| constant_key(&run[first]) == constant_key(inst))
        {
            pushes[constant].push(position);
        }
    }
    let mut placed = vec![false; run.len()];
    for &step in moves {
        match step {
            Move::Perform { operation, mirrored } => {
                let operation_info = &summary.operations[usize::from(operation)];
                let arity = operation_info.operands.len();
                let mut expected = operation_info.operands.clone();
                if stack.len() >= arity
                    && (mirrored || (operation_info.commutative && stack[..arity] != expected[..]))
                {
                    expected.reverse();
                }
                if stack.len() < arity || stack[..arity] != expected[..] {
                    debug_assert!(false, "stack-reschedule replay mismatch");
                    return None;
                }
                stack.drain(..arity);
                if operation_info.produces {
                    stack.insert(0, Word::Result(operation));
                }
                performed |= 1 << operation;
                let original = &run[operation_info.position];
                placed[operation_info.position] = true;
                if operation_info.branch {
                    placed[operation_info.position + 1] = true;
                    instructions.push(original.clone());
                    instructions.push(run[operation_info.position + 1].clone());
                    continue;
                }
                instructions.push(match (mirrored, operation_info.mirror) {
                    (true, Some(mirror)) => {
                        Instruction::opcode(mirror).with_metadata(original.metadata.clone())
                    }
                    _ => original.clone(),
                });
            }
            Move::Push(constant) => {
                stack.insert(0, Word::Constant(constant));
                let originals = &pushes[usize::from(constant)];
                match originals.iter().copied().find(|&position| !placed[position]) {
                    Some(position) => {
                        placed[position] = true;
                        instructions.push(run[position].clone());
                    }
                    None => {
                        // A constant pushed more often than in the run copies the first push's
                        // source only.
                        let first = &run[originals[0]];
                        let mut inst = first.clone();
                        inst.metadata = Default::default();
                        inst.metadata.copy_source_debug_from(&first.metadata);
                        instructions.push(inst);
                    }
                }
            }
            Move::Stack(stack_op) => {
                match stack_op {
                    StackOp::Dup(depth) => stack.insert(0, stack[usize::from(depth) - 1]),
                    StackOp::Swap(depth) => stack.swap(0, usize::from(depth)),
                    StackOp::Pop => {
                        stack.remove(0);
                    }
                    StackOp::Exchange(..) => return None,
                }
                let mut inst = Instruction::stack_op(stack_op);
                inst.metadata.mark_debug_info_dropped();
                instructions.push(inst);
            }
        }
    }
    let all_done = (1u32 << summary.operations.len()) - 1;
    if performed != all_done || stack != summary.exit {
        debug_assert!(false, "stack-reschedule schedule does not reproduce the run");
        return None;
    }
    // NOTE: The replaced stack operations, and pushes the schedule makes fewer times, have no
    // counterpart in the new order. Their origins and function events move to the run's last
    // instruction, so the run as a whole keeps every event it had.
    if let Some(last) = instructions.last_mut() {
        for (position, inst) in run.iter().enumerate() {
            if !placed[position] {
                last.metadata.absorb_debug_info(&inst.metadata);
            }
        }
    }
    Some(instructions)
}

/// Whether the instructions at `index` push a label and branch to it, where the target aborts
/// without reading the stack: the branch then depends on its condition alone.
fn is_branch_pair(instructions: &[Instruction], index: usize, halts: &[bool]) -> bool {
    let (Some(push), Some(jumpi)) = (instructions.get(index), instructions.get(index + 1)) else {
        return false;
    };
    push.pushed_block().is_some_and(|block| halts.get(block.index()).copied().unwrap_or(false))
        && jumpi.as_stack_op().is_none()
        && !jumpi.is_encoded_push()
        && jumpi.opcode == op::JUMPI
        && is_split_point(instructions, index)
        && is_split_point(instructions, index + 1)
        && !jumpi.keeps_with_next()
}

/// Whether a block aborts without reading a word that was on the stack when it was entered.
///
/// A run may move work across a branch to such a block: the work after the branch runs on the
/// path that continues, which is the one that matters. A block that returns or stops is a
/// normal exit, which a hoisted operation would make pay for work it does not need.
fn aborts_without_stack(block: &Block) -> bool {
    let mut depth = 0usize;
    for inst in &block.instructions {
        let effect = inst.stack_effect();
        let Some(rest) = depth.checked_sub(usize::from(effect.inputs)) else { return false };
        depth = rest + usize::from(effect.outputs);
    }
    match block.terminator.as_ref().map(|terminator| &terminator.kind) {
        Some(TerminatorKind::Op(opcode)) if matches!(*opcode, op::REVERT | op::INVALID) => {
            op::stack_io(*opcode).is_some_and(|(inputs, _)| usize::from(inputs) <= depth)
        }
        _ => false,
    }
}

/// The operations a slice of a run performs: a branch pair's push is part of its `JUMPI`.
fn operation_count(instructions: &[Instruction]) -> usize {
    instructions
        .iter()
        .filter(|inst| inst.as_stack_op().is_none() && !inst.is_encoded_push())
        .count()
}

/// The cheapest schedule the search finds for a run, if it beats the run's own.
fn schedule(
    run: &[Instruction],
    target: Target,
    budget: Budget,
    halts: &[bool],
    scratch: &mut Scratch,
) -> Option<Vec<Instruction>> {
    let summary = summarize(run, target, budget.operations, halts)?;
    // Unrolled copies and repeated checks produce the same runs over and over, and runs that
    // only push other immediates of the same widths take the same schedule.
    let mut immediates = SmallVec::<[U256; 8]>::new();
    let key = (
        run.iter().map(|inst| run_key(inst, &mut immediates)).collect::<SmallVec<[RunKey; 24]>>(),
        budget,
    );
    let Scratch { schedules, buffers } = scratch;
    let moves = schedules.entry(key).or_insert_with(|| search(&summary, target, budget, buffers));
    emit(run, &summary, moves.as_ref()?)
}

/// Schedules the runs between a window's checks separately, keeping each check in place; `None`
/// when the window has no check or no run improves.
fn split_schedule(
    window: &[Instruction],
    target: Target,
    budget: Budget,
    halts: &[bool],
    scratch: &mut Scratch,
) -> Option<Vec<Instruction>> {
    let mut rebuilt = Vec::with_capacity(window.len());
    let mut improved = false;
    let mut start = 0;
    let mut index = 0;
    while index < window.len() {
        if !is_branch_pair(window, index, halts) {
            index += 1;
            continue;
        }
        // The pushed target closes the run before the `JUMPI`, which stays where it is.
        let run = &window[start..index + 1];
        match schedule(run, target, budget, halts, scratch) {
            Some(replacement) => {
                improved = true;
                rebuilt.extend(replacement);
            }
            None => rebuilt.extend_from_slice(run),
        }
        rebuilt.push(window[index + 1].clone());
        index += 2;
        start = index;
    }
    if start == 0 {
        return None;
    }
    let run = &window[start..];
    match schedule(run, target, budget, halts, scratch) {
        Some(replacement) => {
            improved = true;
            rebuilt.extend(replacement);
        }
        None => rebuilt.extend_from_slice(run),
    }
    improved.then_some(rebuilt)
}

/// The price of a sequence's stack operations and pushes, which is all a schedule changes.
fn price(instructions: &[Instruction], target: Target) -> u64 {
    let cost = instructions.iter().fold(Cost::ZERO, |cost, inst| {
        if let Some(stack_op) = inst.as_stack_op() {
            cost.plus(stack_op_cost(stack_op, target))
        } else if inst.is_encoded_push() {
            cost.plus(push_cost(inst, target))
        } else {
            cost
        }
    });
    scalar(cost, target)
}

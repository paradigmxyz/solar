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
//! and one pop for every surplus copy. The search orders states by their cost plus twice that
//! bound, which reaches a cheaper schedule after far fewer states than an exact search, and then
//! keeps going below each schedule's cost with the exact bound, so given the room it returns a
//! cheapest one. Swaps only bring up a word that a ready operation reads or a surplus copy, until
//! every operation ran and the exit order is all that is left; the bound changes with a single
//! word's count, so it is updated per move rather than recomputed.
//!
//! The search is exponential in the run's operations and stack height. A run longer than its
//! budget's operations is searched in windows cut after its checks, or skipped, the stack may
//! grow at most [`STACK_SLACK`] words above the run's entry and exit heights, and a run that
//! yields no cheaper schedule within the first part of its [`Budget`] keeps its code, while one
//! that does keeps refining for the rest. Runs in hot loop blocks get [`LOOP_BUDGET`] when
//! optimizing for gas, as their code runs many times per call; all others get the far smaller
//! [`BLOCK_BUDGET`], which mostly saves bytes. Equal runs share their result, as unrolled copies
//! and repeated checks repeat them.
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
use std::{cmp::Reverse, collections::BinaryHeap};

/// Words the search may grow the stack by above the run's entry and exit heights.
const STACK_SLACK: usize = 3;
/// How large a run the search takes and how many states it may expand for it.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Budget {
    /// Operations a searched run may perform.
    operations: usize,
    /// States to expand before some cheaper schedule turns up, after which the run keeps its
    /// code.
    first: usize,
    /// States to expand in all.
    refine: usize,
}

/// The budget of a run in a hot loop block, when optimizing for gas.
const LOOP_BUDGET: Budget = Budget { operations: 16, first: 3_000, refine: 20_000 };
/// The budget of any other run: its code runs at most a few times per call, so a short search
/// mostly saves bytes.
const BLOCK_BUDGET: Budget = Budget { operations: 12, first: 300, refine: 1_000 };
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
/// performed operations.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct State {
    stack: u128,
    len: u8,
    done: u16,
}

impl State {
    fn word(self, depth: usize) -> usize {
        ((self.stack >> (WORD_BITS as usize * depth)) & 63) as usize
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
    /// Arena entries ordered by priority, then by cost.
    open: BinaryHeap<(Reverse<u64>, u64, u32)>,
}

/// What a run's schedule depends on in one of its instructions.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum RunKey {
    Stack(StackOp),
    Push(u8, U256),
    Operation(u8),
}

fn run_key(inst: &Instruction) -> RunKey {
    if let Some(stack_op) = inst.as_stack_op() {
        RunKey::Stack(stack_op)
    } else if let Some((kind, value)) = constant_key(inst) {
        RunKey::Push(kind, value)
    } else {
        RunKey::Operation(inst.opcode)
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
    let operands = run
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
    // The operations reading each word at least once, twice and three times, so the uses left
    // after a set of performed operations are three population counts.
    let mut readers = vec![[0u16; 3]; word_count];
    for (index, operation_operands) in operands.iter().enumerate() {
        for &operand in operation_operands {
            let masks = &mut readers[operand];
            let bit = 1u16 << index;
            if masks[0] & bit == 0 {
                masks[0] |= bit;
            } else if masks[1] & bit == 0 {
                masks[1] |= bit;
            } else {
                masks[2] |= bit;
            }
        }
    }
    let mut exit_uses = [0u8; 64];
    for &word in &run.exit {
        exit_uses[words.index(word)] += 1;
    }
    let exit =
        run.exit.iter().rev().fold(State { stack: 0, len: 0, done: all_done }, |state, &word| {
            state.push(words.index(word))
        });
    let demand = |word: usize, done: u16| -> u8 {
        let [once, twice, thrice] = readers[word];
        exit_uses[word]
            + (once & !done).count_ones() as u8
            + (twice & !done).count_ones() as u8
            + (thrice & !done).count_ones() as u8
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
    let counts_of = |state: State| -> [u8; 64] {
        let mut counts = [0u8; 64];
        for depth in 0..usize::from(state.len) {
            counts[state.word(depth)] += 1;
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
        let counts = counts_of(state);
        (0..word_count)
            .map(|word| term(word, counts[word], demand(word, state.done), state.done))
            .sum()
    };
    let mut budget = scalar(run.cost, target);
    let start = (0..run.entry)
        .rev()
        .fold(State { stack: 0, len: 0, done: 0 }, |state, depth| state.push(depth));
    if estimate(start) >= budget {
        return None;
    }

    // state, parent, move, cost, bound on the remaining cost
    let Buffers { arena, best, open } = buffers;
    arena.clear();
    best.clear();
    open.clear();
    arena.push((start, u32::MAX, None, 0, estimate(start)));
    best.insert(start, 0);
    open.push((Reverse(2 * estimate(start)), 0u64, 0u32));
    let mut expansions = 0;
    let mut found = None;
    let mut successors = SmallVec::<[(State, Move, u64, u64); 48]>::new();
    while let Some((_, cost, node)) = open.pop() {
        let (state, _, _, _, remaining) = arena[node as usize];
        if arena[node as usize].3 != cost
            || best.get(&state) != Some(&node)
            || cost + remaining >= budget
        {
            continue;
        }
        if state == exit {
            // A cheaper complete schedule: keep searching below its cost.
            budget = cost;
            found = Some(node);
            continue;
        }
        expansions += 1;
        if expansions > limits.refine || (found.is_none() && expansions > limits.first) {
            break;
        }
        let counts = counts_of(state);
        let mut need = [0u8; 64];
        for (word, slot) in need.iter_mut().enumerate().take(word_count) {
            *slot = demand(word, state.done);
        }
        let here = (0..word_count)
            .map(|word| term(word, counts[word], need[word], state.done))
            .sum::<u64>();
        // The bound after one word's count moves by `delta`; swaps keep every count.
        let shifted = |word: usize, delta: i8| -> u64 {
            let count = counts[word].wrapping_add_signed(delta);
            here - term(word, counts[word], need[word], state.done)
                + term(word, count, need[word], state.done)
        };
        let len = usize::from(state.len);
        let finishing = state.done == all_done;
        let ready = |index: usize| {
            state.done & (1 << index) == 0 && state.done & needs[index] == needs[index]
        };
        // Words some ready operation reads.
        let mut wanted = 0u64;
        for (index, operation_operands) in operands.iter().enumerate() {
            if ready(index) {
                for &operand in operation_operands {
                    wanted |= 1 << operand;
                }
            }
        }
        successors.clear();
        // perform a ready operation
        for (index, operation) in run.operations.iter().enumerate() {
            let arity = operands[index].len();
            if !ready(index) || len < arity {
                continue;
            }
            let in_order = (0..arity).all(|depth| state.word(depth) == operands[index][depth]);
            let swapped = arity == 2
                && state.word(0) == operands[index][1]
                && state.word(1) == operands[index][0];
            let mirrored = !in_order && swapped && !operation.commutative;
            if !(in_order || (swapped && (operation.commutative || operation.mirror.is_some()))) {
                continue;
            }
            let mut next = state.drop(arity);
            if operation.produces {
                next = next.push(run.entry + index);
            }
            next.done |= 1 << index;
            successors.push((
                next,
                Move::Perform { operation: index as u8, mirrored },
                0,
                estimate(next),
            ));
        }
        if len < height {
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
                let word = state.word(depth - 1);
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
        // swap up a word a ready operation reads, a surplus copy, or any word once all
        // operations ran
        for depth in 1..len.min(REACH + 1) {
            let word = state.word(depth);
            if word == state.word(0)
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
        if len > 0 && counts[state.word(0)] > need[state.word(0)] {
            successors.push((
                state.drop(1),
                Move::Stack(StackOp::Pop),
                pop,
                shifted(state.word(0), -1),
            ));
        }
        for &(successor, step, step_cost, remaining) in &successors {
            let next_cost = cost + step_cost;
            if next_cost + remaining >= budget {
                continue;
            }
            if best.get(&successor).is_some_and(|&known| arena[known as usize].3 <= next_cost) {
                continue;
            }
            let id = arena.len() as u32;
            arena.push((successor, node, Some(step), next_cost, remaining));
            best.insert(successor, id);
            open.push((Reverse(next_cost + 2 * remaining), next_cost, id));
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
    // Unrolled copies and repeated checks produce the same runs over and over.
    let key = (run.iter().map(run_key).collect::<SmallVec<[RunKey; 24]>>(), budget);
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

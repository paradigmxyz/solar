//! Search-based rescheduling of the stack operations around the computations of straight-line
//! code.
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
//! joins the run as one more ordered operation. Work may move below it, as only the path that
//! continues needs it, but never above it, so the aborting path spends no more gas than before
//! and still reaches its revert. Such a block reaches its `REVERT` or `INVALID` without a branch,
//! within at most [`MAX_ABORT_JUMPS`] blocks linked by direct jumps, as tail merging shares the
//! ends of reverts, and without an operation that depends on the gas left, such as a `GAS` reading
//! in its payload, which could tell where the work ran. A block that returns or stops is a normal
//! exit and ends the run. A longer run gives the search more freedom but can exhaust its budget
//! where the runs between its checks would not, so those are scheduled separately as well, split
//! after each operation that depends on the gas left too, and the cheaper result is kept. A run
//! with more operations than one search takes is cut into windows after its checks.
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
//! recomputed. States with the same performed operations reuse their remaining uses and ready
//! operations, regardless of their stack order. The queue keeps one small heap per value of a
//! priority's first part, popping in the order a single heap would.
//!
//! The search is exponential in the run's operations and stack height, and dominated the compile
//! time of small contracts with a hot loop, so its budgets are tight, and large only in hot loops.
//! A run longer than its budget's operations is searched in windows or skipped, the stack may grow
//! at most [`STACK_SLACK`] words above the run's entry and exit heights, and a run that yields no
//! cheaper schedule within the first part of its [`Limits`] keeps its code, while one that does
//! keeps refining for the rest, or until no cheaper schedule turned up for its stall part, as
//! later improvements are rare and small. Runs in hot loop blocks get [`LOOP_BUDGET`], which takes
//! longer runs and waits longer for a first cheaper schedule, as their code runs many times per
//! call: the schedule of a loop that carries four words and stores two bytes per trip turns up
//! only after about a thousand states. All others get [`BLOCK_BUDGET`]. A run of one operation,
//! where a dead word is popped before the operation instead of swapped around it, is searched in
//! hot loop blocks and when optimizing for size, the latter through [`SIZE_BLOCK_BUDGET`];
//! elsewhere in gas mode it saved no gas in the benchmarks and grew the code. A run whose bound
//! already reaches its cost in the objective's first part is not searched, as only the second part
//! could improve, and one at most a swap above it gets its budget's smaller near limits. Unrolled
//! copies, repeated checks and the library code that programs and contracts embed repeat each
//! other's runs, so a search that would expand states is looked up in a cache shared by every
//! program of the compilation, kept on its global context, keyed by everything the search reads of
//! the run and its target prices. Runs that push other immediates of the same widths share a
//! result: a schedule depends only on which pushes repeat a value and how wide each is.
//!
//! Safety: the replacement performs exactly the original operations on the same operand words,
//! keeps memory and storage accesses in order, and leaves the same words in the same stack
//! slots, so the code after the run observes no difference; a schedule whose replay does not
//! reproduce the run is discarded. A check keeps its order with every access and its pushed
//! target right before its `JUMPI`. The gas left is observable as well: no operation moves above
//! a check or an operation that depends on it, such as an `SSTORE` with its 2,300-gas sentry,
//! and a schedule that would spend more gas before one of them keeps the original code, so each
//! sees at least the gas it saw before within its window. When optimizing for size, a schedule
//! that saves bytes may spend more gas, and a check or access after its window then sees less,
//! as with other size optimizations. A run that ends in a label push keeps it last, so the jump
//! after the run still sees its target pushed right before it. Runs end at every other instruction,
//! including `GAS`, calls, other branches and an instruction glued to the next one. Performed
//! operations and the original pushes keep their debug metadata, function events included, while
//! the new stack operations carry none, which is marked as intentionally dropped, and the origins
//! and events of the replaced stack operations move to the run's last instruction. Debug metadata
//! decides nothing, so requesting it leaves the code unchanged.
//!
//! The pass runs after the late push compaction, so it sees the final pushes, and before the
//! last stack cleanup and loop layout.

use super::{EvmPass, utils::is_split_point};
use crate::{
    backend::evm::{
        CodegenState,
        ir::{BlockId, BlockTarget, Instruction, Module, TerminatorKind},
        op::{self, StackOp},
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    map::{FxBuildHasher, FxHashMap},
    sync::RwLock,
};
use solar_sema::Gcx;
use std::{
    collections::{BinaryHeap, hash_map::Entry},
    hash::{BuildHasher, Hash, Hasher},
    ops::Range,
};

/// Words the search may grow the stack by above the run's entry and exit heights.
const STACK_SLACK: usize = 3;
/// How large a run the search takes and how many states it may expand for it.
#[derive(Clone, Copy)]
struct Budget {
    /// Operations a searched run must perform at least.
    min_operations: usize,
    /// Operations a searched run may perform.
    operations: usize,
    /// The states a run may expand.
    states: Limits,
    /// The states a run whose first objective is at most one swap above its bound may expand:
    /// the search can save little there, and keeps going long when it saves nothing.
    near: Limits,
}

/// How many states one search may expand.
#[derive(Clone, Copy)]
struct Limits {
    /// States to expand before some cheaper schedule turns up, after which the run keeps its
    /// code.
    first: usize,
    /// States to expand in all.
    refine: usize,
    /// States to expand after the last cheaper schedule before giving up on finding another.
    stall: usize,
}

/// The budget of a run in a hot loop block: longer runs, and a longer search for a first cheaper
/// schedule, as their code runs many times per call.
const LOOP_BUDGET: Budget = Budget {
    min_operations: 1,
    operations: 16,
    states: Limits { first: 1600, refine: 2400, stall: 100 },
    near: Limits { first: 100, refine: 200, stall: 50 },
};
/// The budget of any other run, whose code runs at most a few times per call.
const BLOCK_BUDGET: Budget = Budget {
    min_operations: 2,
    operations: 12,
    states: Limits { first: 200, refine: 600, stall: 100 },
    near: Limits { first: 50, refine: 100, stall: 25 },
};
/// The budget of a run outside hot loops when optimizing for size, which searches runs of one
/// operation too.
const SIZE_BLOCK_BUDGET: Budget = Budget { min_operations: 1, ..BLOCK_BUDGET };
/// How many times its bound the search adds to a state's cost to order it: greedier than an
/// exact search, it reaches cheaper schedules after far fewer states.
const BOUND_WEIGHT: u64 = 3;
/// Deepest stack word a `DUP` or `SWAP` reaches.
const REACH: usize = 16;
/// Bits of a word's index in a packed stack.
const WORD_BITS: u32 = 6;
/// Words a packed stack holds.
const MAX_HEIGHT: usize = 21;
/// Blocks an abort may span through direct jumps.
const MAX_ABORT_JUMPS: usize = 4;
/// Bits of a key's hash that pick its shard of the cache shared by all programs.
const SHARED_SHARD_BITS: u32 = 6;
/// Shards of the cache shared by all programs.
const SHARED_SHARDS: usize = 1 << SHARED_SHARD_BITS;

pub(super) struct StackReschedule;

impl EvmPass for StackReschedule {
    fn name(&self) -> &'static str {
        "stack-reschedule"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let code_follows = module.code_follows;
        let optimization = module.optimization;
        let mut halts = DenseBitSet::new_empty(module.blocks.len());
        for id in module.blocks.indices() {
            if aborts_without_stack(module, id) {
                halts.insert(id);
            }
        }
        let mut buffers = Buffers::default();
        let mut placed = DenseBitSet::new_empty(0);
        let shared = &gcx.codegen_state::<CodegenState>().schedules;
        let mut changed = false;
        for block in &mut module.blocks {
            let target = &block.target(gcx, optimization, code_follows);
            let hot_loop = block.metadata.in_loop && !block.metadata.hotness.is_cold();
            let budget = if hot_loop {
                LOOP_BUDGET
            } else if target.optimization().is_size() {
                SIZE_BLOCK_BUDGET
            } else {
                BLOCK_BUDGET
            };
            let mut rescheduler = Rescheduler {
                target,
                budget,
                halts: &halts,
                buffers: &mut buffers,
                placed: &mut placed,
                shared,
            };
            changed |= rescheduler.reschedule_block(&mut block.instructions);
        }
        changed
    }
}

/// A stack word of a run: one found on entry, by depth, an operation's result, or a constant.
#[derive(Clone, Copy, PartialEq, Eq)]
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
    /// The opcode that computes the same result from swapped operands: the operation's own when
    /// it is commutative, or the mirrored comparison.
    swapped: Option<u8>,
    /// The operations that have to run before this one, as a mask: the producers of its operands,
    /// and the previous check, memory or storage access for one of those, or the last check or
    /// gas-dependent access for any other.
    needs: u16,
}

/// The symbolic summary of a run.
struct Run {
    operations: Vec<Operation>,
    /// Positions of each constant's pushes in the run.
    pushes: SmallVec<[SmallVec<[usize; 4]>; 8]>,
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

impl Run {
    /// The dense index of a word: entry words, then results, then constants.
    fn index(&self, word: Word) -> usize {
        match word {
            Word::Entry(depth) => usize::from(depth),
            Word::Result(operation) => self.entry + usize::from(operation),
            Word::Constant(constant) => self.entry + self.operations.len() + usize::from(constant),
        }
    }
}

/// A step of a schedule.
#[derive(Clone, Copy, Debug)]
enum Move {
    Perform { operation: u8, swapped: bool },
    Push(u8),
    Stack(StackOp),
}

/// A search state: the stack's dense word indices packed six bits each, top lowest, and the
/// performed operations. Eight-byte alignment shrinks the search's arena entries from 64 to 48
/// bytes and its table entries from 48 to 32.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(Rust, packed(8))]
struct State {
    stack: u128,
    len: u8,
    done: u16,
}

impl State {
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

    /// Swaps the top word with `depth`, given their already decoded XOR.
    fn swap(self, depth: usize, difference: usize) -> Self {
        let difference = difference as u128;
        Self {
            stack: self.stack ^ difference ^ (difference << (WORD_BITS as usize * depth)),
            ..self
        }
    }
}

impl Hash for State {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        let Self { stack, len, done } = *self;
        stack.hash(hasher);
        // Pack the smaller fields into one hash input without losing any bits.
        ((u32::from(len) << u16::BITS) | u32::from(done)).hash(hasher);
    }
}

/// The settings of one block's search, and the buffers and schedules it shares with the others.
struct Rescheduler<'a> {
    target: &'a Target,
    budget: Budget,
    halts: &'a DenseBitSet<BlockId>,
    buffers: &'a mut Buffers,
    /// The positions of a run that `emit` has placed, reused across runs.
    placed: &'a mut DenseBitSet<usize>,
    shared: &'a SharedSchedules,
}

impl Rescheduler<'_> {
    fn reschedule_block(&mut self, instructions: &mut Vec<Instruction>) -> bool {
        let mut start = 0;
        while start < instructions.len() {
            // A run continues through checks that branch to a block aborting without the stack.
            let mut end = start;
            self.buffers.cuts.clear();
            loop {
                if is_branch_pair(instructions, end, self.halts) {
                    end += 2;
                    self.buffers.cuts.push(end);
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
            self.buffers.cuts.push(end);
            // Search windows of at most the budget's operations, cut after the checks.
            let mut count = 0;
            let operations_before = &mut self.buffers.operations_before;
            operations_before.clear();
            operations_before.push(0);
            operations_before.extend(instructions[start..end].iter().map(|inst| {
                count += usize::from(is_operation(inst));
                count
            }));
            // A run with too few operations has no schedule, nor does any window of it.
            if self.buffers.operations_before[end - start] < self.budget.min_operations {
                start = end;
                continue;
            }
            let mut window_start = start;
            while window_start < end {
                // The last cut that fits, or the first one when none does.
                let operations = |from: usize, to: usize| {
                    let before = &self.buffers.operations_before;
                    before[to - start] - before[from - start]
                };
                let cuts = &self.buffers.cuts;
                let later = cuts.partition_point(|&cut| cut <= window_start);
                let fitting = cuts[later..].partition_point(|&cut| {
                    operations(window_start, cut) <= self.budget.operations
                });
                let window_end = cuts[later + fitting.saturating_sub(1)];
                if let Some(replacement) =
                    self.best_schedule(&instructions[window_start..window_end])
                {
                    self.buffers.replacements.push((window_start..window_end, replacement));
                }
                window_start = window_end;
            }
            start = end;
        }
        let Buffers { replacements, rebuilt, .. } = &mut *self.buffers;
        if replacements.is_empty() {
            return false;
        }
        // prefix; window; suffix -> prefix; schedule; suffix
        rebuilt.clear();
        let mut old = instructions.drain(..);
        let mut cursor = 0;
        for (window, replacement) in replacements.drain(..) {
            rebuilt.extend(old.by_ref().take(window.start - cursor));
            old.by_ref().take(window.len()).for_each(drop);
            rebuilt.extend(replacement);
            cursor = window.end;
        }
        rebuilt.extend(old);
        std::mem::swap(instructions, rebuilt);
        true
    }

    /// The cheaper of the schedule that spans a window's checks and the one that splits at them,
    /// when it beats the window's own. Spanning the checks gives the search more freedom, but a
    /// longer run can exhaust its budget where the runs between the checks would not.
    fn best_schedule(&mut self, window: &[Instruction]) -> Option<Vec<Instruction>> {
        let mut best = None;
        let mut best_price = self.price(window);
        for candidate in [self.schedule(window), self.split_schedule(window)].into_iter().flatten()
        {
            let candidate_price = self.price(&candidate);
            if candidate_price < best_price
                && self.spends_no_more_before_observations(window, &candidate)
            {
                best_price = candidate_price;
                best = Some(candidate);
            }
        }
        best
    }

    /// The cheapest schedule the search finds for a run, if it beats the run's own.
    fn schedule(&mut self, run: &[Instruction]) -> Option<Vec<Instruction>> {
        // A run that ends in a label push, the target of the jump after it, keeps the push last so
        // the jump's edge stays visible.
        // run; push label -> schedule; push label
        if let [rest @ .., last] = run
            && last.pushed_block().is_some()
        {
            let mut scheduled = self.schedule(rest)?;
            scheduled.push(last.clone());
            return Some(scheduled);
        }
        let summary = summarize(run, self.target, self.budget, self.halts)?;
        let moves = search(&summary, self.target, self.budget, self.buffers, self.shared)?;
        emit(run, &summary, &moves, self.placed)
    }

    /// Schedules the runs between a window's checks and gas-dependent accesses separately, keeping
    /// each check in place; `None` when the window has neither or no run improves.
    ///
    /// A run that ends at an access spends no more gas before it than the original whenever its
    /// schedule costs no more gas, as nothing of the original ran after the access.
    fn split_schedule(&mut self, window: &[Instruction]) -> Option<Vec<Instruction>> {
        let mut rebuilt = Vec::new();
        let mut improved = false;
        let mut start = 0;
        let mut index = 0;
        while index < window.len() {
            let branch = is_branch_pair(window, index, self.halts);
            if !branch && !observes_gas(&window[index]) {
                index += 1;
                continue;
            }
            // The only split is an access that ends the window: its run is the window itself.
            if start == 0 && !branch && index + 1 == window.len() {
                return None;
            }
            // run; push target; jumpi -> schedule; push target; jumpi
            // run; access -> schedule ending in access
            improved |= self.extend_scheduled(&mut rebuilt, &window[start..=index]);
            start = index + 1;
            if branch {
                rebuilt.push(window[start].clone());
                start += 1;
            }
            index = start;
        }
        if start == 0 {
            return None;
        }
        improved |= self.extend_scheduled(&mut rebuilt, &window[start..]);
        improved.then_some(rebuilt)
    }

    /// Appends a run's cheapest schedule, or the run itself when the search finds none, and
    /// returns whether it found one.
    fn extend_scheduled(&mut self, rebuilt: &mut Vec<Instruction>, run: &[Instruction]) -> bool {
        let replacement = self.schedule(run);
        rebuilt.extend_from_slice(replacement.as_deref().unwrap_or(run));
        replacement.is_some()
    }

    /// The price of a sequence's stack operations and pushes, which is all a schedule changes.
    fn price(&self, instructions: &[Instruction]) -> u64 {
        let cost = instructions
            .iter()
            .filter(|inst| !is_operation(inst))
            .map(|inst| instruction_cost(inst, self.target))
            .sum::<Cost>();
        scalar(cost, self.target)
    }

    /// Whether a schedule spends at most the original's gas before each check and each operation
    /// that depends on the gas left, so the path that aborts at the check and the operation see at
    /// least the gas they saw before.
    ///
    /// The same operations run before each of them, or fewer, as none moves above one, so comparing
    /// the static gas of the instructions before it is enough.
    fn spends_no_more_before_observations(
        &self,
        original: &[Instruction],
        schedule: &[Instruction],
    ) -> bool {
        let spent_before = |instructions: &[Instruction]| {
            let mut spent = 0u64;
            let mut before = SmallVec::<[u64; 8]>::new();
            for inst in instructions {
                if is_operation(inst) && (inst.opcode == op::JUMPI || observes_gas(inst)) {
                    before.push(spent);
                }
                spent += u64::from(instruction_cost(inst, self.target).gas);
            }
            before
        };
        let (original, schedule) = (spent_before(original), spent_before(schedule));
        original.len() == schedule.len()
            && schedule.iter().zip(&original).all(|(schedule, original)| schedule <= original)
    }
}

/// The results of every search of a compilation, kept on its [`Gcx`]. A search is a function of
/// its key, so threads may race to fill an entry and every program may read any entry.
pub(in crate::backend::evm) struct SharedSchedules([SharedShard; SHARED_SHARDS]);

impl Default for SharedSchedules {
    fn default() -> Self {
        Self(std::array::from_fn(|_| SharedShard::default()))
    }
}

/// One shard of the shared schedules: moves by [`search_key`]. Each shard has cache lines of its
/// own, so threads on different shards do not contend.
#[derive(Default)]
#[repr(align(128))]
struct SharedShard(RwLock<SharedMoves>);

type SharedMoves = FxHashMap<Box<[u8]>, Option<Box<[Move]>>>;

/// The search's state storage, cleared and reused for every run.
#[derive(Default)]
struct Buffers {
    /// Visited states.
    arena: Vec<Node>,
    /// The cheapest arena entry of each state.
    best: FxHashMap<State, u32>,
    /// Arena entries by priority.
    open: Queue,
    /// Remaining-operation facts, independent of stack order, keyed by performed operations.
    pending: FxHashMap<u16, Pending>,
    /// Start of each cut of the run being windowed: after its checks, and at its end.
    cuts: Vec<usize>,
    /// Operations before each instruction of that run.
    operations_before: Vec<usize>,
    /// The windows of the block to replace, with their schedules.
    replacements: Vec<(Range<usize>, Vec<Instruction>)>,
    /// The block's instructions with the windows replaced.
    rebuilt: Vec<Instruction>,
    /// Dense operand words in pop order, reused across searches.
    operands: Vec<SmallVec<[usize; 3]>>,
}

/// Uses and ready operations shared by every state with the same performed operations.
#[derive(Clone, Copy)]
struct Pending {
    need: [u8; 64],
    ready: u16,
    wanted: u64,
}

/// A visited search state.
#[derive(Clone, Copy)]
struct Node {
    state: State,
    /// The arena entry this one was reached from.
    parent: u32,
    /// The move from the parent.
    step: Option<Move>,
    cost: u64,
    /// The bound on the remaining cost.
    bound: u64,
    refined: Refined,
}

/// Whether an arena entry's next operation needs a swap the bound leaves out.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Refined {
    /// Not yet checked.
    Unknown,
    NoSwap,
    Swap,
    /// A cheaper entry of the same state replaced this one.
    Superseded,
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

/// Whether an instruction may be part of a searched run.
fn schedulable(inst: &Instruction) -> bool {
    if let Some(stack_op) = inst.as_stack_op() {
        return stack_op.single_byte_evm_opcode().is_some();
    }
    if inst.is_encoded_push() {
        return constant(inst).is_some();
    }
    let Some(definition) = inst.definition() else { return false };
    matches!(definition.stack_io, Some((_, outputs)) if outputs <= 1)
        && (op::reads_only_operands_or_calldata(definition.opcode)
            || op::is_plain_access(definition.opcode))
}

/// The constant a push pushes, when pushing it again is free of side conditions.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Constant {
    Immediate(U256),
    Block(BlockId),
}

fn constant(inst: &Instruction) -> Option<Constant> {
    inst.concrete_immediate()
        .map(Constant::Immediate)
        .or_else(|| inst.pushed_block().map(Constant::Block))
}

/// Price of a push as assembly encodes it: the narrowest `PUSH` of its value.
fn push_cost(inst: &Instruction, target: &Target) -> Cost {
    match inst.concrete_immediate() {
        Some(value) if value.is_zero() && target.evm_version().has_push0() => {
            target.opcode(op::PUSH0)
        }
        Some(value) => target.opcode(op::push(value.byte_len().max(1) as u8)),
        None => target.label_push(),
    }
}

/// Price of an instruction a run holds.
fn instruction_cost(inst: &Instruction, target: &Target) -> Cost {
    if let Some(stack_op) = inst.as_stack_op() {
        target.opcode(stack_op.single_byte_evm_opcode().expect("runs hold one-byte stack ops"))
    } else if inst.is_encoded_push() {
        push_cost(inst, target)
    } else {
        target.opcode(inst.opcode)
    }
}

/// Applies a stack operation to a run's symbolic stack.
fn apply(stack: &mut SmallVec<[Word; 24]>, stack_op: StackOp) {
    match stack_op {
        StackOp::Dup(depth) => stack.insert(0, stack[usize::from(depth) - 1]),
        StackOp::Swap(depth) => stack.swap(0, usize::from(depth)),
        StackOp::Pop => {
            stack.remove(0);
        }
        StackOp::Exchange(..) => unreachable!("runs hold one-byte stack ops"),
    }
}

/// Orders costs under the objective as one number: gas first, then bytes.
fn scalar(cost: Cost, target: &Target) -> u64 {
    let [first, second] = target.objective_key(cost);
    (u64::from(first) << 32) | u64::from(second)
}

/// Executes a run symbolically, stopping when another operation would exceed the budget.
fn summarize(
    run: &[Instruction],
    target: &Target,
    budget: Budget,
    halts: &DenseBitSet<BlockId>,
) -> Option<Run> {
    let mut stack = SmallVec::<[Word; 24]>::new();
    let mut entry = 0usize;
    let mut operations = Vec::new();
    let mut pushes = SmallVec::<[SmallVec<[usize; 4]>; 8]>::new();
    let mut constant_costs = SmallVec::<[u64; 8]>::new();
    let mut keys = SmallVec::<[Constant; 8]>::new();
    let mut cost = Cost::ZERO;
    let mut last_ordered = None;
    // The last check or gas-dependent access: the operations after it stay after it.
    let mut last_observed = None;
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
        let branch = is_branch_pair(run, position, halts);
        if let Some(stack_op) = inst.as_stack_op() {
            deepen(&mut stack, &mut entry, stack_op.required_depth())?;
            apply(&mut stack, stack_op);
            cost += instruction_cost(inst, target);
        } else if inst.is_encoded_push() && !branch {
            let key = constant(inst)?;
            let index = match keys.iter().position(|&existing| existing == key) {
                Some(index) => index,
                None => {
                    keys.push(key);
                    pushes.push(SmallVec::new());
                    constant_costs.push(scalar(push_cost(inst, target), target));
                    keys.len() - 1
                }
            };
            pushes[index].push(position);
            stack.insert(0, Word::Constant(u8::try_from(index).ok()?));
            cost += push_cost(inst, target);
        } else {
            if operations.len() >= budget.operations {
                return None;
            }
            // A branch takes its condition, is ordered with every access and observes the gas.
            let (inputs, outputs, ordered, observes, swapped) = if branch {
                (1, 0, true, true, None)
            } else {
                let definition = inst.definition()?;
                let (inputs, outputs) = definition.stack_io?;
                (
                    inputs,
                    outputs,
                    !op::reads_only_operands_or_calldata(definition.opcode),
                    definition.observes_gas(),
                    op::swapped_binary_opcode(definition.opcode),
                )
            };
            deepen(&mut stack, &mut entry, usize::from(inputs))?;
            let operands = stack.drain(..usize::from(inputs)).collect::<SmallVec<[Word; 3]>>();
            let index = u8::try_from(operations.len()).ok()?;
            let after = if ordered { last_ordered } else { last_observed };
            let needs =
                operands.iter().fold(after.map_or(0, |after| 1u16 << after), |mask, word| {
                    mask | if let Word::Result(producer) = word { 1 << producer } else { 0 }
                });
            operations.push(Operation {
                position,
                operands,
                produces: outputs == 1,
                swapped,
                needs,
            });
            if ordered {
                last_ordered = Some(index);
            }
            if observes {
                last_observed = Some(index);
            }
            if outputs == 1 {
                stack.insert(0, Word::Result(index));
            }
        }
        peak = peak.max(stack.len());
        position += if branch { 2 } else { 1 };
    }
    (operations.len() >= budget.min_operations
        && operations.len() <= budget.operations
        && entry <= REACH
        && keys.len() <= 8)
        .then_some(Run { operations, pushes, constant_costs, entry, exit: stack, cost, peak })
}

/// Operation indices in ascending order, as in a full operation scan filtered by the mask.
fn operation_indices(mut mask: u16) -> impl Iterator<Item = usize> {
    std::iter::from_fn(move || {
        if mask == 0 {
            return None;
        }
        let index = mask.trailing_zeros() as usize;
        mask &= mask - 1;
        Some(index)
    })
}

/// Searches for the cheapest schedule of a run's operations that leaves its stack, when one
/// beats the run's own stack operations. A run that needs a search is looked up in the schedules
/// shared by all programs before it, and stored there after it.
fn search(
    run: &Run,
    target: &Target,
    limits: Budget,
    buffers: &mut Buffers,
    shared: &SharedSchedules,
) -> Option<Vec<Move>> {
    let operation_count = run.operations.len();
    let word_count = run.entry + operation_count + run.pushes.len();
    let height = run.peak.max(run.entry.max(run.exit.len()) + STACK_SLACK);
    if word_count > 64 || height > MAX_HEIGHT {
        return None;
    }
    let Buffers { arena, best, open, pending, operands, .. } = buffers;
    let all_done = ((1u32 << operation_count) - 1) as u16;
    operands.clear();
    operands.extend(run.operations.iter().map(|operation| {
        operation.operands.iter().map(|&word| run.index(word)).collect::<SmallVec<[usize; 3]>>()
    }));
    let mut exit_uses = [0u8; 64];
    for &word in &run.exit {
        exit_uses[run.index(word)] += 1;
    }
    let exit =
        run.exit.iter().rev().fold(State { stack: 0, len: 0, done: all_done }, |state, &word| {
            state.push(run.index(word))
        });
    let exit_len = run.exit.len();
    let exit_words = exit.words();
    // The uses left of each word: by the exit and by the operations not yet performed.
    let demand = |done: u16| -> [u8; 64] {
        let mut need = exit_uses;
        for (index, operation_operands) in operands.iter().enumerate() {
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
    // Whether a word is the result of an operation not yet performed.
    let pending_result = |word: usize, done: u16| {
        result_of(word).is_some_and(|operation| done & (1 << operation) == 0)
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
    // The cheaper of a `DUP` and a push of the word.
    let copy_cost = |word: usize| {
        constant_of(word).map_or(dup, |constant| dup.min(run.constant_costs[constant]))
    };
    // Every missing copy of a word needs a `DUP` or a push, and every surplus copy a `POP`.
    let term = |word: usize, count: u8, need: u8, done: u16| -> u64 {
        if count > need {
            return u64::from(count - need) * pop;
        }
        let have = count + u8::from(pending_result(word, done));
        if need <= have {
            return 0;
        }
        u64::from(need - have) * copy_cost(word)
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
    let initial_bound = estimate(start);
    let gap = (budget >> 32).saturating_sub(initial_bound >> 32);
    if gap == 0 {
        return None;
    }
    // Runs the search would take up are looked up, and stored after it, in the shared schedules.
    let key = search_key(run, [dup, swap, pop, budget], limits);
    // The shard takes the hash's high bits, as its table indexes by the low ones.
    let shard =
        &shared.0[(FxBuildHasher.hash_one(&key[..]) >> (u64::BITS - SHARED_SHARD_BITS)) as usize];
    if let Some(moves) = shard.0.read().get(&key[..]) {
        return moves.as_deref().map(<[Move]>::to_vec);
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
                let pending = pending_result(word, state.done);
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
                        || pending_result(usize::from(word), state.done)
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
                    for index in operation_indices(ready) {
                        let operation = &run.operations[index];
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
                        let reachable = match *operands[index] {
                            [a] => top(0) == Some(a) || owed(&counts, a, 1),
                            [a, b] => {
                                reaches(a, b) || (operation.swapped.is_some() && reaches(b, a))
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

    arena.clear();
    best.clear();
    open.clear();
    pending.clear();
    arena.push(Node {
        state: start,
        parent: u32::MAX,
        step: None,
        cost: 0,
        bound: initial_bound,
        refined: Refined::Unknown,
    });
    best.insert(start, 0);
    open.push(BOUND_WEIGHT * initial_bound, 0, 0);
    let mut expansions = 0;
    let mut found = None;
    let mut found_at = 0;
    let mut successors = SmallVec::<[(State, Move, u64, u64); 48]>::new();
    while let Some((cost, node)) = open.pop() {
        let Node { state, bound: here, refined, .. } = arena[node as usize];
        let raised = match refined {
            Refined::Superseded => continue,
            Refined::Swap => swap,
            Refined::Unknown | Refined::NoSwap => 0,
        };
        if cost + here + raised >= budget {
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
        let facts = pending.entry(state.done).or_insert_with(|| {
            let need = demand(state.done);
            let ready = (0..operation_count).fold(0u16, |ready, index| {
                let needs = run.operations[index].needs;
                let ready_now = state.done & (1 << index) == 0 && state.done & needs == needs;
                ready | (u16::from(ready_now) << index)
            });
            // Words some ready operation reads.
            let mut wanted = 0u64;
            for index in operation_indices(ready) {
                for &operand in &operands[index] {
                    wanted |= 1 << operand;
                }
            }
            Pending { need, ready, wanted }
        });
        let need = &facts.need;
        let ready = facts.ready;
        let wanted = facts.wanted;
        // Raise the bound of a state that needs a swap before its next operation, and order
        // it again, before expanding it.
        if refined == Refined::Unknown {
            if needs_swap(state, stack, ready, &counts, need) {
                arena[node as usize].refined = Refined::Swap;
                if cost + here + swap < budget {
                    open.push(cost + BOUND_WEIGHT * (here + swap), cost, node);
                }
                continue;
            }
            arena[node as usize].refined = Refined::NoSwap;
        }
        expansions += 1;
        let stalled = found.is_some() && expansions > found_at + limits.stall;
        if expansions > limits.refine || (found.is_none() && expansions > limits.first) || stalled {
            break;
        }
        let finishing = state.done == all_done;
        let at = |depth: usize| usize::from(stack[depth]);
        // A surplus copy on top is popped or consumed before anything else, and until every
        // operation ran a swap follows only an operation or a pop: swapping again, or right after
        // a copy or push, mostly reorders words the bound cannot tell apart.
        let surplus_top = len > 0 && counts[at(0)] > need[at(0)];
        let swaps = !surplus_top
            && (finishing
                || !matches!(
                    arena[node as usize].step,
                    Some(Move::Push(_) | Move::Stack(StackOp::Dup(_) | StackOp::Swap(_)))
                ));
        successors.clear();
        // Perform a ready operation.
        for index in operation_indices(ready) {
            let operation = &run.operations[index];
            let operation_operands = &operands[index];
            let arity = operation_operands.len();
            if len < arity || (surplus_top && !operation_operands.contains(&at(0))) {
                continue;
            }
            let in_order = (0..arity).all(|depth| at(depth) == operation_operands[depth]);
            let swapped = !in_order
                && operation.swapped.is_some()
                && arity == 2
                && at(0) == operation_operands[1]
                && at(1) == operation_operands[0];
            if !in_order && !swapped {
                continue;
            }
            let mut next = state.drop(arity);
            if operation.produces {
                next = next.push(run.entry + index);
            }
            next.done |= 1 << index;
            // Consuming ready operands decreases their copies and remaining uses equally.
            // Producing a result replaces its pending copy with a real one; only an unused
            // result adds a surplus copy that needs a POP.
            let result = run.entry + index;
            debug_assert!(operation_operands.iter().all(|&word| {
                result_of(word).is_none_or(|producer| state.done & (1 << producer) != 0)
            }));
            debug_assert_eq!(counts[result], 0);
            let bound = here + if operation.produces && need[result] == 0 { pop } else { 0 };
            successors.push((next, Move::Perform { operation: index as u8, swapped }, 0, bound));
        }
        if len < height && !surplus_top {
            // Push a constant a ready operation, or the finished run, still needs.
            for (constant, &constant_cost) in run.constant_costs.iter().enumerate() {
                let word = run.entry + operation_count + constant;
                if (wanted & (1 << word) != 0 || (finishing && exit_uses[word] != 0))
                    && counts[word] < need[word]
                {
                    successors.push((
                        state.push(word),
                        Move::Push(constant as u8),
                        constant_cost,
                        here - copy_cost(word),
                    ));
                }
            }
            // Copy a word that has more uses than copies.
            let mut seen = 0u64;
            for depth in 1..=len.min(REACH) {
                let word = at(depth - 1);
                if seen & (1 << word) != 0 {
                    continue;
                }
                seen |= 1 << word;
                if counts[word] < need[word] {
                    // A live copy proves its producer ran; one fewer copy is missing.
                    debug_assert!(
                        result_of(word).is_none_or(|producer| state.done & (1 << producer) != 0)
                    );
                    successors.push((
                        state.push(word),
                        Move::Stack(StackOp::Dup(depth as u8)),
                        dup,
                        here - copy_cost(word),
                    ));
                }
            }
        }
        if swaps {
            // Swap up a word a ready operation reads, a surplus copy, or any word once all
            // operations ran.
            for depth in 1..len.min(REACH + 1) {
                let word = at(depth);
                if word == at(0)
                    || !(finishing || wanted & (1 << word) != 0 || counts[word] > need[word])
                {
                    continue;
                }
                successors.push((
                    state.swap(depth, at(0) ^ word),
                    Move::Stack(StackOp::Swap(depth as u8)),
                    swap,
                    here,
                ));
            }
        }
        if surplus_top {
            successors.push((state.drop(1), Move::Stack(StackOp::Pop), pop, here - pop));
        }
        for &(successor, step, step_cost, remaining) in &successors {
            let next_cost = cost + step_cost;
            // An operation without operands, such as `CALLDATASIZE`, grows the stack like a push.
            if usize::from(successor.len) > height || next_cost + remaining >= budget {
                continue;
            }
            debug_assert_eq!(remaining, estimate(successor));
            let id = arena.len() as u32;
            match best.entry(successor) {
                Entry::Occupied(mut entry) => {
                    let known = *entry.get();
                    if arena[known as usize].cost <= next_cost {
                        continue;
                    }
                    arena[known as usize].refined = Refined::Superseded;
                    entry.insert(id);
                }
                Entry::Vacant(entry) => {
                    entry.insert(id);
                }
            }
            arena.push(Node {
                state: successor,
                parent: node,
                step: Some(step),
                cost: next_cost,
                bound: remaining,
                refined: Refined::Unknown,
            });
            open.push(next_cost + BOUND_WEIGHT * remaining, next_cost, id);
        }
    }
    let moves = found.map(|mut cursor| {
        let mut moves = Vec::new();
        while let Some(step) = arena[cursor as usize].step {
            moves.push(step);
            cursor = arena[cursor as usize].parent;
        }
        moves.reverse();
        moves
    });
    shard.0.write().insert(key.as_slice().into(), moves.as_deref().map(Box::from));
    moves
}

/// Builds the instructions of a schedule, after replaying it against the run's summary.
fn emit(
    run: &[Instruction],
    summary: &Run,
    moves: &[Move],
    placed: &mut DenseBitSet<usize>,
) -> Option<Vec<Instruction>> {
    let mut stack: SmallVec<[Word; 24]> =
        (0..summary.entry).map(|depth| Word::Entry(depth as u8)).collect();
    let mut performed = 0u32;
    let mut instructions = Vec::with_capacity(moves.len());
    placed.clear_to(run.len());
    for &step in moves {
        match step {
            // operands -> op, or swapped operands -> swapped op
            Move::Perform { operation, swapped } => {
                let operation_info = &summary.operations[usize::from(operation)];
                let arity = operation_info.operands.len();
                let mut expected = operation_info.operands.clone();
                if swapped {
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
                let position = operation_info.position;
                let original = &run[position];
                placed.insert(position);
                instructions.push(match operation_info.swapped {
                    Some(opcode) if swapped && opcode != original.opcode => {
                        Instruction::opcode(opcode).with_metadata(original.metadata.clone())
                    }
                    _ => original.clone(),
                });
                // A branch's operation is its push, followed by its `JUMPI`.
                if original.is_encoded_push() {
                    placed.insert(position + 1);
                    instructions.push(run[position + 1].clone());
                }
            }
            // push constant
            Move::Push(constant) => {
                stack.insert(0, Word::Constant(constant));
                let originals = &summary.pushes[usize::from(constant)];
                match originals.iter().copied().find(|&position| !placed.contains(position)) {
                    Some(position) => {
                        placed.insert(position);
                        instructions.push(run[position].clone());
                    }
                    None => {
                        // A constant pushed more often than in the run copies the first push's
                        // source only.
                        let first = &run[originals[0]];
                        instructions.push(
                            first
                                .clone()
                                .with_metadata(Default::default())
                                .with_source_debug(&first.metadata),
                        );
                    }
                }
            }
            // dupN | swapN | pop
            Move::Stack(stack_op) => {
                apply(&mut stack, stack_op);
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
            if !placed.contains(position) {
                last.metadata.absorb_debug_info(&inst.metadata);
            }
        }
    }
    Some(instructions)
}

/// Whether the instructions at `index` push a label and branch to it, where the target aborts
/// without reading the stack: the branch then depends on its condition alone.
fn is_branch_pair(
    instructions: &[Instruction],
    index: usize,
    halts: &DenseBitSet<BlockId>,
) -> bool {
    let (Some(push), Some(jumpi)) = (instructions.get(index), instructions.get(index + 1)) else {
        return false;
    };
    push.pushed_block().is_some_and(|block| halts.contains(block))
        && jumpi.as_evm_opcode() == Some(op::JUMPI)
        && is_split_point(instructions, index)
        && is_split_point(instructions, index + 1)
        && !jumpi.keeps_with_next()
}

/// Whether a block aborts without reading a word that was on the stack when it was entered, and
/// without anything that tells how much work ran before the branch into it. The abort may end in
/// a block that a few direct jumps reach.
///
/// A run may move work below a branch to such a block: the work then runs only on the path that
/// continues, which is the one that needs it. A block that returns or stops is a normal exit, and
/// so is one that may branch elsewhere before its end. A `GAS` reading, a call or an `SSTORE` would
/// see how much gas the path spent, so a block with one keeps its branch out of runs.
fn aborts_without_stack(module: &Module, mut id: BlockId) -> bool {
    let mut depth = 0usize;
    // Shared tails move the end of an abort into another block, so follow a few direct jumps.
    for _ in 0..MAX_ABORT_JUMPS {
        let block = &module.blocks[id];
        for inst in &block.instructions {
            if inst.opcode == op::JUMPI
                || inst
                    .definition()
                    .is_some_and(|definition| definition.is_terminal() || definition.observes_gas())
            {
                return false;
            }
            let effect = inst.stack_effect();
            let Some(rest) = depth.checked_sub(usize::from(effect.inputs)) else { return false };
            depth = rest + usize::from(effect.outputs);
        }
        match block.terminator.as_ref().map(|terminator| &terminator.kind) {
            Some(kind @ TerminatorKind::Op(op::REVERT | op::INVALID)) => {
                return kind.stack_io().is_some_and(|(inputs, _)| usize::from(inputs) <= depth);
            }
            Some(&TerminatorKind::Jump(target)) => id = target,
            _ => return false,
        }
    }
    false
}

/// Whether an instruction of a run is an operation: a branch pair's push is part of its `JUMPI`.
fn is_operation(inst: &Instruction) -> bool {
    inst.as_stack_op().is_none() && !inst.is_encoded_push()
}

/// Everything [`search`] reads of a run, its budget and its target, as bytes: the operands,
/// ordering and exit words, the prices of moves, the run and its constants, and the budget's
/// limits. It leaves out which opcodes and immediates the run holds, which a schedule does not
/// depend on.
fn search_key(run: &Run, prices: [u64; 4], budget: Budget) -> SmallVec<[u8; 192]> {
    let mut key = SmallVec::new();
    // The header fixes the three word ranges, so dense indices identify their kinds too.
    key.extend(
        [run.entry, run.peak, run.pushes.len(), run.exit.len(), run.operations.len()]
            .map(|len| len as u8),
    );
    for operation in &run.operations {
        key.extend([
            u8::from(operation.produces) | u8::from(operation.swapped.is_some()) << 1,
            operation.operands.len() as u8,
        ]);
        key.extend(operation.needs.to_le_bytes());
        for &operand in &operation.operands {
            key.push(run.index(operand) as u8);
        }
    }
    for &exit in &run.exit {
        key.push(run.index(exit) as u8);
    }
    let Budget { states, near, .. } = budget;
    for limit in [states.first, states.refine, states.stall, near.first, near.refine, near.stall] {
        key.extend((limit as u32).to_le_bytes());
    }
    for price in prices.into_iter().chain(run.constant_costs.iter().copied()) {
        key.extend(price.to_le_bytes());
    }
    key
}

/// Whether an instruction depends on the gas left, such as an `SSTORE` with its sentry.
fn observes_gas(inst: &Instruction) -> bool {
    inst.definition().is_some_and(|definition| definition.observes_gas())
}

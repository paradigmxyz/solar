//! Hoisting of pushed words to the height they end at.
//!
//! The planner pushes a word that it materializes, such as an immediate, a retained constant, or
//! a calldata word at a constant offset, where its consumer needs it: on top of the words the
//! block computed before. When the word belongs below some of them, the planner sinks it with
//! `SWAP`s, as in `ADD; PUSH c; SWAP1`, where `c` goes under the sum. Pushed earlier, at the
//! height where it ends, the word needs no swap, and the `DUP`s and `SWAP`s in between reach
//! their words one place deeper. The planner cannot make this choice itself, because it plans
//! each instruction on the stack the previous ones left.
//!
//! The pass runs once on each non-cold block of the plan that [`plan_cached`] keeps, never on the
//! trial plans: hoisting in every trial changed the loop header layouts and the retained
//! constants that the search picked, and the code got larger (15 kB on seaport). Cold blocks keep
//! the planner's order, because hoisting there stopped the EVM IR passes from merging the tails of
//! revert blocks.
//!
//! [`plan_cached`]: super::EvmCodegen::plan_cached
//!
//! ## What moves
//!
//! A *run* is a sequence of [`Step::Materialize`] and `SWAP` steps that starts at a
//! materialization and ends at its last swap. Simulating the run on the words it touches shows
//! whether it only inserts the pushed words below other words, and where: the words that were
//! on the stack keep their order, and each pushed word ends under a fixed number of them. A
//! pushed word that ends at height `a`, counted from the block's start, can be pushed instead
//! at the latest kept step `k` where the stack is `a` high, if no step from `k` on
//!
//! - pops a word below height `a`, which would pop the pushed word or reach under it,
//! - is a `SWAP` or `EXCHANGE` that moves the top word while the pushed word is on top, or
//! - is a barrier: an internal call or a multi-result publication, an opcode that is not position
//!   independent, or a push of the gas left. Moving a push across the first would change the stack
//!   height that the callee is entered at, and across the others would change what they observe.
//!
//! Every `DUP`, `SWAP` and `EXCHANGE` from `k` to the end of the run that reaches below the
//! pushed word then names a deeper word, and is renumbered. The hoist is rejected when a
//! renumbered operation is out of reach or has no lowering for the target, and kept only when
//! the renumbered operations cost less, by the target model, than the run's swaps plus the same
//! operations before renumbering. The block ends with the words in the same order, so its exit
//! does not change.
//!
//! The pass appends the steps it keeps to a new list in one forward pass over the block. A hoist
//! searches at most [`MAX_HOIST_STEPS`] earlier steps, so the cost per block is linear in its
//! length.
//!
//! ## What stays
//!
//! The pass leaves a block alone when it has more than [`MAX_BLOCK_STEPS`] steps: long blocks
//! are mostly straight-line code that repeats the same sequences, which `outline` shares until a
//! push moves out of one of the copies. A push of zero stays where the planner puts it: later
//! passes match zero stores, and `pack-data` turns a run of them into one `CALLDATACOPY` past the
//! end of calldata, which a zero pushed earlier splits. A run that permutes the words that were
//! on the stack stays, as does a push with no `SWAP` after it. Trampolines are not hoisted, and
//! no word moves to another block.
//!
//! ## Debug info
//!
//! A moved push keeps the origin it had before the move, not the one of the instruction it lands
//! in: [`Step::Begin`] or [`Step::BeginTerminator`] sets that origin before the push and the
//! origin of the place it lands in after it. The emitter gives the code before a block's first
//! instruction the origin of that instruction, so a push that lands there gets no origin instead
//! of one that would replace it. The steps hold no code, so the bytecode does not depend on
//! whether debug info is on.

use super::{BlockPlan, FunctionPlan, ModuleInfo, Step};
use crate::{
    backend::evm::{
        codegen::{StackOp, values::gas_minus},
        op,
    },
    mir::{Function, InstId, InstKind, Value, ValueId},
    target::{Cost, Target},
};
use smallvec::SmallVec;
use std::cmp::Reverse;

/// Most steps a hoist looks back for the height its word ends at.
const MAX_HOIST_STEPS: usize = 128;

/// Most steps of a block the pass hoists in. Longer blocks are straight-line code that repeats
/// the same sequences, which outlining shares until a push moves out of one of the copies.
const MAX_BLOCK_STEPS: usize = 256;

/// Hoists the pushed words of each non-cold block of `plan`, a plan for `func` or for the copy of
/// it in `plan.rebuilt`, and raises the plan's peak by the most that any block pushed earlier.
pub(super) fn hoist_plan(
    func: &Function,
    info: &ModuleInfo,
    target: &Target,
    plan: &mut FunctionPlan,
) {
    let func = plan.rebuilt.as_deref().unwrap_or(func);
    let hoister = Hoister { func, info, target };
    let mut extra = 0;
    for (block, block_plan) in plan.blocks.iter_mut_enumerated() {
        if let Some(block_plan) = block_plan
            && !func.blocks[block].hotness.is_cold()
        {
            extra = extra.max(hoister.hoist_block(block_plan));
        }
    }
    // Each step of a block holds at most `extra` more words than it did.
    plan.peak += extra;
}

struct Hoister<'a> {
    func: &'a Function,
    info: &'a ModuleInfo,
    target: &'a Target,
}

/// The debug origin the code at some point of a block takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Origin {
    /// The instruction a [`Step::Begin`] names, or none.
    Inst(Option<InstId>),
    /// The block's terminator.
    Terminator,
}

impl Origin {
    /// The step that sets this origin.
    fn step(self) -> Step {
        match self {
            Self::Inst(inst) => Step::Begin(inst),
            Self::Terminator => Step::BeginTerminator,
        }
    }
}

/// A kept step with the stack height and the origin before it.
struct Placed {
    step: Step,
    /// Words on the stack relative to the block's start.
    height: isize,
    /// Words that hoists added to the stack at this step.
    extra: usize,
    origin: Origin,
}

/// A pushed word of a run, with where it goes.
struct Insert {
    /// Index in the run of the push.
    index: usize,
    /// Height the stack has before the word is pushed in its new place.
    height: isize,
    /// Index in the kept steps before which the word is pushed.
    point: usize,
    /// The origin of the push in the planned code.
    origin: Origin,
}

/// The steps of a block that are kept so far.
struct Kept<'a> {
    hoister: &'a Hoister<'a>,
    steps: Vec<Placed>,
    height: isize,
    origin: Origin,
    /// Index in `steps` of the first step of the terminator, once reached.
    terminator: Option<usize>,
    /// Index in `steps` of the first `Begin(Some(_))` of the body.
    first_inst: Option<usize>,
    /// The most words that hoists added to the stack at any step.
    extra: usize,
}

impl Hoister<'_> {
    /// Returns the most words that hoists added to the stack at any step.
    fn hoist_block(&self, block: &mut BlockPlan) -> usize {
        if block.steps.len() > MAX_BLOCK_STEPS
            || !block.steps.iter().any(|step| matches!(step, Step::Materialize(_)))
        {
            return 0;
        }
        let steps = std::mem::take(&mut block.steps);
        let split = block.terminator_start;
        // The emitter gives the code before the first `Begin(Some(_))` of the body the origin of
        // that instruction, or the terminator's when there is none.
        let start_origin = steps[..split]
            .iter()
            .find_map(|step| match *step {
                Step::Begin(Some(inst)) => Some(Origin::Inst(Some(inst))),
                _ => None,
            })
            .unwrap_or(Origin::Terminator);
        let mut kept = Kept {
            hoister: self,
            steps: Vec::with_capacity(steps.len()),
            height: 0,
            origin: start_origin,
            terminator: None,
            first_inst: None,
            extra: 0,
        };
        let mut i = 0;
        while i < steps.len() {
            if i == split {
                kept.terminator = Some(kept.steps.len());
                kept.origin = Origin::Terminator;
            }
            if matches!(steps[i], Step::Materialize(_))
                && let Some(end) = self.run_end(&steps, i)
                && kept.hoist(&steps[i..end], i, split)
            {
                i = end;
                continue;
            }
            kept.push(steps[i].clone());
            i += 1;
        }
        block.terminator_start = kept.terminator.unwrap_or(kept.steps.len());
        block.steps = kept.steps.into_iter().map(|placed| placed.step).collect();
        kept.extra
    }

    /// The end of the run that starts at `start`, or `None` when it has no swap.
    fn run_end(&self, steps: &[Step], start: usize) -> Option<usize> {
        let mut end = None;
        for (offset, step) in steps[start..].iter().enumerate() {
            match *step {
                Step::Materialize(value) if self.hoistable(value) && !self.is_zero(value) => {}
                Step::Stack(StackOp::Swap(_)) => end = Some(start + offset + 1),
                _ => break,
            }
        }
        end
    }

    /// Whether `value` is zero, whose push stays where the planner puts it.
    fn is_zero(&self, value: ValueId) -> bool {
        matches!(self.func.value(value), Value::Undef(_))
            || self.func.value_u256(value).is_some_and(|word| word.is_zero())
    }

    /// Whether code may move across the push of `value`: not when it reads the gas left.
    fn hoistable(&self, value: ValueId) -> bool {
        gas_minus(self.func, value).is_none()
            && !matches!(
                *self.func.value(value),
                Value::Inst(inst) if matches!(self.func.inst(inst).kind, InstKind::Gas)
            )
    }

    /// The words a step pops and pushes, and whether code may not move across it.
    fn effect(&self, step: &Step) -> (usize, usize, bool) {
        match *step {
            Step::Stack(StackOp::Dup(_)) => (0, 1, false),
            Step::Stack(StackOp::Pop) => (1, 0, false),
            Step::Stack(StackOp::Swap(_) | StackOp::Exchange(..)) => (0, 0, false),
            Step::Materialize(value) => (0, 1, !self.hoistable(value)),
            Step::Reload(_) | Step::ReloadRet | Step::Filler => (0, 1, false),
            Step::Spill(_) | Step::SpillRet => (1, 0, false),
            Step::Begin(_) | Step::BeginTerminator => (0, 0, false),
            Step::Op(opcode) => {
                let def = op::definition(opcode).expect("known opcode");
                let (pops, pushes) = def.stack_io.expect("fixed stack effect");
                (pops.into(), pushes.into(), !def.is_position_independent())
            }
            Step::Inst(inst) => match self.func.inst(inst).kind {
                InstKind::Select(..) => (3, 1, false),
                InstKind::DataCopy(..) => (2, 0, false),
                InstKind::Alloc { .. }
                | InstKind::LibraryAddress(_)
                | InstKind::LoadImmutable(_)
                | InstKind::ConstructorArgsBase
                | InstKind::ConstructorArgsEnd
                | InstKind::InternalFrameAddr(_)
                | InstKind::HeapFloor => (0, 1, false),
                ref kind => unreachable!("instruction `{}` has no stack effect", kind.mnemonic()),
            },
            Step::Call(callee, _) => (self.info.params[callee], self.info.returns[callee], true),
            Step::Publish { arity, .. } => (arity, 1, true),
        }
    }
}

impl Kept<'_> {
    fn push(&mut self, step: Step) {
        let (pops, pushes, _) = self.hoister.effect(&step);
        let origin = self.origin;
        match step {
            Step::Begin(inst) => {
                if inst.is_some() && self.first_inst.is_none() && self.terminator.is_none() {
                    self.first_inst = Some(self.steps.len());
                }
                self.origin = Origin::Inst(inst);
            }
            Step::BeginTerminator => self.origin = Origin::Terminator,
            _ => {}
        }
        self.steps.push(Placed { step, height: self.height, extra: 0, origin });
        self.height += pushes as isize - pops as isize;
    }

    /// Replaces `run`, the steps of the block from index `start`, by the pushes at their new
    /// places when they only insert words below other words of the stack. `split` is the index of
    /// the block's first terminator step. Returns whether it hoisted.
    fn hoist(&mut self, run: &[Step], start: usize, split: usize) -> bool {
        let hoister = self.hoister;
        let len = self.steps.len();
        // Words a run touches, bottom first: `0..deepest` for the ones that were on the stack, and
        // `usize::MAX - index` for the push at `index` of the run.
        let mut deepest = 0;
        let mut pushed = 0;
        for step in run {
            match *step {
                Step::Materialize(_) => pushed += 1,
                Step::Stack(StackOp::Swap(n)) => {
                    deepest = deepest.max((usize::from(n) + 1).saturating_sub(pushed));
                }
                _ => unreachable!("a run holds only pushes and swaps"),
            }
        }
        let mut words = (0..deepest).collect::<SmallVec<[usize; 24]>>();
        for (index, step) in run.iter().enumerate() {
            match *step {
                Step::Materialize(_) => words.push(usize::MAX - index),
                Step::Stack(StackOp::Swap(n)) => {
                    let top = words.len() - 1;
                    words.swap(top, top - usize::from(n));
                }
                _ => unreachable!("a run holds only pushes and swaps"),
            }
        }
        if !words.iter().filter(|&&word| word < deepest).copied().eq(0..deepest) {
            return false;
        }
        let straddles = start < split && split < start + run.len();
        // A word that ends above `below` of the old words is pushed when the stack holds `below`
        // of them above `floor`.
        let floor = self.height - deepest as isize;
        let mut inserts = SmallVec::<[Insert; 4]>::new();
        let mut below = 0;
        for &word in &words {
            if word < deepest {
                below += 1;
            } else {
                let index = usize::MAX - word;
                let origin = if straddles && start + index >= split {
                    Origin::Terminator
                } else {
                    self.origin
                };
                inserts.push(Insert { index, height: floor + below, point: len, origin });
            }
        }
        for insert in &mut inserts {
            if insert.height == self.height {
                continue;
            }
            let Some(point) = self.point(insert.height, len) else { return false };
            insert.point = point;
        }
        let first = inserts.iter().map(|insert| insert.point).min().expect("a run pushes");

        // Operations that reach below a pushed word name a deeper word.
        let mut renumbered = SmallVec::<[(usize, StackOp); 8]>::new();
        let mut old_cost = Cost::ZERO;
        let mut new_cost = Cost::ZERO;
        let cost = |op| hoister.target.stack_op(op);
        for step in run {
            if let Step::Stack(op) = *step {
                let Some(price) = cost(op) else { return false };
                old_cost += price;
            }
        }
        for (k, placed) in self.steps.iter().enumerate().skip(first) {
            let Step::Stack(old) = placed.step else { continue };
            let h = placed.height;
            // Pushed words above the word that `index` of the old stack names.
            let above = |index: isize| {
                inserts.iter().filter(|ins| ins.point <= k && ins.height > index).count()
            };
            let deeper = |n: u8, index: isize| u8::try_from(usize::from(n) + above(index)).ok();
            let renumber = match old {
                StackOp::Dup(n) => deeper(n, h - isize::from(n)).map(StackOp::Dup),
                StackOp::Swap(n) => deeper(n, h - 1 - isize::from(n)).map(StackOp::Swap),
                StackOp::Exchange(x, y) => {
                    deeper(x, h - 1 - isize::from(x)).zip(deeper(y, h - 1 - isize::from(y))).map(
                        |(x, y)| StackOp::Exchange(x - above(h - 1) as u8, y - above(h - 1) as u8),
                    )
                }
                StackOp::Pop => Some(StackOp::Pop),
            };
            // A depth past `u8` has no operation.
            let Some(new) = renumber else { return false };
            if new != old {
                let (Some(old_price), Some(new_price)) = (cost(old), cost(new)) else {
                    return false;
                };
                old_cost += old_price;
                new_cost += new_price;
                renumbered.push((k, new));
            }
        }
        if !hoister.target.cmp(new_cost, old_cost).is_lt() {
            return false;
        }

        // Each push keeps the origin it had. The emitter takes the origin of the code before the
        // first instruction from the first `Begin(Some(_))`, which another one in front of it
        // would replace, so a push that lands there is marked unknown instead.
        for insert in &mut inserts {
            let before_first = self.terminator.is_none_or(|t| insert.point < t)
                && self.first_inst.is_none_or(|f| insert.point <= f);
            if before_first
                && matches!(insert.origin, Origin::Inst(Some(_)))
                && insert.origin != self.landing(insert.point)
            {
                insert.origin = Origin::Inst(None);
            }
        }

        // ...; DUPn; ...; PUSH c; SWAP1; consumer -> PUSH c; ...; DUP(n+1); ...; consumer
        for &(k, new) in &renumbered {
            self.steps[k].step = Step::Stack(new);
        }
        // The pushes are in order from the bottom, so a stable sort keeps the deepest first.
        inserts.sort_by_key(|insert| Reverse(insert.point));
        for group in inserts.chunk_by(|a, b| a.point == b.point) {
            self.insert_group(group, run);
        }
        self.height += pushed as isize;
        if straddles {
            self.terminator = Some(self.steps.len());
            self.origin = Origin::Terminator;
        }
        true
    }

    /// The latest kept step, within [`MAX_HOIST_STEPS`], before which the stack is `height` high
    /// and after which no step pops below `height`, swaps the word on top of it, or is a barrier.
    fn point(&self, height: isize, len: usize) -> Option<usize> {
        for k in (len.saturating_sub(MAX_HOIST_STEPS)..len).rev() {
            let placed = &self.steps[k];
            let (pops, _, barrier) = self.hoister.effect(&placed.step);
            // A step that pops below the word, or swaps the word that is on top of it.
            if barrier
                || placed.height - (pops as isize) < height
                || placed.step.is_swap() && placed.height <= height
            {
                return None;
            }
            if placed.height == height {
                return Some(k);
            }
        }
        None
    }

    /// The origin of the code a push before kept step `point` takes.
    fn landing(&self, point: usize) -> Origin {
        self.steps.get(point).map_or(self.origin, |placed| placed.origin)
    }

    /// Inserts the pushes of `inserts` before kept step `point`, the deepest first, each under the
    /// origin that it had.
    ///
    /// ```text
    /// BEGIN landing; PUSH a; ...  ->  BEGIN src; PUSH x; BEGIN landing; PUSH a; ...
    /// ```
    fn insert_group(&mut self, inserts: &[Insert], run: &[Step]) {
        let point = inserts[0].point;
        let landing = self.landing(point);
        let mut height = self.steps.get(point).map_or(self.height, |placed| placed.height);
        let mut origin = landing;
        let mut group = Vec::new();
        let mut pushes = 0;
        for insert in inserts {
            if insert.origin != origin {
                group.push(Placed { step: insert.origin.step(), height, extra: 0, origin });
                origin = insert.origin;
            }
            group.push(Placed { step: run[insert.index].clone(), height, extra: 0, origin });
            height += 1;
            pushes += 1;
        }
        if origin != landing {
            group.push(Placed { step: landing.step(), height, extra: 0, origin });
        }
        let added = group.len();
        for placed in &mut self.steps[point..] {
            placed.height += pushes;
            placed.extra += pushes as usize;
            self.extra = self.extra.max(placed.extra);
        }
        self.steps.splice(point..point, group);
        if let Some(terminator) = &mut self.terminator
            && point < *terminator
        {
            *terminator += added;
        }
        if let Some(first) = &mut self.first_inst
            && point <= *first
        {
            *first += added;
        }
    }
}

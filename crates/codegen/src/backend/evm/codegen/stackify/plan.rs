//! Per-function planning: block layouts, operand preparation, shuffles, and spills.

use super::{
    Analysis, BlockPlan, Edge, Exit, Fail, FunctionPlan, Layout, ModuleInfo, Operands, Slot, Step,
    Victim, Want, argument_values, icall, materialized_at_use,
    shuffle::{self, Move},
};
use crate::{
    backend::evm::{
        codegen::{
            StackOp,
            select::{
                InstKindOpcode, OpcodeLowering, opcode_lowering, rematerializable_nullary_value,
            },
            values::{gas_minus, late_gas_reads},
        },
        ir::INDEXED_JUMP_STACK_GROWTH,
        op,
    },
    mir::{
        ArgIdx, BlockId, EffectKind, Function, FunctionId, InstId, InstKind, OpTraits, PtrLocation,
        Terminator, Value, ValueId,
        analysis::{CfgInfo, Liveness},
        memory::EvmMemoryLayout,
    },
    target::{Cost, StackCosts, Target},
};
use alloy_primitives::U256;
use smallvec::{SmallVec, smallvec};
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet},
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};

/// The modeled stack of a block being planned and the steps that produced it.
#[derive(Clone, Default)]
struct Sim {
    stack: Layout,
    steps: Vec<Step>,
    cost: Cost,
    /// The highest physical stack seen, counting the words below `stack`.
    peak: usize,
    /// Physical words below `stack` that a floating block's layout leaves out.
    below: usize,
}

impl Sim {
    fn new(stack: Layout) -> Self {
        Self::with_below(stack, 0)
    }

    fn with_below(stack: Layout, below: usize) -> Self {
        let peak = below + stack.len();
        Self { stack, steps: Vec::new(), cost: Cost::ZERO, peak, below }
    }

    fn height(&self) -> usize {
        self.stack.len()
    }

    /// Depth from the top of the shallowest copy of `slot`.
    fn depth_of(&self, slot: Slot) -> Option<usize> {
        self.stack.iter().rev().position(|&s| s == slot)
    }

    /// Number of dead words.
    fn junk(&self) -> u32 {
        self.stack.iter().filter(|&&slot| slot == Slot::Junk).count() as u32
    }

    fn observe(&mut self, extra: usize) {
        self.peak = self.peak.max(self.below + self.stack.len() + extra);
    }

    /// Copies the state without the steps recorded so far.
    fn fork(&self) -> Self {
        let mut stack = Layout::with_capacity(self.stack.len() + FORK_HEADROOM);
        stack.extend_from_slice(&self.stack);
        Self { stack, steps: Vec::new(), cost: self.cost, peak: self.peak, below: self.below }
    }

    /// Makes this a fork of `other`, reusing its buffers.
    fn refork(&mut self, other: &Self) {
        self.stack.clear();
        self.stack.reserve(other.stack.len() + FORK_HEADROOM);
        self.stack.extend_from_slice(&other.stack);
        self.steps.clear();
        self.cost = other.cost;
        self.peak = other.peak;
        self.below = other.below;
    }

    /// Continues with a fork's state, appending its steps. The fork is left empty to reuse.
    fn join(&mut self, fork: &mut Self) {
        std::mem::swap(&mut self.stack, &mut fork.stack);
        self.steps.append(&mut fork.steps);
        self.cost = fork.cost;
        self.peak = fork.peak;
    }
}

/// An instruction of the block being planned with its operands.
struct PlannedInst {
    inst: InstId,
    operands: Operands,
}

/// The cost of `POP` and of each `DUP` and `SWAP` up to depth 16.
struct StackOpCosts {
    dup: [Option<Cost>; 17],
    swap: [Option<Cost>; 17],
    pop: Option<Cost>,
}

impl StackOpCosts {
    fn new(target: &Target) -> Self {
        Self {
            dup: std::array::from_fn(|n| target.stack_op(StackOp::Dup(n as u8))),
            swap: std::array::from_fn(|n| target.stack_op(StackOp::Swap(n as u8))),
            pop: target.stack_op(StackOp::Pop),
        }
    }

    fn get(&self, op: StackOp) -> Option<Cost> {
        match op {
            StackOp::Dup(n) => self.dup.get(usize::from(n)).copied().flatten(),
            StackOp::Swap(n) => self.swap.get(usize::from(n)).copied().flatten(),
            StackOp::Pop => self.pop,
            StackOp::Exchange(..) => None,
        }
    }
}

/// A consumer whose deeper operands are copied before the chain computing its top operand.
struct Preplacement {
    consumer: usize,
    pre: Operands,
    /// A plan that copies other words at this offset into the window; it wins ties.
    rival: Option<(usize, Operands)>,
}

/// Longest instruction chain before a consumer considered for copying its deeper operands first.
const MAX_PREPLACEMENT_WINDOW: usize = 16;

/// Most instructions a result placement trial plans ahead.
const MAX_PLACEMENT_WINDOW: usize = 4;

/// Most dying words a result placement tries to swap with.
const MAX_PLACEMENT_CANDIDATES: usize = 2;

/// Spare words a fork of the modeled stack reserves for the words planned on it.
const FORK_HEADROOM: usize = 8;

/// Most opcodes recomputing one value live across a write that may reach the spill area.
const MAX_RECOMPUTED_OPS: u32 = 4;

/// Most words a switch dispatch holds above its selector: the jump-table index computed from a
/// copy of the selector, and the words the indexed jump holds above that index.
const SWITCH_DISPATCH_WORDS: usize = 1 + INDEXED_JUMP_STACK_GROWTH;

/// A planned function together with the layouts its loop latches would choose.
pub(super) struct Planned {
    pub(super) plan: FunctionPlan,
    /// For each loop header, the layout its latch leaves before shuffling.
    pub(super) natural: FxHashMap<BlockId, Layout>,
    /// For each loop header, its words ordered by last use in the loop body.
    pub(super) first_use: FxHashMap<BlockId, Layout>,
}

pub(super) struct Planner<'a> {
    info: &'a ModuleInfo,
    func_id: FunctionId,
    func: &'a Function,
    liveness: &'a Liveness,
    target: &'a Target,
    spilled: &'a DenseBitSet<ValueId>,
    hints: &'a FxHashMap<BlockId, Layout>,
    /// Whether the function is entered with a return address.
    has_ret: bool,
    /// Whether the return address moves to its memory slot on entry and is reloaded to return.
    ret_spilled: bool,
    reach: usize,
    cfg: &'a CfgInfo,
    /// Values pushed fresh at each use instead of kept on the stack.
    remat: DenseBitSet<ValueId>,
    /// The values of `remat` and `spilled`; `is_fresh` leaves out those `held` in the block.
    fresh_values: DenseBitSet<ValueId>,
    /// The cost of pushing each value of `remat`.
    remat_costs: IndexVec<ValueId, Cost>,
    /// The instructions that overwrite this function's fixed frame, with the values live across
    /// each.
    saved: &'a FxHashMap<InstId, DenseBitSet<ValueId>>,
    /// Whether a failure lists every spillable value beyond reach, for spilling in batches.
    batch_spills: bool,
    /// Instructions with no code of their own.
    skipped: DenseBitSet<InstId>,
    /// Extra results of multi-word calls, adopted from the returned stack words.
    projections: FxHashMap<InstId, SmallVec<[Option<ValueId>; 4]>>,
    /// Multi-word calls that store their extra results to the multi-return buffer.
    publish: DenseBitSet<InstId>,
    /// Branch conditions with their single-use zero tests peeled off, and whether that
    /// inverts the branch.
    conditions: FxHashMap<BlockId, (ValueId, bool)>,
    /// External arguments loaded at the start of a block and kept on the stack.
    arg_defs: FxHashMap<BlockId, SmallVec<[ValueId; 2]>>,
    /// The constants retained by this trial, with the blocks that first materialize them.
    constant_defs: Vec<(BlockId, ValueId)>,
    /// The blocks in which each retained constant is kept on the stack. The constant is pushed
    /// fresh at its uses elsewhere.
    held: FxHashMap<ValueId, DenseBitSet<BlockId>>,
    /// The block being planned.
    current: Cell<BlockId>,
    /// Number of operand positions reading each value.
    use_counts: IndexVec<ValueId, u32>,
    /// Blocks that return the results of their last instruction, an internal call, which
    /// becomes a tail call.
    tail_calls: FxHashMap<BlockId, InstId>,
    args: IndexVec<ArgIdx, Option<ValueId>>,
    layouts: IndexVec<BlockId, Option<Layout>>,
    blocks: IndexVec<BlockId, Option<BlockPlan>>,
    natural: FxHashMap<BlockId, Layout>,
    first_use: FxHashMap<BlockId, Layout>,
    gas_mode: bool,
    /// Relative executions of each block, which scale the gas of its planned code.
    executions: IndexVec<BlockId, u32>,
    backedges: FxHashSet<(BlockId, BlockId)>,
    /// Blocks that never return through the caller's address and leave no loop: their layouts
    /// constrain only the words at the top of the stack.
    floating: DenseBitSet<BlockId>,
    /// Physical words below a floating block's layout on its deepest entry, which stack heights
    /// planned from that layout leave out.
    floating_below: IndexVec<BlockId, usize>,
    /// Number of forward (non-back) edges into each block.
    forward_preds: IndexVec<BlockId, u32>,
    /// Edges into joins whose layout waits until every forward predecessor is planned.
    pending: FxHashMap<BlockId, Vec<Pending>>,
    /// The latest plain plan of a placement decision.
    plain_trial: Option<PlainTrial>,
    /// The cost of each stack operation the planner emits most.
    stack_costs: StackOpCosts,
    /// The fork `place_result` tries each swap on.
    place_sim: Sim,
    /// The candidate and the cheapest arrangement `prepare_any` keeps, reused across calls.
    scratch: RefCell<[Sim; 2]>,
    /// The fork a `trial` plans on.
    trial_sim: RefCell<Sim>,
    peak: usize,
    calls: Vec<(FunctionId, usize)>,
    cost: Cost,
    non_loop_gas: u32,
}

impl<'a> Planner<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        func: &'a Function,
        info: &'a ModuleInfo,
        func_id: FunctionId,
        analysis: &'a Analysis,
        target: &'a Target,
        spilled: &'a DenseBitSet<ValueId>,
        ret_spilled: bool,
        batch_spills: bool,
        hints: &'a FxHashMap<BlockId, Layout>,
    ) -> Result<Self, &'static str> {
        let Analysis { liveness, cfg, loops, pinned, recomputable, saved, .. } = analysis;
        let internal = info.internal.contains(func_id);
        let has_ret = info.returning.contains(func_id);
        let args = if internal { argument_values(func)? } else { IndexVec::new() };

        let mut use_counts = index_vec![0u32; func.num_values()];
        for block in &func.blocks {
            for &inst in &block.instructions {
                func.inst(inst).kind.visit_operands(|operand| use_counts[operand] += 1);
            }
            if let Some(term) = &block.terminator {
                term.visit_operands(|operand| use_counts[operand] += 1);
            }
        }
        // External arguments are calldata words. One read more than once loads at the nearest
        // block outside every loop that dominates its uses and then stays on the stack.
        let gas_mode = target.optimization().is_gas();
        let mut use_blocks = FxHashMap::<ValueId, BlockId>::default();
        if !internal {
            // In gas mode a read inside a loop also loads once, outside the loop.
            let mut note = |value: ValueId, block: BlockId| {
                if matches!(func.value(value), Value::Arg(_))
                    && (use_counts[value] >= 2 || (gas_mode && cfg.cyclic_blocks().contains(block)))
                {
                    use_blocks
                        .entry(value)
                        .and_modify(|other| *other = common_dominator(cfg, *other, block))
                        .or_insert(block);
                }
            };
            for (block_id, block) in func.blocks.iter_enumerated() {
                for &inst in &block.instructions {
                    match &func.inst(inst).kind {
                        InstKind::Phi(incoming) => {
                            for &(pred, value) in incoming {
                                note(value, pred);
                            }
                        }
                        kind => kind.visit_operands(|value| note(value, block_id)),
                    }
                }
                if let Some(term) = &block.terminator {
                    term.visit_operands(|value| note(value, block_id));
                }
            }
        }
        let mut arg_defs = FxHashMap::<BlockId, SmallVec<[ValueId; 2]>>::default();
        // A spilled argument is read from calldata again at each use instead.
        use_blocks.retain(|&value, _| !spilled.contains(value));
        for (value, mut block) in use_blocks {
            while cfg.cyclic_blocks().contains(block)
                && let Some(idom) = cfg.dominators().idom(block)
            {
                block = idom;
            }
            arg_defs.entry(block).or_default().push(value);
        }
        for defs in arg_defs.values_mut() {
            defs.sort_unstable();
        }
        let mut remat = DenseBitSet::new_empty(func.num_values());
        let mut skipped = DenseBitSet::new_empty(func.num_insts());
        for value in func.live_values() {
            if matches!(func.value(value), Value::Immediate(_) | Value::Undef(_))
                || (!internal
                    && matches!(func.value(value), Value::Arg(_))
                    && !arg_defs.values().any(|defs| defs.contains(&value)))
            {
                remat.insert(value);
            }
        }
        for inst in func.instructions() {
            if let Some(result) = func.inst_result_value(inst)
                && materialized_at_use(func, internal, result)
            {
                remat.insert(result);
                skipped.insert(inst);
            }
        }
        // A gas read used only as a call's gas operand is read immediately before the call.
        for read in late_gas_reads(func) {
            remat.insert(read.gas);
            for inst in read.insts {
                skipped.insert(inst);
            }
        }

        let mut projections = FxHashMap::default();
        for (block_id, block) in func.blocks.iter_enumerated() {
            for (idx, &inst) in block.instructions.iter().enumerate() {
                let Some((callee, _)) = icall(&func.inst(inst).kind) else { continue };
                let arity = info.returns[callee];
                if arity <= 1 {
                    continue;
                }
                let Some(Projection { elided, extras, tracked }) =
                    call_projection(func, block_id, idx, arity)
                else {
                    return Err("multi-word call result through memory");
                };
                // The protocol's pointer and addresses have no readers beyond the elided
                // instructions.
                let protocol_reads = elided
                    .iter()
                    .map(|&inst| {
                        let operands = func.inst(inst).kind.operands();
                        operands.iter().filter(|op| tracked.contains(op)).count() as u32
                    })
                    .sum::<u32>();
                let escapes =
                    tracked.iter().map(|&value| use_counts[value]).sum::<u32>() != protocol_reads;
                if escapes {
                    return Err("multi-word call result through memory");
                }
                for &inst in &elided {
                    skipped.insert(inst);
                }
                projections.insert(inst, extras);
            }
        }
        // A read of the multi-return buffer that no call binds may observe the results of a
        // call whose protocol is not adjacent to it, so such calls publish their results.
        let mut publish = DenseBitSet::new_empty(func.num_insts());
        if func.instructions().any(|inst| {
            !skipped.contains(inst)
                && matches!(func.inst(inst).kind, InstKind::Load(addr, PtrLocation::Memory)
                    if func.value_u64(addr) == Some(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT))
        }) {
            for (&inst, extras) in &projections {
                if extras.iter().all(Option::is_none) {
                    let (callee, _) = icall(&func.inst(inst).kind).unwrap();
                    if info.components[callee].is_some() {
                        return Err("recursive multi-word call through memory");
                    }
                    publish.insert(inst);
                }
            }
        }
        // icall f(args); return results -> jump f with the inherited return address
        let mut tail_calls = FxHashMap::default();
        if has_ret {
            for (block_id, block) in func.blocks.iter_enumerated() {
                let Some(Terminator::Return { values }) = &block.terminator else { continue };
                let Some(&last) =
                    block.instructions.iter().rev().find(|&&inst| !skipped.contains(inst))
                else {
                    continue;
                };
                let Some((callee, _)) = icall(&func.inst(last).kind) else { continue };
                if !info.internal.contains(callee)
                    || info.returns[callee] != values.len()
                    || publish.contains(last)
                {
                    continue;
                }
                let mut results: SmallVec<[Option<ValueId>; 4]> =
                    smallvec![func.inst_result_value(last)];
                if let Some(extras) = projections.get(&last) {
                    results.extend(extras.iter().copied());
                }
                if values.iter().enumerate().all(|(i, &value)| results.get(i) == Some(&Some(value)))
                {
                    tail_calls.insert(block_id, last);
                }
            }
        }
        // jumpi (eq x, 0) -> jumpi x with exchanged targets; jumpi (ne x, 0) -> jumpi x
        let mut conditions = FxHashMap::default();
        for (block_id, block) in func.blocks.iter_enumerated() {
            let Some(Terminator::Branch { condition, .. }) = &block.terminator else { continue };
            let mut condition = *condition;
            let mut inverted = false;
            while use_counts[condition] == 1
                && !remat.contains(condition)
                && let Value::Inst(def) = *func.value(condition)
                && block.instructions.contains(&def)
                && let kind @ (InstKind::Eq(..) | InstKind::Ne(..)) = &func.inst(def).kind
                && let Some(input) =
                    Target::zero_test_input(&kind.op(), |value| func.value_u256(value))
            {
                inverted ^= matches!(kind, InstKind::Eq(..));
                skipped.insert(def);
                condition = input;
            }
            if condition != block.terminator.as_ref().unwrap().operands()[0] {
                conditions.insert(block_id, (condition, inverted));
            }
        }

        let mut executions = index_vec![1u32; func.blocks.len()];
        let mut backedges = FxHashSet::default();
        let mut first_use = FxHashMap::default();
        for lp in loops.all_loops() {
            for block in lp.blocks.iter() {
                executions[block] = Target::nested_block_weight(executions[block]);
            }
            for &latch in &lp.back_edges {
                backedges.insert((latch, lp.header));
            }
            let header = lp.header;
            let mut words: Vec<ValueId> = liveness.live_in(header).iter().collect();
            words.extend(func.block_phi_results(header).iter());
            words.retain(|&value| !remat.contains(value) && !spilled.contains(value));
            // Order the header's words by their last read in the loop body.
            let body = cfg
                .rpo()
                .iter()
                .copied()
                .filter(|&block| block != header && lp.blocks.contains(block));
            let layout = ordered_layout(func, body, words, has_ret && !ret_spilled, Read::Last);
            first_use.insert(header, layout);
        }

        let mut forward_preds = index_vec![0u32; func.blocks.len()];
        for (block, data) in func.blocks.iter_enumerated() {
            for &pred in &data.predecessors {
                if cfg.is_reachable(pred) && !cfg.dominators().dominates(block, pred) {
                    forward_preds[block] += 1;
                }
            }
        }
        // A pinned value chosen to leave the stack is recomputed at each use.
        for value in spilled.iter() {
            if pinned.contains(value)
                && recomputable.contains_key(&value)
                && let Value::Inst(def) = *func.value(value)
            {
                remat.insert(value);
                skipped.insert(def);
            }
        }

        let mut floating = DenseBitSet::new_empty(func.blocks.len());
        for &block in cfg.rpo().iter().rev() {
            let Some(term) = &func.blocks[block].terminator else { continue };
            let returns = match term {
                Terminator::Return { .. } => has_ret,
                Terminator::TailCall { function, .. } => {
                    has_ret && info.returning.contains(*function)
                }
                _ => false,
            };
            if !returns
                && !cfg.cyclic_blocks().contains(block)
                && term.successors().iter().all(|&succ| floating.contains(succ))
            {
                floating.insert(block);
            }
        }

        Ok(Self {
            info,
            func_id,
            func,
            liveness,
            target,
            spilled,
            hints,
            has_ret,
            ret_spilled: has_ret && ret_spilled,
            reach: target.evm_version().reachable_stack_depth(),
            cfg,
            remat_costs: {
                let mut costs = index_vec![Cost::ZERO; func.num_values()];
                for value in remat.iter() {
                    costs[value] = match recomputable.get(&value) {
                        Some(&(cost, _)) => cost,
                        None => materialize_cost(func, target, value),
                    };
                }
                costs
            },
            fresh_values: {
                let mut fresh = remat.clone();
                fresh.union(spilled);
                fresh
            },
            remat,
            saved,
            batch_spills,
            skipped,
            projections,
            publish,
            conditions,
            arg_defs,
            constant_defs: Vec::new(),
            held: FxHashMap::default(),
            current: Cell::new(BlockId::ENTRY),
            use_counts,
            tail_calls,
            args,
            layouts: index_vec![None; func.blocks.len()],
            blocks: index_vec![None; func.blocks.len()],
            natural: FxHashMap::default(),
            first_use,
            gas_mode,
            executions,
            backedges,
            floating,
            floating_below: index_vec![0; func.blocks.len()],
            forward_preds,
            pending: FxHashMap::default(),
            plain_trial: None,
            stack_costs: StackOpCosts::new(target),
            place_sim: Sim::default(),
            scratch: RefCell::default(),
            trial_sim: RefCell::default(),
            peak: 0,
            calls: Vec::new(),
            cost: Cost::ZERO,
            non_loop_gas: 0,
        })
    }

    pub(super) fn plan(mut self) -> Result<Planned, Fail> {
        let cfg = self.cfg;
        for &block in cfg.rpo() {
            self.current.set(block);
            self.resolve_join(block)?;
            let layout = match self.layouts[block].clone() {
                Some(layout) => layout,
                None if block == BlockId::ENTRY => self.entry_layout(),
                None => return Err(Fail::Unsupported("block without a planned predecessor")),
            };
            let mut sim = Sim::with_below(layout, self.floating_below[block]);
            if block == BlockId::ENTRY {
                if self.ret_spilled {
                    // [args, return] -> [args]
                    self.spill_ret_top(&mut sim);
                }
                self.spill_entry_arguments(&mut sim)?;
            }
            self.pop_top_junk(&mut sim)?;
            for value in self.arg_defs.get(&block).into_iter().flatten().copied() {
                // push 4 + 32 * index; calldataload
                sim.cost += self.materialize_cost(value);
                sim.steps.push(Step::Materialize(value));
                sim.stack.push(Slot::Value(value));
                sim.observe(1);
            }
            self.plan_body(&mut sim, block)?;
            let terminator_start = sim.steps.len();
            let exit = self.plan_terminator(&mut sim, block)?;
            self.charge(&sim, block);
            self.calls.extend(sim.steps.iter().filter_map(|step| match *step {
                Step::Call(callee, base) => Some((callee, base)),
                _ => None,
            }));
            self.blocks[block] = Some(BlockPlan { steps: sim.steps, terminator_start, exit });
        }
        let plan = FunctionPlan {
            blocks: self.blocks,
            spilled: self.spilled.clone(),
            ret_spilled: self.ret_spilled,
            peak: self.peak,
            calls: self.calls,
            cost: self.cost,
            non_loop_gas: self.non_loop_gas,
            rebuilt: None,
        };
        Ok(Planned { plan, natural: self.natural, first_use: self.first_use })
    }

    /// Returns the repeated immediates worth keeping on the stack, the one that saves the most
    /// first, each with the block that first materializes it. A constant used in separate
    /// branches comes once for all its uses, from the nearest block outside loops that dominates
    /// them, and once for each branch of that block with several uses, so a branch that rejects
    /// the first can still keep the constant.
    ///
    /// This is a profitability ranking, not a stack limit: the plan cannot see the later EVM IR
    /// pipeline, and keeping many constants traded small size savings for large gas losses, such
    /// as P256 verification in size mode. The caller keeps a candidate only when a replan
    /// confirms it.
    pub(super) fn constants_to_retain(&self) -> Vec<(BlockId, ValueId)> {
        // Leave cheap literals to local stack scheduling and EVM IR reuse.
        let mut wide = DenseBitSet::new_empty(self.func.num_values());
        for value in self.remat.iter() {
            if matches!(self.func.value(value), Value::Immediate(_))
                && self.use_counts[value] >= 2
                && self.remat_costs[value].bytes
                    > (self.target.dup() + self.dead_word_removal()).bytes
            {
                wide.insert(value);
            }
        }
        if wide.is_empty() {
            return Vec::new();
        }
        let mut use_blocks = FxHashMap::<ValueId, SmallVec<[BlockId; 4]>>::default();
        let mut note = |value: ValueId, block| {
            if wide.contains(value) {
                use_blocks.entry(value).or_default().push(block);
            }
        };
        for (block, data) in self.func.blocks.iter_enumerated() {
            for &inst in &data.instructions {
                match &self.func.inst(inst).kind {
                    InstKind::Phi(incoming) => {
                        for &(pred, operand) in incoming {
                            note(operand, pred);
                        }
                    }
                    kind => kind.visit_operands(|operand| note(operand, block)),
                }
            }
            if let Some(term) = &data.terminator {
                term.visit_operands(|operand| note(operand, block));
            }
        }
        let mut candidates = Vec::new();
        for value in wide.iter() {
            let Some(blocks) = use_blocks.get(&value) else { continue };
            let Some(whole) = self.definition_block(blocks) else { continue };
            candidates.extend(self.retention_saving(value, whole, blocks));
            let dominators = self.cfg.dominators();
            let mut branches = BTreeMap::<BlockId, SmallVec<[BlockId; 4]>>::default();
            for &block in blocks {
                // The child of the dominator of all uses that dominates this use.
                if let Some(&branch) = dominators
                    .children(whole)
                    .iter()
                    .find(|&&child| dominators.dominates(child, block))
                {
                    branches.entry(branch).or_default().push(block);
                }
            }
            if branches.len() < 2 && !blocks.contains(&whole) {
                continue;
            }
            for uses in branches.into_values() {
                // A branch under a loop header climbs back to the dominator of all uses.
                if let Some(definition) = self.definition_block(&uses)
                    && definition != whole
                {
                    candidates.extend(self.retention_saving(value, definition, &uses));
                }
            }
        }
        candidates.sort_by(|(_, a), (_, b)| self.target.cmp_lifetime(*a, *b));
        candidates.into_iter().rev().map(|(candidate, _)| candidate).collect()
    }

    /// What keeping `value` on the stack from `definition` saves over pushing it at each of its
    /// uses in `blocks`, when that is worth a replan.
    fn retention_saving(
        &self,
        value: ValueId,
        definition: BlockId,
        blocks: &[BlockId],
    ) -> Option<((BlockId, ValueId), Cost)> {
        if blocks.len() < 2 {
            return None;
        }
        let price = |cost: Cost, block| if self.gas_mode { self.weigh(cost, block) } else { cost };
        let push = self.remat_costs[value];
        let before = blocks.iter().fold(Cost::ZERO, |sum, &block| sum + price(push, block));
        let copies =
            blocks.iter().fold(Cost::ZERO, |sum, &block| sum + price(self.target.dup(), block));
        // The push at the definition replaces the copy of the first use.
        let after = price(push, definition)
            + copies.saturating_sub(self.target.dup())
            + price(self.dead_word_removal(), definition);
        self.target
            .cmp_lifetime(after, before)
            .is_lt()
            .then_some(((definition, value), before.saturating_sub(after)))
    }

    /// The nearest block outside loops that dominates every block of `uses`.
    fn definition_block(&self, uses: &[BlockId]) -> Option<BlockId> {
        let mut block = uses.iter().copied().reduce(|a, b| common_dominator(self.cfg, a, b))?;
        while self.cfg.cyclic_blocks().contains(block)
            && let Some(idom) = self.cfg.dominators().idom(block)
        {
            block = idom;
        }
        Some(block)
    }

    /// Keeps each constant on the stack from its defining block through the blocks that block
    /// dominates, where `liveness` is [`retention_liveness`] for the same definitions.
    pub(super) fn retain_constants(
        mut self,
        definitions: &[(BlockId, ValueId)],
        liveness: &'a Liveness,
    ) -> Self {
        self.held = held_blocks(self.func, self.cfg, definitions);
        self.liveness = liveness;
        self.constant_defs = definitions.to_vec();
        self
    }

    /// The instructions of a block that plan to code of their own, in MIR order.
    fn planned_insts(&self, block: BlockId) -> Vec<PlannedInst> {
        let func = self.func;
        let tail_call = self.tail_calls.get(&block).copied();
        func.blocks[block]
            .instructions
            .iter()
            .copied()
            .filter(|&inst| {
                !self.skipped.contains(inst)
                    && !matches!(func.inst(inst).kind, InstKind::Phi(_))
                    && Some(inst) != tail_call
            })
            .map(|inst| PlannedInst { inst, operands: func.inst(inst).kind.operands() })
            .collect()
    }

    /// Plans a block's instructions in their MIR order.
    fn plan_body(&mut self, sim: &mut Sim, block: BlockId) -> Result<(), Fail> {
        let func = self.func;
        let early = self.early_branch_phis(block);
        let mut constants = self
            .constant_defs
            .iter()
            .filter(|&&(def, _)| def == block)
            .map(|&(_, value)| value)
            .collect::<SmallVec<[_; 2]>>();
        let mut uses = self.block_uses(block);
        let planned = self.planned_insts(block);
        let optimize = self.target.optimizes_stack_plans();
        let windows =
            if optimize { self.preplacement_windows(&planned) } else { Default::default() };
        let mut placed = FxHashSet::default();
        for (position, step) in planned.iter().enumerate() {
            let inst = step.inst;
            constants.retain(|&mut value| {
                if !step.operands.contains(&value) {
                    return true;
                }
                sim.steps.push(Step::Begin(None));
                // push constant
                self.fresh(sim, value);
                false
            });
            if let Some((early_inst, then_block, else_block)) = early
                && early_inst == inst
            {
                self.branch_phi_prep(sim, block, then_block, else_block)?;
            }
            for Preplacement { consumer, pre, rival } in
                windows.get(&position).into_iter().flatten()
            {
                // Copy a consumer's deeper operands before the expression computing its top
                // operand starts, when planning that expression and the consumer gets cheaper.
                if placed.contains(consumer) {
                    continue;
                }
                let window = &planned[position..=*consumer];
                let plain = self
                    .plain_prefix(sim, block, &planned, position, window.len(), &uses)
                    .get(window.len() - 1)
                    .map(|step| step.price);
                // A copy cannot pay for itself once its plan costs as much as the plain one.
                let Some(price) = self.trial(sim, block, window, &mut uses, 0, pre, plain) else {
                    continue;
                };
                if plain.is_some_and(|plain| !self.target.cmp(price, plain).is_lt()) {
                    continue;
                }
                // The rival copies the words later, and wins ties.
                if let Some((offset, rival_pre)) = rival
                    && let Some(rival_price) =
                        self.trial(sim, block, window, &mut uses, *offset, rival_pre, plain)
                    && self.target.cmp(rival_price, price).is_le()
                {
                    continue;
                }
                for &value in pre {
                    self.copy(sim, value)?;
                }
                placed.insert(*consumer);
            }
            self.plan_inst_inner(sim, block, step, &uses)?;
            uses.consume(&step.operands);
            if optimize
                && let Some(result) = func.inst_result_value(inst)
                && sim.stack.last() == Some(&Slot::Value(result))
            {
                self.place_result(sim, block, &planned, position + 1, &mut uses)?;
            }
        }
        for value in constants {
            sim.steps.push(Step::Begin(None));
            // push constant
            self.fresh(sim, value);
        }
        Ok(())
    }

    /// Swaps a result just left on top into the slot of a deeper word whose last read in the
    /// block comes within a few instructions, when planning up to that read gets cheaper with
    /// the dying word on top.
    fn place_result(
        &mut self,
        sim: &mut Sim,
        block: BlockId,
        planned: &[PlannedInst],
        next: usize,
        uses: &mut Uses,
    ) -> Result<(), Fail> {
        let func = self.func;
        let live_out = self.liveness.live_out(block);
        let rest = &planned[next..planned.len().min(next + MAX_PLACEMENT_WINDOW)];
        // A placement looks ahead no further than the next call.
        let calls = rest.iter().position(|step| icall(&func.inst(step.inst).kind).is_some());
        let rest = &rest[..calls.unwrap_or(rest.len())];
        let mut candidates = SmallVec::<[(usize, usize); MAX_PLACEMENT_CANDIDATES]>::new();
        for depth in 1..sim.stack.len().min(self.reach + 1) {
            if candidates.len() == MAX_PLACEMENT_CANDIDATES {
                break;
            }
            let Slot::Value(value) = sim.stack[sim.stack.len() - 1 - depth] else { continue };
            let mut remaining = uses.count(value);
            if remaining == 0 || live_out.contains(value) || self.is_fresh(value) {
                continue;
            }
            if let Some(end) = rest.iter().position(|step| {
                remaining -=
                    step.operands.iter().filter(|&&operand| operand == value).count() as u32;
                remaining == 0
            }) {
                candidates.push((depth, end));
            }
        }
        let Some(&(_, last)) = candidates.iter().max_by_key(|&&(_, end)| end) else {
            return Ok(());
        };
        let mut swapped = false;
        let plain = self
            .plain_prefix(sim, block, planned, next, last + 1, uses)
            .iter()
            .map(|step| {
                swapped |= step.swapped;
                (step.price, swapped)
            })
            .collect::<SmallVec<[_; MAX_PLACEMENT_WINDOW]>>();
        let mut best: Option<(Cost, usize)> = None;
        let mut swapped = std::mem::take(&mut self.place_sim);
        for (depth, end) in candidates {
            // Without a swap in the plain plan, an early one cannot pay for itself.
            let Some(&(plain, true)) = plain.get(end) else { continue };
            swapped.refork(sim);
            if self.stack_op(&mut swapped, StackOp::Swap(depth as u8)).is_err() {
                continue;
            }
            if let Some(cost) =
                self.trial(&swapped, block, &rest[..=end], uses, 0, &[], Some(plain))
                && self.target.cmp(cost, plain).is_lt()
                && best.is_none_or(|(best, _)| self.target.cmp(cost, best).is_lt())
            {
                best = Some((cost, depth));
            }
        }
        self.place_sim = swapped;
        if let Some((_, depth)) = best {
            self.stack_op(sim, StackOp::Swap(depth as u8))?;
        }
        Ok(())
    }

    /// The cost of eventually removing one dead word from below the top.
    fn dead_word_removal(&self) -> Cost {
        StackCosts::POP.plus(self.target.opcode(op::SWAP1))
    }

    /// Prices planning `window` after copying `pre` before its instruction at `offset`, or
    /// `None` when it cannot be planned or its cost reaches `bound`.
    #[allow(clippy::too_many_arguments)]
    fn trial(
        &self,
        base: &Sim,
        block: BlockId,
        window: &[PlannedInst],
        uses: &mut Uses,
        offset: usize,
        pre: &[ValueId],
        bound: Option<Cost>,
    ) -> Option<Cost> {
        let mut fork = self.trial_sim.borrow_mut();
        let sim = &mut *fork;
        sim.refork(base);
        // Plan on the caller's counts and give back what the window consumed.
        let mut planned = 0;
        for (index, step) in window.iter().enumerate() {
            if index == offset && pre.iter().any(|&value| self.copy(sim, value).is_err()) {
                break;
            }
            if self.plan_inst_inner(sim, block, step, uses).is_err()
                || bound.is_some_and(|bound| !self.target.cmp(sim.cost, bound).is_lt())
            {
                break;
            }
            uses.consume(&step.operands);
            planned += 1;
        }
        for step in &window[..planned] {
            uses.restore(&step.operands);
        }
        if planned < window.len() {
            return None;
        }
        // Dead words left behind are removed eventually.
        let leftovers = pre
            .iter()
            .filter(|&&value| sim.stack.contains(&Slot::Value(value)) && self.is_fresh(value))
            .count() as u32;
        Some(sim.cost.plus(self.dead_word_removal().times(sim.junk() + leftovers)))
    }

    /// Plans the `len` instructions from position `next` as `trial` does, without copies, and
    /// returns a step for each, stopping at the first one that cannot be planned. The plan
    /// continues the previous one when the planner reached a state that plan predicted.
    fn plain_prefix(
        &mut self,
        sim: &Sim,
        block: BlockId,
        planned: &[PlannedInst],
        next: usize,
        len: usize,
        uses: &Uses,
    ) -> &[PlainStep] {
        let reached = |trial: &PlainTrial| {
            trial.block == block
                && next
                    .checked_sub(trial.start)
                    .and_then(|index| trial.states.get(index))
                    .is_some_and(|(stack, cost)| *stack == sim.stack && *cost == sim.cost)
        };
        let mut trial = match self.plain_trial.take() {
            Some(mut trial) if reached(&trial) => {
                let skipped = next - trial.start;
                trial.states.drain(..skipped);
                trial.steps.drain(..skipped);
                trial.start = next;
                trial
            }
            _ => PlainTrial {
                block,
                start: next,
                states: vec![(sim.stack.clone(), sim.cost)],
                steps: Vec::new(),
                sim: sim.fork(),
                uses: uses.clone(),
                stopped: false,
            },
        };
        while trial.steps.len() < len && !trial.stopped {
            let step = &planned[trial.start + trial.steps.len()];
            trial.sim.steps.clear();
            if self.plan_inst_inner(&mut trial.sim, block, step, &trial.uses).is_err() {
                trial.stopped = true;
                break;
            }
            trial.uses.consume(&step.operands);
            trial.states.push((trial.sim.stack.clone(), trial.sim.cost));
            trial.steps.push(PlainStep {
                price: trial.sim.cost.plus(self.dead_word_removal().times(trial.sim.junk())),
                swapped: trial.sim.steps.iter().any(Step::is_swap),
            });
        }
        let trial = self.plain_trial.insert(trial);
        &trial.steps[..len.min(trial.steps.len())]
    }

    /// Finds, for instructions whose topmost computed operand comes from a chain of single-use
    /// instructions in this block, the start of that chain and the deeper operands that are
    /// pushed or copied anyway. Keyed by the chain's first position.
    fn preplacement_windows(&self, planned: &[PlannedInst]) -> FxHashMap<usize, Vec<Preplacement>> {
        let func = self.func;
        let mut def_position = FxHashMap::default();
        for (position, step) in planned.iter().enumerate() {
            if let Some(result) = func.inst_result_value(step.inst) {
                def_position.insert(result, position);
            }
            // A call also defines its extra results.
            for &extra in self.projections.get(&step.inst).into_iter().flatten().flatten() {
                def_position.insert(extra, position);
            }
        }
        let mut windows = FxHashMap::<usize, Vec<Preplacement>>::default();
        for (consumer, step) in planned.iter().enumerate() {
            let kind = &func.inst(step.inst).kind;
            if matches!(
                kind,
                InstKind::Select(..)
                    | InstKind::Eq(..)
                    | InstKind::Ne(..)
                    | InstKind::Zext(_)
                    | InstKind::PtrToInt(..)
                    | InstKind::IntToPtr(_)
            ) {
                continue;
            }
            let push_order: Operands = step.operands.iter().rev().copied().collect();
            let Some(top) = push_order.iter().rposition(|&value| !self.is_fresh(value)) else {
                continue;
            };
            let value = push_order[top];
            let pre: Operands = push_order[..top].iter().copied().collect();
            if pre.is_empty() || self.use_counts[value] != 1 {
                continue;
            }
            let Some(&def) = def_position.get(&value) else { continue };
            // The chain: single-use producers feeding the roots within this block, as the
            // position of its first instruction.
            let first = |roots: &[usize]| {
                let mut work = roots.to_vec();
                let mut start = work[0];
                while let Some(position) = work.pop() {
                    start = start.min(position);
                    for &operand in &planned[position].operands {
                        if self.use_counts[operand] == 1
                            && let Some(&producer) = def_position.get(&operand)
                            && producer < position
                        {
                            work.push(producer);
                        }
                    }
                }
                start
            };
            // Copied values must exist before the chain starts. A spilled value is stored where
            // its instruction runs, so its reload cannot come earlier.
            let available = |operand: &ValueId, start: usize| {
                self.is_remat(*operand)
                    || def_position.get(operand).is_none_or(|&position| position < start)
            };
            let start = first(&[def]);
            let copied = (consumer - start <= MAX_PREPLACEMENT_WINDOW
                && pre.iter().all(|operand| available(operand, start)))
            .then_some((start, pre.clone()));
            // A deeper operand that single-use instructions compute right before the chain
            // can run with it, after the pushes below it, instead of being copied. The plan
            // that copies it stays the rival.
            let computed: SmallVec<[usize; 2]> = pre
                .iter()
                .filter(|&&operand| !self.is_fresh(operand) && self.use_counts[operand] == 1)
                .filter_map(|operand| def_position.get(operand).copied())
                .collect();
            if !computed.is_empty() {
                let roots = [&[def][..], &computed].concat();
                let start = first(&roots);
                let rest: Operands = pre
                    .iter()
                    .copied()
                    .filter(|operand| {
                        def_position.get(operand).is_none_or(|at| !computed.contains(at))
                    })
                    .collect();
                // The computed words must sit above the copied ones, as they are pushed last.
                if !rest.is_empty()
                    && pre.starts_with(&rest)
                    && consumer - start <= MAX_PREPLACEMENT_WINDOW
                    && rest.iter().all(|operand| available(operand, start))
                {
                    let rival = copied.clone().map(|(at, pre)| (at - start, pre));
                    windows.entry(start).or_default().push(Preplacement {
                        consumer,
                        pre: rest,
                        rival,
                    });
                }
            }
            if let Some((start, pre)) = copied {
                windows.entry(start).or_default().push(Preplacement { consumer, pre, rival: None });
            }
        }
        windows
    }

    /// How often `block` runs relative to others, for gas-mode decisions that favor hot paths.
    fn weight(&self, block: BlockId) -> u32 {
        if self.gas_mode { self.executions[block] } else { 1 }
    }

    /// Scales the gas of code in `block` by how often the block runs; its bytes count once.
    fn weigh(&self, cost: Cost, block: BlockId) -> Cost {
        Cost::new(cost.gas.saturating_mul(self.executions[block]), cost.bytes)
            .with_steps(cost.steps.saturating_mul(self.executions[block]))
    }

    /// Adds the cost of code planned in `block` and raises the peak to its stack.
    fn charge(&mut self, sim: &Sim, block: BlockId) {
        self.cost += self.weigh(sim.cost, block);
        if !self.cfg.cyclic_blocks().contains(block) {
            self.non_loop_gas = self.non_loop_gas.saturating_add(self.target.runtime_gas(sim.cost));
        }
        self.peak = self.peak.max(sim.peak);
    }

    fn entry_layout(&self) -> Layout {
        let mut layout: Layout =
            self.args.iter().rev().map(|value| value.map_or(Slot::Junk, Slot::Value)).collect();
        if self.has_ret {
            layout.push(Slot::Ret);
        }
        layout
    }

    fn spill_entry_arguments(&self, sim: &mut Sim) -> Result<(), Fail> {
        for value in self.args.iter().flatten().copied() {
            if self.spilled.contains(value) {
                self.spill_in_place(sim, value)?;
            }
        }
        Ok(())
    }

    /// The words in this function's fixed frame that `inst` overwrites before they are read
    /// again: the spilled values live across it and a spilled return address.
    fn saved_across(&self, inst: InstId) -> SmallVec<[Victim; 4]> {
        let mut saved = SmallVec::new();
        let Some(across) = self.saved.get(&inst) else { return saved };
        if self.ret_spilled {
            saved.push(Victim::Ret);
        }
        saved.extend(
            self.spilled
                .iter()
                .filter(|&value| across.contains(value) && !self.is_remat(value))
                .map(Victim::Value),
        );
        saved
    }

    /// Pushes the words saved across an instruction, each marked so that operand preparation
    /// does not consume it.
    fn push_saved(&self, sim: &mut Sim, saved: &[Victim]) {
        for &victim in saved {
            match victim {
                Victim::Value(value) => {
                    self.fresh(sim, value);
                    *sim.stack.last_mut().unwrap() = victim.saved_slot();
                }
                Victim::Ret => self.fresh_slot(sim, Slot::Ret),
            }
        }
    }

    /// Stores the words saved across an instruction back into their slots: a saved word on top
    /// as it is, else the deepest one in reach, which leaves the next ones on top.
    fn restore_saved(&self, sim: &mut Sim, saved: &[Victim]) -> Result<(), Fail> {
        let mut pending = SmallVec::<[Victim; 4]>::from_slice(saved);
        while !pending.is_empty() {
            let (index, depth) = pending
                .iter()
                .map(|victim| sim.depth_of(victim.saved_slot()).expect("saved word"))
                .enumerate()
                .max_by_key(|&(_, depth)| match depth {
                    0 => usize::MAX,
                    depth if depth <= self.reach => depth,
                    // Beyond reach: the swap fails and names a word to spill.
                    _ => 0,
                })
                .expect("pending saved words");
            // swap saved; store
            self.swap_up(sim, depth)?;
            match pending.swap_remove(index) {
                Victim::Value(value) => {
                    *sim.stack.last_mut().unwrap() = Slot::Value(value);
                    self.spill_top(sim, value);
                }
                Victim::Ret => self.spill_ret_top(sim),
            }
        }
        Ok(())
    }

    /// Stores a spilled value from wherever it sits, leaving the rest of the stack in order.
    fn spill_in_place(&self, sim: &mut Sim, value: ValueId) -> Result<(), Fail> {
        let Some(depth) = sim.depth_of(Slot::Value(value)) else { return Ok(()) };
        self.swap_up(sim, depth)?;
        self.spill_top(sim, value);
        Ok(())
    }

    /// Swaps the word at `depth` to the top.
    #[inline(always)]
    fn swap_up(&self, sim: &mut Sim, depth: usize) -> Result<(), Fail> {
        if depth == 0 {
            return Ok(());
        }
        let Ok(depth) = u8::try_from(depth) else {
            return Err(self.deep(sim, sim.stack.len().checked_sub(depth + 1)));
        };
        self.stack_op(sim, StackOp::Swap(depth))
    }

    // ----------------------------------------------------------------------------------------
    // Primitive operations.
    // ----------------------------------------------------------------------------------------

    #[inline(always)]
    fn stack_op(&self, sim: &mut Sim, op: StackOp) -> Result<(), Fail> {
        let height = sim.stack.len();
        match op {
            StackOp::Dup(n) => {
                let n = n as usize;
                if n == 0 || n > self.reach || n > height {
                    return Err(self.deep(sim, height.checked_sub(n)));
                }
                let slot = sim.stack[height - n];
                sim.stack.push(slot);
            }
            StackOp::Swap(n) => {
                let n = n as usize;
                if n == 0 || n > self.reach || n >= height {
                    return Err(self.deep(sim, height.checked_sub(n + 1)));
                }
                sim.stack.swap(height - 1, height - 1 - n);
            }
            StackOp::Pop => {
                sim.stack.pop().expect("pop from an empty modeled stack");
            }
            StackOp::Exchange(n, m) => {
                let (n, m) = (n as usize, m as usize);
                if op.lowering(self.target.evm_version()).is_none() || m >= height {
                    return Err(self.deep(sim, height.checked_sub(m + 1)));
                }
                sim.stack.swap(height - 1 - n, height - 1 - m);
            }
        }
        sim.cost += self
            .stack_costs
            .get(op)
            .or_else(|| self.target.stack_op(op))
            .expect("stack operation within reach");
        sim.steps.push(Step::Stack(op));
        sim.observe(0);
        Ok(())
    }

    /// Returns a failure for a word beyond reach, naming a value to spill. Without one, the
    /// return address may move to memory.
    fn deep(&self, sim: &Sim, position: Option<usize>) -> Fail {
        let victim = self.stack_victim(sim, position).or_else(|| {
            (self.ret_on_stack() && sim.stack.contains(&Slot::Ret)).then_some(Victim::Ret)
        });
        Fail::Deep(victim, self.beyond_reach(sim, victim))
    }

    /// The spillable values beyond reach other than `victim`, deepest first, when spills come
    /// in batches.
    fn beyond_reach(&self, sim: &Sim, victim: Option<Victim>) -> SmallVec<[ValueId; 8]> {
        if !self.batch_spills {
            return SmallVec::new();
        }
        let end = sim.stack.len().saturating_sub(self.reach);
        sim.stack[..end]
            .iter()
            .filter_map(|&slot| match slot {
                Slot::Value(value)
                    if self.can_spill(value) && victim != Some(Victim::Value(value)) =>
                {
                    Some(value)
                }
                _ => None,
            })
            .collect()
    }

    /// A spillable value at or above `position`, else anywhere on the stack.
    fn stack_victim(&self, sim: &Sim, position: Option<usize>) -> Option<Victim> {
        let spillable = |slot: Slot| match slot {
            Slot::Value(value) => self.can_spill(value).then_some(Victim::Value(value)),
            _ => None,
        };
        // The unreachable word itself, else a word above it: spilling one brings it closer to
        // the top.
        if let Some(position) = position
            && let Some(victim) = sim
                .stack
                .get(position..)
                .and_then(|slots| slots.iter().copied().find_map(spillable))
        {
            return Some(victim);
        }
        sim.stack.iter().copied().find_map(spillable)
    }

    /// Whether the return address lives on the stack.
    fn ret_on_stack(&self) -> bool {
        self.has_ret && !self.ret_spilled
    }

    /// Whether a value is pushed fresh at each use in the block being planned.
    fn is_remat(&self, value: ValueId) -> bool {
        self.remat.contains(value)
            && !self.held.get(&value).is_some_and(|held| held.contains(self.current.get()))
    }

    fn can_spill(&self, value: ValueId) -> bool {
        !self.is_remat(value)
            && !self.spilled.contains(value)
            && matches!(self.func.value(value), Value::Inst(_) | Value::Arg(_))
    }

    /// Pushes a fresh copy of a value that is not kept on the stack.
    #[inline(always)]
    fn fresh(&self, sim: &mut Sim, value: ValueId) {
        if self.spilled.contains(value) && !self.is_remat(value) {
            sim.cost += StackCosts::DIRECT_LOAD;
            sim.steps.push(Step::Reload(value));
        } else {
            sim.cost += self.materialize_cost(value);
            sim.steps.push(Step::Materialize(value));
        }
        sim.stack.push(Slot::Value(value));
        sim.observe(1);
    }

    fn is_fresh(&self, value: ValueId) -> bool {
        self.fresh_values.contains(value)
            && (self.spilled.contains(value)
                || !self.held.get(&value).is_some_and(|held| held.contains(self.current.get())))
    }

    /// Whether a word can be pushed without a stack copy.
    fn is_fresh_slot(&self, slot: Slot) -> bool {
        match slot {
            Slot::Value(value) => self.is_fresh(value),
            Slot::Ret => self.ret_spilled,
            Slot::Junk | Slot::Saved(_) => false,
        }
    }

    /// Pushes a fresh copy of a word that `is_fresh_slot` accepts.
    fn fresh_slot(&self, sim: &mut Sim, slot: Slot) {
        match slot {
            Slot::Value(value) => self.fresh(sim, value),
            Slot::Ret => {
                // push slot; mload
                sim.cost += StackCosts::DIRECT_LOAD;
                sim.steps.push(Step::ReloadRet);
                sim.stack.push(Slot::Ret);
                sim.observe(1);
            }
            Slot::Junk | Slot::Saved(_) => unreachable!("no fresh copy"),
        }
    }

    fn materialize_cost(&self, value: ValueId) -> Cost {
        if self.is_remat(value) {
            self.remat_costs[value]
        } else {
            materialize_cost(self.func, self.target, value)
        }
    }

    fn spill_top(&self, sim: &mut Sim, value: ValueId) {
        debug_assert_eq!(sim.stack.last(), Some(&Slot::Value(value)));
        sim.stack.pop();
        sim.cost += StackCosts::DIRECT_STORE;
        sim.steps.push(Step::Spill(value));
        sim.observe(1);
    }

    fn spill_ret_top(&self, sim: &mut Sim) {
        debug_assert_eq!(sim.stack.last(), Some(&Slot::Ret));
        // push slot; mstore
        sim.stack.pop();
        sim.cost += StackCosts::DIRECT_STORE;
        sim.steps.push(Step::SpillRet);
        sim.observe(1);
    }

    /// Pushes a copy of `value`: a fresh materialization or a `DUP` of its shallowest copy.
    #[inline(always)]
    fn copy(&self, sim: &mut Sim, value: ValueId) -> Result<(), Fail> {
        if self.is_fresh(value) {
            self.fresh(sim, value);
            return Ok(());
        }
        let Some(depth) = sim.depth_of(Slot::Value(value)) else {
            return Err(Fail::Unsupported("value missing from the modeled stack"));
        };
        if depth + 1 > self.reach {
            return Err(self.deep(sim, Some(sim.stack.len() - 1 - depth)));
        }
        self.stack_op(sim, StackOp::Dup(depth as u8 + 1))
    }

    fn filler(&self, sim: &mut Sim) {
        sim.cost += self.target.push(U256::ZERO);
        sim.steps.push(Step::Filler);
        sim.stack.push(Slot::Junk);
        sim.observe(0);
    }

    fn pop_top_junk(&self, sim: &mut Sim) -> Result<(), Fail> {
        while sim.stack.last() == Some(&Slot::Junk) {
            self.stack_op(sim, StackOp::Pop)?;
        }
        Ok(())
    }

    /// Marks every remaining copy of a dead value as junk.
    fn kill(sim: &mut Sim, value: ValueId) {
        for slot in &mut sim.stack {
            if *slot == Slot::Value(value) {
                *slot = Slot::Junk;
            }
        }
    }

    // ----------------------------------------------------------------------------------------
    // Operand preparation and shuffling.
    // ----------------------------------------------------------------------------------------

    /// Prepares an external call's operands. A rematerialized gas operand is read last, so
    /// nothing separates `GAS` from the call.
    fn prepare_call(
        &self,
        sim: &mut Sim,
        ops: &[ValueId],
        dying: &dyn Fn(ValueId) -> bool,
    ) -> Result<(), Fail> {
        let (&gas, rest) = ops.split_last().expect("call has a gas operand");
        if !self.is_remat(gas) {
            return self.prepare(sim, ops, dying);
        }
        self.prepare(sim, rest, dying)?;
        self.fresh(sim, gas);
        Ok(())
    }

    /// Arranges `ops`, in push order, on top of the stack, trying each preparation strategy and
    /// keeping the cheapest.
    fn prepare(
        &self,
        sim: &mut Sim,
        ops: &[ValueId],
        dying: &dyn Fn(ValueId) -> bool,
    ) -> Result<(), Fail> {
        self.prepare_any(sim, &[ops], dying).map(drop)
    }

    /// Arranges one of the operand orders `orders` on top of the stack, choosing the cheapest
    /// order and strategy. The price counts each copy of a dying value left below the operands
    /// as a `POP` and two `SWAP`s. Pricing one `SWAP`, as [`Self::dead_word_removal`] does, leaves
    /// more such words behind and measured larger code. Returns the chosen order's index.
    fn prepare_any(
        &self,
        sim: &mut Sim,
        orders: &[&[ValueId]],
        dying: &dyn Fn(ValueId) -> bool,
    ) -> Result<usize, Fail> {
        if orders[0].is_empty() {
            return Ok(0);
        }
        // Every order holds the same operands.
        let mut dying_ops = SmallVec::<[ValueId; 8]>::new();
        for &value in orders[0] {
            if dying(value) {
                dying_ops.push(value);
            }
        }
        let dying = &*dying_ops;
        // Operands already in place need nothing unless a dying one leaves a copy below. Every
        // strategy that prepares them at no cost does exactly this. Whether each order has them in
        // place also tells how many it keeps when it needs more.
        let mut all_in_place = SmallVec::<[bool; 2]>::new();
        for (index, ops) in orders.iter().enumerate() {
            let in_place = self.in_place(&sim.stack, ops, dying);
            if in_place && leftovers(&sim.stack[..sim.stack.len() - ops.len()], dying) == 0 {
                return Ok(index);
            }
            all_in_place.push(in_place);
        }
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Strategy {
            Reorder,
            Build,
            Cycles,
        }
        // The stack position of the shallowest copy of each dying operand that is not fresh.
        let mut consumable = SmallVec::<[(ValueId, Option<usize>); 8]>::new();
        for &value in dying {
            if !self.is_fresh(value) && !consumable.iter().any(|&(seen, _)| seen == value) {
                let position = sim.stack.iter().rposition(|&slot| slot == Slot::Value(value));
                consumable.push((value, position));
            }
        }
        let mut best: Option<(Cost, usize)> = None;
        // Whether the cheapest arrangement so far swaps at least twice.
        let mut best_swaps = false;
        let mut scratch = self.scratch.borrow_mut();
        let [mut candidate, mut best_sim] = scratch.each_mut();
        let mut failure = None;
        let mut in_place = SmallVec::<[Option<usize>; 8]>::new();
        for (index, ops) in orders.iter().enumerate() {
            Self::consumed_in_place(&consumable, ops, &mut in_place);
            // Longest run of operands already in place.
            let placed = if all_in_place[index] {
                ops.len()
            } else {
                (1..ops.len())
                    .rev()
                    .find(|&t| self.in_place(&sim.stack, &ops[..t], dying))
                    .unwrap_or(0)
            };
            // With nothing in place, building and reordering both copy every operand in order: the
            // swap that building tries first needs a dying operand on the stack, which reordering
            // would have consumed in place. A lone operand consumed in place takes the same swap
            // with either strategy, or fails on the same word.
            let build_is_reorder = (placed == 0 && in_place.iter().all(Option::is_none))
                || (ops.len() == 1 && in_place[0].is_some());
            for strategy in [Strategy::Reorder, Strategy::Build, Strategy::Cycles] {
                if strategy == Strategy::Build && build_is_reorder {
                    continue;
                }
                // Following cycles only saves swaps when an operand is consumed in place and the
                // cheapest arrangement so far needs at least two.
                if strategy == Strategy::Cycles && (consumable.is_empty() || !best_swaps) {
                    continue;
                }
                candidate.refork(sim);
                let result = match strategy {
                    Strategy::Reorder => self.prepare_reorder(candidate, ops, &in_place),
                    Strategy::Build => self.prepare_build(candidate, ops, placed, dying),
                    Strategy::Cycles => {
                        let budget = best.map(|(price, _)| price);
                        self.prepare_cycles(candidate, ops, &in_place, budget)
                    }
                };
                if let Err(fail) = result {
                    failure.get_or_insert(fail);
                    continue;
                }
                // Pricing the leftovers only adds to a candidate that already cannot win.
                if best.is_some_and(|(best, _)| !self.target.cmp(candidate.cost, best).is_lt()) {
                    continue;
                }
                let leftovers =
                    leftovers(&candidate.stack[..candidate.stack.len() - ops.len()], dying);
                let price = candidate.cost.plus(
                    StackCosts::POP.plus(self.target.opcode(op::SWAP1).times(2)).times(leftovers),
                );
                if best.is_none_or(|(best, _)| self.target.cmp(price, best).is_lt()) {
                    best = Some((price, index));
                    best_swaps =
                        candidate.steps.iter().filter(|step| step.is_swap()).nth(1).is_some();
                    std::mem::swap(&mut best_sim, &mut candidate);
                }
            }
        }
        let Some((_, index)) = best else { return Err(failure.expect("a failed candidate")) };
        sim.join(best_sim);
        Ok(index)
    }

    /// Builds the operands bottom up: keeps the `placed` values already in place at the top of
    /// the stack, swaps the deepest operand up when it dies here, and copies every other operand
    /// on top.
    fn prepare_build(
        &self,
        sim: &mut Sim,
        ops: &[ValueId],
        mut placed: usize,
        dying: &[ValueId],
    ) -> Result<(), Fail> {
        if placed == 0
            && !self.is_fresh(ops[0])
            && dying.contains(&ops[0])
            && let Some(depth) = sim.depth_of(Slot::Value(ops[0]))
            && depth > 0
            && depth <= self.reach
        {
            self.stack_op(sim, StackOp::Swap(depth as u8))?;
            placed = 1;
        }
        for &value in &ops[placed..] {
            self.copy(sim, value)?;
        }
        Ok(())
    }

    /// Whether the top of `stack` holds `ops` in push order, each once and consumable: a last
    /// use, a materializable value or a surplus copy.
    #[inline(always)]
    fn in_place(&self, stack: &[Slot], ops: &[ValueId], dying: &[ValueId]) -> bool {
        // The last operand must be on top.
        if let Some(&last) = ops.last()
            && stack.last() != Some(&Slot::Value(last))
        {
            return false;
        }
        let Some(split) = stack.len().checked_sub(ops.len()) else { return false };
        let (below, top) = stack.split_at(split);
        ops.iter().enumerate().all(|(i, &value)| {
            top[i] == Slot::Value(value)
                && !ops[..i].contains(&value)
                && (self.is_fresh(value)
                    || dying.contains(&value)
                    || below.contains(&Slot::Value(value)))
        })
    }

    /// The stack position of the word each operand consumes in place: the shallowest copy of a
    /// value whose last use this is, at its first occurrence. Copies only push, so these
    /// positions hold while the other operands are copied. `consumable` holds the positions of
    /// the dying operands that are not fresh.
    fn consumed_in_place(
        consumable: &[(ValueId, Option<usize>)],
        ops: &[ValueId],
        positions: &mut SmallVec<[Option<usize>; 8]>,
    ) {
        positions.clear();
        for (i, &value) in ops.iter().enumerate() {
            positions.push(
                consumable
                    .iter()
                    .find(|&&(consumed, _)| consumed == value)
                    .filter(|_| !ops[..i].contains(&value))
                    .and_then(|&(_, position)| position),
            );
        }
    }

    /// Consumes the operands `in_place` names in place, copies every other operand, and then
    /// moves each operand to its position with at most two swaps.
    fn prepare_reorder(
        &self,
        sim: &mut Sim,
        ops: &[ValueId],
        in_place: &[Option<usize>],
    ) -> Result<(), Fail> {
        let k = ops.len();
        // The stack position of each operand's word.
        let mut positions = SmallVec::<[usize; 8]>::new();
        for (&value, &position) in ops.iter().zip(in_place) {
            if let Some(position) = position {
                positions.push(position);
            } else {
                self.copy(sim, value)?;
                positions.push(sim.stack.len() - 1);
            }
        }
        for i in 0..k {
            let want = k - 1 - i;
            let depth = sim.stack.len() - 1 - positions[i];
            if depth == want {
                continue;
            }
            // Move the operand between two words below the top in one step when that is cheaper.
            // Before Amsterdam an `EXCHANGE` costs its three swaps and never wins.
            if self.target.evm_version().has_extended_stack_ops()
                && depth != 0
                && want != 0
                && let (Ok(depth8), Ok(want8)) = (u8::try_from(depth), u8::try_from(want))
                && let Some(exchange) = StackOp::from_swaps(depth8, want8, depth8)
                && self.exchanges_beat(&[exchange], &[StackOp::Swap(depth8), StackOp::Swap(want8)])
            {
                self.stack_op(sim, exchange)?;
                let top = sim.stack.len() - 1;
                Self::swap_positions(&mut positions, top - depth, top - want);
                continue;
            }
            self.positioned_swap(sim, &mut positions, depth)?;
            self.positioned_swap(sim, &mut positions, want)?;
        }
        Ok(())
    }

    /// Swaps the word at `depth` to the top, following the swap in the operand positions.
    #[inline(always)]
    fn positioned_swap(
        &self,
        sim: &mut Sim,
        positions: &mut [usize],
        depth: usize,
    ) -> Result<(), Fail> {
        self.swap_up(sim, depth)?;
        let top = sim.stack.len() - 1;
        Self::swap_positions(positions, top, top - depth);
        Ok(())
    }

    /// Follows a swap of the stack words at positions `a` and `b` in the operand positions.
    #[inline(always)]
    fn swap_positions(positions: &mut [usize], a: usize, b: usize) {
        for position in positions {
            if *position == a {
                *position = b;
            } else if *position == b {
                *position = a;
            }
        }
    }

    /// Consumes the operands `in_place` names in place, like [`Self::prepare_reorder`], but
    /// follows permutation cycles through the top and pushes each copy only when it lands in its
    /// position or takes the place of a word that has to move up. Copying every operand first
    /// leaves the copies nothing to displace, so each cycle that misses the top costs one more
    /// `SWAP` to open. Stops once the arrangement costs at least `budget`.
    fn prepare_cycles(
        &self,
        sim: &mut Sim,
        ops: &[ValueId],
        in_place: &[Option<usize>],
        budget: Option<Cost>,
    ) -> Result<(), Fail> {
        // Operand `i` ends at `base + i`; every other word ends anywhere below `base`.
        let base = sim.stack.len() - in_place.iter().flatten().count();
        // The final position of each stack word that is an operand, and the operands to copy.
        let mut dests = SmallVec::<[Option<usize>; 16]>::from_elem(None, sim.stack.len());
        let mut pending = SmallVec::<[usize; 8]>::new();
        for (i, &position) in in_place.iter().enumerate() {
            match position {
                Some(position) => dests[position] = Some(base + i),
                None => pending.push(i),
            }
        }
        let settled = |dests: &[Option<usize>], position: usize| {
            dests[position].map_or(position < base, |dest| dest == position)
        };
        // Prefers a word that lands on top, then one that can move on from there.
        let order = |dest: usize, top: usize| (dest != top, dest > top);
        // Each step settles a word, pushes a copy, or opens a cycle, which the next steps close.
        for _ in 0..4 * (base + ops.len()) {
            if budget.is_some_and(|budget| !self.target.cmp(sim.cost, budget).is_lt()) {
                return Err(Fail::Unsupported("operand cycles cost more than another arrangement"));
            }
            let len = sim.stack.len();
            // Move the top word to its position when that lies below it.
            if let Some(top) = len.checked_sub(1)
                && !settled(&dests, top)
            {
                let target = match dests[top] {
                    Some(dest) => (dest < top).then_some(dest),
                    // Displace an operand below `base`.
                    None => (0..base)
                        .filter(|&p| !settled(&dests, p))
                        .min_by_key(|&p| order(dests[p].unwrap_or(usize::MAX), top)),
                };
                if let Some(target) = target {
                    self.swap_up(sim, top - target)?;
                    dests.swap(top, target);
                    continue;
                }
            }
            // Push the copy that lands in place, or else the one whose displaced word does.
            if !pending.is_empty() {
                let index = (0..pending.len())
                    .min_by_key(|&index| {
                        let dest = base + pending[index];
                        let displaced = dests.get(dest).copied().flatten().unwrap_or(0);
                        (order(dest, len), order(displaced, len))
                    })
                    .unwrap();
                let i = pending.remove(index);
                self.copy(sim, ops[i])?;
                dests.push(Some(base + i));
                continue;
            }
            // Open the next cycle with the deepest unsettled word.
            let Some(position) = (0..len).find(|&p| !settled(&dests, p)) else { return Ok(()) };
            self.swap_up(sim, len - 1 - position)?;
            dests.swap(len - 1, position);
        }
        Err(Fail::Unsupported("operand cycles did not settle"))
    }

    /// Rearranges the whole stack into `target`, bottom to top.
    fn shuffle(&self, sim: &mut Sim, target: &[Want]) -> Result<(), Fail> {
        let fresh = |slot| self.is_fresh_slot(slot);
        let moves = match shuffle::shuffle(&sim.stack, target, self.reach, &fresh) {
            Ok(moves) => moves,
            Err(shuffle::Unreachable(position)) => {
                if let Some(victim) = self.stack_victim(sim, position) {
                    return Err(Fail::Deep(Some(victim), self.beyond_reach(sim, Some(victim))));
                }
                // Nothing on the stack can make room: rebuild the target above the words
                // already in place, or name a word that needs a slot.
                shuffle::rebuild(&sim.stack, target, self.reach, &fresh).map_err(|slot| {
                    let victim = match slot {
                        Slot::Value(value) if self.can_spill(value) => Some(Victim::Value(value)),
                        Slot::Ret if self.ret_on_stack() => Some(Victim::Ret),
                        _ => None,
                    };
                    Fail::Deep(victim, self.beyond_reach(sim, victim))
                })?
            }
        };
        let mut index = 0;
        while let Some(&mv) = moves.get(index) {
            if self.target.evm_version().has_extended_stack_ops()
                && let Some((exchanges, end)) = self.exchange_cycle(&moves, index)
            {
                for exchange in exchanges {
                    self.stack_op(sim, exchange)?;
                }
                index = end;
                continue;
            }
            index += 1;
            match mv {
                Move::Swap(depth) => self.stack_op(sim, StackOp::Swap(depth as u8))?,
                Move::Dup(depth) => self.stack_op(sim, StackOp::Dup(depth as u8 + 1))?,
                Move::Fresh(slot) => self.fresh_slot(sim, slot),
                Move::Filler => self.filler(sim),
                Move::Pop => self.stack_op(sim, StackOp::Pop)?,
            }
        }
        debug_assert!(
            sim.stack.len() == target.len()
                && target.iter().zip(&sim.stack).all(|(want, &slot)| want.accepts(slot)),
            "shuffle missed its target"
        );
        Ok(())
    }

    /// Rewrites the swaps `SWAPa SWAPb1 ... SWAPbj SWAPa` that start at `moves[start]`, a cycle
    /// through the top that leaves the top in place, as `EXCHANGE a, b1 ... EXCHANGE a, bj` when
    /// the exchanges are cheaper. Returns the exchanges and the index after the swaps.
    fn exchange_cycle(
        &self,
        moves: &[Move],
        start: usize,
    ) -> Option<(SmallVec<[StackOp; 4]>, usize)> {
        let swaps = moves[start..].iter().map_while(|&mv| match mv {
            Move::Swap(depth) => u8::try_from(depth).ok().map(StackOp::Swap),
            _ => None,
        });
        let (exchanges, len) = StackOp::exchange_cycle(swaps.clone())?;
        let swaps: SmallVec<[StackOp; 8]> = swaps.take(len).collect();
        self.exchanges_beat(&exchanges, &swaps).then_some((exchanges, start + len))
    }

    /// Whether the target lowers `exchanges` and they cost no more than the equivalent `swaps`
    /// in gas and bytes and less in one.
    fn exchanges_beat(&self, exchanges: &[StackOp], swaps: &[StackOp]) -> bool {
        let cost =
            |ops: &[StackOp]| ops.iter().map(|&op| self.target.stack_op(op)).sum::<Option<Cost>>();
        cost(exchanges)
            .zip(cost(swaps))
            .is_some_and(|(exchanges, swaps)| self.target.dominates(exchanges, swaps))
    }

    // ----------------------------------------------------------------------------------------
    // Instructions.
    // ----------------------------------------------------------------------------------------

    /// Whether `value` has no use after the instruction reading `operands`.
    fn dies(&self, block: BlockId, uses: &Uses, operands: &[ValueId], value: ValueId) -> bool {
        let reads = operands.iter().filter(|&&operand| operand == value).count() as u32;
        !self.liveness.live_out(block).contains(value) && uses.count(value) <= reads
    }

    /// Whether `value` is read after the current instruction, whose reads `uses` still counts.
    fn used_later(&self, block: BlockId, uses: &Uses, value: ValueId) -> bool {
        self.liveness.live_out(block).contains(value) || uses.count(value) > 0
    }

    /// Counts the reads of each value by the block's planned instructions and terminator.
    fn block_uses(&self, block: BlockId) -> Uses {
        let func = self.func;
        let mut uses = Uses::default();
        let mut add = |value: ValueId| *uses.remaining.entry(value).or_default() += 1;
        let tail_call = self.tail_calls.get(&block).copied();
        for &inst in &func.blocks[block].instructions {
            let kind = &func.inst(inst).kind;
            if self.skipped.contains(inst)
                || matches!(kind, InstKind::Phi(_))
                || Some(inst) == tail_call
            {
                continue;
            }
            kind.visit_operands(&mut add);
        }
        match &func.blocks[block].terminator {
            Some(Terminator::Branch { condition, .. }) => {
                add(self.conditions.get(&block).map_or(*condition, |&(condition, _)| condition));
            }
            Some(Terminator::Return { .. }) if let Some(call) = tail_call => {
                icall(&func.inst(call).kind).unwrap().1.iter().copied().for_each(add);
            }
            Some(term) => term.visit_operands(add),
            None => {}
        }
        uses
    }

    fn plan_inst_inner(
        &self,
        sim: &mut Sim,
        block: BlockId,
        planned: &PlannedInst,
        uses: &Uses,
    ) -> Result<(), Fail> {
        let func = self.func;
        let inst = planned.inst;
        let operands = &planned.operands;
        let kind = &func.inst(inst).kind;
        sim.steps.push(Step::Begin(Some(inst)));
        let result = func.inst_result_value(inst);
        // Whether each operand has no use after this instruction.
        let mut dies = SmallVec::<[bool; 8]>::new();
        for &operand in operands {
            dies.push(self.dies(block, uses, operands, operand));
        }
        let dying = |value: ValueId| match operands.iter().position(|&operand| operand == value) {
            Some(index) => dies[index],
            None => self.dies(block, uses, operands, value),
        };
        let push_order: Operands = operands.iter().rev().copied().collect();
        let pushes = |result: Option<ValueId>| result.map_or(0, |_| 1);

        // A write that may reach the spill area, or another activation of this function reusing
        // its fixed frame, would overwrite the spilled words still needed afterwards, so the stack
        // keeps them across the instruction.
        // [...] -> [..., saved]
        let saved = self.saved_across(inst);
        self.push_saved(sim, &saved);

        if let Some((callee, args)) = icall(kind) {
            // Arguments go in reverse, so the first argument sits just below the return address.
            let ops: Operands = args.iter().rev().copied().collect();
            self.prepare(sim, &ops, &dying)?;
            let base = sim.height() - args.len();
            sim.observe(2);
            sim.steps.push(Step::Call(callee, sim.below + base));
            sim.cost += StackCosts::INTERNAL_CALL;
            sim.stack.truncate(base);
            let arity = self.info.returns[callee];
            let extras = self.projections.get(&inst);
            for index in (0..arity).rev() {
                let value = if index == 0 {
                    result
                } else {
                    extras.and_then(|extras| extras.get(index - 1).copied().flatten())
                };
                let live = value.filter(|&value| self.used_later(block, uses, value));
                sim.stack.push(live.map_or(Slot::Junk, Slot::Value));
            }
            sim.observe(0);
            if self.publish.contains(inst) {
                // [result(m-1), ..., result1, result0] -> [result0], storing result1.. to the
                // callee's return area
                sim.steps.push(Step::Publish { callee, arity, params: self.info.params[callee] });
                let top = sim.stack.pop().unwrap();
                sim.stack.truncate(sim.stack.len() + 1 - arity);
                sim.stack.push(top);
                sim.observe(2);
                sim.cost += self
                    .target
                    .opcode(op::SWAP1)
                    .plus(StackCosts::DIRECT_STORE)
                    .times(arity as u32 - 1)
                    .plus(StackCosts::DIRECT_STORE)
                    .plus(self.target.opcode(op::PUSH2));
            }
            for &value in extras.into_iter().flatten().flatten().chain(result.iter()) {
                if self.spilled.contains(value) {
                    self.spill_in_place(sim, value)?;
                }
            }
            // [saved, results] -> [results]; each saved word goes back to its slot
            self.restore_saved(sim, &saved)?;
            self.finish_inst(sim, operands, &dies, block, uses, None)?;
            return Ok(());
        }

        match kind {
            InstKind::Zext(value) | InstKind::PtrToInt(value, _) | InstKind::IntToPtr(value) => {
                // cast value -> the operand word under the result identity
                let result = result.expect("cast has a result");
                if !self.is_fresh(*value)
                    && !self.spilled.contains(result)
                    && dying(*value)
                    && let Some(depth) = sim.depth_of(Slot::Value(*value))
                {
                    // The dying operand's word becomes the result where it is.
                    let position = sim.stack.len() - 1 - depth;
                    sim.stack[position] = Slot::Value(result);
                } else {
                    self.prepare(sim, &[*value], &dying)?;
                    *sim.stack.last_mut().unwrap() = Slot::Value(result);
                }
            }
            InstKind::Eq(..) | InstKind::Ne(..) => {
                let ne = matches!(kind, InstKind::Ne(..));
                if let Some(value) =
                    Target::zero_test_input(&kind.op(), |value| func.value_u256(value))
                {
                    // eq x, 0 -> iszero x; ne x, 0 -> iszero (iszero x)
                    self.prepare(sim, &[value], &dying)?;
                    self.op(sim, op::ISZERO, 1, 1);
                } else {
                    // ne a, b -> iszero (eq a, b)
                    self.prepare_any(sim, &[&push_order, operands], &dying)?;
                    self.op(sim, op::EQ, 2, 1);
                }
                if ne {
                    self.op(sim, op::ISZERO, 1, 1);
                }
                self.set_top_result(sim, result);
            }
            InstKind::Select(cond, true_value, false_value) => {
                // [f, c, t] -> [f, c, f, t] -> [f, c, t-f] -> [f, c*(t-f)] -> [f + c*(t-f)]
                self.prepare(sim, &[*false_value, *cond, *true_value], &dying)?;
                sim.observe(1);
                sim.steps.push(Step::Inst(inst));
                sim.cost += self.target.opcode(op::DUP3).plus(self.target.opcode(op::SWAP1));
                sim.stack.truncate(sim.stack.len() - 3);
                sim.stack.push(result.map_or(Slot::Junk, Slot::Value));
            }
            InstKind::DataCopy(_, dest, size) => {
                // [size, dest] -> push_data data; swap1; codecopy
                self.prepare(sim, &[*size, *dest], &dying)?;
                sim.observe(1);
                sim.steps.push(Step::Inst(inst));
                sim.cost += self.target.opcode(op::SWAP1);
                sim.stack.truncate(sim.stack.len() - 2);
            }
            InstKind::Call { gas, addr, value, args_offset, args_size, ret_offset, ret_size }
            | InstKind::CallCode {
                gas,
                addr,
                value,
                args_offset,
                args_size,
                ret_offset,
                ret_size,
            } => {
                let opcode =
                    if matches!(kind, InstKind::Call { .. }) { op::CALL } else { op::CALLCODE };
                let ops = [*ret_size, *ret_offset, *args_size, *args_offset, *value, *addr, *gas];
                self.prepare_call(sim, &ops, &dying)?;
                self.op(sim, opcode, 7, pushes(result));
                self.set_top_result(sim, result);
            }
            InstKind::StaticCall { gas, addr, args_offset, args_size, ret_offset, ret_size }
            | InstKind::DelegateCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                let opcode = if matches!(kind, InstKind::StaticCall { .. }) {
                    op::STATICCALL
                } else {
                    op::DELEGATECALL
                };
                let ops = [*ret_size, *ret_offset, *args_size, *args_offset, *addr, *gas];
                self.prepare_call(sim, &ops, &dying)?;
                self.op(sim, opcode, 6, pushes(result));
                self.set_top_result(sim, result);
            }
            InstKind::Alloc { .. }
            | InstKind::LibraryAddress(_)
            | InstKind::InternalFrameAddr(_) => {
                sim.steps.push(Step::Inst(inst));
                sim.cost += self.target.opcode(op::PUSH2);
                sim.stack.push(result.map_or(Slot::Junk, Slot::Value));
                sim.observe(0);
            }
            InstKind::ConstructorArgsBase | InstKind::ConstructorArgsEnd => {
                // push base; [push offset; codesize; sub; add]
                sim.steps.push(Step::Inst(inst));
                sim.cost += self.target.opcode(op::PUSH2);
                if matches!(kind, InstKind::ConstructorArgsEnd) {
                    sim.cost += self
                        .target
                        .opcode(op::PUSH2)
                        .plus(StackCosts::NULLARY_READ)
                        .plus(self.target.opcode(op::SUB).plus(self.target.opcode(op::ADD)));
                }
                sim.stack.push(result.map_or(Slot::Junk, Slot::Value));
                sim.observe(2);
            }
            InstKind::LoadImmutable(_) | InstKind::HeapFloor => {
                // A heap floor in a constructor is recomputed from the argument blob.
                let push = if matches!(kind, InstKind::HeapFloor) { op::PUSH2 } else { op::PUSH32 };
                sim.steps.push(Step::Inst(inst));
                sim.cost += self.target.opcode(push);
                sim.stack.push(result.map_or(Slot::Junk, Slot::Value));
                sim.observe(1);
            }
            _ => {
                let Some(lowering) = opcode_lowering(&kind.op()) else {
                    return Err(Fail::Unsupported("instruction without a stack lowering"));
                };
                let opcode = lowering.opcode();
                match lowering {
                    OpcodeLowering::Binary { opcode }
                        if let Some(mirror) = op::swapped_binary_opcode(opcode) =>
                    {
                        let flipped = self.prepare_any(sim, &[&push_order, operands], &dying)? == 1;
                        self.op(sim, if flipped { mirror } else { opcode }, 2, pushes(result));
                    }
                    _ => {
                        self.prepare(sim, &push_order, &dying)?;
                        self.op(sim, opcode, operands.len(), pushes(result));
                    }
                }
                self.set_top_result(sim, result);
            }
        }
        self.finish_inst(sim, operands, &dies, block, uses, result)?;
        // [saved, result] -> [result]
        self.restore_saved(sim, &saved)
    }

    /// Records an opcode with its stack effect; the result word is anonymous until named.
    fn op(&self, sim: &mut Sim, opcode: u8, pops: usize, pushes: usize) {
        sim.steps.push(Step::Op(opcode));
        sim.stack.truncate(sim.stack.len() - pops);
        for _ in 0..pushes {
            sim.stack.push(Slot::Junk);
        }
        sim.observe(0);
    }

    fn set_top_result(&self, sim: &mut Sim, result: Option<ValueId>) {
        if let Some(result) = result {
            *sim.stack.last_mut().expect("instruction result") = Slot::Value(result);
        }
    }

    fn finish_inst(
        &self,
        sim: &mut Sim,
        operands: &[ValueId],
        dies: &[bool],
        block: BlockId,
        uses: &Uses,
        result: Option<ValueId>,
    ) -> Result<(), Fail> {
        for (&operand, &dies) in operands.iter().zip(dies) {
            if dies {
                Self::kill(sim, operand);
            }
        }
        if let Some(result) = result {
            if !self.used_later(block, uses, result) {
                Self::kill(sim, result);
            } else if self.spilled.contains(result)
                && sim.stack.last() == Some(&Slot::Value(result))
            {
                self.spill_top(sim, result);
            }
        }
        self.pop_top_junk(sim)
    }

    // ----------------------------------------------------------------------------------------
    // Terminators and edges.
    // ----------------------------------------------------------------------------------------

    fn plan_terminator(&mut self, sim: &mut Sim, block: BlockId) -> Result<Exit, Fail> {
        let func = self.func;
        let Some(term) = &func.blocks[block].terminator else {
            return Err(Fail::Unsupported("block without a terminator"));
        };
        let live_out = self.liveness.live_out(block);
        let dying = |value: ValueId| !live_out.contains(value);
        match term {
            Terminator::Jump(target) => {
                if self.defers(*target) {
                    self.defer(block, *target, sim, None)?;
                } else {
                    self.jump_edge(sim, block, *target)?;
                }
                Ok(Exit::Jump(*target))
            }
            Terminator::Branch { condition, then_block, else_block } => {
                // jumpi (eq x, 0), a, b -> jumpi x, b, a
                let (condition, inverted) =
                    self.conditions.get(&block).copied().unwrap_or((*condition, false));
                let (then_block, else_block) =
                    if inverted { (*else_block, *then_block) } else { (*then_block, *else_block) };
                self.plan_branch(sim, block, condition, then_block, else_block)
            }
            Terminator::Switch { value, default, cases } => {
                self.plan_switch(sim, block, *value, *default, cases)
            }
            Terminator::Return { values } => {
                if !self.has_ret {
                    return Ok(Exit::Terminal);
                }
                if let Some(&call) = self.tail_calls.get(&block) {
                    // [...] -> [arg(n-1), ..., arg0, return]; jump callee
                    let (callee, args) = icall(&func.inst(call).kind).unwrap();
                    self.shuffle(sim, &with_return_address(args))?;
                    self.calls.push((callee, 0));
                    sim.observe(1);
                    return Ok(Exit::TailCall(callee));
                }
                // [..., return] -> [result(m-1), ..., result0, return]
                self.shuffle(sim, &with_return_address(values))?;
                Ok(Exit::Return)
            }
            Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                self.prepare(sim, &[*size, *offset], &dying)?;
                Ok(Exit::Terminal)
            }
            Terminator::SelfDestruct { recipient } => {
                self.prepare(sim, &[*recipient], &dying)?;
                Ok(Exit::Terminal)
            }
            Terminator::RevertReturndata => {
                sim.observe(3);
                Ok(Exit::Terminal)
            }
            Terminator::Stop | Terminator::Invalid => Ok(Exit::Terminal),
            Terminator::TailCall { function, args } => {
                let callee = *function;
                let base;
                if self.info.returning.contains(callee) && self.has_ret {
                    // [...] -> [arg(n-1), ..., arg0, return]
                    self.shuffle(sim, &with_return_address(args))?;
                    base = 0;
                } else {
                    let ops: Operands = args.iter().rev().copied().collect();
                    self.prepare(sim, &ops, &dying)?;
                    base = sim.height() - args.len();
                    if self.info.returning.contains(callee) {
                        // This body has no return address to forward.
                        self.filler(sim);
                    }
                }
                self.calls.push((callee, sim.below + base));
                sim.observe(1);
                Ok(Exit::TailCall(callee))
            }
        }
    }

    fn phi_inputs(&self, pred: BlockId, succ: BlockId) -> SmallVec<[(ValueId, ValueId); 4]> {
        let mut inputs = SmallVec::new();
        // Phis lead their block.
        for &inst in &self.func.blocks[succ].instructions {
            let InstKind::Phi(incoming) = &self.func.inst(inst).kind else { break };
            let Some(result) = self.func.inst_result_value(inst) else { continue };
            if let Some(&(_, value)) = incoming.iter().find(|(block, _)| *block == pred) {
                inputs.push((result, value));
            }
        }
        inputs
    }

    fn is_join(&self, block: BlockId) -> bool {
        self.func.blocks[block].predecessors.len() > 1
    }

    /// The physical words a fixed successor layout requires on the edge from `pred`, for a stack
    /// of `height` words. A floating layout only constrains the words at the top.
    fn edge_target(
        &self,
        pred: BlockId,
        succ: BlockId,
        layout: &Layout,
        height: usize,
    ) -> Vec<Want> {
        let inputs = self.phi_inputs(pred, succ);
        let below =
            if self.floating.contains(succ) { height.saturating_sub(layout.len()) } else { 0 };
        let mut target = vec![Want::Any; below];
        target.extend(layout.iter().map(|&slot| match slot {
            Slot::Value(value) => {
                let input = inputs.iter().find(|(result, _)| *result == value);
                Want::Value(input.map_or(value, |&(_, input)| input))
            }
            Slot::Ret => Want::Ret,
            Slot::Junk | Slot::Saved(_) => Want::Any,
        }));
        target
    }

    /// The successor's view of each word of the predecessor's stack, with phi inputs renamed to
    /// their results where an otherwise dead word holds them. Returns `None` when a phi input
    /// has no such word.
    fn view(&self, sim: &Sim, pred: BlockId, succ: BlockId) -> Option<Layout> {
        let live_in = self.liveness.live_in(succ);
        let floating = self.floating.contains(succ);
        let mut view: Layout = sim
            .stack
            .iter()
            .map(|&slot| match slot {
                Slot::Value(value) if live_in.contains(value) => slot,
                Slot::Ret if !floating => Slot::Ret,
                _ => Slot::Junk,
            })
            .collect();
        for (result, input) in self.phi_inputs(pred, succ) {
            if self.spilled.contains(result) {
                return None;
            }
            let position = (0..view.len())
                .rev()
                .find(|&p| sim.stack[p] == Slot::Value(input) && view[p] == Slot::Junk)?;
            view[position] = Slot::Value(result);
        }
        Some(view)
    }

    /// Number of bottom words a floating successor does not model.
    fn floating_cut(&self, succ: BlockId, view: &Layout) -> usize {
        if self.floating.contains(succ) {
            view.iter().take_while(|&&slot| slot == Slot::Junk).count()
        } else {
            0
        }
    }

    /// Stores the inputs of the successor's spilled phi results on the edge, unless a store would
    /// clobber a read; `shuffle_edge` then stores them after the edge's shuffle.
    fn store_spilled_phis(&self, sim: &mut Sim, pred: BlockId, succ: BlockId) -> Result<(), Fail> {
        if self.stores_clobber_reads(pred, succ) {
            return Ok(());
        }
        for (result, input) in self.spilled_phi_inputs(pred, succ) {
            self.copy(sim, input)?;
            *sim.stack.last_mut().unwrap() = Slot::Value(result);
            self.spill_top(sim, result);
        }
        Ok(())
    }

    /// The spilled phi results of `succ` with their inputs on the edge from `pred`.
    fn spilled_phi_inputs(
        &self,
        pred: BlockId,
        succ: BlockId,
    ) -> SmallVec<[(ValueId, ValueId); 4]> {
        if self.spilled.is_empty() {
            return SmallVec::new();
        }
        let mut inputs = self.phi_inputs(pred, succ);
        inputs.retain(|&mut (result, _)| self.spilled.contains(result));
        inputs
    }

    /// Whether the edge from `pred` reads the old value of a spilled phi result of `succ` that it
    /// also stores. Its stores must then wait until the edge's shuffle has read every input.
    fn stores_clobber_reads(&self, pred: BlockId, succ: BlockId) -> bool {
        if self.spilled.is_empty() {
            return false;
        }
        let inputs = self.phi_inputs(pred, succ);
        inputs.iter().any(|&(result, _)| {
            self.spilled.contains(result) && inputs.iter().any(|&(_, input)| input == result)
        })
    }

    /// Shuffles to `target`. When the edge's spilled phi stores would clobber its reads, the
    /// shuffle also places their inputs above `target` and then stores each into its result's
    /// slot, so every read happens before the first store.
    fn shuffle_edge(
        &self,
        sim: &mut Sim,
        pred: BlockId,
        succ: BlockId,
        mut target: Vec<Want>,
    ) -> Result<(), Fail> {
        if !self.stores_clobber_reads(pred, succ) {
            return self.shuffle(sim, &target);
        }
        let stores = self.spilled_phi_inputs(pred, succ);
        target.extend(stores.iter().map(|&(_, input)| Want::Value(input)));
        self.shuffle(sim, &target)?;
        // [layout, input_1, ..., input_k] -> [layout]; each input stores into its result's slot
        for &(result, _) in stores.iter().rev() {
            *sim.stack.last_mut().unwrap() = Slot::Value(result);
            self.spill_top(sim, result);
        }
        Ok(())
    }

    /// Plans an unshared edge: adopts the stack as the successor's layout, or shuffles to it.
    fn jump_edge(&mut self, sim: &mut Sim, pred: BlockId, succ: BlockId) -> Result<(), Fail> {
        self.store_spilled_phis(sim, pred, succ)?;
        let layout = if let Some(layout) = self.layouts[succ].clone() {
            self.record_natural(sim, pred, succ);
            layout
        } else if let Some(hint) = self.hints.get(&succ).filter(|hint| self.hint_fits(succ, hint)) {
            self.layouts[succ] = Some(hint.clone());
            hint.clone()
        } else if self.stores_clobber_reads(pred, succ) {
            // Take the adopted layout without its code; the shuffle below reaches it.
            let layout = self.adopt(&mut sim.fork(), pred, succ)?;
            self.layouts[succ] = Some(layout.clone());
            layout
        } else {
            let layout = self.adopt(sim, pred, succ)?;
            self.layouts[succ] = Some(layout);
            self.note_entry(succ, sim);
            return Ok(());
        };
        let target = self.edge_target(pred, succ, &layout, sim.height());
        self.shuffle_edge(sim, pred, succ, target)?;
        self.note_entry(succ, sim);
        Ok(())
    }

    /// Records, on the first backedge into a loop header, the layout its latch would choose.
    fn record_natural(&mut self, sim: &Sim, pred: BlockId, succ: BlockId) {
        if self.backedges.contains(&(pred, succ))
            && !self.natural.contains_key(&succ)
            && let Ok(layout) = self.adopt(&mut sim.fork(), pred, succ)
        {
            self.natural.insert(succ, layout);
        }
    }

    /// Raises the physical words below the layout of `succ` to cover an edge that leaves the
    /// stack of `sim`.
    fn note_entry(&mut self, succ: BlockId, sim: &Sim) {
        if let Some(layout) = &self.layouts[succ] {
            let below = sim.below + sim.height().saturating_sub(layout.len());
            self.floating_below[succ] = self.floating_below[succ].max(below);
        }
    }

    /// Whether the layout of `succ` waits for all of its forward predecessors.
    fn defers(&self, succ: BlockId) -> bool {
        self.layouts[succ].is_none()
            && self.forward_preds[succ] >= 2
            && !self.floating.contains(succ)
            && !self.hints.contains_key(&succ)
    }

    /// Records an edge into a join whose layout is chosen later. Spilled phi results take
    /// their inputs before the edge is recorded, unless the stores would clobber the edge's
    /// reads; the join's shuffle then stores them.
    fn defer(
        &mut self,
        pred: BlockId,
        succ: BlockId,
        sim: &mut Sim,
        side: Option<bool>,
    ) -> Result<(), Fail> {
        if side.is_none() {
            self.store_spilled_phis(sim, pred, succ)?;
        }
        self.pending.entry(succ).or_default().push(Pending {
            pred,
            stack: sim.stack.clone(),
            side,
        });
        Ok(())
    }

    /// Chooses the layout of a join from its recorded edges and completes each edge. The
    /// candidates are each predecessor's proposal, priced by the weighted cost of the shuffles and
    /// trampolines into it. Outside loops, the join's wanted order, its words by first read with
    /// the first on top, competes with the cheapest proposal: each costs its incoming edges plus
    /// a trial plan of the join's own instructions, so a layout the join consumes in place can
    /// beat one the edges reach more cheaply.
    fn resolve_join(&mut self, join: BlockId) -> Result<(), Fail> {
        let Some(pending) = self.pending.remove(&join) else { return Ok(()) };
        if self.layouts[join].is_none() {
            let mut best: Option<(Cost, Layout)> = None;
            let mut candidates = Vec::<Layout>::new();
            // Without any candidate, the first edge that failed to adopt names a word to spill.
            let mut failure = None;
            for edge in &pending {
                if edge.side.is_some() && !self.spilled_phi_inputs(edge.pred, join).is_empty() {
                    continue;
                }
                let mut sim = Sim::new(edge.stack.clone());
                self.current.set(edge.pred);
                match self.adopt(&mut sim, edge.pred, join) {
                    Ok(candidate) if !candidates.contains(&candidate) => candidates.push(candidate),
                    Ok(_) => {}
                    Err(fail) => {
                        failure.get_or_insert(fail);
                    }
                }
            }
            self.current.set(join);
            for candidate in candidates {
                let total = self.incoming_cost(&pending, join, &candidate);
                if best.as_ref().is_none_or(|(best, _)| self.target.cmp(total, *best).is_lt()) {
                    best = Some((total, candidate));
                }
            }
            if let Some((incoming, proposal)) = &best
                && !self.cfg.cyclic_blocks().contains(join)
                && let Some(wanted) = self.wanted_beats(&pending, join, *incoming, proposal)
            {
                best = Some(wanted);
            }
            let layout = match (best, failure) {
                (Some((_, layout)), _) => layout,
                (None, Some(fail)) => return Err(fail),
                // No edge proposes a layout; every edge can still rebuild a canonical one.
                (None, None) => self.canonical_layout(join),
            };
            self.layouts[join] = Some(layout);
        }
        let layout = self.layouts[join].clone().unwrap();
        for edge in pending {
            // The edge's code runs in its predecessor, where a retained constant may not be held.
            self.current.set(edge.pred);
            let mut sim = Sim::new(edge.stack.clone());
            let target = self.edge_target(edge.pred, join, &layout, sim.height());
            match edge.side {
                None => {
                    self.shuffle_edge(&mut sim, edge.pred, join, target)?;
                    self.note_entry(join, &sim);
                    self.charge(&sim, edge.pred);
                    let plan = self.blocks[edge.pred].as_mut().expect("planned predecessor");
                    plan.steps.extend(sim.steps);
                }
                Some(then) => {
                    if self.satisfies(&sim, edge.pred, join) {
                        self.note_entry(join, &sim);
                        continue;
                    }
                    self.jump_edge(&mut sim, edge.pred, join)?;
                    sim.cost += StackCosts::EDGE_JUMP;
                    self.charge(&sim, edge.pred);
                    let plan = self.blocks[edge.pred].as_mut().expect("planned predecessor");
                    let Exit::Branch { then_edge, else_edge } = &mut plan.exit else {
                        unreachable!("deferred branch edge from a non-branch block")
                    };
                    // Both arms share one recorded edge when they reach the same join.
                    let both = then_edge.target == join && else_edge.target == join;
                    if then_edge.target == join && (then || both) {
                        then_edge.trampoline = Some(sim.steps.clone());
                    }
                    if else_edge.target == join && (!then || both) {
                        else_edge.trampoline = Some(sim.steps);
                    }
                }
            }
        }
        self.current.set(join);
        Ok(())
    }

    /// The join's wanted order with its incoming cost, when it differs from `proposal`, the
    /// cheapest proposal with incoming cost `incoming`, and the incoming edges plus a trial plan
    /// of the join's own code cost less under it. A proposal whose trial cannot be planned loses.
    fn wanted_beats(
        &self,
        pending: &[Pending],
        join: BlockId,
        incoming: Cost,
        proposal: &Layout,
    ) -> Option<(Cost, Layout)> {
        let wanted = self.wanted_layout(join);
        if wanted == *proposal {
            return None;
        }
        let wanted_incoming = self.incoming_cost(pending, join, &wanted);
        if wanted_incoming == Cost::MAX {
            return None;
        }
        let planned = self.planned_insts(join);
        let wanted_cost = self.join_cost(join, &planned, &wanted)?;
        let wins = match self.join_cost(join, &planned, proposal) {
            Some(proposal_cost) => {
                let proposal_total = incoming + self.weigh(proposal_cost, join);
                let wanted_total = wanted_incoming + self.weigh(wanted_cost, join);
                self.target.cmp(wanted_total, proposal_total).is_lt()
            }
            None => true,
        };
        wins.then_some((wanted_incoming, wanted))
    }

    /// The weighted cost of the shuffles and trampolines that bring every recorded edge into
    /// `layout`, or `Cost::MAX` when one cannot reach it.
    fn incoming_cost(&self, pending: &[Pending], join: BlockId, layout: &Layout) -> Cost {
        let mut total = Cost::ZERO;
        for edge in pending {
            // An unreachable edge fails again when the join completes it, naming a word to spill.
            let Ok(cost) = self.edge_cost(edge, join, layout) else { return Cost::MAX };
            total += self.weigh(cost, edge.pred);
        }
        total
    }

    /// The join's words, by first read in the join block with the first on top, and the return
    /// address deepest. The block's instructions and terminator read them in this order, so a
    /// layout like it needs no shuffle before them.
    fn wanted_layout(&self, join: BlockId) -> Layout {
        let words = self.join_words(join);
        ordered_layout(self.func, [join], words, self.ret_on_stack(), Read::First)
    }

    /// Prices the join's own code entered with `layout`: its planned instructions and the shuffle
    /// that places the operands its terminator reads. Returns `None` when they cannot be planned.
    fn join_cost(&self, join: BlockId, planned: &[PlannedInst], layout: &Layout) -> Option<Cost> {
        let base = Sim::new(layout.clone());
        let body = self.trial(&base, join, planned, &mut self.block_uses(join), 0, &[], None)?;
        let mut sim = self.trial_sim.borrow().fork();
        let before = sim.cost;
        self.terminator_operands(&mut sim, join).ok()?;
        Some(body.plus(sim.cost.saturating_sub(before)))
    }

    /// Places the operands a block's return, branch, or revert reads, as `plan_terminator` does.
    fn terminator_operands(&self, sim: &mut Sim, block: BlockId) -> Result<(), Fail> {
        let live_out = self.liveness.live_out(block);
        let dying = |value: ValueId| !live_out.contains(value);
        match &self.func.blocks[block].terminator {
            Some(Terminator::Return { values }) if self.has_ret => {
                let values = match self.tail_calls.get(&block) {
                    Some(&call) => icall(&self.func.inst(call).kind).unwrap().1,
                    None => values.as_slice(),
                };
                self.shuffle(sim, &with_return_address(values))
            }
            Some(Terminator::Branch { condition, .. }) => {
                let (condition, _) =
                    self.conditions.get(&block).copied().unwrap_or((*condition, false));
                self.prepare(sim, &[condition], &dying)
            }
            Some(Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size }) => {
                self.prepare(sim, &[*size, *offset], &dying)
            }
            _ => Ok(()),
        }
    }

    /// A join layout derived from no edge: the return address, then the join's live words and
    /// phi results kept on the stack, by value.
    fn canonical_layout(&self, join: BlockId) -> Layout {
        let mut layout = Layout::new();
        if self.ret_on_stack() {
            layout.push(Slot::Ret);
        }
        layout.extend(self.join_words(join).into_iter().map(Slot::Value));
        layout
    }

    /// The join's live words and phi results kept on the stack, by value.
    fn join_words(&self, join: BlockId) -> Vec<ValueId> {
        let mut words: Vec<ValueId> = self.liveness.live_in(join).iter().collect();
        words.extend(self.func.block_phi_results(join).iter());
        words.retain(|&value| !self.is_fresh(value));
        words.sort_unstable();
        words.dedup();
        words
    }

    /// Prices one recorded edge into a candidate join layout.
    fn edge_cost(&self, edge: &Pending, join: BlockId, layout: &Layout) -> Result<Cost, Fail> {
        let join_block = self.current.replace(edge.pred);
        let cost = self.edge_cost_in_pred(edge, join, layout);
        self.current.set(join_block);
        cost
    }

    fn edge_cost_in_pred(
        &self,
        edge: &Pending,
        join: BlockId,
        layout: &Layout,
    ) -> Result<Cost, Fail> {
        let mut sim = Sim::new(edge.stack.clone());
        let target = self.edge_target(edge.pred, join, layout, sim.height());
        if edge.side.is_none() {
            self.shuffle_edge(&mut sim, edge.pred, join, target)?;
            return Ok(sim.cost);
        }
        // A side edge that needs no code costs nothing, as `satisfies` decides.
        if self.spilled_phi_inputs(edge.pred, join).is_empty()
            && target.len() == sim.stack.len()
            && target.iter().zip(&sim.stack).all(|(want, &slot)| want.accepts(slot))
        {
            return Ok(Cost::ZERO);
        }
        self.store_spilled_phis(&mut sim, edge.pred, join)?;
        self.shuffle_edge(&mut sim, edge.pred, join, target)?;
        Ok(sim.cost.plus(StackCosts::EDGE_JUMP))
    }

    /// Whether a layout from an earlier round still names exactly the successor's live words.
    fn hint_fits(&self, succ: BlockId, hint: &Layout) -> bool {
        let live_in = self.liveness.live_in(succ);
        let phis: SmallVec<[ValueId; 4]> = self.func.blocks[succ]
            .instructions
            .iter()
            .filter(|&&inst| matches!(self.func.inst(inst).kind, InstKind::Phi(_)))
            .filter_map(|&inst| self.func.inst_result_value(inst))
            .collect();
        hint.iter().all(|slot| match *slot {
            Slot::Value(value) => {
                !self.spilled.contains(value) && (live_in.contains(value) || phis.contains(&value))
            }
            Slot::Ret => self.ret_on_stack(),
            Slot::Junk | Slot::Saved(_) => true,
        }) && (hint.contains(&Slot::Ret) == self.ret_on_stack())
    }

    /// Builds the successor's layout from the current stack, materializing phi inputs that
    /// have no reusable word. Joins drop every junk word so that later edges need not fill them;
    /// floating successors also leave out the junk below their deepest live word.
    fn adopt(&self, sim: &mut Sim, pred: BlockId, succ: BlockId) -> Result<Layout, Fail> {
        // Copy phi inputs that cannot take over a dead word.
        let mut fresh: SmallVec<[(ValueId, ValueId); 4]> = SmallVec::new();
        let mut claimed: SmallVec<[usize; 8]> = SmallVec::new();
        let live_in = self.liveness.live_in(succ);
        for (result, input) in self.phi_inputs(pred, succ) {
            if self.spilled.contains(result) {
                continue;
            }
            let reusable = !self.is_fresh(input)
                && !live_in.contains(input)
                && (0..sim.stack.len()).rev().any(|p| {
                    sim.stack[p] == Slot::Value(input) && !claimed.contains(&p) && {
                        claimed.push(p);
                        true
                    }
                });
            if !reusable {
                fresh.push((result, input));
            }
        }
        for &(_, input) in &fresh {
            self.copy(sim, input)?;
        }
        let mut view = self.view_with_fresh(sim, pred, succ, &fresh);
        let cut = self.floating_cut(succ, &view);
        if self.is_join(succ) && view[cut..].contains(&Slot::Junk) {
            let mut target = vec![Want::Any; cut];
            target.extend(
                view[cut..]
                    .iter()
                    .zip(&sim.stack[cut..])
                    .filter(|(slot, _)| **slot != Slot::Junk)
                    .map(|(_, &physical)| match physical {
                        Slot::Value(value) => Want::Value(value),
                        Slot::Ret => Want::Ret,
                        Slot::Junk | Slot::Saved(_) => unreachable!("live view word over junk"),
                    }),
            );
            self.shuffle(sim, &target)?;
            view = view[cut..].iter().copied().filter(|slot| *slot != Slot::Junk).collect();
        } else {
            while view.len() > cut && view.last() == Some(&Slot::Junk) {
                self.stack_op(sim, StackOp::Pop)?;
                view.pop();
            }
            view.drain(..cut);
        }
        Ok(view)
    }

    /// The successor's view after copying `fresh` phi inputs to the top.
    fn view_with_fresh(
        &self,
        sim: &Sim,
        pred: BlockId,
        succ: BlockId,
        fresh: &[(ValueId, ValueId)],
    ) -> Layout {
        let live_in = self.liveness.live_in(succ);
        let floating = self.floating.contains(succ);
        let base = sim.stack.len() - fresh.len();
        let mut view: Layout = sim.stack[..base]
            .iter()
            .map(|&slot| match slot {
                Slot::Value(value) if live_in.contains(value) => slot,
                Slot::Ret if !floating => Slot::Ret,
                _ => Slot::Junk,
            })
            .collect();
        view.extend(fresh.iter().map(|&(result, _)| Slot::Value(result)));
        for (result, input) in self.phi_inputs(pred, succ) {
            if self.spilled.contains(result) || fresh.iter().any(|&(r, _)| r == result) {
                continue;
            }
            let position = (0..base)
                .rev()
                .find(|&p| sim.stack[p] == Slot::Value(input) && view[p] == Slot::Junk)
                .expect("reusable phi input word");
            view[position] = Slot::Value(result);
        }
        view
    }

    /// Whether the current stack already satisfies a fixed successor layout, so the edge needs no
    /// code. An edge that stores spilled phi inputs always needs code.
    fn satisfies(&self, sim: &Sim, pred: BlockId, succ: BlockId) -> bool {
        let Some(layout) = &self.layouts[succ] else { return false };
        if !self.spilled_phi_inputs(pred, succ).is_empty() {
            return false;
        }
        let target = self.edge_target(pred, succ, layout, sim.height());
        target.len() == sim.stack.len()
            && target.iter().zip(&sim.stack).all(|(want, &slot)| want.accepts(slot))
    }

    /// Plans one branch successor from the shared exit stack.
    fn branch_edge(
        &mut self,
        sim: &Sim,
        pred: BlockId,
        succ: BlockId,
        then: bool,
    ) -> Result<Edge, Fail> {
        if self.defers(succ) {
            self.defer(pred, succ, &mut sim.fork(), Some(then))?;
            return Ok(Edge { target: succ, trampoline: None });
        }
        if self.layouts[succ].is_some() {
            if self.satisfies(sim, pred, succ) {
                self.note_entry(succ, sim);
                return Ok(Edge { target: succ, trampoline: None });
            }
        } else if let Some(mut view) = self.view(sim, pred, succ) {
            let cut = self.floating_cut(succ, &view);
            view.drain(..cut);
            self.layouts[succ] = Some(view);
            self.note_entry(succ, sim);
            return Ok(Edge { target: succ, trampoline: None });
        }
        let mut trampoline = Sim::with_below(sim.stack.clone(), sim.below);
        self.jump_edge(&mut trampoline, pred, succ)?;
        trampoline.cost += StackCosts::EDGE_JUMP;
        self.charge(&trampoline, pred);
        Ok(Edge { target: succ, trampoline: Some(trampoline.steps) })
    }

    fn plan_branch(
        &mut self,
        sim: &mut Sim,
        block: BlockId,
        condition: ValueId,
        then_block: BlockId,
        else_block: BlockId,
    ) -> Result<Exit, Fail> {
        let live_out = self.liveness.live_out(block);
        for succ in [then_block, else_block] {
            if self.layouts[succ].is_some() {
                self.record_natural(sim, block, succ);
            }
        }
        let then_fixed = self.layouts[then_block].is_some();
        let else_fixed = self.layouts[else_block].is_some();
        // Prefer satisfying a fixed successor directly, in particular a loop header on a
        // conditional backedge, when the other successor's words survive that layout.
        let preferred = match (then_fixed, else_fixed) {
            (true, false) => Some((then_block, else_block)),
            (false, true) => Some((else_block, then_block)),
            (true, true) => {
                if self.weight(then_block) >= self.weight(else_block) {
                    Some((then_block, else_block))
                } else {
                    Some((else_block, then_block))
                }
            }
            (false, false) => None,
        };
        let mut shuffled = false;
        if let Some((fixed, other)) = preferred
            && then_block != else_block
            && !self.floating.contains(fixed)
        {
            let layout = self.layouts[fixed].as_ref().unwrap();
            let mut target = self.edge_target(block, fixed, layout, layout.len());
            let other_needs = self.edge_needs(block, other);
            let survives = other_needs
                .iter()
                .all(|&value| self.is_fresh(value) || target.contains(&Want::Value(value)));
            // The fixed successor's spilled phis store their inputs on its edge.
            let stores_survive = self.phi_inputs(block, fixed).iter().all(|&(result, input)| {
                !self.spilled.contains(result)
                    || self.is_fresh(input)
                    || target.contains(&Want::Value(input))
            });
            if survives
                && stores_survive
                && self.phi_inputs(block, other).iter().all(|&(r, _)| !self.spilled.contains(r))
            {
                target.push(Want::Value(condition));
                let mut candidate = sim.fork();
                if self.shuffle(&mut candidate, &target).is_ok() {
                    sim.join(&mut candidate);
                    shuffled = true;
                }
            }
        }
        if !shuffled {
            self.branch_phi_prep(sim, block, then_block, else_block)?;
            let dying = |value: ValueId| !live_out.contains(value);
            self.prepare(sim, &[condition], &dying)?;
        }
        // jumpi condition
        sim.stack.pop();
        let then_edge = self.branch_edge(sim, block, then_block, true)?;
        let else_edge = if else_block == then_block {
            then_edge.clone()
        } else {
            self.branch_edge(sim, block, else_block, false)?
        };
        sim.cost += self.target.opcode(op::JUMPI).plus(self.target.opcode(op::PUSH2));
        Ok(Exit::Branch { then_edge, else_edge })
    }

    /// Copies the phi inputs of unplanned branch successors that no dead word holds. They sit
    /// below the condition. A successor that runs less often than the other takes its inputs
    /// on its own edge instead, so the hotter path carries no extra words.
    fn branch_phi_prep(
        &self,
        sim: &mut Sim,
        block: BlockId,
        then_block: BlockId,
        else_block: BlockId,
    ) -> Result<(), Fail> {
        for (succ, other) in [(then_block, else_block), (else_block, then_block)] {
            if self.layouts[succ].is_none()
                && self.weight(succ) >= self.weight(other)
                && self.view(sim, block, succ).is_none()
            {
                let live_in = self.liveness.live_in(succ);
                let mut claimed: SmallVec<[usize; 8]> = SmallVec::new();
                for (result, input) in self.phi_inputs(block, succ) {
                    if self.spilled.contains(result) {
                        continue;
                    }
                    let reusable = !self.is_fresh(input)
                        && !live_in.contains(input)
                        && (0..sim.stack.len()).rev().any(|p| {
                            sim.stack[p] == Slot::Value(input) && !claimed.contains(&p) && {
                                claimed.push(p);
                                true
                            }
                        });
                    if !reusable {
                        self.copy(sim, input)?;
                        claimed.push(sim.stack.len() - 1);
                    }
                }
            }
        }
        Ok(())
    }

    /// For a branch whose condition is computed by an instruction of the block for the branch
    /// alone, returns that instruction when unplanned successors take their phi inputs from the
    /// shared stack. Copying those inputs before the condition is computed leaves the condition
    /// on top.
    fn early_branch_phis(&self, block: BlockId) -> Option<(InstId, BlockId, BlockId)> {
        let Some(Terminator::Branch { condition, then_block, else_block }) =
            &self.func.blocks[block].terminator
        else {
            return None;
        };
        let condition = self.conditions.get(&block).map_or(*condition, |&(condition, _)| condition);
        let fixed = |succ: BlockId| self.layouts[succ].is_some() && !self.floating.contains(succ);
        if fixed(*then_block) || fixed(*else_block) || self.use_counts[condition] != 1 {
            return None;
        }
        let Value::Inst(def) = *self.func.value(condition) else { return None };
        let instructions = &self.func.blocks[block].instructions;
        let position = instructions.iter().position(|&inst| inst == def)?;
        // Every phi input must exist before the condition's instruction, including the results
        // a call binds through the instructions after it.
        let later = |value: ValueId| {
            matches!(
                *self.func.value(value),
                Value::Inst(inst) if instructions[position..].contains(&inst)
            )
        };
        if [*then_block, *else_block]
            .into_iter()
            .flat_map(|succ| self.phi_inputs(block, succ))
            .any(|(_, input)| later(input))
        {
            return None;
        }
        Some((def, *then_block, *else_block))
    }

    /// Values a successor reads from the edge, in predecessor terms.
    fn edge_needs(&self, pred: BlockId, succ: BlockId) -> Vec<ValueId> {
        let mut needs: Vec<ValueId> = self.liveness.live_in(succ).iter().collect();
        needs.extend(self.phi_inputs(pred, succ).iter().map(|&(_, input)| input));
        needs
    }

    fn plan_switch(
        &mut self,
        sim: &mut Sim,
        block: BlockId,
        value: ValueId,
        default: BlockId,
        cases: &[(ValueId, BlockId)],
    ) -> Result<Exit, Fail> {
        let live_out = self.liveness.live_out(block);
        let dying = |v: ValueId| !live_out.contains(v);
        let entry_mode = self.func_id == self.info.entry && self.info.emitting_entry;
        let mut targets: SmallVec<[BlockId; 8]> = SmallVec::new();
        targets.push(default);
        for &(_, target) in cases {
            if !targets.contains(&target) {
                targets.push(target);
            }
        }
        self.prepare(sim, &[value], &dying)?;
        sim.observe(SWITCH_DISPATCH_WORDS);
        if entry_mode {
            *sim.stack.last_mut().unwrap() = Slot::Junk;
        } else {
            sim.stack.pop();
        }
        let mut trampolines = Vec::new();
        for target in targets {
            let direct = match &self.layouts[target] {
                Some(_) => self.satisfies(sim, block, target),
                None => match self.view(sim, block, target) {
                    Some(mut view) => {
                        let cut = self.floating_cut(target, &view);
                        view.drain(..cut);
                        self.layouts[target] = Some(view);
                        true
                    }
                    None => false,
                },
            };
            if direct {
                self.note_entry(target, sim);
            } else {
                let mut trampoline = Sim::with_below(sim.stack.clone(), sim.below);
                self.jump_edge(&mut trampoline, block, target)?;
                trampoline.cost += StackCosts::EDGE_JUMP;
                self.charge(&trampoline, block);
                trampolines.push((target, trampoline.steps));
            }
        }
        Ok(Exit::Switch { default, cases: cases.to_vec(), trampolines })
    }
}

/// The plain plan of the next instructions of a block, kept so that the next placement decision
/// in the block can continue it.
struct PlainTrial {
    block: BlockId,
    /// Position of the first instruction in `steps`.
    start: usize,
    /// The stack and cost before that instruction, then after each instruction in `steps`.
    states: Vec<(Layout, Cost)>,
    steps: Vec<PlainStep>,
    /// The state after the last planned instruction.
    sim: Sim,
    uses: Uses,
    /// Whether an instruction could not be planned.
    stopped: bool,
}

/// One instruction of a plain plan.
struct PlainStep {
    /// The cost after the instruction, with the removal of dead words.
    price: Cost,
    /// Whether planning the instruction swapped.
    swapped: bool,
}

/// Uses of values by the unplanned instructions and the terminator of the block being planned.
#[derive(Clone, Default)]
struct Uses {
    remaining: FxHashMap<ValueId, u32>,
}

impl Uses {
    fn count(&self, value: ValueId) -> u32 {
        self.remaining.get(&value).copied().unwrap_or(0)
    }

    fn consume(&mut self, operands: &[ValueId]) {
        for operand in operands {
            if let Some(count) = self.remaining.get_mut(operand) {
                *count -= 1;
            }
        }
    }

    /// Undoes `consume`.
    fn restore(&mut self, operands: &[ValueId]) {
        for operand in operands {
            if let Some(count) = self.remaining.get_mut(operand) {
                *count += 1;
            }
        }
    }
}

/// An edge into a join, recorded until the join's layout is chosen.
struct Pending {
    pred: BlockId,
    /// The predecessor's stack on the edge.
    stack: Layout,
    /// For a branch, whether this is the taken (`then`) edge.
    side: Option<bool>,
}

/// The stack an internal function is entered or left with, bottom to top: `values` with the
/// first on top, then the return address.
fn with_return_address(values: &[ValueId]) -> Vec<Want> {
    values.iter().rev().map(|&value| Want::Value(value)).chain([Want::Ret]).collect()
}

/// Which read of a word places it in an `ordered_layout`.
#[derive(Clone, Copy, PartialEq)]
enum Read {
    /// The first read, counting each operand.
    First,
    /// The last read, counting each instruction.
    Last,
}

/// Orders `words` by their `read` in `blocks`, the return address deepest when `ret`: the word read
/// first sits on top, and words never read sit deepest.
fn ordered_layout(
    func: &Function,
    blocks: impl IntoIterator<Item = BlockId>,
    mut words: Vec<ValueId>,
    ret: bool,
    read: Read,
) -> Layout {
    let mut order = FxHashMap::<ValueId, usize>::default();
    let mut position = 0usize;
    let mut note = |operands: &[ValueId]| {
        let start = position;
        for &operand in operands {
            match read {
                Read::First => {
                    order.entry(operand).or_insert(position);
                    position += 1;
                }
                Read::Last => {
                    order.insert(operand, start);
                }
            }
        }
        if read == Read::Last {
            position += 1;
        }
    };
    for block in blocks {
        for &inst in &func.blocks[block].instructions {
            if !matches!(func.inst(inst).kind, InstKind::Phi(_)) {
                note(&func.inst(inst).kind.operands());
            }
        }
        if let Some(term) = &func.blocks[block].terminator {
            note(&term.operands());
        }
    }
    words.sort_by_key(|value| std::cmp::Reverse(order.get(value).copied().unwrap_or(usize::MAX)));
    let mut layout = Layout::with_capacity(words.len() + 1);
    if ret {
        layout.push(Slot::Ret);
    }
    layout.extend(words.into_iter().map(Slot::Value));
    layout
}

/// The multi-return protocol after one call.
pub(super) struct Projection {
    /// Protocol instructions to elide.
    pub(super) elided: Vec<InstId>,
    /// The value reading each extra result, if any.
    pub(super) extras: SmallVec<[Option<ValueId>; 4]>,
    /// The pointer and address values, which must have no other consumers.
    tracked: FxHashSet<ValueId>,
}

/// Binds the extra results of a multi-word call to the words it returns on the stack.
///
/// MIR reads results `1..N` through the multi-return buffer: the pointer read follows the call
/// with only pure instructions between, and one offset load per used result follows it with
/// only read-only instructions between. The buffer is reachable only through the pointer read,
/// whose other uses reject the projection.
pub(super) fn call_projection(
    func: &Function,
    block: BlockId,
    call_idx: usize,
    arity: usize,
) -> Option<Projection> {
    let tail = &func.blocks[block].instructions[call_idx + 1..];
    let mut extras: SmallVec<[Option<ValueId>; 4]> = smallvec![None; arity - 1];
    let mut elided = Vec::new();
    let mut tracked = FxHashSet::default();
    let mut base = None;
    for (offset, &inst) in tail.iter().enumerate() {
        let kind = &func.inst(inst).kind;
        if let InstKind::Load(addr, PtrLocation::Memory) = *kind
            && func.value_u64(addr) == Some(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT)
        {
            base = Some((offset, inst));
            break;
        }
        if kind.effect_kind() != EffectKind::Pure {
            break;
        }
    }
    let Some((base_offset, base_inst)) = base else {
        // No result beyond the first is read before memory may change.
        return Some(Projection { elided, extras, tracked });
    };
    let base_value = func.inst_result_value(base_inst)?;
    elided.push(base_inst);
    tracked.insert(base_value);
    let mut addresses = FxHashMap::default();
    for &inst in &tail[base_offset + 1..] {
        match &func.inst(inst).kind {
            InstKind::Add(a, b) if *a == base_value || *b == base_value => {
                let imm = if *a == base_value { *b } else { *a };
                let index = func
                    .value_u64(imm)
                    .filter(|offset| offset % EvmMemoryLayout::WORD_SIZE == 0)
                    .map(|offset| (offset / EvmMemoryLayout::WORD_SIZE) as usize)?;
                let address = func.inst_result_value(inst)?;
                if !(1..arity).contains(&index) || addresses.insert(address, index).is_some() {
                    return None;
                }
                tracked.insert(address);
                elided.push(inst);
            }
            InstKind::Load(addr, PtrLocation::Memory) if addresses.contains_key(addr) => {
                let result = func.inst_result_value(inst)?;
                if extras[addresses[addr] - 1].replace(result).is_some() {
                    return None;
                }
                elided.push(inst);
            }
            kind if kind.effect_kind().is_read_only() => {}
            _ => break,
        }
    }
    Some(Projection { elided, extras, tracked })
}

/// Values computed by stable single-opcode arithmetic, calldata loads, and casts from words
/// pushed fresh at no risk of change: immediates, calldata arguments, stable environment reads,
/// and data sizes. Maps each to the cost and opcode count of recomputing it, at most
/// [`MAX_RECOMPUTED_OPS`] opcodes.
pub(super) fn recomputable(
    func: &Function,
    cfg: &CfgInfo,
    internal: bool,
    target: &Target,
) -> FxHashMap<ValueId, (Cost, u32)> {
    let mut values = FxHashMap::default();
    // Operands are defined before their users in reverse postorder; phis never qualify.
    for &block in cfg.rpo() {
        for &inst in &func.blocks[block].instructions {
            let kind = &func.inst(inst).kind;
            let Some(result) = func.inst_result_value(inst) else { continue };
            let (mut cost, mut ops) = match kind {
                InstKind::Zext(_) | InstKind::PtrToInt(..) | InstKind::IntToPtr(_) => {
                    (Cost::ZERO, 0)
                }
                InstKind::Load(_, PtrLocation::Calldata) => (target.opcode(op::CALLDATALOAD), 1),
                _ => {
                    let def = kind.op_def();
                    if !def.traits.contains(OpTraits::REMATERIALIZABLE)
                        || def.effect != EffectKind::Pure
                    {
                        continue;
                    }
                    let Some(opcode) = kind.evm_opcode() else { continue };
                    (target.opcode(opcode), 1)
                }
            };
            let computable = kind.operands().iter().all(|&operand| {
                if materialized_at_use(func, internal, operand) {
                    cost += materialize_cost(func, target, operand);
                } else if let Some(&(operand_cost, operand_ops)) = values.get(&operand) {
                    cost += operand_cost;
                    ops += operand_ops;
                } else {
                    return false;
                }
                true
            });
            if computable && ops <= MAX_RECOMPUTED_OPS {
                values.insert(result, (cost, ops));
            }
        }
    }
    values
}

/// The cost of pushing a value that is materialized at its use.
fn materialize_cost(func: &Function, target: &Target, value: ValueId) -> Cost {
    match func.value(value) {
        Value::Immediate(imm) => target.push(imm.as_u256().unwrap_or_default()),
        Value::Undef(_) => target.push(U256::ZERO),
        Value::Arg(_) => target.opcode(op::PUSH1).plus(target.opcode(op::CALLDATALOAD)),
        Value::Inst(inst) => match &func.inst(*inst).kind {
            InstKind::DataSize(_) => target.opcode(op::PUSH2),
            InstKind::Gas => target.opcode(op::GAS),
            InstKind::Sub(..) if gas_minus(func, value).is_some() => {
                target.opcode(op::GAS).plus(target.opcode(op::PUSH1)).plus(target.opcode(op::SUB))
            }
            _ => rematerializable_nullary_value(func, value)
                .map_or(StackCosts::NULLARY_READ, |opcode| target.opcode(opcode)),
        },
        _ => StackCosts::NULLARY_READ,
    }
}

/// Returns the nearest block that dominates both blocks.
fn common_dominator(cfg: &CfgInfo, a: BlockId, b: BlockId) -> BlockId {
    let ancestors = cfg.dominators().self_and_dominators(a);
    let mut block = b;
    loop {
        if ancestors.contains(&block) {
            return block;
        }
        match cfg.dominators().idom(block) {
            Some(idom) if idom != block => block = idom,
            _ => return BlockId::ENTRY,
        }
    }
}

/// Counts the copies of dying operands left in `below`, each removed eventually.
fn leftovers(below: &[Slot], dying: &[ValueId]) -> u32 {
    if dying.is_empty() {
        return 0;
    }
    if let &[only] = dying {
        return below.iter().filter(|&&slot| slot == Slot::Value(only)).count() as u32;
    }
    below.iter().filter(|slot| matches!(slot, Slot::Value(value) if dying.contains(value))).count()
        as u32
}

/// The liveness of a function when each constant of `definitions` is kept on the stack from its
/// defining block through the blocks that block dominates: the constant is live where a path
/// inside that region reaches one of its uses, and nowhere else. A defining block is entered
/// without it.
pub(super) fn retention_liveness(
    func: &Function,
    cfg: &CfgInfo,
    liveness: &Liveness,
    definitions: &[(BlockId, ValueId)],
) -> Liveness {
    let mut liveness = liveness.clone();
    for (&value, held) in &held_blocks(func, cfg, definitions) {
        let mut used = DenseBitSet::new_empty(func.blocks.len());
        let mut edge_used = DenseBitSet::new_empty(func.blocks.len());
        for (block, data) in func.blocks.iter_enumerated() {
            for &inst in &data.instructions {
                match &func.inst(inst).kind {
                    InstKind::Phi(incoming) => {
                        for &(pred, operand) in incoming {
                            if operand == value {
                                edge_used.insert(pred);
                            }
                        }
                    }
                    kind if kind.operands().contains(&value) => {
                        used.insert(block);
                    }
                    _ => {}
                }
            }
            if data.terminator.as_ref().is_some_and(|term| term.operands().contains(&value)) {
                used.insert(block);
            }
        }
        let mut live_in = DenseBitSet::new_empty(func.blocks.len());
        let mut live_out = DenseBitSet::new_empty(func.blocks.len());
        let mut changed = true;
        while changed {
            changed = false;
            for &block in cfg.rpo().iter().rev() {
                if !held.contains(block) {
                    continue;
                }
                let mut out = edge_used.contains(block);
                if let Some(term) = &func.blocks[block].terminator {
                    term.for_each_successor(|succ| out |= live_in.contains(succ));
                }
                changed |= out && live_out.insert(block);
                if (used.contains(block) || out) && !definitions.contains(&(block, value)) {
                    changed |= live_in.insert(block);
                }
            }
        }
        liveness.set_value_live(value, &live_in, &live_out);
    }
    liveness
}

/// The blocks in which each constant of `definitions` is kept on the stack: those dominated by a
/// block that defines it.
fn held_blocks(
    func: &Function,
    cfg: &CfgInfo,
    definitions: &[(BlockId, ValueId)],
) -> FxHashMap<ValueId, DenseBitSet<BlockId>> {
    let mut held = FxHashMap::<_, DenseBitSet<BlockId>>::default();
    for &(def, value) in definitions {
        let blocks = held.entry(value).or_insert_with(|| DenseBitSet::new_empty(func.blocks.len()));
        let mut pending = vec![def];
        while let Some(block) = pending.pop() {
            if blocks.insert(block) {
                pending.extend_from_slice(cfg.dominators().children(block));
            }
        }
    }
    held
}

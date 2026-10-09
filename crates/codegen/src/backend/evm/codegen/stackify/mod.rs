//! Stack-resident lowering of runtime and constructor code.
//!
//! This lowering keeps every MIR value on the EVM stack for its whole lifetime, across blocks
//! and internal calls, and uses memory only for values that would otherwise fall out of `DUP`
//! and `SWAP` reach. It plans the runtime, and separately the constructor with the helpers it
//! calls, at every optimization level. A shape it does not implement is a codegen error.
//!
//! ## Calling convention
//!
//! An internal function is entered with its arguments and then the caller's return address on
//! the stack, `[arg(n-1), ..., arg0, return]` from bottom to top. The return address is an
//! ordinary modeled word ([`Slot::Ret`]) that the function carries until it returns. A return
//! arranges `[result(m-1), ..., result0, return]` and jumps; the caller resumes with the results
//! in place of its arguments. Words below the arguments belong to the caller, so a value live
//! across a call simply stays where it is. A call whose results the caller returns unchanged
//! becomes a tail call: the callee receives the arguments and the inherited return address.
//!
//! A function that never returns and is only entered through `tail_call`, such as a shared
//! revert helper, takes no return address. External entries and the dispatch entry have none
//! either and terminate the message call; their words below a tail call's arguments are never
//! read again.
//!
//! External functions read their arguments from calldata. An argument read once is loaded at
//! its use. One read more often, or inside a loop in gas mode, is loaded once in the nearest
//! block outside every loop that dominates its reads and then stays on the stack.
//!
//! A caller that reads only some words of a multi-word result drops the others as junk. Where
//! code still reads results through the callee's memory return area, the caller stores the
//! extra words there after the call.
//!
//! The constructor reads its arguments from the copy of the ABI blob that the deployment prefix
//! places in memory, and its ordinary completion jumps to the deployment postlude.
//!
//! ## Planning
//!
//! Planning is a pure pass over one function that records the emitted operations of each block
//! as [`Step`]s together with the block's entry layout, and prices them with the target cost
//! model. Blocks are visited in reverse postorder. A block with one forward predecessor takes
//! that predecessor's stack, with words dead in the block marked as junk and phi inputs renamed
//! to their results. A join waits until all its forward predecessors are planned and then takes
//! the candidate layout that minimizes the weighted cost of every incoming shuffle. Acyclic blocks
//! that never return keep a floating layout: they name only the top of the stack and ignore the
//! words below, so predecessors with different depths share them without a shuffle.
//!
//! A conditional branch shares one physical stack between its successors. A successor whose
//! fixed layout that stack does not satisfy receives a trampoline carrying the shuffle. Before
//! the branch, the planner copies the phi inputs of a successor that runs at least as often as
//! the other, so the colder edge pays for its own inputs. A branch on `eq x, 0` or `ne x, 0`
//! branches on `x` with the targets swapped.
//!
//! The planner prepares instruction operands by duplicating values that remain live, consuming
//! values at their last use in place, materializing immediates and stable reads, and then moving
//! each operand to its position with at most two swaps, or one `EXCHANGE` where the target makes
//! that cheaper, or by copying operands on top of a prefix already in place; the cheaper strategy
//! wins.
//! A shuffle's cycle through the top that leaves the top in place, `SWAPa SWAPb ... SWAPa`,
//! becomes one `EXCHANGE a, b` per inner swap when that is cheaper. Commutative and mirrored
//! comparisons try both operand orders. Two short trials refine these local choices: before a chain
//! of single-use instructions that feeds a consumer, the consumer's deeper operands may be copied
//! first, and after an instruction, its result may be swapped into the slot of a word that dies
//! within the next few instructions. Each is kept only when planning the affected instructions gets
//! cheaper.
//!
//! Loop headers are first laid out by their preheader. Later planning rounds lay each header
//! out by the order of last use in the loop, and then in the order the best plan's latches
//! naturally leave the carried words; the cheapest plan wins under the objective, weighting loop
//! blocks by their nesting depth in gas mode.
//!
//! When a needed word lies beyond the target's reach, the value is spilled: it is stored once
//! when defined, reloaded at every use, and planning restarts. After many restarts, every other
//! value beyond reach at the failure spills with it, in growing batches. When no value can make
//! room, the return address moves to a slot on entry and is reloaded to return, and a shuffle
//! that still cannot move a deep word instead pops down to the words already in place and pushes
//! the rest of its target. A join that no incoming edge can propose a layout for takes its live
//! words by value. Spill slots use the frame and spill-area placement in `frames`. A write that
//! may reach the low spill area, such as a copy to a computed address, and a call into a function
//! that makes one keep the spilled words still needed afterwards, the return address among them,
//! on the stack across them and store them back after. A value live across such a write is
//! recomputed at each use instead of spilled when stable arithmetic over calldata, constants and
//! stable reads can rebuild it.
//!
//! A recursive function spills to a fixed frame like any other. A call that can start another
//! activation of it, one into its own recursive component, would overwrite that frame, so the
//! caller reloads the spilled words it still needs below the call's arguments and stores them
//! back once the call returns.
//!
//! The non-recursive call graph must fit the 1024-word EVM stack. When its deepest chain does
//! not, the callers on that chain plan again with every value live across a call spilled, and
//! then with their return addresses in memory too, until it fits.
//!
//! The plan of a function is complete before any code is emitted, so a failure leaves no partial
//! output. Emission follows the block order of `function`. Blocks that only jump on are
//! skipped, and a branch whose successors both need a jump falls through into a trampoline.

use super::{
    BlockId, CallGraphInfo, CfgInfo, DenseBitSet, EvmCodegen, Function, FunctionId, FxHashMap,
    FxHashSet, IndexVec, InstId, InstKind, Liveness, LoopAnalyzer, MAX_STACK_DEPTH, Module,
    StackOp, Terminator, Value, ValueId, index_vec, select::rematerializable_nullary_value,
};
use crate::{
    mir::{ArgIdx, Callee, analysis::LoopInfo},
    target::{Cost, Target},
};
use plan::{Planned, Planner};
use smallvec::SmallVec;

mod emit;
mod plan;
mod shuffle;

/// One word of the modeled stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Slot {
    /// A MIR value.
    Value(ValueId),
    /// The caller's return address, carried by every internal function.
    Ret,
    /// A word no later code reads.
    Junk,
    /// A spilled value's word held across a call that may overwrite its slot, stored back
    /// after the call.
    Saved(ValueId),
}

/// A requirement on one word of a target layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Want {
    Value(ValueId),
    Ret,
    Any,
}

impl Want {
    fn accepts(self, slot: Slot) -> bool {
        match self {
            Self::Any => true,
            Self::Ret => slot == Slot::Ret,
            Self::Value(value) => slot == Slot::Value(value),
        }
    }
}

/// One emitted operation of a block plan.
#[derive(Clone, Debug)]
enum Step {
    /// A physical stack operation.
    Stack(StackOp),
    /// Pushes a fresh copy of a value that is materialized at each use.
    Materialize(ValueId),
    /// Pushes a spilled value from its slot.
    Reload(ValueId),
    /// Stores the top word into a spilled value's slot.
    Spill(ValueId),
    /// Stores the return address on top into the function's return slot.
    SpillRet,
    /// Pushes the return address from the function's return slot.
    ReloadRet,
    /// Pushes a filler word for a layout position that nothing reads.
    Filler,
    /// Starts the code of an instruction, for debug metadata.
    Begin(InstId),
    /// Emits one opcode of an instruction whose operands are prepared.
    Op(u8),
    /// Emits the remaining code of an instruction whose operands are prepared.
    Inst(InstId),
    /// Calls an internal function whose arguments are prepared, with the height of the stack
    /// below its arguments.
    Call(FunctionId, usize),
    /// Stores the extra results below the first result of a call to the callee's return area
    /// and publishes it as the multi-return buffer.
    Publish { callee: FunctionId, arity: usize, params: usize },
}

/// A control-flow edge, optionally through a trampoline that rearranges the stack.
#[derive(Clone, Debug)]
struct Edge {
    target: BlockId,
    trampoline: Option<Vec<Step>>,
}

/// How a block leaves.
#[derive(Clone, Debug)]
enum Exit {
    /// Jumps to a block whose layout the stack already has.
    Jump(BlockId),
    /// Branches on the top word.
    Branch { then_edge: Edge, else_edge: Edge },
    /// Dispatches on the top word through the switch emitter. Targets with trampolines are
    /// entered through them.
    Switch {
        default: BlockId,
        cases: Vec<(ValueId, BlockId)>,
        trampolines: Vec<(BlockId, Vec<Step>)>,
    },
    /// Returns to the caller; the stack holds the results and the return address.
    Return,
    /// Transfers control to a function whose entry tuple is prepared.
    TailCall(FunctionId),
    /// Ends the message call through the block's own terminator.
    Terminal,
}

#[derive(Clone, Debug)]
struct BlockPlan {
    steps: Vec<Step>,
    /// Index of the first step planned for the terminator.
    terminator_start: usize,
    exit: Exit,
}

/// A word that can leave the stack for memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Victim {
    Value(ValueId),
    /// The return address, kept in the function's return slot.
    Ret,
}

impl Victim {
    /// The stack word that holds this one while it rides the stack across an instruction that
    /// overwrites its slot.
    fn saved_slot(self) -> Slot {
        match self {
            Self::Value(value) => Slot::Saved(value),
            Self::Ret => Slot::Ret,
        }
    }
}

/// Why a function could not be planned.
#[derive(Clone, Debug)]
enum Fail {
    /// A word is beyond reach. Spilling the victim, when given, may make the function
    /// plannable. Once spills come in batches, the failure also lists the other spillable
    /// values beyond reach at that point, deepest first.
    Deep(Option<Victim>, SmallVec<[ValueId; 8]>),
    /// The function uses a shape this lowering does not implement.
    Unsupported(&'static str),
}

/// A complete function plan.
struct FunctionPlan {
    blocks: IndexVec<BlockId, Option<BlockPlan>>,
    spilled: DenseBitSet<ValueId>,
    /// Whether the return address lives in a memory slot instead of on the stack.
    ret_spilled: bool,
    /// Highest modeled stack height above the function's base.
    peak: usize,
    /// Internal calls and the modeled height below each callee's arguments.
    calls: Vec<(FunctionId, usize)>,
    cost: Cost,
}

/// Module-wide facts shared by every function plan.
struct ModuleInfo {
    /// Functions entered from internal calls, with their arguments on the stack.
    internal: DenseBitSet<FunctionId>,
    /// Internal functions entered with a return address. The others never return and are only
    /// entered through tail calls.
    returning: DenseBitSet<FunctionId>,
    /// The recursive component of each recursive function, by its smallest member. A call
    /// within a component can start another activation of the caller, which reuses the
    /// caller's fixed frame.
    components: IndexVec<FunctionId, Option<FunctionId>>,
    /// Physical result words of each function.
    returns: IndexVec<FunctionId, usize>,
    /// Parameters of each function.
    params: IndexVec<FunctionId, usize>,
    entry: FunctionId,
    /// Whether `entry` is the runtime dispatch entry, whose switch leaves its selector on the
    /// stack.
    emitting_entry: bool,
}

/// Facts about one function that every planning attempt shares.
struct Analysis {
    liveness: Liveness,
    cfg: CfgInfo,
    loops: LoopInfo,
    /// The instructions that overwrite this function's fixed frame, with the values live across
    /// each: writes that may reach the spill area, calls into functions that make one, and calls
    /// into the function's own recursive component. The spilled words still needed afterwards
    /// ride the stack across them.
    saved: FxHashMap<InstId, DenseBitSet<ValueId>>,
    /// Values live across a write that may reach the spill area or a call into a function that
    /// makes one.
    pinned: DenseBitSet<ValueId>,
    /// Values that stable arithmetic recomputes from fresh words, with the cost and number of
    /// opcodes of doing so. A pinned one of them leaves the stack this way instead of through
    /// memory.
    recomputable: FxHashMap<ValueId, (Cost, u32)>,
}

/// What a function moves to memory to shorten the stack it keeps below its calls, raised for
/// the callers on a call chain deeper than the EVM stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CallSpill {
    None,
    /// Every value live across an internal call.
    Values,
    /// Those values and the return address.
    ValuesAndReturn,
}

/// Why an artifact could not be lowered.
enum Decline {
    /// The non-recursive call graph cannot fit the EVM stack.
    StackLimit,
    /// The function uses a shape this lowering does not implement.
    Unsupported(FunctionId, &'static str),
}

/// Planning restarts that spill one value each before spills come in batches.
const SINGLE_SPILL_RESTARTS: u32 = 64;

/// Maximum number of planning rounds that refine loop header layouts.
const LAYOUT_ROUNDS: usize = 4;

impl<'gcx> EvmCodegen<'gcx> {
    /// Emits the runtime: the dispatch entry, the external entries, and the internal functions
    /// they call.
    pub(super) fn emit_runtime(&mut self, module: &Module, call_graph: &CallGraphInfo) {
        let Some(entry) = module.dispatch_entry() else {
            assert!(
                !module.functions.iter().any(Self::is_external_entry),
                "evm-shaped module with a runtime interface must have a MIR `entry` function"
            );
            return;
        };
        let internal_targets = Self::internal_call_targets(module, call_graph, entry);
        let emitting_entry = Liveness::is_block_local(&module.functions[entry]);
        let bodies = module
            .functions
            .iter_enumerated()
            .filter(|&(func_id, func)| {
                Self::is_runtime_function(func)
                    && (func_id == entry
                        || Self::is_external_entry(func)
                        || internal_targets.contains(func_id))
            })
            .map(|(func_id, _)| func_id)
            .collect();
        match self.plan_stackified(module, call_graph, entry, emitting_entry, bodies) {
            Ok((info, plans)) => self.emit_runtime_stackified(module, call_graph, &info, plans),
            Err(decline) => self.report_decline(module, decline),
        }
    }

    /// Emits the constructor and the helpers it calls.
    pub(super) fn emit_constructor(
        &mut self,
        module: &Module,
        call_graph: &CallGraphInfo,
        constructor: FunctionId,
        internal_targets: &DenseBitSet<FunctionId>,
    ) {
        let bodies = std::iter::once(constructor).chain(internal_targets.iter()).collect();
        match self.plan_stackified(module, call_graph, constructor, false, bodies) {
            Ok((info, plans)) => self.emit_constructor_stackified(module, call_graph, &info, plans),
            Err(decline) => self.report_decline(module, decline),
        }
    }

    fn report_decline(&self, module: &Module, decline: Decline) {
        match decline {
            Decline::StackLimit => self.report_stack_limit_error(),
            Decline::Unsupported(func, reason) => {
                let func = &module.functions[func];
                self.gcx
                    .dcx()
                    .err(format!("codegen cannot lower `{}`: {reason}", func.name))
                    .span(func.name_span)
                    .emit();
            }
        }
    }

    /// Plans `bodies`: `entry` and the external entries, which terminate the message call,
    /// and the internal functions they call.
    fn plan_stackified(
        &mut self,
        module: &Module,
        call_graph: &CallGraphInfo,
        entry: FunctionId,
        emitting_entry: bool,
        bodies: Vec<FunctionId>,
    ) -> Result<(ModuleInfo, IndexVec<FunctionId, Option<FunctionPlan>>), Decline> {
        let functions = module.functions.len();
        let mut internal = DenseBitSet::new_empty(functions);
        for &func_id in &bodies {
            let func = &module.functions[func_id];
            if func_id != entry && !Self::is_external_entry(func) {
                internal.insert(func_id);
                if call_graph.is_recursive(func_id) {
                    // A recursive activation cannot own a fixed frame.
                    if func
                        .instructions()
                        .any(|inst| matches!(func.inst(inst).kind, InstKind::InternalFrameAddr(_)))
                    {
                        return Err(Decline::Unsupported(
                            func_id,
                            "recursive function with a frame object",
                        ));
                    }
                } else if !Self::static_frame_offsets_are_local(func) {
                    return Err(Decline::Unsupported(
                        func_id,
                        "frame address outside the local region",
                    ));
                }
            }
        }

        // A function that never reaches a return and is entered only through tail calls, here
        // or in the functions it tail-calls, needs no return address.
        let mut returning = DenseBitSet::new_empty(functions);
        for &func_id in &bodies {
            let func = &module.functions[func_id];
            for inst in func.instructions() {
                if let Some((callee, _)) = icall(&func.inst(inst).kind) {
                    returning.insert(callee);
                }
            }
            if internal.contains(func_id)
                && func
                    .blocks
                    .iter()
                    .any(|block| matches!(block.terminator, Some(Terminator::Return { .. })))
            {
                returning.insert(func_id);
            }
        }
        loop {
            let mut changed = false;
            for &func_id in &bodies {
                if internal.contains(func_id)
                    && !returning.contains(func_id)
                    && module.functions[func_id].blocks.iter().any(|block| {
                        matches!(
                            &block.terminator,
                            Some(Terminator::TailCall { function, .. }) if returning.contains(*function)
                        )
                    })
                {
                    returning.insert(func_id);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        returning.intersect(&internal);

        // Recursive components, numbered by their smallest member. A component's members are
        // all bodies, since bodies are closed under calls.
        let mut components = index_vec![None; functions];
        for &func_id in &bodies {
            if components[func_id].is_none() && call_graph.is_recursive(func_id) {
                for member in call_graph.recursive_component(func_id).iter() {
                    components[member] = Some(func_id);
                }
            }
        }
        let info = ModuleInfo {
            internal,
            returning,
            components,
            returns: module.functions.iter().map(|func| func.return_components().len()).collect(),
            params: module.functions.iter().map(|func| func.params.len()).collect(),
            entry,
            emitting_entry,
        };

        // Writes that may reach the spill area. A call into a function that may make one, itself
        // or through its callees, and then return makes one in the caller too; a write on a path
        // that never returns, such as a revert forwarder's, cannot reach the caller.
        let mut hazards = (0..functions)
            .map(|_| FxHashSet::default())
            .collect::<IndexVec<FunctionId, FxHashSet<InstId>>>();
        // For each function that returns: whether it writes on a path that returns, and the
        // functions it calls or tail-calls on such paths.
        let mut summaries =
            IndexVec::<FunctionId, Option<(bool, SmallVec<[FunctionId; 4]>)>>::new();
        summaries.resize(functions, None);
        for &func_id in &bodies {
            let func = &module.functions[func_id];
            hazards[func_id] = self.compute_spill_hazard_insts(func_id, func);
            if !info.returning.contains(func_id) {
                continue;
            }
            let mut writes = false;
            let mut callees = SmallVec::new();
            for block in returning_blocks(func, &info.returning).iter() {
                let data = &func.blocks[block];
                for &inst in &data.instructions {
                    writes |= hazards[func_id].contains(&inst);
                    callees.extend(icall(&func.inst(inst).kind).map(|(callee, _)| callee));
                }
                if let Some(Terminator::TailCall { function, .. }) = data.terminator {
                    callees.push(function);
                }
            }
            summaries[func_id] = Some((writes, callees));
        }
        let mut writers = DenseBitSet::new_empty(functions);
        loop {
            let mut changed = false;
            for (func_id, summary) in summaries.iter_enumerated() {
                if let Some((writes, callees)) = summary
                    && !writers.contains(func_id)
                    && (*writes || callees.iter().any(|&callee| writers.contains(callee)))
                {
                    writers.insert(func_id);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let mut analyses =
            (0..functions).map(|_| None).collect::<IndexVec<FunctionId, Option<Analysis>>>();
        let mut plans =
            (0..functions).map(|_| None).collect::<IndexVec<FunctionId, Option<FunctionPlan>>>();
        for &func_id in &bodies {
            let analysis = analyses[func_id].insert(self.analyze(
                module,
                &info,
                func_id,
                &hazards[func_id],
                &writers,
            ));
            plans[func_id] = Some(self.plan_function_stackified(
                module,
                &info,
                func_id,
                analysis,
                CallSpill::None,
            )?);
        }

        // A call chain deeper than the EVM stack: its callers move words to memory, values
        // live across calls first and then return addresses, until it fits.
        let mut call_spills = index_vec![CallSpill::None; functions];
        while let Some(chain) = Self::overflowing_chain(module, call_graph, &plans) {
            let Some(level) = chain.iter().map(|&func_id| call_spills[func_id]).min() else {
                return Err(Decline::StackLimit);
            };
            let next = match level {
                CallSpill::None => CallSpill::Values,
                CallSpill::Values => CallSpill::ValuesAndReturn,
                CallSpill::ValuesAndReturn => return Err(Decline::StackLimit),
            };
            tracing::debug!(
                module = %module.name,
                callers = chain.len(),
                ?next,
                "call chain deeper than the EVM stack"
            );
            for &func_id in &chain {
                if call_spills[func_id] == level {
                    call_spills[func_id] = next;
                    let analysis = analyses[func_id].as_ref().expect("planned body");
                    plans[func_id] = Some(
                        self.plan_function_stackified(module, &info, func_id, analysis, next)?,
                    );
                }
            }
        }
        Ok((info, plans))
    }

    /// Computes the facts every planning attempt of a function shares. `hazards` are its writes
    /// that may reach the spill area, and `writers` the functions that may make one and return.
    fn analyze(
        &self,
        module: &Module,
        info: &ModuleInfo,
        func_id: FunctionId,
        hazards: &FxHashSet<InstId>,
        writers: &DenseBitSet<FunctionId>,
    ) -> Analysis {
        let func = &module.functions[func_id];
        let liveness = Liveness::compute_live_sets(func);
        let cfg = CfgInfo::new(func);
        let loops = LoopAnalyzer::new().analyze_structure(func);
        // Spill slots live in low memory, which these writes may overwrite. A call's results
        // appear after the callee's writes.
        let hazard_call = |inst: InstId| {
            icall(&func.inst(inst).kind).is_some_and(|(callee, _)| writers.contains(callee))
        };
        let clobber = |inst: InstId| hazards.contains(&inst) || hazard_call(inst);
        // Another activation of this function reuses its fixed frame.
        let component = info.components[func_id];
        let recursive_call = |inst: InstId| {
            component.is_some()
                && icall(&func.inst(inst).kind)
                    .is_some_and(|(callee, _)| info.components[callee] == component)
        };
        let saved = live_across_each(func, &liveness, |inst| clobber(inst) || recursive_call(inst));
        let mut pinned = DenseBitSet::new_empty(func.num_values());
        for (&inst, across) in &saved {
            if clobber(inst) {
                pinned.union(across);
            }
        }
        let recomputable = if pinned.is_empty() {
            FxHashMap::default()
        } else {
            plan::recomputable(func, &cfg, info.internal.contains(func_id), Target::new(self.gcx))
        };
        Analysis { liveness, cfg, loops, saved, pinned, recomputable }
    }

    fn plan_function_stackified(
        &self,
        module: &Module,
        info: &ModuleInfo,
        func_id: FunctionId,
        analysis: &Analysis,
        call_spill: CallSpill,
    ) -> Result<FunctionPlan, Decline> {
        let func = &module.functions[func_id];
        let internal = info.internal.contains(func_id);
        let target = Target::new(self.gcx);
        let mut spilled = DenseBitSet::new_empty(func.num_values());
        if call_spill >= CallSpill::Values {
            // The stack keeps spilled words across a call that overwrites their slots anyway.
            let across = live_after(func, &analysis.liveness, |inst| {
                icall(&func.inst(inst).kind).is_some() && !analysis.saved.contains_key(&inst)
            });
            for value in across.iter() {
                if !materialized_at_use(func, internal, value) {
                    spilled.insert(value);
                }
            }
        }
        let mut ret_spilled = call_spill == CallSpill::ValuesAndReturn;
        let mut restarts = 0;
        // More live words than reach and the single spills can hold: spill in batches from the
        // start.
        let reach = self.gcx.sess.opts.evm_version.reachable_stack_depth();
        let single_spills = if stack_pressure(func, internal, &analysis.liveness)
            > reach + SINGLE_SPILL_RESTARTS as usize
        {
            0
        } else {
            SINGLE_SPILL_RESTARTS
        };
        let mut best: Option<FunctionPlan> = None;
        // Candidate loop header layouts: as the preheader leaves its stack, by first use in the
        // loop, and then repeatedly as the best plan's latches leave their stacks.
        let mut candidates = vec![FxHashMap::default()];
        let mut round = 0;
        let mut tried = Vec::new();
        while let Some(hints) = candidates.pop() {
            round += 1;
            // The same hints give the same plan.
            if tried.contains(&hints) {
                continue;
            }
            tried.push(hints.clone());
            let Planned { plan, mut natural, first_use } = loop {
                let planner = Planner::new(
                    module,
                    info,
                    func_id,
                    analysis,
                    target,
                    &spilled,
                    ret_spilled,
                    restarts >= single_spills,
                    &hints,
                )
                .map_err(|reason| Decline::Unsupported(func_id, reason))?;
                match planner.plan() {
                    Ok(planned) => break planned,
                    Err(Fail::Deep(victim, beyond)) => {
                        restarts += 1;
                        match victim {
                            Some(Victim::Value(value)) if !spilled.contains(value) => {
                                spilled.insert(value);
                                // Each restart replans the whole function; after a few, spill
                                // a growing batch of the other words beyond reach too.
                                if restarts > single_spills {
                                    let batch = 1 << (restarts - single_spills).min(6);
                                    for value in beyond.into_iter().take(batch) {
                                        spilled.insert(value);
                                    }
                                }
                            }
                            Some(Victim::Ret) if !ret_spilled => ret_spilled = true,
                            _ => return Err(Decline::Unsupported(func_id, "stack too deep")),
                        }
                    }
                    Err(Fail::Unsupported(reason)) => {
                        return Err(Decline::Unsupported(func_id, reason));
                    }
                }
            };
            if round == 1 {
                if natural.is_empty() {
                    best = Some(plan);
                    break;
                }
                candidates.push(first_use);
            }
            for (block, layout) in &hints {
                natural.entry(*block).or_insert_with(|| layout.clone());
            }
            if best.as_ref().is_none_or(|best| target.cmp(plan.cost, best.cost).is_lt()) {
                best = Some(plan);
                // Lay each loop header out as this plan's latch left it.
                if natural != hints && round < LAYOUT_ROUNDS {
                    candidates.insert(0, natural);
                }
            }
        }
        Ok(best.expect("at least one plan"))
    }

    /// The callers along the deepest non-recursive call chain, when it is deeper than the EVM
    /// stack.
    fn overflowing_chain(
        module: &Module,
        call_graph: &CallGraphInfo,
        plans: &IndexVec<FunctionId, Option<FunctionPlan>>,
    ) -> Option<Vec<FunctionId>> {
        // height(f) = peak(f) max over calls (base + height(callee))
        let mut memo = index_vec![None; module.functions.len()];
        fn height(
            func_id: FunctionId,
            call_graph: &CallGraphInfo,
            plans: &IndexVec<FunctionId, Option<FunctionPlan>>,
            memo: &mut IndexVec<FunctionId, Option<(usize, Option<FunctionId>)>>,
        ) -> usize {
            if let Some((height, _)) = memo[func_id] {
                return height;
            }
            let Some(plan) = &plans[func_id] else { return 0 };
            // Recursive activations are unbounded by construction, like solc.
            memo[func_id] = Some((plan.peak, None));
            let mut result = (plan.peak, None);
            for &(callee, base) in &plan.calls {
                if call_graph.is_recursive(callee) {
                    continue;
                }
                let through = base + height(callee, call_graph, plans, memo);
                if through > result.0 {
                    result = (through, Some(callee));
                }
            }
            memo[func_id] = Some(result);
            result.0
        }
        let root = module
            .functions
            .indices()
            .filter(|&func_id| plans[func_id].is_some())
            .find(|&func_id| height(func_id, call_graph, plans, &mut memo) > MAX_STACK_DEPTH)?;
        let mut chain = Vec::new();
        let mut func_id = root;
        while let Some((_, Some(callee))) = memo[func_id] {
            chain.push(func_id);
            func_id = callee;
        }
        Some(chain)
    }
}

/// Collects the value used for each argument, requiring one identity per argument.
fn argument_values(func: &Function) -> Result<IndexVec<ArgIdx, Option<ValueId>>, &'static str> {
    let mut args = index_vec![None; func.params.len()];
    let mut result = Ok(());
    let mut visit = |value: ValueId| {
        if let Value::Arg(index) = *func.value(value) {
            if index.index() >= args.len() {
                result = Err("argument outside the signature");
                return;
            }
            match args[index] {
                Some(existing) if existing != value => {
                    result = Err("non-canonical argument");
                }
                _ => args[index] = Some(value),
            }
        }
    };
    for block in &func.blocks {
        for &inst in &block.instructions {
            func.inst(inst).kind.visit_operands(&mut visit);
        }
        if let Some(term) = &block.terminator {
            term.visit_operands(&mut visit);
        }
    }
    result.map(|()| args)
}

/// Whether the planner pushes a fresh copy of `value` at each use instead of keeping it:
/// immediates, stable reads, and an external entry's calldata arguments.
fn materialized_at_use(func: &Function, internal: bool, value: ValueId) -> bool {
    match func.value(value) {
        Value::Immediate(_) | Value::Undef(_) | Value::Error(_) => true,
        Value::Arg(_) => !internal,
        Value::Inst(inst) => {
            rematerializable_nullary_value(func, value).is_some()
                || matches!(func.inst(*inst).kind, InstKind::DataSize(_))
        }
    }
}

/// The most words live into any block that the planner must keep on the stack or spill.
fn stack_pressure(func: &Function, internal: bool, liveness: &Liveness) -> usize {
    func.blocks
        .indices()
        .map(|block| {
            let live = liveness.live_in(block);
            live.iter().filter(|&value| !materialized_at_use(func, internal, value)).count()
        })
        .max()
        .unwrap_or(0)
}

/// The values live right after any instruction that `select` accepts.
fn live_after(
    func: &Function,
    liveness: &Liveness,
    select: impl Fn(InstId) -> bool,
) -> DenseBitSet<ValueId> {
    let mut result = DenseBitSet::new_empty(func.num_values());
    let mut live = DenseBitSet::new_empty(func.num_values());
    for (block, data) in func.blocks.iter_enumerated() {
        let Some(first) = data.instructions.iter().position(|&inst| select(inst)) else {
            continue;
        };
        live.clear();
        live.union(&liveness.live_out(block));
        if let Some(term) = &data.terminator {
            term.visit_operands(|value| {
                live.insert(value);
            });
        }
        for (index, &inst) in data.instructions.iter().enumerate().skip(first).rev() {
            if index == first || select(inst) {
                result.union(&live);
            }
            if let Some(value) = func.inst_result_value(inst) {
                live.remove(value);
            }
            func.inst(inst).kind.visit_operands(|value| {
                live.insert(value);
            });
        }
    }
    result
}

/// The values live across each instruction that `select` accepts, without its results.
fn live_across_each(
    func: &Function,
    liveness: &Liveness,
    select: impl Fn(InstId) -> bool,
) -> FxHashMap<InstId, DenseBitSet<ValueId>> {
    let mut sets = FxHashMap::default();
    let mut live = DenseBitSet::new_empty(func.num_values());
    for (block, data) in func.blocks.iter_enumerated() {
        let Some(first) = data.instructions.iter().position(|&inst| select(inst)) else {
            continue;
        };
        live.clear();
        live.union(&liveness.live_out(block));
        if let Some(term) = &data.terminator {
            term.visit_operands(|value| {
                live.insert(value);
            });
        }
        for (index, &inst) in data.instructions.iter().enumerate().skip(first).rev() {
            if let Some(value) = func.inst_result_value(inst) {
                live.remove(value);
            }
            if index == first || select(inst) {
                sets.insert(inst, live.clone());
            }
            func.inst(inst).kind.visit_operands(|value| {
                live.insert(value);
            });
        }
    }
    sets
}

/// The blocks of `func` from which control may return to its caller: those that reach a
/// return or a tail call into a function that returns.
fn returning_blocks(func: &Function, returning: &DenseBitSet<FunctionId>) -> DenseBitSet<BlockId> {
    let mut blocks = DenseBitSet::new_empty(func.blocks.len());
    let mut work = Vec::new();
    for (block, data) in func.blocks.iter_enumerated() {
        let exits = match &data.terminator {
            Some(Terminator::Return { .. }) => true,
            Some(Terminator::TailCall { function, .. }) => returning.contains(*function),
            _ => false,
        };
        if exits && blocks.insert(block) {
            work.push(block);
        }
    }
    while let Some(block) = work.pop() {
        for &pred in &func.blocks[block].predecessors {
            if blocks.insert(pred) {
                work.push(pred);
            }
        }
    }
    blocks
}

/// Returns the internal callee of an instruction, if it is a call.
fn icall(kind: &InstKind) -> Option<(FunctionId, &[ValueId])> {
    match kind {
        InstKind::ICall { function: Callee::Function(function), args } => Some((*function, args)),
        _ => None,
    }
}

type Layout = Vec<Slot>;
type Operands = SmallVec<[ValueId; 8]>;

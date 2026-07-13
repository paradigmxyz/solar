//! EVM bytecode generation from MIR.
//!
//! This module generates EVM bytecode from MIR using:
//! - Liveness analysis to know when values die
//! - Phi elimination to convert SSA to parallel copies
//! - Stack scheduling to generate DUP/SWAP sequences
//! - Two-pass assembly for label resolution

use super::{
    assembler::{Assembler, AssemblerConfig, DeferredConst, ImmutableRef, Label, op},
    stack::{
        MAX_STACK_ACCESS, ScheduledOp, SpillSlot, StackModel, StackOp, StackScheduler, TargetSlot,
    },
};
use crate::{
    IMMUTABLE_SCRATCH_BASE,
    analysis::{
        CallGraphInfo, CfgInfo, CopyDest, CopySource, Liveness, Loop, LoopAnalyzer, ParallelCopy,
        PhiEliminator,
    },
    mir::{BlockId, Function, FunctionId, InstId, InstKind, MirType, Module, Terminator, ValueId},
    pass::{AnalysisManager, LivenessAnalysis, PipelineOptions, run_default_pipeline_with_options},
};
use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode};
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_interface::{Session, sym};

// 0x00..0x7f follows Solidity's scratch/free-pointer/zero-slot convention, and
// 0x80 is used as the static ABI return buffer. Keep the internal-call frame
// pointer in a dedicated low word so frame loads use PUSH1 instead of PUSH2.
const INTERNAL_FRAME_PTR_SLOT: u64 = 0xa0;
const LOW_MEMORY_START: u64 = 0x80;
const CONSTRUCTOR_FREE_MEMORY_START: u64 = 0x4000;
const CONSTRUCTOR_SPILL_BASE: u64 = 0x1000;
const LINEAR_SELECTOR_DISPATCH_THRESHOLD: usize = 64;
const STACK_PHI_LAYOUT_LIMIT: usize = 8;
const GLOBAL_STACK_LAYOUT_LIMIT: usize = 8;
const GLOBAL_STACK_MAX_ARGS: usize = 3;
const GLOBAL_STACK_MIN_BLOCKS: usize = 8;
const GLOBAL_STACK_MIN_ARG_USES: usize = 6;
const GLOBAL_STACK_DENSE_AMORTIZATION_BLOCKS: usize = 16;

/// Configuration for the EVM backend.
#[derive(Clone, Copy, Debug, Default)]
pub struct EvmCodegenConfig {
    /// EVM version to target when selecting hardfork-gated opcodes.
    pub evm_version: EvmVersion,
    /// Optimization mode for MIR passes and bytecode assembly.
    pub optimization: OptimizationMode,
    /// Print MIR after each pass before bytecode generation.
    pub mir_print_after_each: bool,
    /// Lower dispatch/ABI as MIR phases and consume them here.
    pub mir_dispatch: bool,
    /// Run the experimental EVM IR `StackSchedule` pass in the assembler bridge.
    ///
    /// Off by default: the default bytecode path must stay byte-for-byte
    /// unchanged. When enabled the bridge runs `EvmIrPass::StackSchedule` on the
    /// operand-cleared block IR. On that already-stack-scheduled input the pass
    /// is a verified near no-op in `StructuredAsmProgram::optimize_with_evm_ir`.
    pub evm_ir_stack_schedule: bool,
    /// Run EVM IR layout/code-size passes in the assembler bridge.
    pub evm_ir_layout_passes: bool,
}

impl EvmCodegenConfig {
    /// Creates backend configuration from a compiler session.
    #[must_use]
    pub fn from_session(sess: &Session) -> Self {
        Self {
            evm_version: sess.opts.evm_version,
            optimization: sess.opts.optimization,
            mir_print_after_each: sess.opts.unstable.mir_print_after_each,
            mir_dispatch: !sess.opts.unstable.no_mir_dispatch,
            // Keep the experimental EVM IR stack scheduler off in every default
            // compilation path so produced bytecode is unchanged.
            evm_ir_stack_schedule: false,
            evm_ir_layout_passes: false,
        }
    }

    fn assembler_config(self) -> AssemblerConfig {
        AssemblerConfig {
            evm_version: self.evm_version,
            optimization: self.optimization,
            evm_ir_stack_schedule: self.evm_ir_stack_schedule,
            evm_ir_layout_passes: self.evm_ir_layout_passes,
        }
    }
}

impl From<&Session> for EvmCodegenConfig {
    fn from(sess: &Session) -> Self {
        Self::from_session(sess)
    }
}

impl From<solar_sema::Gcx<'_>> for EvmCodegenConfig {
    fn from(gcx: solar_sema::Gcx<'_>) -> Self {
        Self::from_session(gcx.sess)
    }
}

/// Describes the stack effect of an EVM instruction.
/// This is used to keep the scheduler's stack model in sync with the actual EVM stack.
#[derive(Clone, Copy, Debug)]
struct StackEffect {
    /// Number of values popped from the stack.
    pops: usize,
    /// Number of values pushed to the stack.
    pushes: usize,
}

/// What value to track for a pushed stack entry.
#[derive(Clone, Copy, Debug)]
enum StackPush {
    /// No value is pushed (pushes == 0).
    #[allow(dead_code)]
    None,
    /// Push a tracked ValueId (pushes == 1).
    Tracked(ValueId),
    /// Push an unknown/untracked value (pushes == 1).
    Unknown,
}

#[derive(Clone, Copy, Debug)]
struct SelectorDispatchEntry {
    selector: u32,
    label: Label,
}

#[derive(Clone, Debug, Default)]
struct StackPhiPlan {
    entries: FxHashMap<BlockId, Vec<ValueId>>,
    edges: FxHashMap<BlockId, StackPhiEdge>,
    edge_sources: FxHashMap<BlockId, FxHashSet<ValueId>>,
}

#[derive(Clone, Debug)]
struct StackPhiEdge {
    sources: Vec<ValueId>,
    results: Vec<ValueId>,
}

/// Canonical argument layouts carried between MIR basic blocks.
///
/// A block-local scheduler normally discards its model at every join. Function
/// arguments are special: they have one identity on every incoming edge and can
/// always be rematerialized as a safe fallback. Agreeing on one layout for all
/// predecessors lets the first load remain stack-resident through diamonds and
/// loops instead of repeating `CALLDATALOAD` or frame `MLOAD` in every block.
#[derive(Clone, Debug, Default)]
struct GlobalStackPlan {
    entries: FxHashMap<BlockId, Vec<ValueId>>,
    aliases: FxHashMap<ValueId, ValueId>,
}

impl GlobalStackPlan {
    fn analyze(func: &Function, liveness: &Liveness, stack_phi_plan: &StackPhiPlan) -> Self {
        let cfg = CfgInfo::new(func);
        let mut entries = FxHashMap::default();
        let args_by_index: FxHashMap<_, _> = func
            .values
            .iter_enumerated()
            .filter_map(|(value, kind)| match kind {
                crate::mir::Value::Arg { index, .. } => Some((*index, value)),
                _ => None,
            })
            .collect();
        if func.selector.is_none()
            || !(2..=GLOBAL_STACK_MAX_ARGS).contains(&args_by_index.len())
            || cfg.reachable().len() < GLOBAL_STACK_MIN_BLOCKS
        {
            return Self::default();
        }
        let mut decode_blocks = FxHashMap::default();
        let mut aliases = FxHashMap::default();
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                let InstKind::CalldataLoad(offset) = &func.instructions[inst_id].kind else {
                    continue;
                };
                let Some(offset) = func.value_u64(*offset) else {
                    continue;
                };
                if offset >= 4
                    && (offset - 4) % 32 == 0
                    && let Ok(index) = u32::try_from((offset - 4) / 32)
                    && let Some(&arg) = args_by_index.get(&index)
                {
                    decode_blocks.entry(arg).or_insert(block_id);
                    if let Some(result) = func.inst_result_value(inst_id) {
                        aliases.insert(result, arg);
                    }
                }
            }
        }

        for block_id in func.blocks.indices() {
            if block_id == func.entry_block
                || !cfg.is_reachable(block_id)
                || func.blocks[block_id].predecessors.is_empty()
                || stack_phi_plan.entries.contains_key(&block_id)
                || Self::is_terminal_block(func, block_id)
            {
                continue;
            }

            let values: Vec<_> = liveness
                .live_in(block_id)
                .iter()
                .filter(|&value| {
                    matches!(func.value(value), crate::mir::Value::Arg { .. })
                        && decode_blocks.get(&value).is_none_or(|&decode| {
                            decode != block_id && cfg.dominators().dominates(decode, block_id)
                        })
                })
                .take(GLOBAL_STACK_LAYOUT_LIMIT)
                .collect();
            if !values.is_empty() {
                entries.insert(block_id, values);
            }
        }

        // A branch leaves one physical stack for both outgoing edges after its
        // condition is consumed. Its successors therefore have to agree on the
        // same canonical layout. Use the union so an argument needed by either
        // live successor remains available. Terminal siblings are excluded:
        // carried words are harmless below their abort operands. Iterate
        // because sibling constraints can connect several diamonds.
        let mut changed = true;
        while changed {
            changed = false;
            for block_id in func.blocks.indices() {
                let Some(Terminator::Branch { then_block, else_block, .. }) =
                    func.blocks[block_id].terminator.as_ref()
                else {
                    continue;
                };
                if Self::is_terminal_block(func, *then_block)
                    || Self::is_terminal_block(func, *else_block)
                {
                    continue;
                }
                let mut common = entries.get(then_block).cloned().unwrap_or_default();
                for &value in entries.get(else_block).into_iter().flatten() {
                    if common.len() == GLOBAL_STACK_LAYOUT_LIMIT {
                        break;
                    }
                    if !common.contains(&value) {
                        common.push(value);
                    }
                }
                common.sort_by_key(|value| value.index());
                changed |= Self::set_entry(&mut entries, *then_block, &common);
                changed |= Self::set_entry(&mut entries, *else_block, &common);
            }
        }

        // Switch lowering owns the selector stack, and stack-phi loop headers
        // own their edge layouts. Disable their whole branch-sibling component
        // so every predecessor of every affected block still agrees.
        let mut disabled = FxHashSet::default();
        disabled.extend(stack_phi_plan.entries.keys().copied());
        for block_id in func.blocks.indices() {
            if let Some(Terminator::Switch { default, cases, .. }) =
                func.blocks[block_id].terminator.as_ref()
            {
                disabled.insert(*default);
                disabled.extend(cases.iter().map(|&(_, target)| target));
            }
        }
        let mut changed = true;
        while changed {
            changed = false;
            for block_id in func.blocks.indices() {
                let Some(Terminator::Branch { then_block, else_block, .. }) =
                    func.blocks[block_id].terminator.as_ref()
                else {
                    continue;
                };
                if Self::is_terminal_block(func, *then_block)
                    || Self::is_terminal_block(func, *else_block)
                {
                    continue;
                }
                if disabled.contains(then_block) || disabled.contains(else_block) {
                    changed |= disabled.insert(*then_block);
                    changed |= disabled.insert(*else_block);
                }
            }
        }
        entries.retain(|block, _| !disabled.contains(block));

        // Canonicalization pays DUP/SWAP/POP traffic on every planned edge.
        // Require enough real argument reuse to recover that fixed cost, and
        // reject dense layout plans unless a long CFG can amortize them.
        let mut arg_uses = 0usize;
        for block in func.blocks.iter() {
            for &inst_id in &block.instructions {
                arg_uses += func.instructions[inst_id]
                    .kind
                    .operands()
                    .iter()
                    .filter(|&&value| matches!(func.value(value), crate::mir::Value::Arg { .. }))
                    .count();
            }
            if let Some(term) = &block.terminator {
                arg_uses += term
                    .operands()
                    .iter()
                    .filter(|&&value| matches!(func.value(value), crate::mir::Value::Arg { .. }))
                    .count();
            }
        }
        if arg_uses < GLOBAL_STACK_MIN_ARG_USES
            || (entries.len() * 2 > cfg.reachable().len()
                && cfg.reachable().len() < GLOBAL_STACK_DENSE_AMORTIZATION_BLOCKS)
        {
            entries.clear();
        }
        aliases.retain(|_, arg| entries.values().any(|entry| entry.contains(arg)));
        Self { entries, aliases }
    }

    fn set_entry(
        entries: &mut FxHashMap<BlockId, Vec<ValueId>>,
        block: BlockId,
        layout: &[ValueId],
    ) -> bool {
        if entries.get(&block).map_or(layout.is_empty(), |old| old == layout) {
            return false;
        }
        if layout.is_empty() {
            entries.remove(&block);
        } else {
            entries.insert(block, layout.to_vec());
        }
        true
    }

    fn entry(&self, block: BlockId) -> Option<&[ValueId]> {
        self.entries.get(&block).map(Vec::as_slice)
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn edge_layout(&self, func: &Function, term: &Terminator) -> Option<&[ValueId]> {
        match term {
            Terminator::Jump(target) => self.entry(*target),
            Terminator::Branch { then_block, else_block, .. } => {
                if Self::is_terminal_block(func, *then_block) {
                    return self.entry(*else_block);
                }
                if Self::is_terminal_block(func, *else_block) {
                    return self.entry(*then_block);
                }
                let then_layout = self.entry(*then_block)?;
                (self.entry(*else_block) == Some(then_layout)).then_some(then_layout)
            }
            _ => None,
        }
    }

    fn is_terminal_block(func: &Function, block: BlockId) -> bool {
        matches!(
            func.blocks[block].terminator,
            Some(Terminator::Revert { .. } | Terminator::Invalid)
        )
    }
}

impl StackPhiPlan {
    fn analyze(func: &Function) -> Self {
        StackPhiPlanner::new(func).plan()
    }
}

struct StackPhiPlanner<'a> {
    func: &'a Function,
    loops: Vec<Loop>,
    header_results: FxHashMap<BlockId, Vec<ValueId>>,
}

impl<'a> StackPhiPlanner<'a> {
    fn new(func: &'a Function) -> Self {
        let mut loop_analyzer = LoopAnalyzer::new();
        let loop_info = loop_analyzer.analyze(func);
        let mut loops: Vec<_> = loop_info.all_loops().cloned().collect();
        loops.sort_by_key(|loop_info| loop_info.header.index());

        let mut planner = Self { func, loops, header_results: FxHashMap::default() };
        planner.collect_header_results();
        planner
    }

    fn plan(&self) -> StackPhiPlan {
        let mut plan = StackPhiPlan::default();
        for loop_info in &self.loops {
            self.plan_loop(loop_info, &mut plan);
        }
        plan
    }

    fn collect_header_results(&mut self) {
        for loop_info in &self.loops {
            let block = &self.func.blocks[loop_info.header];
            let phi_insts = self.leading_phi_insts(block);
            if let Some(results) = self.phi_result_values(&phi_insts) {
                self.header_results.insert(loop_info.header, results);
            }
        }
    }

    fn plan_loop(&self, loop_info: &Loop, plan: &mut StackPhiPlan) {
        let Some(preheader) = loop_info.preheader else {
            return;
        };
        let [latch] = loop_info.back_edges.as_slice() else {
            return;
        };
        if !matches!(self.func.blocks[preheader].terminator, Some(Terminator::Jump(target)) if target == loop_info.header)
            || !matches!(self.func.blocks[*latch].terminator, Some(Terminator::Jump(target)) if target == loop_info.header)
        {
            return;
        }
        if plan.edges.contains_key(&preheader) || plan.edges.contains_key(latch) {
            return;
        }

        let block = &self.func.blocks[loop_info.header];
        let phi_insts = self.leading_phi_insts(block);
        if phi_insts.is_empty() || phi_insts.len() > STACK_PHI_LAYOUT_LIMIT {
            return;
        }

        let Some(results) = self.phi_result_values(&phi_insts) else {
            return;
        };
        if results.len() > STACK_PHI_LAYOUT_LIMIT {
            return;
        }

        let carry_through = self.carry_through_values(loop_info);
        if carry_through.len() + results.len() > STACK_PHI_LAYOUT_LIMIT {
            return;
        }

        let mut entry = carry_through.clone();
        entry.extend(results.iter().copied());

        let predecessors = [preheader, *latch];
        let mut edges = Vec::with_capacity(predecessors.len());
        for pred in predecessors {
            let Some(phi_sources) = self.phi_sources_for_pred(&phi_insts, pred) else {
                return;
            };
            let mut sources = carry_through.clone();
            sources.extend(phi_sources);
            debug_assert_eq!(sources.len(), entry.len());
            edges.push((pred, sources));
        }

        plan.entries.insert(loop_info.header, entry.clone());
        for (pred, sources) in edges {
            plan.edge_sources.insert(pred, sources.iter().copied().collect());
            plan.edges.insert(pred, StackPhiEdge { sources, results: entry.clone() });
        }
    }

    fn leading_phi_insts(&self, block: &crate::mir::BasicBlock) -> Vec<InstId> {
        block
            .instructions
            .iter()
            .copied()
            .take_while(|&inst| matches!(self.func.instructions[inst].kind, InstKind::Phi(_)))
            .collect()
    }

    fn carry_through_values(&self, loop_info: &Loop) -> Vec<ValueId> {
        let mut carry_through = Vec::new();
        for outer in &self.loops {
            if outer.header == loop_info.header || !outer.blocks.contains(&loop_info.header) {
                continue;
            }
            let Some(results) = self.header_results.get(&outer.header) else {
                continue;
            };
            for &value in results {
                if carry_through.contains(&value)
                    || !self.value_used_in_blocks(&loop_info.blocks, value)
                {
                    continue;
                }
                carry_through.push(value);
            }
        }
        carry_through
    }

    fn value_used_in_blocks(&self, blocks: &FxHashSet<BlockId>, value: ValueId) -> bool {
        for &block_id in blocks {
            let block = &self.func.blocks[block_id];
            for &inst_id in &block.instructions {
                if matches!(self.func.instructions[inst_id].kind, InstKind::Phi(_)) {
                    continue;
                }
                if self.func.instructions[inst_id].kind.operands().contains(&value) {
                    return true;
                }
            }
            if block.terminator.as_ref().is_some_and(|term| term.operands().contains(&value)) {
                return true;
            }
        }
        false
    }

    fn phi_result_values(&self, phi_insts: &[InstId]) -> Option<Vec<ValueId>> {
        phi_insts.iter().map(|&inst| self.func.inst_result_value(inst)).collect()
    }

    fn phi_sources_for_pred(&self, phi_insts: &[InstId], pred: BlockId) -> Option<Vec<ValueId>> {
        phi_insts
            .iter()
            .map(|&inst| {
                let InstKind::Phi(incoming) = &self.func.instructions[inst].kind else {
                    return None;
                };
                incoming.iter().find_map(|&(block, value)| (block == pred).then_some(value))
            })
            .collect()
    }
}

/// EVM code generator.
pub struct EvmCodegen {
    /// The assembler for bytecode generation.
    asm: Assembler,
    /// Stack scheduler.
    scheduler: StackScheduler,
    /// Block labels.
    block_labels: FxHashMap<BlockId, Label>,
    /// Function labels for direct internal calls.
    function_labels: FxHashMap<FunctionId, Label>,
    /// Per-function static local frame sizes for direct internal calls.
    ///
    /// Spill slots are allocated lazily during body emission, so the spill part
    /// of each frame is recorded separately after the function has emitted.
    function_static_frame_sizes: FxHashMap<FunctionId, u64>,
    /// Exact per-function spill area sizes, in bytes, recorded after emission.
    function_spill_sizes: FxHashMap<FunctionId, u64>,
    /// Internal-call frame-size constants waiting for exact callee spill sizes.
    pending_frame_size_consts: Vec<(DeferredConst, FunctionId, u64)>,
    /// Per static-frame callee: which argument indices every runtime call
    /// site can re-emit after the stack drain, so they ride the stack above
    /// the return address instead of being stored to the callee frame at
    /// each site. The callee prologue stores them once.
    stack_arg_masks: FxHashMap<FunctionId, Vec<bool>>,
    /// Whether the current assembly is the runtime (stack-passed arguments
    /// apply). The constructor assembly emits its own copies of internal
    /// functions with the plain frame-store convention.
    runtime_stack_args: bool,
    /// Deferred spill-slot address pushes of the external body being emitted,
    /// keyed by the slot's allocation offset, with their reference counts.
    /// Resolved hottest-first at body end so the most reloaded slots take the
    /// shortest addresses.
    spill_addr_consts: FxHashMap<u64, (DeferredConst, usize)>,
    /// Callees whose internal-call frame can be deallocated after return.
    restorable_internal_frames: FxHashSet<FunctionId>,
    /// Functions whose frame lives at a compile-time-fixed address (static
    /// frames): internal-convention, non-recursive functions in the runtime
    /// passes. Their arg/local/spill accesses are absolute pushes and their
    /// call sites skip all frame-pointer and free-pointer bookkeeping.
    static_frame_functions: FxHashSet<FunctionId>,
    /// Interned deferred constants for absolute static-frame addresses, keyed
    /// by (function, byte offset within its frame). Resolved at the end of
    /// the pass, once every body's exact spill size is known.
    static_frame_addr_consts: FxHashMap<(FunctionId, u64), DeferredConst>,
    /// The pass's single free-memory-pointer constant, emitted once in the
    /// runtime prologue and resolved at the end of the pass: the heap must
    /// start above every entry's locals/spills and the static frame region.
    runtime_free_memory_const: Option<DeferredConst>,
    /// Every external body emitted this pass, for sizing the heap floor.
    runtime_entry_funcs: Vec<FunctionId>,
    /// The internal-convention function currently being emitted.
    current_internal_function: Option<FunctionId>,
    /// Copies to insert at block exits (from phi elimination).
    block_copies: FxHashMap<BlockId, Vec<ParallelCopy>>,
    /// Values carried by planned stack-resident phi edges, keyed by predecessor block.
    stack_phi_sources: FxHashMap<BlockId, FxHashSet<ValueId>>,
    /// Whether the current function has canonical cross-block argument layouts.
    global_stack_active: bool,
    /// Calldata words physically identical to arguments in the active global
    /// layout, adopted after their final validation use.
    global_stack_aliases: FxHashMap<ValueId, ValueId>,
    /// Immutable `PUSH32` placeholders in the last assembled runtime code.
    runtime_immutable_refs: Vec<ImmutableRef>,
    /// Whether we're currently generating constructor code.
    /// When true, LoadArg uses CODECOPY from the end of code instead of CALLDATALOAD.
    in_constructor: bool,
    /// Number of constructor parameters (used for CODECOPY offset calculation).
    constructor_param_count: u32,
    /// Whether we're emitting an internal function body.
    in_internal_function: bool,
    /// Whether we're emitting the MIR dispatch `entry` function. Its switch
    /// keeps the selector on the physical stack through the case chain and
    /// leaves it inert below the taken arm, like the backend dispatcher. Only
    /// sound there: the entry runs once and every arm terminates externally,
    /// so the leftover word can neither accumulate nor disturb an internal
    /// return.
    emitting_dispatch_entry: bool,
    /// Optimization mode for MIR passes and bytecode assembly.
    optimization: OptimizationMode,
    /// Print MIR after each pass before bytecode generation.
    mir_print_after_each: bool,
    mir_dispatch: bool,
}

impl EvmCodegen {
    /// Creates a new EVM code generator.
    #[must_use]
    pub fn new(config: impl Into<EvmCodegenConfig>) -> Self {
        let config = config.into();
        Self {
            asm: Assembler::with_config(config.assembler_config()),
            scheduler: StackScheduler::new(),
            block_labels: FxHashMap::default(),
            function_labels: FxHashMap::default(),
            function_static_frame_sizes: FxHashMap::default(),
            function_spill_sizes: FxHashMap::default(),
            pending_frame_size_consts: Vec::new(),
            stack_arg_masks: FxHashMap::default(),
            runtime_stack_args: false,
            spill_addr_consts: FxHashMap::default(),
            restorable_internal_frames: FxHashSet::default(),
            static_frame_functions: FxHashSet::default(),
            static_frame_addr_consts: FxHashMap::default(),
            runtime_free_memory_const: None,
            runtime_entry_funcs: Vec::new(),
            current_internal_function: None,
            block_copies: FxHashMap::default(),
            stack_phi_sources: FxHashMap::default(),
            global_stack_active: false,
            global_stack_aliases: FxHashMap::default(),
            runtime_immutable_refs: Vec::new(),
            in_constructor: false,
            constructor_param_count: 0,
            in_internal_function: false,
            emitting_dispatch_entry: false,
            optimization: config.optimization,
            mir_print_after_each: config.mir_print_after_each,
            mir_dispatch: config.mir_dispatch,
        }
    }

    // ==================== Stack-Aware Emitter API ====================
    //
    // These helpers ensure that all EVM stack mutations are tracked by the scheduler.
    // Any opcode that changes the EVM stack must be emitted through these methods
    // to keep the scheduler's StackModel in sync with the actual EVM stack.

    /// Emits a stack manipulation operation (DUP, SWAP, POP) and updates the scheduler.
    fn emit_stack_op(&mut self, op: StackOp) {
        self.asm.emit_op(op.opcode());
        match op {
            StackOp::Dup(n) => self.scheduler.stack.dup(n),
            StackOp::Swap(n) => self.scheduler.stack.swap(n),
            StackOp::Pop => {
                self.scheduler.stack.pop();
            }
        }
    }

    /// Emits an opcode with known stack effects and updates the scheduler.
    ///
    /// This is the core method for stack-aware emission. After emitting the opcode:
    /// - `effect.pops` values are removed from the scheduler's stack model
    /// - Values are pushed according to `push`:
    ///   - `StackPush::None`: no value pushed (effect.pushes must be 0)
    ///   - `StackPush::Tracked(v)`: push a tracked ValueId (effect.pushes must be 1)
    ///   - `StackPush::Unknown`: push an untracked value (effect.pushes must be 1)
    fn emit_op_with_effect(&mut self, opcode: u8, effect: StackEffect, push: StackPush) {
        #[cfg(debug_assertions)]
        let before = self.scheduler.depth();

        self.asm.emit_op(opcode);

        // Pop consumed values
        for _ in 0..effect.pops {
            self.scheduler.stack.pop();
        }

        // Push produced values
        match (effect.pushes, push) {
            (0, StackPush::None) => {}
            (1, StackPush::Tracked(v)) => self.scheduler.stack.push(v),
            (1, StackPush::Unknown) => self.scheduler.stack.push_unknown(),
            (n, _) if n > 1 => {
                // Multi-push: push unknown values
                for _ in 0..n {
                    self.scheduler.stack.push_unknown();
                }
            }
            _ => {}
        }

        #[cfg(debug_assertions)]
        {
            let expected = before + effect.pushes - effect.pops;
            debug_assert_eq!(
                self.scheduler.depth(),
                expected,
                "Stack model drift after opcode 0x{:02x}: expected depth {}, got {}",
                opcode,
                expected,
                self.scheduler.depth()
            );
        }
    }

    /// Generates bytecode for a module (runtime code only).
    /// Returns empty bytecode for interfaces (they have no implementation).
    ///
    /// This runs optimization passes (including DCE) on the module before codegen unless disabled.
    pub fn generate_module(&mut self, module: &mut Module) -> Vec<u8> {
        if module.is_interface {
            return Vec::new();
        }
        self.run_optimization_passes(module);
        // Immutable reads are `PUSH32` zero placeholders here; only a
        // constructor run patches them with actual values.
        self.generate_runtime_code(module)
    }

    /// Generates deployment bytecode for a module.
    /// Returns (deployment_bytecode, runtime_bytecode).
    /// Returns empty bytecodes for interfaces (they have no implementation).
    ///
    /// This runs optimization passes (including DCE) on the module before codegen unless disabled.
    pub fn generate_deployment_bytecode(&mut self, module: &mut Module) -> (Vec<u8>, Vec<u8>) {
        if module.is_interface {
            return (Vec::new(), Vec::new());
        }
        self.run_optimization_passes(module);
        // First generate the runtime code
        let runtime_code = self.generate_runtime_code(module);
        let runtime_len = runtime_code.len();
        let immutable_refs = std::mem::take(&mut self.runtime_immutable_refs);

        // The constructor copies the runtime code to memory and patches the
        // immutable placeholders with the staged scratch words before
        // returning. Copy to offset 0 unless that would overwrite the scratch
        // words before the patch loop reads them.
        let copy_base = if !immutable_refs.is_empty() && runtime_len as u64 > IMMUTABLE_SCRATCH_BASE
        {
            IMMUTABLE_SCRATCH_BASE + module.immutable_data_len() as u64
        } else {
            0
        };

        // Generate constructor initialization code (if any). Constructor arguments are appended
        // after the generated deployment bytecode, so the constructor arg offset depends on the
        // constructor code length. Iterate until the push widths stabilize.
        let mut deploy_code_len = 0usize;
        let mut constructor_arg_offset = runtime_len;
        let mut constructor_code = self.generate_constructor_code(module, Some(runtime_len));
        for _ in 0..8 {
            let postlude = self.build_deployment_postlude(
                deploy_code_len,
                runtime_len,
                copy_base,
                &immutable_refs,
            );
            let next_deploy_code_len = constructor_code.len() + postlude.len();
            let next_arg_offset = next_deploy_code_len + runtime_len;
            if next_deploy_code_len == deploy_code_len && next_arg_offset == constructor_arg_offset
            {
                break;
            }
            deploy_code_len = next_deploy_code_len;
            constructor_arg_offset = next_arg_offset;
            constructor_code = self.generate_constructor_code(module, Some(constructor_arg_offset));
        }

        // Deploy code structure:
        // [constructor_code]    ; run constructor (SSTOREs + immutable staging)
        // PUSH<n> runtime_len   ; size to copy from creation code
        // DUP1                  ; duplicate for the final RETURN size
        // PUSH<n> offset        ; where runtime starts
        // PUSH<n> copy_base     ; memory destination
        // CODECOPY              ; copy runtime to memory
        // [immutable patches]   ; patch staged words into the PUSH32 placeholders
        // PUSH<n> copy_base     ; memory offset
        // RETURN                ; return the runtime code
        let postlude = self.build_deployment_postlude(
            deploy_code_len,
            runtime_len,
            copy_base,
            &immutable_refs,
        );

        // Build the deployment bytecode
        let mut deploy_bytecode = Vec::new();

        // Add constructor code first
        deploy_bytecode.extend_from_slice(&constructor_code);
        deploy_bytecode.extend_from_slice(&postlude);

        // Append runtime code
        deploy_bytecode.extend_from_slice(&runtime_code);

        // The returned runtime artifact keeps the zero placeholders, like
        // solc's `deployedBytecode` for contracts with immutables.
        (deploy_bytecode, runtime_code)
    }

    fn build_deployment_postlude(
        &mut self,
        deploy_code_len: usize,
        runtime_len: usize,
        copy_base: u64,
        immutable_refs: &[ImmutableRef],
    ) -> Vec<u8> {
        self.asm.clear();

        // Copy runtime code from creation code to memory at `copy_base`.
        self.asm.emit_push(U256::from(runtime_len as u64));
        self.asm.emit_op(op::dup(1));
        self.asm.emit_push(U256::from(deploy_code_len as u64));
        self.asm.emit_push(U256::from(copy_base));
        self.asm.emit_op(op::CODECOPY);

        // Patch each `PUSH32` placeholder with its staged immutable word.
        // The placeholder data starts one byte after the PUSH32 opcode.
        for r in immutable_refs {
            self.asm.emit_push(U256::from(IMMUTABLE_SCRATCH_BASE + u64::from(r.id)));
            self.asm.emit_op(op::MLOAD);
            self.asm.emit_push(U256::from(copy_base + r.code_offset as u64 + 1));
            self.asm.emit_op(op::MSTORE);
        }

        // Return the patched runtime code; the DUP'd length is still on the stack.
        self.asm.emit_push(U256::from(copy_base));
        self.asm.emit_op(op::RETURN);
        self.asm.assemble().bytecode
    }

    /// Generates constructor code that runs during deployment.
    /// This includes state variable initializers.
    ///
    /// Constructor arguments are read from the end of the initcode using CODECOPY.
    /// The args are ABI-encoded and appended after the deployment bytecode.
    fn generate_constructor_code(
        &mut self,
        module: &Module,
        constructor_arg_offset: Option<usize>,
    ) -> Vec<u8> {
        // Find constructor function if it exists
        let constructor =
            module.functions.iter_enumerated().find(|(_, f)| f.attributes.is_constructor);

        if let Some((ctor_id, ctor)) = constructor {
            // Generate constructor bytecode
            self.asm.clear();

            // Clear state and generate function body
            self.block_labels.clear();
            self.block_copies.clear();
            self.function_labels.clear();
            self.function_static_frame_sizes.clear();
            self.function_spill_sizes.clear();
            self.pending_frame_size_consts.clear();
            self.restorable_internal_frames.clear();
            self.static_frame_functions.clear();
            self.stack_arg_masks.clear();
            self.runtime_stack_args = false;
            self.static_frame_addr_consts.clear();
            self.runtime_free_memory_const = None;
            self.runtime_entry_funcs.clear();
            self.current_internal_function = None;
            self.stack_phi_sources.clear();

            for (func_id, func) in module.functions.iter_enumerated() {
                self.function_static_frame_sizes.insert(func_id, func.internal_frame_size);
                if !func.params.iter().chain(&func.returns).any(|ty| matches!(ty, MirType::MemPtr))
                {
                    self.restorable_internal_frames.insert(func_id);
                }
            }

            let call_graph = CallGraphInfo::new(module);
            let internal_targets = call_graph.reachable_bodies_from(std::iter::once(ctor_id));
            for &func_id in &internal_targets {
                self.function_labels.insert(func_id, self.asm.new_label());
            }

            // Constructor spill slots are absolute addresses starting at
            // 0x1000. Keep the historical 0x4000 heap start as a floor, but
            // patch it upward after emission if the lazily allocated spill area
            // needs more room.
            let constructor_free_memory_start = self.asm.new_deferred_const();
            self.asm.emit_push_deferred(constructor_free_memory_start);
            self.asm.emit_push(U256::from(0x40));
            self.asm.emit_op(op::MSTORE);

            // Set constructor context for LoadArg handling
            self.in_constructor = true;
            self.constructor_param_count = ctor.params.len() as u32;

            // If constructor has parameters, copy the full ABI-encoded argument blob to memory.
            // Constructor args are appended after generated deployment bytecode, so the copy size
            // is `CODESIZE - constructor_arg_offset`.
            if !ctor.params.is_empty() {
                let arg_offset = constructor_arg_offset.unwrap_or(0);
                self.asm.emit_push(U256::from(arg_offset));
                self.asm.emit_op(op::CODESIZE);
                self.asm.emit_op(op::SUB); // size = CODESIZE - arg_offset
                self.asm.emit_push(U256::from(arg_offset)); // code offset
                self.asm.emit_push(U256::from(0x80)); // destOffset in memory
                self.asm.emit_op(op::CODECOPY);
            }

            if !internal_targets.is_empty() {
                let constructor_entry = self.asm.new_label();
                self.asm.emit_push_label(constructor_entry);
                self.asm.emit_op(op::JUMP);

                for (func_id, func) in module.functions.iter_enumerated() {
                    if !internal_targets.contains(&func_id) {
                        continue;
                    }
                    let label = self.function_labels[&func_id];
                    self.asm.define_label(label);
                    self.in_internal_function = true;
                    self.generate_function_body(func);
                    self.in_internal_function = false;
                    self.record_function_spill_size(func_id);
                }

                self.asm.define_label(constructor_entry);
            }

            // Generate the constructor body (which includes SSTORE for initializers)
            self.generate_function_body(ctor);
            let constructor_spill_size = self.record_function_spill_size(ctor_id);
            self.asm.set_deferred_const(
                constructor_free_memory_start,
                U256::from(Self::constructor_free_memory_start(constructor_spill_size)),
            );

            self.resolve_pending_frame_size_consts(module);

            // Reset constructor context
            self.in_constructor = false;
            self.constructor_param_count = 0;

            let mut bytecode = self.asm.assemble().bytecode;

            // Remove trailing STOP (0x00) if present - we want to fall through to CODECOPY/RETURN
            if bytecode.last() == Some(&op::STOP) {
                bytecode.pop();
            }

            bytecode
        } else {
            Vec::new()
        }
    }

    /// Runs the canonical MIR optimization pipeline on the module.
    fn run_optimization_passes(&mut self, module: &mut Module) {
        module.optimize_for_size = self.optimization == OptimizationMode::Size;
        if self.optimization != OptimizationMode::None {
            run_default_pipeline_with_options(
                module,
                PipelineOptions {
                    print_after_each: self.mir_print_after_each,
                    ..PipelineOptions::default()
                },
            );
            // After the pipeline has settled the shapes: panic and constant
            // custom-error revert blocks repeat at every check site, and the
            // assembler cannot dedup them (they are reached by fallthrough).
            crate::pass::run_pass(module, &crate::pass::OUTLINE_REVERTS_PASS);
        }
        // Progressive lowering: materialize ABI wrappers, the dispatcher, and
        // tail-call edges as MIR. Each pass bails without advancing the phase
        // when the module is outside its scope, in which case runtime
        // generation falls back to the backend dispatcher.
        if self.mir_dispatch {
            crate::pass::run_pass(module, &crate::pass::LOWER_ABI_PASS);
            crate::pass::run_pass(module, &crate::pass::LOWER_DISPATCH_PASS);
            crate::pass::run_pass(module, &crate::pass::LOWER_EVM_SHAPED_PASS);
        }
    }

    /// Generates runtime bytecode for a module.
    fn generate_runtime_code(&mut self, module: &Module) -> Vec<u8> {
        self.asm.clear();
        self.block_labels.clear();
        self.function_labels.clear();
        self.function_static_frame_sizes.clear();
        self.function_spill_sizes.clear();
        self.pending_frame_size_consts.clear();
        self.restorable_internal_frames.clear();
        self.static_frame_functions.clear();
        self.static_frame_addr_consts.clear();
        self.runtime_free_memory_const = None;
        self.runtime_entry_funcs.clear();
        self.current_internal_function = None;
        self.block_copies.clear();
        self.stack_phi_sources.clear();
        self.stack_arg_masks.clear();
        self.runtime_stack_args = true;
        self.emitting_dispatch_entry = false;

        if !module.functions.is_empty() {
            if module.phase >= crate::mir::MirPhase::Dispatch {
                self.generate_mir_dispatched(module);
            } else {
                // The dispatcher generates function bodies inline
                self.generate_dispatcher(module);
            }
        }

        self.asm.set_structural_outlining(true);
        let result = self.asm.assemble();
        self.asm.set_structural_outlining(false);
        self.runtime_immutable_refs = result.immutable_refs;
        result.bytecode
    }

    /// Generates the runtime from a `dispatch`-phase module: the MIR `entry`
    /// function is the runtime prologue, its `tail_call`s jump to the ABI
    /// wrappers, and no backend dispatcher is synthesized.
    ///
    /// Selector matching, receive/fallback routing, and callvalue checks all
    /// live in the MIR `entry`, so wrappers are emitted without the selector
    /// pop and payable check the backend dispatcher would add.
    fn generate_mir_dispatched(&mut self, module: &Module) {
        let Some((entry_id, _)) = module
            .functions
            .iter_enumerated()
            .find(|(_, f)| f.selector.is_none() && f.name.name == sym::entry)
        else {
            // Phase says dispatch but there is nothing to route; fall back.
            self.generate_dispatcher(module);
            return;
        };

        let call_graph = CallGraphInfo::new(module);
        let internal_targets = call_graph.reachable_bodies_from(
            module.functions.iter_enumerated().filter_map(|(func_id, func)| {
                (func_id == entry_id || Self::is_external_entry(func)).then_some(func_id)
            }),
        );

        for (func_id, func) in module.functions.iter_enumerated() {
            self.function_static_frame_sizes.insert(func_id, func.internal_frame_size);
            if !func.params.iter().chain(&func.returns).any(|ty| matches!(ty, MirType::MemPtr)) {
                self.restorable_internal_frames.insert(func_id);
            }
            // Non-recursive internal functions get compile-time-fixed frames.
            if func_id != entry_id
                && !Self::is_external_entry(func)
                && Self::has_body(func)
                && !call_graph.is_recursive(func_id)
            {
                self.static_frame_functions.insert(func_id);
            }
        }
        self.compute_stack_arg_masks(module);

        // Labels for every tail-call and internal-call target.
        for (func_id, func) in module.functions.iter_enumerated() {
            if func_id == entry_id {
                continue;
            }
            let needs_body = Self::is_external_entry(func)
                || (Self::has_body(func) && internal_targets.contains(&func_id));
            if needs_body {
                let label = self.asm.new_label();
                self.function_labels.insert(func_id, label);
            }
        }

        // The MIR entry is the runtime prologue: one shared free-memory
        // store covers every wrapper reached through it.
        self.in_internal_function = false;
        self.emitting_dispatch_entry = true;
        let entry_free = self.emit_external_free_memory_start();
        self.runtime_free_memory_const = Some(entry_free);
        self.generate_function_body(&module.functions[entry_id]);
        self.emitting_dispatch_entry = false;
        self.record_function_spill_size(entry_id);
        self.runtime_entry_funcs.push(entry_id);

        // External entries, reached only through `tail_call` jumps.
        for (func_id, func) in module.functions.iter_enumerated() {
            if func_id == entry_id || !Self::is_external_entry(func) {
                continue;
            }
            let Some(&label) = self.function_labels.get(&func_id) else { continue };
            self.asm.define_label(label);
            self.in_internal_function = false;
            self.generate_function_body(func);
            self.record_function_spill_size(func_id);
            self.runtime_entry_funcs.push(func_id);
        }

        // Internal-call targets, exactly as in the backend dispatcher path.
        for (func_id, func) in module.functions.iter_enumerated() {
            if func_id == entry_id || Self::is_external_entry(func) || !Self::has_body(func) {
                continue;
            }
            let Some(&label) = self.function_labels.get(&func_id) else { continue };
            self.asm.define_label(label);
            self.emit_stack_arg_prologue(func_id, func);
            self.in_internal_function = true;
            self.current_internal_function = Some(func_id);
            self.generate_function_body(func);
            self.in_internal_function = false;
            self.current_internal_function = None;
            self.record_function_spill_size(func_id);
        }

        self.resolve_pending_frame_size_consts(module);
        self.resolve_static_frames(module);
    }

    /// Generates the function dispatcher.
    ///
    /// The dispatcher logic is:
    /// ```text
    /// if calldatasize == 0:
    ///     if has_receive: jump to receive
    ///     elif has_fallback: jump to fallback
    ///     else: revert
    /// else:
    ///     match selector...
    ///     if no match and has_fallback: jump to fallback
    ///     else: revert
    /// ```
    fn generate_dispatcher(&mut self, module: &Module) {
        // Find executable receive and fallback functions. Interface/abstract declarations can
        // have ABI entries but no MIR body, so they must not participate in runtime dispatch.
        let receive_idx =
            module.functions.iter().position(|f| f.attributes.is_receive && Self::has_body(f));
        let fallback_idx =
            module.functions.iter().position(|f| f.attributes.is_fallback && Self::has_body(f));

        let call_graph = CallGraphInfo::new(module);
        let internal_targets = call_graph.reachable_bodies_from(
            module
                .functions
                .iter_enumerated()
                .filter_map(|(func_id, func)| Self::is_external_entry(func).then_some(func_id)),
        );

        for (func_id, func) in module.functions.iter_enumerated() {
            self.function_static_frame_sizes.insert(func_id, func.internal_frame_size);
            if !func.params.iter().chain(&func.returns).any(|ty| matches!(ty, MirType::MemPtr)) {
                self.restorable_internal_frames.insert(func_id);
            }
            // Non-recursive internal functions get compile-time-fixed frames.
            if !Self::is_external_entry(func)
                && Self::has_body(func)
                && !call_graph.is_recursive(func_id)
            {
                self.static_frame_functions.insert(func_id);
            }
        }
        self.compute_stack_arg_masks(module);

        // Create labels for externally reachable runtime entry points and internal-call targets.
        let mut func_labels: Vec<Option<Label>> = Vec::new();
        for (func_id, func) in module.functions.iter_enumerated() {
            let external = Self::is_external_entry(func);
            let needs_body =
                external || (Self::has_body(func) && internal_targets.contains(&func_id));
            let label = needs_body.then(|| self.asm.new_label());
            if let Some(label) = label {
                self.function_labels.insert(func_id, label);
            }
            func_labels.push(label);
        }
        let revert_label = self.asm.new_label();
        self.asm.mark_label_cold(revert_label);
        let has_calldata_label = self.asm.new_label();
        let all_external_entries_reject_value =
            module.functions.iter().any(Self::is_external_entry)
                && module
                    .functions
                    .iter()
                    .filter(|func| Self::is_external_entry(func))
                    .all(Self::rejects_callvalue);

        // One shared free-memory store for every entry reached through the
        // dispatcher (solc does the same); its value is the maximum entry
        // frame end, or the static-frame region end.
        let dispatcher_free = self.emit_external_free_memory_start();
        self.runtime_free_memory_const = Some(dispatcher_free);

        if all_external_entries_reject_value {
            self.emit_callvalue_check(revert_label);
        }

        // Empty calldata: route to receive/fallback when they exist; with
        // neither, invert the check so nonempty calldata falls straight
        // through to the selector load and empty calldata takes the shared
        // revert stub (a byte shorter than a dedicated arm).
        if receive_idx.is_some() || fallback_idx.is_some() {
            self.asm.emit_op(op::CALLDATASIZE);
            self.asm.emit_push_label(has_calldata_label);
            self.asm.emit_op(op::JUMPI);
            if let Some(recv_idx) = receive_idx {
                self.asm.emit_push_label(func_labels[recv_idx].expect("receive label missing"));
                self.asm.emit_op(op::JUMP);
            } else if let Some(fb_idx) = fallback_idx {
                self.asm.emit_push_label(func_labels[fb_idx].expect("fallback label missing"));
                self.asm.emit_op(op::JUMP);
            }
        } else {
            self.asm.emit_op(op::CALLDATASIZE);
            self.asm.emit_op(op::ISZERO);
            self.asm.emit_push_label(revert_label);
            self.asm.emit_op(op::JUMPI);
        }

        // calldatasize > 0: Load selector and match
        self.asm.define_label(has_calldata_label);

        // Load selector from calldata
        self.asm.emit_push(U256::ZERO);
        self.asm.emit_op(op::CALLDATALOAD);
        self.asm.emit_push(U256::from(0xe0));
        self.asm.emit_op(op::SHR);

        let mut selectors: Vec<_> = module
            .functions
            .iter()
            .enumerate()
            .filter_map(|(i, func)| {
                if !Self::is_external_entry(func) {
                    return None;
                }
                let selector = func.selector?;
                Some(SelectorDispatchEntry {
                    selector: u32::from_be_bytes(selector),
                    label: func_labels[i].expect("selector label missing"),
                })
            })
            .collect();
        selectors.sort_by_key(|entry| entry.selector);

        let fallback_label =
            fallback_idx.map(|idx| func_labels[idx].expect("fallback label missing"));
        self.emit_selector_dispatch(&selectors, fallback_label, revert_label);

        // Define external function entry points.
        for (func_id, func) in module.functions.iter_enumerated() {
            if !Self::is_external_entry(func) {
                continue;
            }
            let Some(label) = func_labels[func_id.index()] else { continue };
            self.asm.define_label(label);

            // The dispatcher's shr'd selector is still on the physical stack
            // for regular functions. It is untracked and below everything the
            // wrapper's stack model describes, so it is inert (the same
            // invariant stack-passed return addresses rely on) — the wrapper
            // terminates externally and never reaches it; popping it per
            // wrapper was a wasted byte each.

            if !all_external_entries_reject_value {
                self.emit_payable_check(func, revert_label);
            }

            // Generate function body
            self.in_internal_function = false;
            self.generate_function_body(func);

            self.record_function_spill_size(func_id);
            self.runtime_entry_funcs.push(func_id);
        }

        // Define internal-call targets once. Calls jump here and return
        // through the stack-passed return address.
        for (func_id, func) in module.functions.iter_enumerated() {
            if Self::is_external_entry(func) || !Self::has_body(func) {
                continue;
            }
            let Some(label) = func_labels[func_id.index()] else { continue };
            self.asm.define_label(label);
            self.emit_stack_arg_prologue(func_id, func);
            self.in_internal_function = true;
            self.current_internal_function = Some(func_id);
            self.generate_function_body(func);
            self.in_internal_function = false;
            self.current_internal_function = None;
            self.record_function_spill_size(func_id);
        }

        // Revert label
        self.asm.define_label(revert_label);
        self.asm.emit_push(U256::ZERO);
        self.asm.emit_push(U256::ZERO);
        self.asm.emit_op(op::REVERT);

        self.resolve_pending_frame_size_consts(module);
        self.resolve_static_frames(module);
    }

    /// Records the exact spill area size of the function body that just emitted.
    fn record_function_spill_size(&mut self, func_id: FunctionId) -> u64 {
        let spill_size = u64::from(self.scheduler.spills.spill_area_size());
        self.function_spill_sizes.insert(func_id, spill_size);
        spill_size
    }

    /// Resolves all pending internal-call frame-size constants.
    ///
    /// The normal runtime path records every emitted callee's exact spill size
    /// first. The conservative fallback covers unusual paths where a body was
    /// not emitted by this assembler.
    fn resolve_pending_frame_size_consts(&mut self, module: &Module) {
        for (id, callee, static_size) in std::mem::take(&mut self.pending_frame_size_consts) {
            let spill_size =
                self.function_spill_sizes.get(&callee).copied().unwrap_or_else(|| {
                    Self::conservative_spill_frame_size(&module.functions[callee])
                });
            self.asm.set_deferred_const(id, U256::from(static_size + spill_size));
        }
    }

    fn is_external_entry(func: &Function) -> bool {
        Self::has_body(func)
            && (func.selector.is_some()
                || func.attributes.is_receive
                || func.attributes.is_fallback)
    }

    fn has_body(func: &Function) -> bool {
        !func.attributes.is_constructor && !func.blocks.is_empty()
    }

    fn emit_selector_dispatch(
        &mut self,
        selectors: &[SelectorDispatchEntry],
        fallback_label: Option<Label>,
        revert_label: Label,
    ) {
        if selectors.len() <= LINEAR_SELECTOR_DISPATCH_THRESHOLD {
            self.emit_linear_selector_dispatch(selectors, fallback_label, revert_label);
        } else {
            self.emit_binary_selector_dispatch(selectors, fallback_label, revert_label);
        }
    }

    fn emit_linear_selector_dispatch(
        &mut self,
        selectors: &[SelectorDispatchEntry],
        fallback_label: Option<Label>,
        revert_label: Label,
    ) {
        for entry in selectors {
            self.emit_selector_eq_jump(*entry);
        }
        self.emit_selector_dispatch_miss(fallback_label, revert_label);
    }

    fn emit_binary_selector_dispatch(
        &mut self,
        selectors: &[SelectorDispatchEntry],
        fallback_label: Option<Label>,
        revert_label: Label,
    ) {
        if selectors.len() <= LINEAR_SELECTOR_DISPATCH_THRESHOLD {
            self.emit_linear_selector_dispatch(selectors, fallback_label, revert_label);
            return;
        }

        let mid = selectors.len() / 2;
        let left_label = self.asm.new_label();

        // Stack has the selector. With the pivot pushed on top, GT checks
        // `pivot > selector`, so jump left when selector < pivot.
        self.asm.emit_op(op::dup(1));
        self.asm.emit_push(U256::from(selectors[mid].selector));
        self.asm.emit_op(op::GT);
        self.asm.emit_push_label(left_label);
        self.asm.emit_op(op::JUMPI);

        self.emit_binary_selector_dispatch(&selectors[mid..], fallback_label, revert_label);

        self.asm.define_label(left_label);
        self.emit_binary_selector_dispatch(&selectors[..mid], fallback_label, revert_label);
    }

    fn emit_selector_eq_jump(&mut self, entry: SelectorDispatchEntry) {
        self.asm.emit_op(op::dup(1));
        self.asm.emit_push(U256::from(entry.selector));
        self.asm.emit_op(op::EQ);
        self.asm.emit_push_label(entry.label);
        self.asm.emit_op(op::JUMPI);
    }

    fn emit_selector_dispatch_miss(&mut self, fallback_label: Option<Label>, _revert_label: Label) {
        if let Some(fallback_label) = fallback_label {
            // The unmatched selector below the fallback's stack model is
            // inert, like the one below regular wrappers.
            self.asm.emit_push_label(fallback_label);
            self.asm.emit_op(op::JUMP);
        } else {
            // Reverting inline is a byte shorter than jumping to the shared
            // revert stub, and terminal-span dedup still merges the copies.
            self.asm.emit_push(U256::ZERO);
            self.asm.emit_push(U256::ZERO);
            self.asm.emit_op(op::REVERT);
        }
    }

    /// Emits a payable check for non-payable functions.
    /// Non-payable, view, and pure functions revert if called with value.
    fn emit_payable_check(&mut self, func: &Function, revert_label: Label) {
        if Self::rejects_callvalue(func) {
            self.emit_callvalue_check(revert_label);
        }
    }

    fn rejects_callvalue(func: &Function) -> bool {
        use solar_sema::hir::StateMutability;

        matches!(
            func.attributes.state_mutability,
            StateMutability::NonPayable | StateMutability::View | StateMutability::Pure
        )
    }

    fn emit_callvalue_check(&mut self, revert_label: Label) {
        self.asm.emit_op(op::CALLVALUE);
        self.asm.emit_push_label(revert_label);
        self.asm.emit_op(op::JUMPI);
    }

    /// Generates bytecode for a function.
    #[allow(dead_code)]
    fn generate_function(&mut self, func: &Function) {
        self.generate_function_body(func);
    }

    /// Generates the body of a function.
    fn generate_function_body(&mut self, func: &Function) {
        // Run analysis pipeline via the pass manager.
        // Currently just liveness; future analyses (dominance, loops, etc.)
        // can be added here and queried from the same AnalysisManager.
        let mut am = AnalysisManager::new();
        let liveness: &Liveness = am.get_or_compute(&LivenessAnalysis, func);

        // Eliminate phis
        let phi_result = PhiEliminator::analyze(func);
        for (block_id, copies) in phi_result.block_copies {
            self.block_copies.insert(block_id, copies.copies);
        }
        let stack_phi_plan = StackPhiPlan::analyze(func);
        self.stack_phi_sources = stack_phi_plan.edge_sources.clone();
        let global_stack_plan = GlobalStackPlan::analyze(func, liveness, &stack_phi_plan);
        self.global_stack_active = !global_stack_plan.is_empty();
        self.global_stack_aliases = global_stack_plan.aliases.clone();

        // Reset scheduler
        self.scheduler = StackScheduler::new();
        self.spill_addr_consts.clear();

        self.preallocate_cross_block_spills(func, liveness);

        // Create labels for each block
        self.block_labels.clear();
        for block_id in func.blocks.indices() {
            let label = self.asm.new_label();
            if Self::block_is_cold(func, block_id) {
                self.asm.mark_label_cold(label);
            }
            self.block_labels.insert(block_id, label);
        }

        // Generate each block.
        let block_order = Self::block_layout_order(func);
        let block_pos: FxHashMap<BlockId, usize> =
            block_order.iter().enumerate().map(|(pos, &b)| (b, pos)).collect();
        // Stack layout a block must start with when it is reached by a stack-
        // preserving jump from its single predecessor (recorded by that
        // predecessor, restored here).
        let mut block_entry_stacks: FxHashMap<BlockId, StackModel> = FxHashMap::default();
        let mut preserved_fallthrough: Option<BlockId> = None;
        for (pos, &block_id) in block_order.iter().enumerate() {
            let block = &func.blocks[block_id];
            let fallthrough = block_order.get(pos + 1).copied();
            let entered_by_preserved_fallthrough = preserved_fallthrough == Some(block_id);
            preserved_fallthrough = None;

            let label = self.block_labels[&block_id];
            if !entered_by_preserved_fallthrough
                && (block_id != func.entry_block || !block.predecessors.is_empty())
            {
                self.asm.define_label(label);
            }

            // Reset stack at block entry unless the block is reached with a
            // known live stack: a physical fallthrough carries the scheduler's
            // stack directly, and a stack-preserving jump from a single
            // predecessor restores the recorded layout. All other cross-block
            // values live in spill slots.
            if !entered_by_preserved_fallthrough {
                if let Some(entry_stack) = block_entry_stacks.remove(&block_id) {
                    self.scheduler.stack = entry_stack;
                    // Live-ins not on the carried stack still arrive in memory.
                    self.mark_live_in_spills(func, liveness, block_id);
                } else if let Some(entry) = stack_phi_plan.entries.get(&block_id) {
                    self.set_stack_to_values(entry);
                    self.mark_live_in_spills(func, liveness, block_id);
                } else if let Some(entry) = global_stack_plan.entry(block_id) {
                    self.set_stack_to_values(entry);
                    self.mark_live_in_spills(func, liveness, block_id);
                } else {
                    self.scheduler.clear_stack();
                    self.mark_live_in_spills(func, liveness, block_id);
                }
            }

            // Generate instructions
            for (inst_idx, &inst_id) in block.instructions.iter().enumerate() {
                let inst = &func.instructions[inst_id];

                // Skip phi instructions (they're handled by copies)
                if matches!(inst.kind, InstKind::Phi(_)) {
                    continue;
                }

                // Find the value ID that corresponds to this instruction (if any)
                let result_value = func.inst_result_value(inst_id);

                // Generate the instruction
                self.generate_inst(func, &inst.kind, liveness, block_id, inst_idx, result_value);
                if let Some(result) = result_value {
                    self.spill_reserved_result_if_live(func, liveness, block_id, inst_idx, result);
                }
            }

            let stack_phi_preserved = stack_phi_plan.edges.get(&block_id).is_some_and(|edge| {
                if !self.can_prepare_stack_phi_edge(func, edge) {
                    return false;
                }
                self.spill_live_out_values_except(func, liveness, block_id, &edge.sources);
                self.pop_stack_values_not_needed_by(&edge.sources);
                self.try_emit_stack_phi_edge(func, edge)
            });

            // Insert phi copies before terminator. If the edge was materialized
            // as a stack-resident phi layout, the copies for this unconditional
            // predecessor are represented by the edge stack itself.
            if stack_phi_preserved {
                self.block_copies.remove(&block_id);
            } else if let Some(copies) = self.block_copies.remove(&block_id) {
                let mut temps = FxHashMap::default();
                for copy in &copies {
                    self.generate_copy(func, copy, &mut temps);
                }
            }

            let preserve_stack_to_fallthrough =
                self.can_preserve_stack_fallthrough(func, block_id, fallthrough);

            // A jump to a single-predecessor target that is emitted later can
            // keep its live stack instead of spilling: the target has exactly
            // one entry stack (this block's exit), so it can be restored there.
            let preserve_jump_target = (!preserve_stack_to_fallthrough)
                .then(|| self.single_pred_jump_target(func, block_id, fallthrough))
                .flatten()
                .filter(|target| block_pos.get(target).copied() > Some(pos));

            // A conditional branch whose other arm is a cold revert can carry
            // its single freshly-computed live-out on the stack into the hot
            // arm, which restores it as its recorded entry layout.
            let preserve_branch_targets =
                if !preserve_stack_to_fallthrough && preserve_jump_target.is_none() {
                    self.branch_preserve_targets(func, liveness, block_id, pos, &block_pos)
                } else {
                    Vec::new()
                };

            let global_stack_preserved = if !preserve_stack_to_fallthrough
                && preserve_jump_target.is_none()
                && preserve_branch_targets.is_empty()
                && !stack_phi_preserved
                && let Some(term) = block.terminator.as_ref()
                && let Some(layout) = global_stack_plan.edge_layout(func, term)
            {
                self.spill_live_out_values_except(func, liveness, block_id, layout);
                self.try_emit_global_stack_edge(func, term, layout)
            } else {
                false
            };

            let preserve_stack = preserve_stack_to_fallthrough
                || preserve_jump_target.is_some()
                || !preserve_branch_targets.is_empty()
                || stack_phi_preserved
                || global_stack_preserved;

            // Spill all live-out values before the terminator so they can be reloaded in successor
            // blocks. For a preserved edge, keep stack values live instead.
            if !preserve_stack {
                self.spill_live_out_values(func, liveness, block_id);
            }

            // Generate terminator
            if let Some(term) = &block.terminator {
                self.generate_terminator(func, term, fallthrough, preserve_stack);
            }
            if preserve_stack_to_fallthrough {
                preserved_fallthrough = fallthrough;
            } else if let Some(target) = preserve_jump_target {
                block_entry_stacks.insert(target, self.scheduler.stack.clone());
            }
            for target in preserve_branch_targets {
                block_entry_stacks.insert(target, self.scheduler.stack.clone());
            }
        }

        self.assign_ranked_spill_addrs(func);
    }

    /// Returns the target of a stack-preservable jump: the block ends in
    /// `Jump(T)` to a non-fallthrough, single-predecessor block with no phis
    /// (whose copies would otherwise interfere with the carried layout).
    fn single_pred_jump_target(
        &self,
        func: &Function,
        block_id: BlockId,
        fallthrough: Option<BlockId>,
    ) -> Option<BlockId> {
        let Some(Terminator::Jump(target)) = func.blocks[block_id].terminator.as_ref() else {
            return None;
        };
        if Some(*target) == fallthrough
            || func.blocks[*target].predecessors.as_slice() != [block_id]
        {
            return None;
        }
        let has_phi = func.blocks[*target]
            .instructions
            .iter()
            .any(|&inst| matches!(func.instructions[inst].kind, InstKind::Phi(_)));
        (!has_phi).then_some(*target)
    }

    /// Returns branch successors that can receive the current stack layout.
    ///
    /// This handles loop headers after stack-resident phi planning: the header
    /// computes the branch condition while the carried phi values remain below
    /// it. If both successors are private, later blocks, we can leave those
    /// values on the stack for both edges instead of spilling them before every
    /// loop condition.
    fn branch_preserve_targets(
        &self,
        func: &Function,
        liveness: &Liveness,
        block_id: BlockId,
        pos: usize,
        block_pos: &FxHashMap<BlockId, usize>,
    ) -> Vec<BlockId> {
        let Some(Terminator::Branch { condition, then_block, else_block }) =
            func.blocks[block_id].terminator.as_ref()
        else {
            return Vec::new();
        };

        if self.scheduler.stack.depth() <= 1 || self.scheduler.stack.top() != Some(*condition) {
            return Vec::new();
        }

        let Some(carried) = self
            .scheduler
            .stack
            .iter()
            .skip(1)
            .map(|slot| {
                let value = slot?;
                liveness.live_out(block_id).contains(value).then_some(value)
            })
            .collect::<Option<Vec<_>>>()
        else {
            return Vec::new();
        };
        if carried.len() > STACK_PHI_LAYOUT_LIMIT {
            return Vec::new();
        }

        let targets = [*then_block, *else_block];
        let mut live_in_any_target = FxHashSet::default();
        for target in targets {
            live_in_any_target.extend(liveness.live_in(target).iter());
        }
        if carried.iter().any(|value| !live_in_any_target.contains(value)) {
            return Vec::new();
        }

        for target in targets {
            if target == block_id
                || func.blocks[target].predecessors.as_slice() != [block_id]
                || block_pos.get(&target).copied() <= Some(pos)
                || func.blocks[target]
                    .instructions
                    .iter()
                    .any(|&inst| matches!(func.instructions[inst].kind, InstKind::Phi(_)))
            {
                return Vec::new();
            }
        }

        targets.into()
    }

    /// Returns true when a block is statically cold: it only aborts execution
    /// (revert or invalid), as panic and revert-string blocks do.
    fn block_is_cold(func: &Function, block_id: BlockId) -> bool {
        matches!(
            func.blocks[block_id].terminator,
            Some(Terminator::Revert { .. } | Terminator::Invalid)
        )
    }

    fn block_layout_order(func: &Function) -> Vec<BlockId> {
        let cfg = CfgInfo::new(func);
        let reachable = cfg.reachable();
        let mut order = Vec::with_capacity(func.blocks.len());
        let mut placed = FxHashSet::default();

        Self::append_layout_chain(func, func.entry_block, reachable, &mut placed, &mut order);
        for block_id in func.blocks.indices() {
            if reachable.contains(&block_id) {
                Self::append_layout_chain(func, block_id, reachable, &mut placed, &mut order);
            }
        }

        order
    }

    fn append_layout_chain(
        func: &Function,
        mut block_id: BlockId,
        reachable: &FxHashSet<BlockId>,
        placed: &mut FxHashSet<BlockId>,
        order: &mut Vec<BlockId>,
    ) {
        loop {
            if !reachable.contains(&block_id) || !placed.insert(block_id) {
                return;
            }
            order.push(block_id);

            let Some(Terminator::Jump(target)) = func.blocks[block_id].terminator.as_ref() else {
                return;
            };
            if placed.contains(target) || func.blocks[*target].predecessors.as_slice() != [block_id]
            {
                return;
            }

            block_id = *target;
        }
    }

    fn set_stack_to_values(&mut self, values: &[ValueId]) {
        self.scheduler.stack.clear();
        for &value in values.iter().rev() {
            self.scheduler.stack.push(value);
        }
    }

    fn try_emit_global_stack_edge(
        &mut self,
        func: &Function,
        term: &Terminator,
        layout: &[ValueId],
    ) -> bool {
        if layout.is_empty() || layout.len() > GLOBAL_STACK_LAYOUT_LIMIT {
            return false;
        }

        let mut needed = Vec::with_capacity(layout.len() + 1);
        if let Terminator::Branch { condition, .. } = term {
            needed.push(*condition);
        }
        needed.extend_from_slice(layout);

        self.pop_stack_values_not_needed_by(&needed);
        for value in Self::missing_stack_phi_sources(&self.scheduler.stack, &needed) {
            self.emit_operand(func, value);
        }

        let target: Vec<_> = needed.iter().copied().map(TargetSlot::Value).collect();
        let shuffle = self.scheduler.shuffle_to_layout(&target);
        for op in shuffle.ops {
            self.asm.emit_op(op.opcode());
        }

        self.scheduler.depth() == needed.len()
            && self.scheduler.stack.iter().eq(needed.iter().copied().map(Some))
    }

    fn try_emit_stack_phi_edge(&mut self, func: &Function, edge: &StackPhiEdge) -> bool {
        if edge.sources.len() != edge.results.len()
            || edge.sources.is_empty()
            || edge.sources.len() > STACK_PHI_LAYOUT_LIMIT
        {
            return false;
        }
        if !self.stack_contains_only_phi_sources(&edge.sources) {
            return false;
        }

        for &source in Self::missing_stack_phi_sources(&self.scheduler.stack, &edge.sources).iter()
        {
            if !self.scheduler.can_emit_value(source, func) {
                return false;
            }
            self.emit_operand(func, source);
        }
        if !self.stack_contains_only_phi_sources(&edge.sources) {
            return false;
        }

        let target: Vec<_> = edge.sources.iter().copied().map(TargetSlot::Value).collect();
        let shuffle = self.scheduler.shuffle_to_layout(&target);
        for op in shuffle.ops {
            self.asm.emit_op(op.opcode());
        }

        if self.scheduler.depth() != edge.sources.len() {
            return false;
        }
        self.set_stack_to_values(&edge.results);
        true
    }

    fn can_prepare_stack_phi_edge(&self, func: &Function, edge: &StackPhiEdge) -> bool {
        if edge.sources.len() != edge.results.len()
            || edge.sources.is_empty()
            || edge.sources.len() > STACK_PHI_LAYOUT_LIMIT
        {
            return false;
        }

        let present =
            Self::stack_phi_source_counts_after_trim(&self.scheduler.stack, &edge.sources);
        if present.len() > STACK_PHI_LAYOUT_LIMIT {
            return false;
        }

        let mut seen = Self::value_counts(present);
        for &source in &edge.sources {
            if let Some(count) = seen.get_mut(&source)
                && *count > 0
            {
                *count -= 1;
                continue;
            }
            if !self.scheduler.can_emit_value(source, func) {
                return false;
            }
        }
        true
    }

    fn stack_phi_source_counts_after_trim(stack: &StackModel, sources: &[ValueId]) -> Vec<ValueId> {
        let mut remaining = Self::value_counts(sources.iter().copied());
        let mut kept = Vec::new();
        for value in stack.iter().flatten() {
            if let Some(count) = remaining.get_mut(&value)
                && *count > 0
            {
                *count -= 1;
                kept.push(value);
            }
        }
        kept
    }

    fn stack_contains_only_phi_sources(&self, sources: &[ValueId]) -> bool {
        let mut remaining = Self::value_counts(sources.iter().copied());
        for slot in self.scheduler.stack.iter() {
            let Some(value) = slot else {
                return false;
            };
            let Some(count) = remaining.get_mut(&value) else {
                return false;
            };
            if *count == 0 {
                return false;
            }
            *count -= 1;
        }
        true
    }

    fn missing_stack_phi_sources(stack: &StackModel, sources: &[ValueId]) -> Vec<ValueId> {
        let mut needed = Self::value_counts(sources.iter().copied());
        for value in stack.iter().flatten() {
            if let Some(count) = needed.get_mut(&value)
                && *count > 0
            {
                *count -= 1;
            }
        }

        let mut missing = Vec::new();
        for &source in sources {
            if let Some(count) = needed.get_mut(&source)
                && *count > 0
            {
                missing.push(source);
                *count -= 1;
            }
        }
        missing
    }

    fn value_counts(values: impl IntoIterator<Item = ValueId>) -> FxHashMap<ValueId, usize> {
        let mut counts = FxHashMap::default();
        for value in values {
            *counts.entry(value).or_insert(0) += 1;
        }
        counts
    }

    fn can_preserve_stack_fallthrough(
        &self,
        func: &Function,
        block_id: BlockId,
        fallthrough: Option<BlockId>,
    ) -> bool {
        let Some(Terminator::Jump(target)) = func.blocks[block_id].terminator.as_ref() else {
            return false;
        };
        if Some(*target) != fallthrough {
            return false;
        }

        // This block is the target's only predecessor, so no non-fallthrough edge can observe or
        // depend on a JUMPDEST at the target label.
        func.blocks[*target].predecessors.as_slice() == [block_id]
    }

    fn is_stack_phi_source(&self, block: BlockId, value: ValueId) -> bool {
        self.stack_phi_sources.get(&block).is_some_and(|sources| sources.contains(&value))
    }

    /// Preallocates stable spill slots for values that may cross block boundaries.
    ///
    /// Blocks are emitted in layout order, not necessarily dominance order, so a block can be
    /// emitted before the predecessor that stores one of its live-in values. Reserving the slot up
    /// front lets the later load use a stable memory location; stores still happen only when the
    /// value is actually available on the stack.
    fn preallocate_cross_block_spills(&mut self, func: &Function, liveness: &Liveness) {
        for val in Self::cross_block_spill_values(func, liveness) {
            self.scheduler.spills.allocate(val);
        }
    }

    fn cross_block_spill_values(func: &Function, liveness: &Liveness) -> FxHashSet<ValueId> {
        let mut values = FxHashSet::default();
        for block_id in func.blocks.indices() {
            for val in liveness.live_in(block_id).iter().chain(liveness.live_out(block_id).iter()) {
                if matches!(func.value(val), crate::mir::Value::Inst(_)) {
                    values.insert(val);
                }
            }
            for &inst_id in &func.blocks[block_id].instructions {
                if matches!(func.instructions[inst_id].kind, InstKind::Phi(_))
                    && let Some(val) = func.inst_result_value(inst_id)
                {
                    values.insert(val);
                }
            }
        }
        // A cheap-recomputable value that is live-out is re-executed in the
        // successor block instead of spilled. That recomputation needs its
        // non-rematerializable operand leaves, which may be live only within the
        // defining block; spill those leaves too, or the recomputation reads a
        // value that is neither on the stack nor stored.
        let seeds: Vec<ValueId> = values.iter().copied().collect();
        for val in seeds {
            if Self::is_cheap_recomputable_value(func, val) {
                Self::collect_recompute_leaves(func, val, &mut values);
            }
        }
        values
    }

    /// Adds the non-rematerializable values that recomputing `val` depends on:
    /// operands are followed through further cheap-recomputable values, and
    /// every other non-rematerializable operand is a leaf that must be spilled.
    fn collect_recompute_leaves(func: &Function, val: ValueId, out: &mut FxHashSet<ValueId>) {
        let crate::mir::Value::Inst(inst_id) = func.value(val) else {
            return;
        };
        for op in func.instructions[*inst_id].kind.operands() {
            if Self::is_rematerializable_value(func, op) {
                continue;
            }
            if Self::is_cheap_recomputable_value(func, op) {
                Self::collect_recompute_leaves(func, op, out);
            } else {
                out.insert(op);
            }
        }
    }

    /// Spills all live-out values that are currently on the stack to memory.
    /// This ensures values that need to be accessed in successor blocks can be reloaded.
    fn spill_live_out_values(&mut self, func: &Function, liveness: &Liveness, block_id: BlockId) {
        let live_out = liveness.live_out(block_id);

        for val in live_out.iter() {
            self.spill_value_if_needed(func, val);
        }
    }

    fn spill_live_out_values_except(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block_id: BlockId,
        exempt: &[ValueId],
    ) {
        let exempt: FxHashSet<_> = exempt.iter().copied().collect();
        for val in liveness.live_out(block_id).iter() {
            if !exempt.contains(&val) {
                self.spill_value_if_needed(func, val);
            }
        }
    }

    fn pop_stack_values_not_needed_by(&mut self, needed: &[ValueId]) {
        while let Some(depth) = self.first_stack_value_not_needed_by(needed) {
            if depth > 0 {
                self.emit_stack_op(StackOp::Swap(depth as u8));
            }
            self.emit_stack_op(StackOp::Pop);
        }
    }

    fn first_stack_value_not_needed_by(&self, needed: &[ValueId]) -> Option<usize> {
        let mut remaining = Self::value_counts(needed.iter().copied());
        for (depth, slot) in self.scheduler.stack.iter().enumerate() {
            let Some(value) = slot else {
                return Some(depth);
            };
            let Some(count) = remaining.get_mut(&value) else {
                return Some(depth);
            };
            if *count == 0 {
                return Some(depth);
            }
            *count -= 1;
        }
        None
    }

    fn mark_live_in_spills(&mut self, func: &Function, liveness: &Liveness, block_id: BlockId) {
        // Values already on the stack (carried in from a preserved predecessor
        // edge) are read directly; marking them reloadable would point at a
        // spill slot that may never have been stored.
        for val in liveness.live_in(block_id).iter() {
            if !self.scheduler.stack.contains(val) && self.scheduler.spills.get(val).is_some() {
                self.scheduler.spills.mark_reloadable(val);
            }
        }
        for &inst_id in &func.blocks[block_id].instructions {
            if matches!(func.instructions[inst_id].kind, InstKind::Phi(_))
                && let Some(val) = func.inst_result_value(inst_id)
                && !self.scheduler.stack.contains(val)
                && self.scheduler.spills.get(val).is_some()
            {
                self.scheduler.spills.mark_reloadable(val);
            }
        }
    }

    fn spill_values_before_stack_clear(&mut self, func: &Function, values: &[ValueId]) {
        for &value in values {
            self.spill_value_if_needed(func, value);
        }
    }

    /// Spills a single value to memory if it's on the stack and not already stored.
    /// Skips immediates and args since they can be re-emitted without spilling.
    fn spill_value_if_needed(&mut self, func: &Function, val: ValueId) {
        // Skip immediates and args - they can be re-emitted without spilling
        match func.value(val) {
            crate::mir::Value::Immediate(_) | crate::mir::Value::Arg { .. } => return,
            _ => {}
        }

        if self.scheduler.spills.is_stored(val) {
            return;
        }

        if let Some(depth) = self.scheduler.stack.find(val) {
            let slot = self.scheduler.spills.allocate(val);
            if depth >= MAX_STACK_ACCESS {
                self.spill_deep_stack_value(func, val, slot, depth);
                return;
            }

            self.spill_accessible_stack_value(func, val, slot, depth);
        }
    }

    fn spill_value_to_reserved_slot(&mut self, func: &Function, val: ValueId) -> bool {
        if Self::is_rematerializable_value(func, val) || self.scheduler.spills.get(val).is_none() {
            return false;
        }

        let Some(depth) = self.scheduler.stack.find(val) else {
            return false;
        };
        let slot = self.scheduler.spills.allocate(val);
        if depth >= MAX_STACK_ACCESS {
            self.spill_deep_stack_value(func, val, slot, depth);
        } else {
            self.spill_accessible_stack_value(func, val, slot, depth);
        }
        true
    }

    fn spill_reserved_result_if_live(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
        value: ValueId,
    ) {
        // This is not the normal first-store path; `generate_inst` handles live-out results.
        // It repairs physical emission orders where a successor block emitted first has already
        // marked this reserved cross-block slot as stored/reloadable before the defining block
        // materializes the value.
        if self.scheduler.spills.get(value).is_none()
            || !self.scheduler.spills.is_stored(value)
            || liveness.is_dead_after(value, block, inst_idx)
        {
            return;
        }

        self.spill_value_to_reserved_slot(func, value);
    }

    fn spill_accessible_stack_value(
        &mut self,
        func: &Function,
        val: ValueId,
        slot: SpillSlot,
        depth: usize,
    ) {
        debug_assert!(depth < MAX_STACK_ACCESS);

        // DUP the value to top of stack for storing.
        // We need to DUP (not just use ensure_on_top) because:
        // 1. If value is on top, ensure_on_top does nothing but we need a copy
        // 2. MSTORE will consume the value, and we want to preserve the original
        let dup_n = (depth + 1) as u8;
        self.asm.emit_op(op::dup(dup_n));
        self.scheduler.stack.dup(dup_n);

        self.store_stack_top_to_spill(func, val, slot);
    }

    fn spill_deep_stack_value(
        &mut self,
        func: &Function,
        val: ValueId,
        slot: SpillSlot,
        depth: usize,
    ) {
        debug_assert!(depth >= MAX_STACK_ACCESS);

        let mut saved_above = Vec::with_capacity(depth + 1 - MAX_STACK_ACCESS);
        for _ in 0..(depth + 1 - MAX_STACK_ACCESS) {
            let Some(top) = self.scheduler.stack.top() else {
                panic!("cannot spill deep stack value {val:?}: untracked stack entry above it");
            };
            let top_slot = self.scheduler.spills.allocate(top);
            if self.scheduler.spills.is_reloadable(top) {
                self.emit_stack_op(StackOp::Pop);
            } else {
                self.store_stack_top_to_spill(func, top, top_slot);
            }
            saved_above.push((top, top_slot));
        }

        let Some(accessible_depth) = self.scheduler.stack.find(val) else {
            panic!("cannot spill deep stack value {val:?}: value disappeared while exposing it");
        };
        self.spill_accessible_stack_value(func, val, slot, accessible_depth);

        for (saved, saved_slot) in saved_above.into_iter().rev() {
            self.emit_spill_slot_addr(func, saved_slot);
            self.asm.emit_op(op::MLOAD);
            self.scheduler.stack.push(saved);
        }
    }

    fn store_stack_top_to_spill(&mut self, func: &Function, value: ValueId, slot: SpillSlot) {
        // Store to spill slot: PUSH offset, MSTORE.
        // The PUSH creates an untracked stack entry, so we track it as unknown.
        self.emit_spill_slot_addr(func, slot);
        self.scheduler.stack.push_unknown();

        self.asm.emit_op(op::MSTORE);
        // MSTORE consumes 2 values: the untracked offset and the value being spilled.
        self.scheduler.stack.pop();
        self.scheduler.stack.pop();
        self.scheduler.spills.mark_stored(value);
    }

    /// Spills operands that are live-out before an instruction consumes them.
    /// This ensures cross-block values are preserved in memory.
    fn spill_live_out_operands(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block_id: BlockId,
        operands: &[ValueId],
    ) {
        let live_out = liveness.live_out(block_id);

        for &op in operands {
            if live_out.contains(op) && !self.is_stack_phi_source(block_id, op) {
                self.spill_value_if_needed(func, op);
            }
        }
    }

    /// Values that are always re-emitted at each use instead of being kept on
    /// the stack or spilled.
    ///
    /// `Arg` MUST stay in this set. With static frames an argument reload is a
    /// 3-4 byte `PUSH addr; MLOAD`/`CALLDATALOAD`, cheaper than the spill
    /// traffic that tracking would create — and the spill machinery assumes
    /// arguments never own slots: making `Arg` non-rematerializable was
    /// measured to REGRESS every bench contract's size (erc20 +61 B, maple
    /// +72 B, fractional +127 B) and to break 4 of 8 bench harnesses at
    /// runtime. Do not re-attempt without redesigning argument spilling.
    fn is_rematerializable_value(func: &Function, value: ValueId) -> bool {
        matches!(func.value(value), crate::mir::Value::Immediate(_) | crate::mir::Value::Arg { .. })
    }

    fn is_cheap_recomputable_value(func: &Function, value: ValueId) -> bool {
        let crate::mir::Value::Inst(inst_id) = func.value(value) else {
            return false;
        };
        matches!(
            func.instruction(*inst_id).kind,
            InstKind::Add(_, _)
                | InstKind::Sub(_, _)
                | InstKind::Mul(_, _)
                | InstKind::And(_, _)
                | InstKind::Or(_, _)
                | InstKind::Xor(_, _)
                | InstKind::Shl(_, _)
                | InstKind::Shr(_, _)
                | InstKind::Sar(_, _)
        )
    }

    /// Returns true when `value` needs no spill before the instruction that
    /// is about to consume it: it owns no reserved cross-block slot, it is
    /// not live out of the block, and more stack copies exist at this point
    /// than the instruction will consume net of the emissions still to come
    /// (`consumed`). Later in-block uses DUP the survivor, or deep-spill it
    /// on demand if it sinks past `MAX_STACK_ACCESS`, so skipping the store
    /// cannot strand the value and adds no stack depth.
    fn block_local_copy_survives(
        &self,
        liveness: &Liveness,
        block: BlockId,
        value: ValueId,
        consumed: usize,
    ) -> bool {
        self.scheduler.spills.get(value).is_none()
            && !liveness.live_out(block).contains(value)
            && self.scheduler.stack.iter().flatten().filter(|&v| v == value).count() > consumed
    }

    fn spill_top_value_if_live(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
        value: ValueId,
    ) {
        if Self::is_rematerializable_value(func, value) {
            return;
        }

        let has_reserved_cross_block_slot = self.scheduler.spills.get(value).is_some();
        if liveness.is_dead_after(value, block, inst_idx) && !has_reserved_cross_block_slot {
            return;
        }

        debug_assert_eq!(self.scheduler.stack.top(), Some(value));
        if !self.spill_value_to_reserved_slot(func, value) {
            self.spill_value_if_needed(func, value);
        }
    }

    /// Generates bytecode for an instruction.
    fn generate_inst(
        &mut self,
        func: &Function,
        kind: &InstKind,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
        result_value: Option<ValueId>,
    ) {
        let operands = kind.operands();
        // Keep one lazy stack copy of an argument when this instruction is not
        // its last use. The consuming occurrence uses a DUP of that copy, so
        // later blocks can inherit it without an eager prologue load.
        for &operand in &operands {
            if self.global_stack_active
                && matches!(func.value(operand), crate::mir::Value::Arg { .. })
                && !self.scheduler.stack.contains(operand)
                && !liveness.is_dead_after(operand, block, inst_idx)
            {
                self.emit_value(func, operand);
            }
        }

        // Spill any operands that are live-out before they get consumed.
        // This ensures cross-block values are preserved in memory.
        self.spill_live_out_operands(func, liveness, block, &operands);

        match kind {
            // Binary arithmetic operations
            InstKind::Add(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::ADD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Sub(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::SUB,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Mul(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::MUL,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Div(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::DIV,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SDiv(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::SDIV,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Mod(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::MOD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SMod(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::SMOD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Exp(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::EXP,
                result_value,
                liveness,
                block,
                inst_idx,
            ),

            // Bitwise operations
            InstKind::And(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::AND,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Or(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::OR,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Xor(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::XOR,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Not(a) => self.emit_unary_op_with_result(
                func,
                *a,
                op::NOT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Shl(shift, val) => self.emit_binary_op_with_result(
                func,
                *shift,
                *val,
                op::SHL,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Shr(shift, val) => self.emit_binary_op_with_result(
                func,
                *shift,
                *val,
                op::SHR,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Sar(shift, val) => self.emit_binary_op_with_result(
                func,
                *shift,
                *val,
                op::SAR,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Byte(i, x) => self.emit_binary_op_with_result(
                func,
                *i,
                *x,
                op::BYTE,
                result_value,
                liveness,
                block,
                inst_idx,
            ),

            // Comparison operations - track results for branch conditions and Select
            InstKind::Lt(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::LT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Gt(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::GT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SLt(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::SLT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SGt(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::SGT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Eq(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::EQ,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::IsZero(a) => self.emit_unary_op_with_result(
                func,
                *a,
                op::ISZERO,
                result_value,
                liveness,
                block,
                inst_idx,
            ),

            // Memory operations
            // Track MLOAD results so they can be used as operands in subsequent instructions.
            // This is essential for nested external calls where the return value from one call
            // becomes an argument to another call.
            InstKind::MLoad(addr) => self.emit_unary_op_with_result(
                func,
                *addr,
                op::MLOAD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::MStore(addr, val) => self.emit_store_op_live_aware(
                func,
                *addr,
                *val,
                op::MSTORE,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::MStore8(addr, val) => self.emit_store_op_live_aware(
                func,
                *addr,
                *val,
                op::MSTORE8,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::MSize => {
                self.asm.emit_op(op::MSIZE);
                self.scheduler.instruction_executed(0, result_value);
            }

            // Storage operations
            InstKind::SLoad(slot) => self.emit_unary_op_with_result(
                func,
                *slot,
                op::SLOAD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SStore(slot, val) => self.emit_store_op_live_aware(
                func,
                *slot,
                *val,
                op::SSTORE,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::TLoad(slot) => self.emit_unary_op_with_result(
                func,
                *slot,
                op::TLOAD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::TStore(slot, val) => self.emit_store_op_live_aware(
                func,
                *slot,
                *val,
                op::TSTORE,
                liveness,
                block,
                inst_idx,
            ),

            // Calldata operations
            InstKind::CalldataLoad(off) => self.emit_unary_op_with_result(
                func,
                *off,
                op::CALLDATALOAD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::CalldataSize => {
                self.asm.emit_op(op::CALLDATASIZE);
                self.scheduler.instruction_executed(0, result_value);
            }

            // Hash operations
            InstKind::Keccak256(off, len) => self.emit_binary_op_with_result(
                func,
                *off,
                *len,
                op::KECCAK256,
                result_value,
                liveness,
                block,
                inst_idx,
            ),

            // Environment operations
            InstKind::Caller => {
                self.asm.emit_op(op::CALLER);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::CallValue => {
                self.asm.emit_op(op::CALLVALUE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Address => {
                self.asm.emit_op(op::ADDRESS);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Origin => {
                self.asm.emit_op(op::ORIGIN);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::GasPrice => {
                self.asm.emit_op(op::GASPRICE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Gas => {
                self.asm.emit_op(op::GAS);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Timestamp => {
                self.asm.emit_op(op::TIMESTAMP);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::BlockNumber => {
                self.asm.emit_op(op::NUMBER);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Coinbase => {
                self.asm.emit_op(op::COINBASE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::ChainId => {
                self.asm.emit_op(op::CHAINID);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::SelfBalance => {
                self.asm.emit_op(op::SELFBALANCE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::BaseFee => {
                self.asm.emit_op(op::BASEFEE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::BlobBaseFee => {
                self.asm.emit_op(op::BLOBBASEFEE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::GasLimit => {
                self.asm.emit_op(op::GASLIMIT);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::PrevRandao => {
                self.asm.emit_op(op::PREVRANDAO);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Balance(addr) => self.emit_unary_op_with_result(
                func,
                *addr,
                op::BALANCE,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::BlockHash(num) => self.emit_unary_op_with_result(
                func,
                *num,
                op::BLOCKHASH,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::BlobHash(idx) => self.emit_unary_op_with_result(
                func,
                *idx,
                op::BLOBHASH,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::ExtCodeSize(addr) => self.emit_unary_op_with_result(
                func,
                *addr,
                op::EXTCODESIZE,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::ExtCodeHash(addr) => self.emit_unary_op_with_result(
                func,
                *addr,
                op::EXTCODEHASH,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::CodeSize => {
                self.asm.emit_op(op::CODESIZE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::LoadImmutable(offset) => {
                if self.in_constructor {
                    // The running constructor's own placeholders are never
                    // patched; read the staged scratch word instead.
                    self.asm.emit_push(U256::from(IMMUTABLE_SCRATCH_BASE + u64::from(*offset)));
                    self.asm.emit_op(op::MLOAD);
                } else {
                    self.asm.emit_push_immutable(*offset);
                }
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::ReturnDataSize => {
                self.asm.emit_op(op::RETURNDATASIZE);
                self.scheduler.instruction_executed(0, result_value);
            }

            // Ternary operations
            InstKind::AddMod(a, b, n) => {
                self.emit_ternary_op(func, *a, *b, *n, op::ADDMOD, result_value)
            }
            InstKind::MulMod(a, b, n) => {
                self.emit_ternary_op(func, *a, *b, *n, op::MULMOD, result_value)
            }

            // Select is like a ternary conditional
            InstKind::Select(cond, true_val, false_val) => {
                // select(cond, t, f) = f + cond * (t - f)
                //
                // We emit all three values to the stack, then do inline computation.
                // Stack notation: rightmost = top (depth 0).
                // Stack after emit_value calls: [f, t, cond] with cond on top.

                self.emit_value(func, *false_val); // Stack: [f]
                self.emit_operand(func, *true_val); // Stack: [f, t]
                self.emit_operand(func, *cond); // Stack: [f, t, cond]

                // Now compute: f + cond * (t - f)
                // Stack is [f, t, cond] with cond on top (depth 0), t at depth 1, f at depth 2
                //
                // Step 1: DUP3 to get f -> [f, t, cond, f]
                self.emit_stack_op(StackOp::Dup(3));
                // Step 2: DUP3 to get t (now at depth 2) -> [f, t, cond, f, t]
                self.emit_stack_op(StackOp::Dup(3));
                // Step 3: SUB (top - second = t - f) -> [f, t, cond, t-f]
                self.emit_op_with_effect(
                    op::SUB,
                    StackEffect { pops: 2, pushes: 1 },
                    StackPush::Unknown,
                );
                // Step 4: MUL (cond * (t-f)) -> [f, t, cond*(t-f)]
                self.emit_op_with_effect(
                    op::MUL,
                    StackEffect { pops: 2, pushes: 1 },
                    StackPush::Unknown,
                );
                // Step 5: SWAP1 -> [f, cond*(t-f), t]
                self.emit_stack_op(StackOp::Swap(1));
                // Step 6: POP (remove t) -> [f, cond*(t-f)]
                self.emit_stack_op(StackOp::Pop);
                // Step 7: ADD (cond*(t-f) + f = f + cond*(t-f)) -> [result]
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(op::ADD, StackEffect { pops: 2, pushes: 1 }, push);
            }

            // Sign extend
            InstKind::SignExtend(b, x) => self.emit_binary_op_with_result(
                func,
                *b,
                *x,
                op::SIGNEXTEND,
                result_value,
                liveness,
                block,
                inst_idx,
            ),

            // Phi nodes are skipped (handled by copies)
            InstKind::Phi(_) => {}

            // Contract creation
            InstKind::Create(value, offset, size) => {
                self.emit_value(func, *size);
                self.emit_operand(func, *offset);
                self.emit_operand(func, *value);
                self.asm.emit_op(op::CREATE);
                // CREATE consumes 3 values and produces 1 (new contract address)
                self.scheduler.instruction_executed(3, result_value);
            }

            InstKind::Create2(value, offset, size, salt) => {
                // CREATE2 expects stack (top to bottom): salt, size, offset, value
                // So we push in reverse order: value first (goes deepest), then offset, size, salt
                self.emit_value(func, *value);
                self.emit_operand(func, *offset);
                self.emit_operand(func, *size);
                self.emit_operand(func, *salt);
                self.asm.emit_op(op::CREATE2);
                // CREATE2 consumes 4 values and produces 1 (new contract address)
                self.scheduler.instruction_executed(4, result_value);
            }

            // External calls
            //
            // These use emit_value_fresh to guarantee correct values regardless of scheduler state.
            // The stack-aware emit_op_with_effect ensures proper tracking after emission.
            InstKind::Call { gas, addr, value, args_offset, args_size, ret_offset, ret_size } => {
                // CALL(gas, addr, value, argsOffset, argsSize, retOffset, retSize)
                // EVM pops in order: gas (TOS), addr, value, argsOffset, argsSize, retOffset,
                // retSize So we push in reverse order: retSize first (deepest), gas
                // last (TOS)
                self.emit_value_fresh(func, *ret_size);
                self.emit_value_fresh(func, *ret_offset);
                self.emit_value_fresh(func, *args_size);
                self.emit_value_fresh(func, *args_offset);
                self.emit_value_fresh(func, *value);
                self.emit_value_fresh(func, *addr);
                self.emit_value_fresh(func, *gas);

                // CALL consumes 7 values and produces 1 (success bool)
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(op::CALL, StackEffect { pops: 7, pushes: 1 }, push);
            }

            InstKind::StaticCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                // STATICCALL(gas, addr, argsOffset, argsSize, retOffset, retSize)
                self.emit_value_fresh(func, *ret_size);
                self.emit_value_fresh(func, *ret_offset);
                self.emit_value_fresh(func, *args_size);
                self.emit_value_fresh(func, *args_offset);
                self.emit_value_fresh(func, *addr);
                self.emit_value_fresh(func, *gas);
                // STATICCALL consumes 6 values and produces 1 (success bool)
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(op::STATICCALL, StackEffect { pops: 6, pushes: 1 }, push);
            }

            InstKind::DelegateCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                // DELEGATECALL(gas, addr, argsOffset, argsSize, retOffset, retSize)
                self.emit_value_fresh(func, *ret_size);
                self.emit_value_fresh(func, *ret_offset);
                self.emit_value_fresh(func, *args_size);
                self.emit_value_fresh(func, *args_offset);
                self.emit_value_fresh(func, *addr);
                self.emit_value_fresh(func, *gas);
                // DELEGATECALL consumes 6 values and produces 1 (success bool)
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(
                    op::DELEGATECALL,
                    StackEffect { pops: 6, pushes: 1 },
                    push,
                );
            }

            InstKind::InternalCall { function, args, returns } => {
                self.emit_internal_call(
                    func,
                    *function,
                    args,
                    *returns as usize,
                    result_value,
                    liveness,
                    block,
                    inst_idx,
                );
            }

            InstKind::InternalFrameAddr(offset) => {
                self.emit_own_frame_addr(*offset);
                if let Some(result) = result_value {
                    self.scheduler.stack.push(result);
                }
            }

            // Log operations
            InstKind::Log0(offset, size) => {
                // LOG0(offset, size) - stack order: offset on top, then size
                self.emit_log(func, op::LOG0, &[*size, *offset], liveness, block, inst_idx);
            }
            InstKind::Log1(offset, size, topic1) => {
                // LOG1(offset, size, topic1) - stack order: offset, size, topic1
                self.emit_log(
                    func,
                    op::LOG1,
                    &[*topic1, *size, *offset],
                    liveness,
                    block,
                    inst_idx,
                );
            }
            InstKind::Log2(offset, size, topic1, topic2) => {
                // LOG2(offset, size, topic1, topic2) - stack order: offset, size, topic1, topic2
                self.emit_log(
                    func,
                    op::LOG2,
                    &[*topic2, *topic1, *size, *offset],
                    liveness,
                    block,
                    inst_idx,
                );
            }
            InstKind::Log3(offset, size, topic1, topic2, topic3) => {
                // LOG3(offset, size, topic1, topic2, topic3)
                self.emit_log(
                    func,
                    op::LOG3,
                    &[*topic3, *topic2, *topic1, *size, *offset],
                    liveness,
                    block,
                    inst_idx,
                );
            }
            InstKind::Log4(offset, size, topic1, topic2, topic3, topic4) => {
                // LOG4(offset, size, topic1, topic2, topic3, topic4)
                self.emit_log(
                    func,
                    op::LOG4,
                    &[*topic4, *topic3, *topic2, *topic1, *size, *offset],
                    liveness,
                    block,
                    inst_idx,
                );
            }

            // Memory copy operations
            InstKind::CalldataCopy(dest, offset, size) => {
                // CALLDATACOPY(destOffset, offset, size)
                self.emit_copy_op_live_aware(
                    func,
                    &[*size, *offset, *dest],
                    op::CALLDATACOPY,
                    liveness,
                    block,
                    inst_idx,
                );
            }

            InstKind::CodeCopy(dest, offset, size) => {
                // CODECOPY(destOffset, offset, size)
                self.emit_copy_op_live_aware(
                    func,
                    &[*size, *offset, *dest],
                    op::CODECOPY,
                    liveness,
                    block,
                    inst_idx,
                );
            }

            InstKind::ReturnDataCopy(dest, offset, size) => {
                // RETURNDATACOPY(destOffset, offset, size)
                self.emit_copy_op_live_aware(
                    func,
                    &[*size, *offset, *dest],
                    op::RETURNDATACOPY,
                    liveness,
                    block,
                    inst_idx,
                );
            }

            InstKind::MCopy(dest, src, size) => {
                // MCOPY(destOffset, srcOffset, size)
                self.emit_copy_op_live_aware(
                    func,
                    &[*size, *src, *dest],
                    op::MCOPY,
                    liveness,
                    block,
                    inst_idx,
                );
            }

            InstKind::ExtCodeCopy(addr, dest, offset, size) => {
                // EXTCODECOPY(address, destOffset, offset, size)
                self.emit_copy_op_live_aware(
                    func,
                    &[*size, *offset, *dest, *addr],
                    op::EXTCODECOPY,
                    liveness,
                    block,
                    inst_idx,
                );
            }
        }

        if let Some(result) = result_value
            && liveness.live_out(block).contains(result)
            && !self.is_stack_phi_source(block, result)
        {
            self.spill_value_if_needed(func, result);
        }

        // A constant-offset calldata load is the same physical word as the
        // corresponding external argument. Once its instruction result dies,
        // adopt a surviving stack copy as the argument instead of loading that
        // word again on the first planned edge.
        for operand in operands {
            if liveness.is_dead_after(operand, block, inst_idx)
                && let Some(&arg) = self.global_stack_aliases.get(&operand)
                && !liveness.is_dead_after(arg, block, inst_idx)
                && !self.scheduler.stack.contains(arg)
            {
                self.scheduler.stack.rename(operand, arg);
            }
        }

        // Drop dead values after the instruction
        let dead_ops = self.scheduler.drop_dead_values(liveness, block, inst_idx);
        for op in dead_ops {
            self.asm.emit_op(op.opcode());
        }
        #[cfg(debug_assertions)]
        {
            debug_assert!(self.scheduler.depth() <= 1024);
        }
    }

    fn emit_new_internal_frame_base_tracked(&mut self) {
        self.asm.emit_push(U256::from(0x40));
        self.asm.emit_op(op::MLOAD);
        self.scheduler.stack.push_unknown();
    }

    fn emit_internal_frame_store_from_top_preserving_base(&mut self, offset: u64) {
        self.emit_stack_op(StackOp::Dup(2));
        if offset != 0 {
            self.asm.emit_push(U256::from(offset));
            self.scheduler.stack.push_unknown();
            self.emit_op_with_effect(
                op::ADD,
                StackEffect { pops: 2, pushes: 1 },
                StackPush::Unknown,
            );
        }
        self.asm.emit_op(op::MSTORE);
        self.scheduler.instruction_executed(2, None);
    }

    fn emit_store_frame_base_to_current_frame_slot(&mut self) {
        self.emit_stack_op(StackOp::Dup(1));
        self.asm.emit_push(U256::from(INTERNAL_FRAME_PTR_SLOT));
        self.scheduler.stack.push_unknown();
        self.asm.emit_op(op::MSTORE);
        self.scheduler.instruction_executed(2, None);
    }

    fn emit_store_new_free_pointer_from_frame_base(&mut self, frame_size: DeferredConst) {
        self.asm.emit_push_deferred(frame_size);
        self.scheduler.stack.push_unknown();
        self.emit_op_with_effect(op::ADD, StackEffect { pops: 2, pushes: 1 }, StackPush::Unknown);
        self.asm.emit_push(U256::from(0x40));
        self.scheduler.stack.push_unknown();
        self.asm.emit_op(op::MSTORE);
        self.scheduler.instruction_executed(2, None);
    }

    /// Address of `offset` within whatever frame the frame-pointer slot
    /// currently holds. Dynamic call sites use this to reach the callee frame
    /// right after a call (before the pointer is restored); dynamic functions
    /// use it for their own frame. For accesses that are statically about the
    /// CURRENT function's own frame, use [`Self::emit_own_frame_addr`], which
    /// resolves to an absolute address when the function has a static frame.
    fn emit_current_internal_frame_addr(&mut self, offset: u64) {
        self.asm.emit_push(U256::from(INTERNAL_FRAME_PTR_SLOT));
        self.asm.emit_op(op::MLOAD);
        if offset != 0 {
            self.asm.emit_push(U256::from(offset));
            self.asm.emit_op(op::ADD);
        }
    }

    /// Address of `offset` within the current function's own frame: a single
    /// absolute push for static-frame functions, the frame-pointer indirection
    /// otherwise.
    fn emit_own_frame_addr(&mut self, offset: u64) {
        if let Some(func_id) = self.current_internal_function
            && self.static_frame_functions.contains(&func_id)
        {
            let addr = self.static_frame_addr(func_id, offset);
            self.asm.emit_push_deferred(addr);
            return;
        }
        self.emit_current_internal_frame_addr(offset);
    }

    /// Interns the deferred constant for absolute address `base(func_id) +
    /// offset`. The value is set by [`Self::resolve_static_frames`] once every
    /// body has emitted and exact spill sizes are known; the assembler then
    /// encodes it as a minimal-width push.
    /// Computes which arguments of each static-frame callee pass on the
    /// stack. A site can always deliver a stack argument: raw re-emission
    /// after the drain for immediates and position-independently reloadable
    /// caller arguments, or a spill-slot reload for computed values. The
    /// per-argument choice is scored across all sites — raw and
    /// already-stored (cross-block) values save the four-byte frame store,
    /// while a fresh block-local value must first pay its own spill — and an
    /// argument passes on the stack when the sites' savings outweigh the
    /// callee's one-time prologue store. A callee reached by an
    /// argument-carrying tail call keeps the plain convention.
    fn compute_stack_arg_masks(&mut self, module: &Module) {
        let mut scores: FxHashMap<FunctionId, Vec<i32>> = FxHashMap::default();
        let mut excluded: FxHashSet<FunctionId> = FxHashSet::default();
        for (caller_id, func) in module.functions.iter_enumerated() {
            let caller_is_entry = Self::is_external_entry(func);
            let caller_static = self.static_frame_functions.contains(&caller_id);
            let raw_leaves_ok = caller_is_entry || caller_static;
            // Where each instruction result is defined, to spot cross-block
            // arguments (already stored at their definition).
            let mut inst_block: FxHashMap<InstId, usize> = FxHashMap::default();
            let mut use_counts: FxHashMap<ValueId, usize> = FxHashMap::default();
            for (block_idx, block) in func.blocks.iter().enumerate() {
                for &inst_id in &block.instructions {
                    inst_block.insert(inst_id, block_idx);
                    for operand in func.instructions[inst_id].kind.operands() {
                        *use_counts.entry(operand).or_default() += 1;
                    }
                }
                if let Some(term) = &block.terminator {
                    for operand in term.operands() {
                        *use_counts.entry(operand).or_default() += 1;
                    }
                }
            }
            for (block_idx, block) in func.blocks.iter().enumerate() {
                if let Some(Terminator::TailCall { function, args }) = &block.terminator
                    && !args.is_empty()
                {
                    excluded.insert(*function);
                }
                for &inst_id in &block.instructions {
                    let InstKind::InternalCall { function, args, .. } =
                        &func.instructions[inst_id].kind
                    else {
                        continue;
                    };
                    let score = scores.entry(*function).or_insert_with(|| vec![0; args.len()]);
                    if score.len() != args.len() {
                        excluded.insert(*function);
                        continue;
                    }
                    for (i, &arg) in args.iter().enumerate() {
                        score[i] += if Self::raw_arg_emittable(func, raw_leaves_ok, arg) {
                            // The frame store disappears outright.
                            4
                        } else if !raw_leaves_ok {
                            // A dynamic-frame caller cannot reload a spill
                            // slot without its frame pointer; it can only
                            // deliver raw values, so this argument must stay
                            // frame-passed everywhere.
                            -100_000
                        } else {
                            match func.value(arg) {
                                crate::mir::Value::Inst(def)
                                    if inst_block.get(def) != Some(&block_idx) =>
                                {
                                    // Cross-block values are stored at their
                                    // definition; the site keeps only the
                                    // slot reload it would have paid anyway.
                                    4
                                }
                                crate::mir::Value::Inst(_)
                                    if use_counts.get(&arg).copied().unwrap_or(0) > 1 =>
                                {
                                    // Multi-use block-local values usually
                                    // have a stack copy; the extra spill is
                                    // partially amortized.
                                    1
                                }
                                // A fresh single-use value pays a spill it
                                // did not need before.
                                _ => -5,
                            }
                        };
                    }
                }
            }
        }
        scores.retain(|func_id, _| {
            self.static_frame_functions.contains(func_id) && !excluded.contains(func_id)
        });
        let mut masks: FxHashMap<FunctionId, Vec<bool>> = FxHashMap::default();
        for (func_id, score) in scores {
            // The callee prologue pays one store per stack argument.
            let mask: Vec<bool> = score.iter().map(|&benefit| benefit > 4).collect();
            if mask.iter().any(|&stack| stack) {
                masks.insert(func_id, mask);
            }
        }
        self.stack_arg_masks = masks;
    }

    /// Returns true when the caller can re-emit `val` raw (untracked) after
    /// its stack drain: an immediate, or a caller argument whose reload is
    /// position independent.
    fn raw_arg_emittable(func: &Function, raw_leaves_ok: bool, val: ValueId) -> bool {
        match func.value(val) {
            crate::mir::Value::Immediate(imm) => imm.as_u256().is_some(),
            crate::mir::Value::Arg { .. } => raw_leaves_ok,
            _ => false,
        }
    }

    /// Emits a mask-qualified argument without touching the scheduler model:
    /// the value lands on the physical stack for the callee prologue, below
    /// everything the caller's model describes.
    fn emit_raw_stack_arg(&mut self, func: &Function, val: ValueId) {
        match func.value(val) {
            crate::mir::Value::Immediate(imm) => {
                self.asm.emit_push(imm.as_u256().expect("mask requires a word immediate"));
            }
            crate::mir::Value::Arg { index, .. } => {
                if self.in_internal_function {
                    let func_id = self
                        .current_internal_function
                        .expect("internal caller has a current function");
                    let addr = self.static_frame_addr(func_id, 64 + u64::from(*index) * 32);
                    self.asm.emit_push_deferred(addr);
                    self.asm.emit_op(op::MLOAD);
                } else {
                    self.asm.emit_push(U256::from(4 + u64::from(*index) * 32));
                    self.asm.emit_op(op::CALLDATALOAD);
                }
            }
            crate::mir::Value::Inst(_) => {
                let slot = self
                    .scheduler
                    .spills
                    .get(val)
                    .expect("computed stack argument has a spill slot");
                self.emit_spill_slot_addr(func, slot);
                self.asm.emit_op(op::MLOAD);
            }
            other => unreachable!("stack-arg mask admitted an unsupported value: {other:?}"),
        }
    }

    /// Stores the stack-passed arguments of `func_id` into their frame slots.
    /// Arguments were pushed in index order, so the highest index is on top;
    /// after the last store only the return address remains above the
    /// caller's drained stack.
    fn emit_stack_arg_prologue(&mut self, func_id: FunctionId, func: &Function) {
        if !self.runtime_stack_args {
            return;
        }
        let Some(mask) = self.stack_arg_masks.get(&func_id).cloned() else { return };
        if mask.len() != func.params.len() {
            return;
        }
        for i in (0..mask.len()).rev() {
            if mask[i] {
                let addr = self.static_frame_addr(func_id, 64 + i as u64 * 32);
                self.asm.emit_push_deferred(addr);
                self.asm.emit_op(op::MSTORE);
            }
        }
    }

    fn static_frame_addr(&mut self, func_id: FunctionId, offset: u64) -> DeferredConst {
        if let Some(&id) = self.static_frame_addr_consts.get(&(func_id, offset)) {
            return id;
        }
        let id = self.asm.new_deferred_const();
        self.static_frame_addr_consts.insert((func_id, offset), id);
        id
    }

    /// Total frame size of `func_id`: the fixed prefix plus the exact spill
    /// area recorded after its body emitted (conservative when unavailable).
    fn emitted_frame_size(&self, module: &Module, func_id: FunctionId) -> u64 {
        let func = &module.functions[func_id];
        let spill = self
            .function_spill_sizes
            .get(&func_id)
            .copied()
            .unwrap_or_else(|| Self::conservative_spill_frame_size(func));
        64 + ((func.params.len() + func.returns.len()) as u64) * 32
            + func.internal_frame_size
            + spill
    }

    /// Places every referenced static frame and resolves the address and
    /// free-memory-pointer constants recorded during this pass.
    ///
    /// Placement is an overlay: `base(f) = region_start + depth(f)`, where
    /// `depth(f)` is the longest chain of static frames that can be live below
    /// an activation of `f`. Depth propagates along every call edge — a static
    /// caller contributes its frame size, a dynamic caller (recursive, or an
    /// external entry whose locals live below the region) only forwards its
    /// own depth, so a static function reached THROUGH a dynamic one is still
    /// placed above its static ancestors. Static functions are acyclic by
    /// construction, so every cycle in the graph is weight-zero and the
    /// relaxation converges. Functions that can never be simultaneously live
    /// end up sharing addresses; that is the point of the overlay.
    ///
    /// The heap floor moves up to `region_end`: each entry's free-pointer
    /// constant is the maximum entry frame end when no static frame was
    /// referenced (today's behavior), and `region_end` otherwise.
    fn resolve_static_frames(&mut self, module: &Module) {
        let max_entry_base = std::mem::take(&mut self.runtime_entry_funcs)
            .into_iter()
            .map(|func_id| {
                let func = &module.functions[func_id];
                let spill = self
                    .function_spill_sizes
                    .get(&func_id)
                    .copied()
                    .unwrap_or_else(|| Self::conservative_spill_frame_size(func));
                Self::external_spill_base(func) + spill
            })
            .max()
            .unwrap_or(0);

        if self.static_frame_addr_consts.is_empty() {
            if let Some(id) = self.runtime_free_memory_const.take() {
                self.asm.set_deferred_const(id, U256::from(max_entry_base));
            }
            return;
        }

        let region_start = max_entry_base.max(INTERNAL_FRAME_PTR_SLOT + 32);

        // Longest live-chain depth below each function, over all call edges.
        // Only emitted callers count: an unemitted function (an internal
        // `.body` clone nobody calls, dispatched-away dead code) stacks no
        // real frame below its callees, and its conservative spill estimate
        // would inflate every callee's depth — and with it the free-memory
        // start and the width of every frame push in the runtime.
        let mut edges = Vec::new();
        for (func_id, func) in module.functions.iter_enumerated() {
            if !self.function_labels.contains_key(&func_id) {
                continue;
            }
            for inst in &func.instructions {
                if let InstKind::InternalCall { function, .. } = inst.kind {
                    edges.push((func_id, function));
                }
            }
            for block in func.blocks.iter() {
                if let Some(Terminator::TailCall { function, .. }) = &block.terminator {
                    edges.push((func_id, *function));
                }
            }
        }
        let mut depth: FxHashMap<FunctionId, u64> = FxHashMap::default();
        for _ in 0..=module.functions.len() {
            let mut changed = false;
            for &(caller, callee) in &edges {
                let mut contribution = depth.get(&caller).copied().unwrap_or(0);
                if self.static_frame_functions.contains(&caller) {
                    contribution += self.emitted_frame_size(module, caller);
                }
                if contribution > depth.get(&callee).copied().unwrap_or(0) {
                    depth.insert(callee, contribution);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let placed: FxHashSet<FunctionId> =
            self.static_frame_addr_consts.keys().map(|&(func_id, _)| func_id).collect();
        let mut region_end = region_start;
        let mut bases = FxHashMap::default();
        for &func_id in &placed {
            let base = region_start + depth.get(&func_id).copied().unwrap_or(0);
            region_end = region_end.max(base + self.emitted_frame_size(module, func_id));
            bases.insert(func_id, base);
        }
        for (&(func_id, offset), &id) in &self.static_frame_addr_consts {
            self.asm.set_deferred_const(id, U256::from(bases[&func_id] + offset));
        }
        if let Some(id) = self.runtime_free_memory_const.take() {
            self.asm.set_deferred_const(id, U256::from(region_end));
        }
    }

    fn conservative_spill_frame_size(func: &Function) -> u64 {
        // The scheduler may allocate spill slots lazily while exposing deep stack values, not only
        // for values preallocated as cross-block live-ins/live-outs. Use this only when a body's
        // exact post-emission spill size is unavailable.
        func.values.len() as u64 * 32
    }

    fn external_spill_base(func: &Function) -> u64 {
        let low_memory_start = if Self::uses_internal_frame_slot(func) {
            INTERNAL_FRAME_PTR_SLOT + 32
        } else {
            LOW_MEMORY_START
        };
        low_memory_start + func.internal_frame_size.max(func.external_static_return_size)
    }

    fn constructor_free_memory_start(spill_size: u64) -> u64 {
        CONSTRUCTOR_FREE_MEMORY_START.max(CONSTRUCTOR_SPILL_BASE + spill_size)
    }

    fn uses_internal_frame_slot(func: &Function) -> bool {
        func.instructions.iter().any(|inst| matches!(inst.kind, InstKind::InternalCall { .. }))
    }

    fn emit_external_free_memory_start(&mut self) -> DeferredConst {
        let id = self.asm.new_deferred_const();
        self.asm.emit_push_deferred(id);
        self.asm.emit_push(U256::from(0x40));
        self.asm.emit_op(op::MSTORE);
        id
    }

    fn emit_spill_slot_addr(&mut self, func: &Function, slot: SpillSlot) {
        if self.in_internal_function {
            let spill_base =
                64 + (func.params.len() as u64) * 32 + (func.returns.len() as u64) * 32;
            self.emit_own_frame_addr(
                spill_base + func.internal_frame_size + u64::from(slot.offset) * 32,
            );
        } else if self.in_constructor {
            self.asm.emit_push(U256::from(slot.byte_offset()));
        } else {
            // Route the address through a deferred constant and count the
            // reference; `assign_ranked_spill_addrs` renumbers the body's
            // slots hottest-first when it completes.
            let key = u64::from(slot.offset);
            let id = if let Some(entry) = self.spill_addr_consts.get_mut(&key) {
                entry.1 += 1;
                entry.0
            } else {
                let id = self.asm.new_deferred_const();
                self.spill_addr_consts.insert(key, (id, 1));
                id
            };
            self.asm.emit_push_deferred(id);
        }
    }

    /// Assigns the external body's spill-slot addresses by reference count,
    /// hottest first, so the most reloaded slots take the shortest push
    /// widths. The assignment is a bijection over the same slot area — every
    /// site of a slot goes through one deferred constant — so sizes and
    /// disjointness are unchanged.
    fn assign_ranked_spill_addrs(&mut self, func: &Function) {
        if self.spill_addr_consts.is_empty() {
            return;
        }
        let base = Self::external_spill_base(func);
        let mut slots: Vec<(u64, (DeferredConst, usize))> =
            self.spill_addr_consts.drain().collect();
        slots.sort_by(|a, b| b.1.1.cmp(&a.1.1).then(a.0.cmp(&b.0)));
        for (rank, (_, (id, _))) in slots.into_iter().enumerate() {
            self.asm.set_deferred_const(id, U256::from(base + rank as u64 * 32));
        }
    }

    fn emit_internal_arg_load(&mut self, index: u32) {
        self.emit_own_frame_addr(64 + u64::from(index) * 32);
        self.asm.emit_op(op::MLOAD);
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_internal_call(
        &mut self,
        func: &Function,
        callee: FunctionId,
        args: &[ValueId],
        returns: usize,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        let Some(&callee_label) = self.function_labels.get(&callee) else {
            return;
        };
        let return_label = self.asm.new_label();

        // A static-frame callee needs none of the frame-pointer or
        // free-pointer bookkeeping below: its addresses are compile-time
        // constants.
        if self.static_frame_functions.contains(&callee) {
            self.emit_internal_call_static(
                func,
                callee,
                callee_label,
                return_label,
                args,
                returns,
                result,
                liveness,
                block,
                inst_idx,
            );
            return;
        }

        let static_local_frame_size =
            self.function_static_frame_sizes.get(&callee).copied().unwrap_or_default();
        // Frame layout: [reserved][saved frame ptr][args][returns][locals][spills].
        // The first slot is reserved (the return address used to live there;
        // it now travels on the EVM stack) so downstream offsets stay stable.
        // The spill suffix is only known after the callee body has emitted.
        let static_frame_size = 64 + ((args.len() + returns) as u64) * 32 + static_local_frame_size;
        let frame_size = self.asm.new_deferred_const();
        self.pending_frame_size_consts.push((frame_size, callee, static_frame_size));

        // Spill values that are live after this call BEFORE consuming the
        // arguments. An argument that is also used later (e.g. a flag passed to
        // a helper and then stored, as in `tryAdd`) would otherwise be popped by
        // the arg-store loop below and then lost when the stack is cleared for
        // the call, leaving it unavailable at its later use.
        self.spill_live_stack_values(func, liveness, block, inst_idx);

        self.emit_new_internal_frame_base_tracked();

        // frame[32] = previous frame pointer
        self.asm.emit_push(U256::from(INTERNAL_FRAME_PTR_SLOT));
        self.asm.emit_op(op::MLOAD);
        self.scheduler.stack.push_unknown();
        self.emit_internal_frame_store_from_top_preserving_base(32);

        for (i, &arg) in args.iter().enumerate() {
            self.emit_operand(func, arg);
            self.emit_internal_frame_store_from_top_preserving_base(64 + (i as u64) * 32);
        }

        // current_frame = frame
        self.emit_store_frame_base_to_current_frame_slot();

        // free_ptr += frame_size
        self.emit_store_new_free_pointer_from_frame_base(frame_size);

        self.pop_all_stack_values();
        self.scheduler.clear_stack();

        // The return address travels on the EVM stack, not in the frame: it is
        // pushed after the caller's stack is fully drained, so it is the only
        // physical value below the callee's execution. It is deliberately not
        // tracked by the scheduler — the model only describes the region above
        // it and every emitted DUP/SWAP/POP is model-relative, so nothing in
        // the callee can reach it. The callee's return consumes it with a bare
        // JUMP, and a tail call within the callee forwards it untouched.
        self.asm.emit_push_label(return_label);

        self.asm.emit_push_label(callee_label);
        self.asm.emit_op(op::JUMP);

        self.asm.define_label(return_label);
        self.scheduler.clear_stack();

        if let Some(result) = result
            && returns > 0
        {
            self.emit_current_internal_frame_addr(64 + (args.len() as u64) * 32);
            self.asm.emit_op(op::MLOAD);
            self.scheduler.stack.push(result);
            // Store the result to its reserved slot now, while it is on top.
            // Other value-producing instructions do this; internal calls did
            // not, so a reserved result (e.g. a recompute leaf of a live-out
            // cheap value) was never stored. No-op unless reserved and live.
            self.spill_top_value_if_live(func, liveness, block, inst_idx, result);
        }

        // Copy return values 2..N from the callee frame into scratch memory at
        // offset `i * 32`, matching what the caller's `lower_multi_var_decl`
        // reads via `mload(i * 32)`. This must happen before the frame pointer is
        // restored below, while the callee frame is still addressable. The first
        // return flows back as `result` on the stack (above); these copies have a
        // net-zero stack effect so they leave it untouched.
        for i in 1..returns {
            self.emit_current_internal_frame_addr(64 + (args.len() as u64) * 32 + (i as u64) * 32);
            self.asm.emit_op(op::MLOAD);
            self.asm.emit_push(U256::from((i as u64) * 32));
            self.asm.emit_op(op::MSTORE);
        }

        // Deallocate the callee frame in strict LIFO order by restoring the
        // free memory pointer to the callee frame base. This must happen before
        // restoring the caller frame pointer because `emit_current_internal_frame_addr`
        // reads `INTERNAL_FRAME_PTR_SLOT`. Do this only when the callee's declared
        // params/returns contain no memory pointer: memory pointer returns may
        // reference the callee's frame/heap region, and a memory pointer param lets
        // the callee install a fresh pointer into caller-visible memory. Solidity
        // allocation lowering zero-initializes new arrays/bytes/structs, so reclaimed
        // frame bytes need not be wiped.
        if self.restorable_internal_frames.contains(&callee) {
            self.emit_current_internal_frame_addr(0);
            self.asm.emit_push(U256::from(0x40));
            self.asm.emit_op(op::MSTORE);
        }

        // Restore the caller frame pointer. If a result is on the stack, this leaves it there.
        self.emit_current_internal_frame_addr(32);
        self.asm.emit_op(op::MLOAD);
        self.asm.emit_push(U256::from(INTERNAL_FRAME_PTR_SLOT));
        self.asm.emit_op(op::MSTORE);
    }

    /// Call to a static-frame callee: arguments are stored at absolute
    /// addresses, the return address rides the EVM stack (same invariants as
    /// the dynamic path), and there is no frame-pointer save/update/restore
    /// and no free-pointer traffic — the callee's frame is a fixed region
    /// below the heap that its single live activation owns.
    #[allow(clippy::too_many_arguments)]
    fn emit_internal_call_static(
        &mut self,
        func: &Function,
        callee: FunctionId,
        callee_label: Label,
        return_label: Label,
        args: &[ValueId],
        returns: usize,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        // Spill values that are live after this call BEFORE consuming the
        // arguments (see the dynamic path).
        self.spill_live_stack_values(func, liveness, block, inst_idx);

        let stack_mask =
            if self.runtime_stack_args { self.stack_arg_masks.get(&callee).cloned() } else { None };

        for (i, &arg) in args.iter().enumerate() {
            if stack_mask.as_ref().is_some_and(|mask| mask[i]) {
                continue;
            }
            self.emit_operand(func, arg);
            let addr = self.static_frame_addr(callee, 64 + (i as u64) * 32);
            self.asm.emit_push_deferred(addr);
            self.scheduler.stack.push_unknown();
            self.asm.emit_op(op::MSTORE);
            self.scheduler.instruction_executed(2, None);
        }

        // A stack-passed computed argument survives the drain in its spill
        // slot and is reloaded raw after it; make sure the slot is written
        // while the value is still reachable.
        if let Some(mask) = &stack_mask {
            for (i, &arg) in args.iter().enumerate() {
                if mask[i]
                    && matches!(func.value(arg), crate::mir::Value::Inst(_))
                    && !self.scheduler.spills.is_stored(arg)
                {
                    self.spill_value_if_needed(func, arg);
                    debug_assert!(
                        self.scheduler.spills.is_stored(arg),
                        "stack argument neither on the stack nor stored"
                    );
                }
            }
        }

        self.pop_all_stack_values();
        self.scheduler.clear_stack();

        self.asm.emit_push_label(return_label);
        // Stack-passed arguments ride above the return address, untracked by
        // the model like the return address itself; the callee prologue
        // stores them into its frame before its body runs.
        if let Some(mask) = &stack_mask {
            for (i, &arg) in args.iter().enumerate() {
                if mask[i] {
                    self.emit_raw_stack_arg(func, arg);
                }
            }
        }
        self.asm.emit_push_label(callee_label);
        self.asm.emit_op(op::JUMP);

        self.asm.define_label(return_label);
        self.scheduler.clear_stack();

        if let Some(result) = result
            && returns > 0
        {
            let addr = self.static_frame_addr(callee, 64 + (args.len() as u64) * 32);
            self.asm.emit_push_deferred(addr);
            self.asm.emit_op(op::MLOAD);
            self.scheduler.stack.push(result);
            self.spill_top_value_if_live(func, liveness, block, inst_idx, result);
        }

        // Copy return values 2..N into scratch, matching the dynamic path.
        for i in 1..returns {
            let addr = self.static_frame_addr(callee, 64 + ((args.len() + i) as u64) * 32);
            self.asm.emit_push_deferred(addr);
            self.asm.emit_op(op::MLOAD);
            self.asm.emit_push(U256::from((i as u64) * 32));
            self.asm.emit_op(op::MSTORE);
        }
    }

    fn spill_live_stack_values(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        let stack_values: Vec<_> = self.scheduler.stack.iter().flatten().collect();
        for value in stack_values {
            if !liveness.is_dead_after(value, block, inst_idx) {
                self.spill_value_if_needed(func, value);
            }
        }
    }

    /// Emits a value to the stack.
    fn emit_value(&mut self, func: &Function, val: ValueId) {
        self.emit_value_impl(func, val, true);
    }

    /// Emits a consuming operand occurrence to the stack.
    fn emit_operand(&mut self, func: &Function, val: ValueId) {
        self.emit_value_impl(func, val, false);
    }

    fn emit_value_impl(&mut self, func: &Function, val: ValueId, claim_top: bool) {
        if let Some(depth) = self.scheduler.stack.find(val)
            && depth >= MAX_STACK_ACCESS
            && !self.scheduler.spills.is_reloadable(val)
            && !matches!(
                func.value(val),
                crate::mir::Value::Immediate(_) | crate::mir::Value::Arg { .. }
            )
        {
            let slot = self.scheduler.spills.allocate(val);
            self.spill_deep_stack_value(func, val, slot, depth);
        }

        if self.scheduler.stack.find(val).is_none()
            && self.scheduler.spills.get(val).is_some()
            && !self.scheduler.spills.is_stored(val)
            && Self::is_cheap_recomputable_value(func, val)
        {
            self.emit_value_fresh(func, val);
            return;
        }

        let ops = if claim_top {
            self.scheduler.ensure_on_top(val, func)
        } else {
            self.scheduler.ensure_operand_on_top(val, func)
        }
        .to_vec();
        for op in ops {
            match op {
                ScheduledOp::Stack(stack_op) => {
                    self.asm.emit_op(stack_op.opcode());
                }
                ScheduledOp::PushImmediate(imm) => {
                    self.asm.emit_push(imm);
                }
                ScheduledOp::LoadSpill(slot) => {
                    // PUSH slot_offset, MLOAD
                    self.emit_spill_slot_addr(func, slot);
                    self.asm.emit_op(op::MLOAD);
                }
                ScheduledOp::SaveSpill(slot) => {
                    // PUSH slot_offset, MSTORE
                    self.emit_spill_slot_addr(func, slot);
                    self.asm.emit_op(op::MSTORE);
                }
                ScheduledOp::LoadArg(index) => {
                    if self.in_internal_function {
                        self.emit_internal_arg_load(index);
                    } else if self.in_constructor {
                        // Constructor args were copied to memory at 0x80
                        // Load from memory: 0x80 + index * 32
                        let offset = 0x80 + (index as u64) * 32;
                        self.asm.emit_push(U256::from(offset));
                        self.asm.emit_op(op::MLOAD);
                    } else {
                        // Runtime function: load from calldata
                        // ABI encoding: selector (4 bytes) + args (32 bytes each)
                        // Offset = 4 + index * 32
                        let offset = 4 + (index as u64) * 32;
                        self.asm.emit_push(U256::from(offset));
                        self.asm.emit_op(op::CALLDATALOAD);
                    }
                }
            }
        }
    }

    /// Emits a value fresh, without trying to DUP from the stack.
    /// This is used for CALL operands where we need to guarantee correct values
    /// regardless of scheduler stack tracking state.
    fn emit_value_fresh(&mut self, func: &Function, val: ValueId) {
        match func.value(val) {
            crate::mir::Value::Immediate(imm) => {
                if let Some(u256) = imm.as_u256() {
                    self.asm.emit_push(u256);
                    self.scheduler.stack.push(val);
                }
            }
            crate::mir::Value::Arg { index, .. } => {
                if self.in_internal_function {
                    self.emit_internal_arg_load(*index);
                } else if self.in_constructor {
                    let offset = 0x80 + (*index as u64) * 32;
                    self.asm.emit_push(U256::from(offset));
                    self.asm.emit_op(op::MLOAD);
                } else {
                    let offset = 4 + (*index as u64) * 32;
                    self.asm.emit_push(U256::from(offset));
                    self.asm.emit_op(op::CALLDATALOAD);
                }
                self.scheduler.stack.push(val);
            }
            crate::mir::Value::Inst(inst_id) => {
                // For instruction results, we need to check if they're spilled
                // or if they're instruction results that produce fresh values (like GAS, MLOAD)
                if let Some(slot) = self.scheduler.spills.get(val)
                    && self.scheduler.spills.is_stored(val)
                {
                    // Load from spill slot
                    self.emit_spill_slot_addr(func, slot);
                    self.asm.emit_op(op::MLOAD);
                    self.scheduler.stack.push(val);
                } else {
                    // Check if the instruction is one that we can "re-execute" to get a fresh value
                    // This handles GAS (which is always fresh) and MLOAD (which re-reads from
                    // memory)
                    let inst_kind = &func.instruction(*inst_id).kind;
                    match inst_kind {
                        crate::mir::InstKind::Gas => {
                            self.asm.emit_op(op::GAS);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::LoadImmutable(offset) => {
                            // Same emission as the scheduled path: a patched
                            // placeholder at runtime, the staged scratch word
                            // inside the running constructor.
                            if self.in_constructor {
                                self.asm.emit_push(U256::from(
                                    IMMUTABLE_SCRATCH_BASE + u64::from(*offset),
                                ));
                                self.asm.emit_op(op::MLOAD);
                            } else {
                                self.asm.emit_push_immutable(*offset);
                            }
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::CallValue => {
                            self.asm.emit_op(op::CALLVALUE);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::Caller => {
                            self.asm.emit_op(op::CALLER);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::Origin => {
                            self.asm.emit_op(op::ORIGIN);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::CalldataSize => {
                            self.asm.emit_op(op::CALLDATASIZE);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::InternalFrameAddr(offset) => {
                            self.emit_own_frame_addr(*offset);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::Timestamp => {
                            self.asm.emit_op(op::TIMESTAMP);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::BlockNumber => {
                            self.asm.emit_op(op::NUMBER);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::MLoad(offset) => {
                            // Note: Re-emitting MLOAD(0x40) is incorrect for struct pointers
                            // because the free memory pointer changes. However, with spill
                            // slots now at 0x1000+ (away from dynamic allocations), values
                            // should be properly spilled and reloaded, so we shouldn't hit
                            // this path for struct pointers.
                            //
                            // For other MLOAD addresses (reading from constant locations),
                            // re-emit is safe.
                            self.emit_value_fresh(func, *offset);
                            self.asm.emit_op(op::MLOAD);
                            // Pop offset, push result
                            self.scheduler.stack.pop();
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::Keccak256(offset, size) => {
                            // Re-emit KECCAK256 - memory content should still be valid.
                            // KECCAK256 reads s[0] = offset, s[1] = size, so emit the
                            // offset last so it ends up on top.
                            self.emit_value_fresh(func, *size);
                            self.emit_value_fresh(func, *offset);
                            self.asm.emit_op(op::KECCAK256);
                            // Pop offset and size, push result
                            self.scheduler.stack.pop();
                            self.scheduler.stack.pop();
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::Add(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::ADD, true);
                        }
                        crate::mir::InstKind::Sub(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::SUB, false);
                        }
                        crate::mir::InstKind::Mul(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::MUL, true);
                        }
                        crate::mir::InstKind::And(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::AND, true);
                        }
                        crate::mir::InstKind::Or(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::OR, true);
                        }
                        crate::mir::InstKind::Xor(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::XOR, true);
                        }
                        crate::mir::InstKind::Shl(shift, value) => {
                            self.emit_fresh_binary(func, val, *shift, *value, op::SHL, false);
                        }
                        crate::mir::InstKind::Shr(shift, value) => {
                            self.emit_fresh_binary(func, val, *shift, *value, op::SHR, false);
                        }
                        crate::mir::InstKind::Div(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::DIV, false);
                        }
                        crate::mir::InstKind::SDiv(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::SDIV, false);
                        }
                        crate::mir::InstKind::Mod(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::MOD, false);
                        }
                        crate::mir::InstKind::SMod(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::SMOD, false);
                        }
                        crate::mir::InstKind::Lt(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::LT, false);
                        }
                        crate::mir::InstKind::Gt(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::GT, false);
                        }
                        crate::mir::InstKind::SLt(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::SLT, false);
                        }
                        crate::mir::InstKind::SGt(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::SGT, false);
                        }
                        crate::mir::InstKind::Eq(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::EQ, true);
                        }
                        crate::mir::InstKind::Sar(shift, value) => {
                            self.emit_fresh_binary(func, val, *shift, *value, op::SAR, false);
                        }
                        crate::mir::InstKind::SLoad(slot) => {
                            // Re-emit SLOAD. CALL operands are materialized in a
                            // tight sequence with no intervening store, so the
                            // storage slot reads the same value as the original
                            // load (same recompute contract as MLOAD above).
                            self.emit_value_fresh(func, *slot);
                            self.asm.emit_op(op::SLOAD);
                            self.scheduler.stack.pop();
                            self.scheduler.stack.push(val);
                        }
                        _ => {
                            // A value that cannot be re-executed (e.g. an
                            // internal-call result used to compute a CALL
                            // operand) is live on the stack: duplicate it rather
                            // than re-running it. If it is buried too deep to
                            // `DUP`, spill it to a reserved slot and reload.
                            if let Some(depth) = self.scheduler.stack.find(val) {
                                if depth < 16 {
                                    self.asm.emit_op(op::DUP1 + depth as u8);
                                    self.scheduler.stack.push(val);
                                } else {
                                    let slot = self.scheduler.spills.allocate(val);
                                    self.spill_deep_stack_value(func, val, slot, depth);
                                    self.emit_spill_slot_addr(func, slot);
                                    self.asm.emit_op(op::MLOAD);
                                    self.scheduler.stack.push(val);
                                }
                            } else if let Some(slot) = self.scheduler.spills.get(val) {
                                // Tracked in a spill slot even though not flagged
                                // stored; reloading its address is still correct.
                                self.emit_spill_slot_addr(func, slot);
                                self.asm.emit_op(op::MLOAD);
                                self.scheduler.stack.push(val);
                            } else {
                                panic!(
                                    "emit_value_fresh: value {val:?} ({:?}) is neither on the \
                                     stack, spilled, nor re-executable",
                                    func.instruction(*inst_id).kind
                                );
                            }
                        }
                    }
                }
            }
            crate::mir::Value::Undef(_) => {
                // Undef values shouldn't appear in CALL operands
                panic!(
                    "emit_value_fresh: unexpected undef value {val:?}. \
                     CALL operands should be concrete values."
                );
            }
            crate::mir::Value::Error(_) => {
                // A lowering error fails compilation before codegen runs.
                panic!("emit_value_fresh: error sentinel {val:?} reached the backend");
            }
        }
    }

    fn emit_fresh_binary(
        &mut self,
        func: &Function,
        result: ValueId,
        a: ValueId,
        b: ValueId,
        opcode: u8,
        commutative: bool,
    ) {
        if commutative {
            self.emit_value_fresh(func, a);
            self.emit_value_fresh(func, b);
        } else {
            // EVM binary opcodes consume `a` from the top of stack and `b`
            // from the word below, matching the normal binary emitter.
            self.emit_value_fresh(func, b);
            self.emit_value_fresh(func, a);
        }
        self.asm.emit_op(opcode);
        self.scheduler.stack.pop();
        self.scheduler.stack.pop();
        self.scheduler.stack.push(result);
    }

    /// Emits a binary operation with result tracking and liveness awareness.
    /// If an operand is still live after this instruction, we DUP it before it gets consumed.
    #[allow(clippy::too_many_arguments)]
    fn emit_binary_op_with_result(
        &mut self,
        func: &Function,
        a: ValueId,
        b: ValueId,
        opcode: u8,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        // Check if operands are still live after this instruction
        let a_is_live = !liveness.is_dead_after(a, block, inst_idx);

        // Special case: same operand used twice (e.g., a + a, a - a)
        if a == b {
            self.emit_value(func, a);
            if !self.block_local_copy_survives(liveness, block, a, 1) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, a);
            }
            // DUP for the second operand
            self.asm.emit_op(op::DUP1);
            self.scheduler.stack.dup(1);
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(2, result);
            return;
        }

        // Operands that already sit on top of the tracked stack are consumed
        // in place when they are dead afterwards and own no reserved spill
        // slot, instead of being re-emitted and the stale copy nipped later
        // (`DUP2 <op> ... SWAP1 POP` becomes `<op>`).
        let a_dead_free =
            liveness.is_dead_after(a, block, inst_idx) && self.scheduler.spills.get(a).is_none();
        let b_dead_free =
            liveness.is_dead_after(b, block, inst_idx) && self.scheduler.spills.get(b).is_none();
        if self.scheduler.stack.top() == Some(a)
            && self.scheduler.stack.peek(1) == Some(b)
            && a_dead_free
            && b_dead_free
        {
            // The stack is already [b, a].
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(2, result);
            return;
        }
        if self.scheduler.stack.top() == Some(b)
            && b_dead_free
            && self.scheduler.can_emit_value(a, func)
        {
            // b is in place below; put a above it.
            self.emit_value(func, a);
            if a_is_live
                && !Self::is_rematerializable_value(func, a)
                && !self.block_local_copy_survives(liveness, block, a, 1)
            {
                self.spill_value_if_needed(func, a);
            }
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(2, result);
            return;
        }
        if self.scheduler.stack.top() == Some(a)
            && a_dead_free
            && self.scheduler.can_emit_value(b, func)
        {
            // a is in place; emit b above it and swap into [b, a].
            self.emit_value(func, b);
            if !self.block_local_copy_survives(liveness, block, b, 1) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, b);
            }
            self.asm.emit_op(op::SWAP1);
            self.scheduler.stack_swapped();
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(2, result);
            return;
        }

        // Check if either operand is already on stack as an untracked value
        let a_can_emit = self.scheduler.can_emit_value(a, func);
        let b_can_emit = self.scheduler.can_emit_value(b, func);
        let has_untracked = self.scheduler.has_untracked_on_top();
        let has_untracked_at_1 = self.scheduler.has_untracked_at_depth(1);

        if !a_can_emit && b_can_emit && has_untracked {
            // a is an untracked value on top of stack, emit b, then SWAP
            self.emit_value(func, b);
            if !self.block_local_copy_survives(liveness, block, b, 1) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, b);
            }
            self.asm.emit_op(op::SWAP1);
            self.scheduler.stack_swapped();
        } else if a_can_emit && !b_can_emit && has_untracked {
            // b is an untracked value on top of stack, emit a on top
            self.emit_value(func, a);
            // Spill a if live-after (it's now at depth 0)
            if a_is_live
                && !Self::is_rematerializable_value(func, a)
                && !self.block_local_copy_survives(liveness, block, a, 1)
            {
                self.spill_value_if_needed(func, a);
            }
        } else if !a_can_emit && b_can_emit && has_untracked_at_1 {
            // a is an untracked value at depth 1, b is tracked on top
            // Stack is [b, a_untracked], need [a, b]
            self.asm.emit_op(op::SWAP1);
            self.scheduler.stack_swapped();
        } else {
            // Normal case: emit b first (bottom), then a (top)
            self.emit_value(func, b);
            if !self.block_local_copy_survives(liveness, block, b, 1) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, b);
            }
            self.emit_value(func, a);
            // Spill a if live-after (it's now at depth 0)
            if a_is_live
                && !Self::is_rematerializable_value(func, a)
                && !self.block_local_copy_survives(liveness, block, a, 1)
            {
                self.spill_value_if_needed(func, a);
            }
        }

        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(2, result);
    }

    /// Emits a unary operation with result tracking and liveness awareness.
    /// If the operand is still live after this instruction, we spill it after emitting.
    #[allow(clippy::too_many_arguments)]
    fn emit_unary_op_with_result(
        &mut self,
        func: &Function,
        a: ValueId,
        opcode: u8,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        self.emit_value(func, a);
        if !self.block_local_copy_survives(liveness, block, a, 1) {
            self.spill_top_value_if_live(func, liveness, block, inst_idx, a);
        }

        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(1, result);
    }

    /// Emits a `LOG0`..=`LOG4` instruction. `operands` are given in stack order
    /// (deepest first, top last) and pushed in that order; the `LOG` then
    /// consumes all of them. Each operand still live after this instruction is
    /// spilled once it reaches the top, so a later use in the same block can
    /// reload it — the same operand-liveness handling as the arithmetic, store
    /// and copy paths. Without it, a topic value consumed by the `LOG` and used
    /// again later (e.g. an event that also stores its data word) would be lost.
    fn emit_log(
        &mut self,
        func: &Function,
        opcode: u8,
        operands: &[ValueId],
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        for (i, &operand) in operands.iter().enumerate() {
            if i == 0 {
                self.emit_value(func, operand);
            } else {
                // Repeated operands (e.g. duplicate topics) need their own stack item.
                self.emit_operand(func, operand);
            }
            // Occurrences of `operand` emitted so far, this one included: the
            // instruction consumes that many copies net of the occurrences
            // still to be pushed.
            let seen = operands[..=i].iter().filter(|&&op| op == operand).count();
            if !self.block_local_copy_survives(liveness, block, operand, seen) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, operand);
            }
        }
        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(operands.len(), None);
    }

    /// Emits a store operation with liveness awareness.
    /// If the value operand is still live after this instruction, we spill it after emitting
    /// to preserve it for later use.
    #[allow(clippy::too_many_arguments)]
    fn emit_store_op_live_aware(
        &mut self,
        func: &Function,
        addr: ValueId,
        val: ValueId,
        opcode: u8,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        // Check if addr is still live after this instruction
        let addr_is_live = !liveness.is_dead_after(addr, block, inst_idx);

        // Operands already sitting on top of the tracked stack are consumed
        // in place when they are dead afterwards and own no reserved spill
        // slot, instead of being re-emitted and the stale copies popped later
        // (`DUP2 DUP2 MSTORE ... POP POP` becomes `MSTORE`). Mirrors the
        // binary-op fast paths.
        let addr_dead_free = !addr_is_live && self.scheduler.spills.get(addr).is_none();
        let val_dead_free = liveness.is_dead_after(val, block, inst_idx)
            && self.scheduler.spills.get(val).is_none();
        if addr_dead_free && val_dead_free && self.scheduler.stack.depth() >= 2 {
            if self.scheduler.stack.top() == Some(addr) && self.scheduler.stack.peek(1) == Some(val)
            {
                // The stack is already [addr, val].
                self.asm.emit_op(opcode);
                self.scheduler.instruction_executed(2, None);
                return;
            }
            if self.scheduler.stack.top() == Some(val) && self.scheduler.stack.peek(1) == Some(addr)
            {
                self.asm.emit_op(op::SWAP1);
                self.scheduler.stack_swapped();
                self.asm.emit_op(opcode);
                self.scheduler.instruction_executed(2, None);
                return;
            }
        }

        // Emit val
        self.emit_value(func, val);
        if !self.block_local_copy_survives(liveness, block, val, 1) {
            self.spill_top_value_if_live(func, liveness, block, inst_idx, val);
        }

        // Emit addr
        self.emit_operand(func, addr);
        // Spill addr if live-after (it's now at depth 0)
        let addr_consumed = if addr == val { 2 } else { 1 };
        if addr_is_live
            && !Self::is_rematerializable_value(func, addr)
            && !self.block_local_copy_survives(liveness, block, addr, addr_consumed)
        {
            self.spill_value_if_needed(func, addr);
        }

        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(2, None);
    }

    /// Emits a copy-style instruction (no result) with liveness awareness.
    /// `operands` are pushed in order, so the last one ends up on top of the
    /// stack; any operand still live after this instruction is spilled before
    /// the instruction consumes it, preserving it for later uses.
    fn emit_copy_op_live_aware(
        &mut self,
        func: &Function,
        operands: &[ValueId],
        opcode: u8,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        for (i, &op) in operands.iter().enumerate() {
            if i == 0 {
                self.emit_value(func, op);
            } else {
                // Repeated operands need their own stack item each.
                self.emit_operand(func, op);
            }
            // See `emit_log`: copies consumed net of occurrences still to come.
            let seen = operands[..=i].iter().filter(|&&o| o == op).count();
            if !self.block_local_copy_survives(liveness, block, op, seen) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, op);
            }
        }

        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(operands.len(), None);
    }

    /// Emits a ternary operation.
    fn emit_ternary_op(
        &mut self,
        func: &Function,
        a: ValueId,
        b: ValueId,
        c: ValueId,
        opcode: u8,
        result: Option<ValueId>,
    ) {
        self.emit_value(func, c);
        self.emit_operand(func, b);
        self.emit_operand(func, a);
        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(3, result);
    }

    /// Generates a parallel copy.
    ///
    /// Phi copies move values from source to destination. The destination is typically
    /// a phi result that needs to be available in the successor block. We handle this
    /// by spilling the source value to the destination's spill slot.
    fn generate_copy(
        &mut self,
        func: &Function,
        copy: &ParallelCopy,
        temps: &mut FxHashMap<u32, ValueId>,
    ) {
        // Handle source: either a MIR value or a temporary
        match &copy.src {
            CopySource::Value(val) => {
                self.emit_operand(func, *val);
            }
            CopySource::Temp(temp_id) => {
                // Temporaries are tracked in our temps map with their ValueId
                if let Some(&temp_val) = temps.get(temp_id) {
                    // DUP the temp value to top of stack
                    if let Some(depth) = self.scheduler.stack.find(temp_val) {
                        let dup_n = (depth + 1) as u8;
                        self.asm.emit_op(op::dup(dup_n));
                        self.scheduler.stack.dup(dup_n);
                    }
                }
            }
        }

        // Handle destination: either a MIR value or a temporary
        match &copy.dst {
            CopyDest::Value(dst_val) => {
                // Spill the value on top of stack to the destination's spill slot
                // This allows the successor block to reload it
                let slot = self.scheduler.spills.allocate(*dst_val);
                self.emit_spill_slot_addr(func, slot);
                self.scheduler.stack.push_unknown();
                self.asm.emit_op(op::MSTORE);
                self.scheduler.stack.pop(); // pop the untracked offset
                self.scheduler.stack.pop(); // pop the value
                self.scheduler.spills.mark_stored(*dst_val);
            }
            CopyDest::Temp(temp_id) => {
                // Mark this temporary as defined - it's now on the stack
                // Get the ValueId of the value currently on top
                if let Some(val_on_top) = self.scheduler.stack.top() {
                    temps.insert(*temp_id, val_on_top);
                }
            }
        }
    }

    /// Pops all remaining values from the stack.
    /// This ensures the stack is empty before control flow transfer to another block.
    fn pop_all_stack_values(&mut self) {
        while self.scheduler.stack_depth() > 0 {
            self.asm.emit_op(op::POP);
            self.scheduler.stack.pop();
        }
    }

    fn emit_internal_return(&mut self, func: &Function, values: &[ValueId]) {
        let return_base = 64 + (func.params.len() as u64) * 32;
        for (i, &value) in values.iter().enumerate() {
            self.emit_operand(func, value);
            self.emit_own_frame_addr(return_base + (i as u64) * 32);
            self.asm.emit_op(op::MSTORE);
            self.scheduler.stack.pop();
        }

        self.pop_all_stack_values();
        // The caller's return address is the untracked value at the bottom of
        // the stack; after popping every tracked value it is on top.
        self.asm.emit_op(op::JUMP);
    }

    /// Generates bytecode for a terminator.
    fn generate_terminator(
        &mut self,
        func: &Function,
        term: &Terminator,
        fallthrough: Option<BlockId>,
        preserve_stack: bool,
    ) {
        match term {
            Terminator::TailCall { function, args } => {
                // Control transfers to the target and never returns: store the
                // arguments at the callee's compile-time frame addresses and
                // jump. No return address is pushed and the caller's tracked
                // stack is not drained — whatever stays below the callee's
                // model (including the caller's own inherited return address)
                // is unreachable by model-relative operations, and the callee
                // never executes a `ret` that would consume it.
                if !args.is_empty() {
                    // `lower-evm-shaped` only forms argument-carrying tail
                    // calls to callees the backend statically frames.
                    assert!(
                        self.static_frame_functions.contains(function),
                        "argument-carrying tail call to a non-static-frame callee"
                    );
                    for (i, &arg) in args.iter().enumerate() {
                        self.emit_operand(func, arg);
                        let addr = self.static_frame_addr(*function, 64 + (i as u64) * 32);
                        self.asm.emit_push_deferred(addr);
                        self.scheduler.stack.push_unknown();
                        self.asm.emit_op(op::MSTORE);
                        self.scheduler.instruction_executed(2, None);
                    }
                }
                let label = self.function_labels[function];
                self.asm.emit_push_label(label);
                self.asm.emit_op(op::JUMP);
            }
            Terminator::Jump(target) => {
                // Pop any remaining values from the stack before jumping.
                // Each block normally starts with an empty stack, so we must
                // clean the stack before jumping — unless this edge preserves
                // its live stack into a single-predecessor target.
                if Some(*target) == fallthrough {
                    if !preserve_stack {
                        self.pop_all_stack_values();
                    }
                    return;
                }
                if !preserve_stack {
                    self.pop_all_stack_values();
                }
                self.asm.emit_push_label(self.block_labels[target]);
                self.asm.emit_op(op::JUMP);
            }

            Terminator::Branch { condition, then_block, else_block } => {
                // Emit the condition first (it's still on the stack)
                self.emit_value(func, *condition);

                // Now pop all OTHER values (condition is on top, keep it)
                // We do this by tracking that condition was just pushed and emitting POPs for
                // everything else. When this edge preserves its stack into the hot arm, the
                // values beneath the condition are the carried layout: leave them in place.
                if !preserve_stack {
                    while self.scheduler.depth() > 1 {
                        // SWAP to get unwanted value to top, then POP
                        self.asm.emit_op(op::SWAP1);
                        self.scheduler.stack_swapped();
                        self.asm.emit_op(op::POP);
                        self.scheduler.stack.pop();
                    }
                }

                match fallthrough {
                    Some(next) if *else_block == next => {
                        // JUMPI consumes the condition; false falls through to `else_block`.
                        self.asm.emit_push_label(self.block_labels[then_block]);
                        self.asm.emit_op(op::JUMPI);
                        self.scheduler.stack.pop(); // condition consumed by JUMPI
                    }
                    Some(next) if *then_block == next => {
                        // Invert the condition so true falls through to `then_block`.
                        self.asm.emit_op(op::ISZERO);
                        self.scheduler.instruction_executed_untracked(1);
                        self.asm.emit_push_label(self.block_labels[else_block]);
                        self.asm.emit_op(op::JUMPI);
                        self.scheduler.stack.pop(); // inverted condition consumed by JUMPI
                    }
                    _ => {
                        // Neither target falls through. Route the likely-hot
                        // edge through JUMPI (16 gas) and leave the cold
                        // revert path on the trailing unconditional jump,
                        // instead of paying JUMPI + JUMP (24 gas) on the hot
                        // path.
                        if Self::block_is_cold(func, *then_block)
                            && !Self::block_is_cold(func, *else_block)
                        {
                            self.asm.emit_op(op::ISZERO);
                            self.scheduler.instruction_executed_untracked(1);
                            self.asm.emit_push_label(self.block_labels[else_block]);
                            self.asm.emit_op(op::JUMPI);
                            self.scheduler.stack.pop(); // inverted condition consumed by JUMPI

                            self.asm.emit_push_label(self.block_labels[then_block]);
                            self.asm.emit_op(op::JUMP);
                        } else {
                            // JUMPI consumes the condition
                            self.asm.emit_push_label(self.block_labels[then_block]);
                            self.asm.emit_op(op::JUMPI);
                            self.scheduler.stack.pop(); // condition consumed by JUMPI

                            self.asm.emit_push_label(self.block_labels[else_block]);
                            self.asm.emit_op(op::JUMP);
                        }
                    }
                }
            }

            Terminator::Switch { value, default, cases } => {
                if self.emitting_dispatch_entry {
                    // The dispatch entry's selector switch mirrors the backend
                    // dispatcher's shape: the just-computed selector stays on
                    // the stack through the case chain — no spill, clear and
                    // reload — and is left inert below the taken arm instead
                    // of paying a POP ("popping it per wrapper was a wasted
                    // byte each"). Every successor terminates externally and
                    // the entry runs once, so the leftover word is unreachable
                    // and cannot accumulate.
                    self.emit_value(func, *value);
                    while self.scheduler.depth() > 1 {
                        self.emit_stack_op(StackOp::Swap(1));
                        self.emit_stack_op(StackOp::Pop);
                    }
                } else {
                    let mut operands = Vec::with_capacity(cases.len() + 1);
                    operands.push(*value);
                    operands.extend(cases.iter().map(|(case_val, _)| *case_val));
                    self.spill_values_before_stack_clear(func, &operands);

                    // Pop all stack values first (live-out values are already
                    // spilled)
                    self.pop_all_stack_values();

                    // Emit the switch value (will reload from spill if needed)
                    self.emit_value(func, *value);
                }

                for (case_val, target) in cases {
                    // DUP the value, compare, jump if equal
                    self.asm.emit_op(op::DUP1);
                    self.scheduler.stack.dup(1);
                    self.emit_operand(func, *case_val);
                    self.asm.emit_op(op::EQ);
                    self.scheduler.instruction_executed_untracked(2);
                    self.asm.emit_push_label(self.block_labels[target]);
                    self.asm.emit_op(op::JUMPI);
                    self.scheduler.instruction_executed(1, None); // JUMPI consumes condition
                }

                if !self.emitting_dispatch_entry {
                    // Pop the value before the default edge.
                    self.asm.emit_op(op::POP);
                    self.scheduler.stack.pop();
                }
                if Some(*default) != fallthrough {
                    self.asm.emit_push_label(self.block_labels[default]);
                    self.asm.emit_op(op::JUMP);
                }
            }

            Terminator::Return { values } => {
                if self.in_internal_function {
                    self.emit_internal_return(func, values);
                    return;
                }

                assert!(values.is_empty(), "external ABI returns with values must use ReturnData");
                self.asm.emit_push(U256::ZERO);
                self.asm.emit_push(U256::ZERO);
                self.asm.emit_op(op::RETURN);
            }

            Terminator::Revert { offset, size } => {
                self.emit_value(func, *size);
                self.emit_operand(func, *offset);
                self.asm.emit_op(op::REVERT);
            }

            Terminator::ReturnData { offset, size } => {
                // Valid in internal functions too: a fused external body called
                // through an ABI wrapper returns straight to the external
                // caller, abandoning the internal frame.
                self.emit_value(func, *size);
                self.emit_operand(func, *offset);
                self.asm.emit_op(op::RETURN);
            }

            Terminator::Stop => {
                if self.in_internal_function {
                    self.emit_internal_return(func, &[]);
                } else {
                    self.asm.emit_op(op::STOP);
                }
            }

            Terminator::SelfDestruct { recipient } => {
                self.emit_value(func, *recipient);
                self.asm.emit_op(op::SELFDESTRUCT);
            }

            Terminator::Invalid => {
                self.asm.emit_op(op::INVALID);
            }
        }
    }
}

impl Default for EvmCodegen {
    fn default() -> Self {
        Self::new(EvmCodegenConfig::default())
    }
}

/// The artifact produced by the EVM backend: deployment and runtime bytecode.
#[derive(Clone, Debug, Default)]
pub struct EvmArtifact {
    /// Deployment (init) bytecode that, when run, returns the runtime code.
    pub deployment: Vec<u8>,
    /// Runtime bytecode, i.e. the code stored on-chain.
    pub runtime: Vec<u8>,
}

impl crate::backend::Backend for EvmCodegen {
    type Output = EvmArtifact;

    fn name(&self) -> &str {
        "evm"
    }

    fn lower_module(&mut self, module: &mut Module) -> EvmArtifact {
        let (deployment, runtime) = self.generate_deployment_bytecode(module);
        EvmArtifact { deployment, runtime }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower;
    use solar_config::{CompileOpts, UnstableOpts};
    use solar_interface::{Session, sym};
    use solar_sema::Compiler;
    use std::{ops::ControlFlow, path::PathBuf};

    /// Helper to compile Solidity source to bytecode, returning Result.
    fn compile_source(source: &str) -> Result<Vec<u8>, String> {
        compile_source_with_stack_schedule(source, false)
    }

    /// Compiles Solidity source to runtime bytecode, optionally enabling the
    /// experimental EVM IR `StackSchedule` bridge pass.
    fn compile_source_with_stack_schedule(
        source: &str,
        evm_ir_stack_schedule: bool,
    ) -> Result<Vec<u8>, String> {
        let opts = CompileOpts {
            unstable: UnstableOpts { codegen: true, ..Default::default() },
            ..Default::default()
        };
        let sess = Session::builder().with_buffer_emitter(Default::default()).opts(opts).build();
        let mut compiler = Compiler::new(sess);

        // Parse
        let parse_result = compiler.enter_mut(|c| -> solar_interface::Result<_> {
            let mut ctx = c.parse();
            let file = c
                .sess()
                .source_map()
                .new_source_file(PathBuf::from("test.sol"), source.to_string())
                .unwrap();
            ctx.add_file(file);
            ctx.parse();
            Ok(())
        });
        if parse_result.is_err() {
            return Err("Parse error".to_string());
        }

        // Lower and codegen
        compiler.enter_mut(|c| -> Result<Vec<u8>, String> {
            let ControlFlow::Continue(()) = c.lower_asts().map_err(|_| "Lower AST error")? else {
                return Err("Lower AST break".to_string());
            };
            let ControlFlow::Continue(()) = c.analysis().map_err(|_| "Analysis error")? else {
                return Err("Analysis break".to_string());
            };

            let gcx = c.gcx();
            for (contract_id, contract) in gcx.hir.contracts_enumerated() {
                if contract.name.name == sym::Test {
                    let mut module = lower::lower_contract(gcx, contract_id);
                    let config =
                        EvmCodegenConfig { evm_ir_stack_schedule, ..EvmCodegenConfig::from(gcx) };
                    let mut codegen = EvmCodegen::new(config);
                    let bytecode = codegen.generate_module(&mut module);
                    return Ok(bytecode);
                }
            }
            Err("Contract 'Test' not found".to_string())
        })
    }

    #[test]
    fn test_local_var_in_conditional_ice() {
        // Minimal repro for stack underflow ICE:
        // 1. Read storage into local variable
        // 2. Use local variable in conditional check
        // 3. Use local variable inside the conditional body
        let source = r#"
            // SPDX-License-Identifier: MIT
            pragma solidity ^0.8.0;
            contract Test {
                uint256 public value;
                function test() public {
                    uint256 v = value;
                    if (v != 0) value = v - 1;
                }
            }
        "#;

        let result = compile_source(source);
        assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
        let bytecode = result.unwrap();
        assert!(!bytecode.is_empty(), "Bytecode should not be empty");
    }

    #[test]
    fn test_direct_storage_in_conditional_works() {
        // This works: directly referencing storage in both condition and body
        let source = r#"
            // SPDX-License-Identifier: MIT
            pragma solidity ^0.8.0;
            contract Test {
                uint256 public value;
                function test() public {
                    if (value != 0) value = value - 1;
                }
            }
        "#;

        let result = compile_source(source);
        assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
    }

    #[test]
    fn test_phi_value_used_after_if_else() {
        // Test case for phi node handling when a variable is assigned in both
        // if/else branches and then used after the if/else.
        // This pattern is common in Uniswap V2 and similar contracts.
        let source = r#"
            // SPDX-License-Identifier: MIT
            pragma solidity ^0.8.0;
            contract Test {
                uint256 public totalSupply;
                function mint() external returns (uint256 liquidity) {
                    if (totalSupply == 0) {
                        liquidity = 1;
                    } else {
                        liquidity = 2;
                    }
                    totalSupply += liquidity;
                }
            }
        "#;

        let result = compile_source(source);
        assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
        let bytecode = result.unwrap();
        assert!(!bytecode.is_empty(), "Bytecode should not be empty");
    }

    #[test]
    fn test_phi_value_used_multiple_times_after_if_else() {
        // Test case where the phi result is used multiple times after the if/else
        let source = r#"
            // SPDX-License-Identifier: MIT
            pragma solidity ^0.8.0;
            contract Test {
                uint256 public totalSupply;
                function mint() external returns (uint256 result) {
                    uint256 liquidity;
                    if (totalSupply == 0) {
                        liquidity = 1;
                    } else {
                        liquidity = 2;
                    }
                    totalSupply += liquidity;
                    uint256 x = liquidity * 2;
                    result = x + liquidity;
                }
            }
        "#;

        let result = compile_source(source);
        assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
        let bytecode = result.unwrap();
        assert!(!bytecode.is_empty(), "Bytecode should not be empty");
    }

    /// Differential check for the experimental EVM IR `StackSchedule` bridge
    /// flag: for each sample contract, compiling with `evm_ir_stack_schedule`
    /// OFF (the default) and ON must produce byte-for-byte identical runtime
    /// bytecode. The bridge feeds the scheduler operand-cleared IR, where the
    /// pass is a verified near no-op, and `optimize_with_evm_ir` additionally
    /// guards the scheduled module behind the verifier oracle and a code-equality
    /// check — so turning the flag on can never diverge or produce invalid code.
    #[test]
    fn stack_schedule_bridge_flag_is_bytecode_neutral() {
        let samples = [
            // Simple storage read + conditional store.
            r#"
                // SPDX-License-Identifier: MIT
                pragma solidity ^0.8.0;
                contract Test {
                    uint256 public value;
                    function test() public {
                        uint256 v = value;
                        if (v != 0) value = v - 1;
                    }
                }
            "#,
            // Phi merge whose result is used after the if/else.
            r#"
                // SPDX-License-Identifier: MIT
                pragma solidity ^0.8.0;
                contract Test {
                    uint256 public totalSupply;
                    function mint() external returns (uint256 liquidity) {
                        if (totalSupply == 0) {
                            liquidity = 1;
                        } else {
                            liquidity = 2;
                        }
                        totalSupply += liquidity;
                    }
                }
            "#,
            // A small loop with arithmetic, exercising multiple blocks.
            r#"
                // SPDX-License-Identifier: MIT
                pragma solidity ^0.8.0;
                contract Test {
                    function sum(uint256 n) public pure returns (uint256 acc) {
                        for (uint256 i = 0; i < n; i++) {
                            acc += i;
                        }
                    }
                }
            "#,
        ];

        for source in samples {
            let off = compile_source_with_stack_schedule(source, false);
            let on = compile_source_with_stack_schedule(source, true);
            let off = off.expect("baseline compilation should succeed");
            let on = on.expect("stack-schedule compilation should succeed");
            assert!(!off.is_empty(), "baseline bytecode should not be empty");
            assert_eq!(
                off, on,
                "enabling evm_ir_stack_schedule changed produced bytecode for sample:\n{source}"
            );
        }
    }

    #[test]
    fn test_phi_with_ternary_in_branch() {
        // Complex phi case with nested ternary operators
        let source = r#"
            // SPDX-License-Identifier: MIT
            pragma solidity ^0.8.0;
            contract Test {
                uint256 public totalSupply;
                uint256 public reserve0;
                uint256 public reserve1;

                function mint() external returns (uint256 liquidity) {
                    uint256 amount0 = 100;
                    uint256 amount1 = 200;

                    if (totalSupply == 0) {
                        liquidity = amount0 * amount1;
                    } else {
                        uint256 l1 = (amount0 * totalSupply) / reserve0;
                        uint256 l2 = (amount1 * totalSupply) / reserve1;
                        liquidity = l1 < l2 ? l1 : l2;
                    }

                    totalSupply += liquidity;
                }
            }
        "#;

        let result = compile_source(source);
        assert!(result.is_ok(), "Compilation failed: {:?}", result.err());
        let bytecode = result.unwrap();
        assert!(!bytecode.is_empty(), "Bytecode should not be empty");
    }
}

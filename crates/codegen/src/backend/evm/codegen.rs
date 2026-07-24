//! EVM bytecode generation from MIR.
//!
//! This module generates EVM bytecode from MIR using:
//! - Liveness analysis to know when values die
//! - Phi elimination to convert SSA to parallel copies
//! - Stack scheduling to generate DUP/SWAP sequences
//! - EVM IR optimization, relocation, and byte encoding

use super::{
    assembler::{Assembler, DeferredAlloc, DeferredConst, ImmutableRef, Label, PreparedAssembly},
    ir,
    layout::{RelayoutAddress, preserves_push_width},
    op,
    stack::{
        MAX_STACK_ACCESS, ScheduledOp, SpillSlot, StackModel, StackOp, StackScheduler, TargetSlot,
    },
    switch::{
        SwitchDefault, SwitchPlan, bucket_index, select_switch_plan,
        select_switch_plan_with_linear_values,
    },
};
use crate::{
    analysis::{
        CallGraphInfo, CfgInfo, CopyDest, CopySource, Liveness, Loop, LoopAnalyzer, ParallelCopy,
        PhiEliminator,
    },
    memory::EvmMemoryLayout,
    mir::{BlockId, Function, FunctionId, InstId, InstKind, Module, Terminator, ValueId},
    pass::run_default_pipeline,
};
use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode};
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    map::{FxHashMap, FxHashSet},
};
use solar_interface::{Span, sym};
use solar_sema::{Gcx, hir::StateMutability};

const STACK_PHI_LAYOUT_LIMIT: usize = 8;
const GLOBAL_STACK_LAYOUT_LIMIT: usize = 8;
const GLOBAL_STACK_MAX_ARGS: usize = 3;
const GLOBAL_STACK_MIN_BLOCKS: usize = 8;
const GLOBAL_STACK_MIN_ARG_USES: usize = 6;
const GLOBAL_STACK_DENSE_AMORTIZATION_BLOCKS: usize = 16;
const STACK_ARG_ROTATION_LIMIT: usize = 16;

#[derive(Default)]
struct GeneratedCode {
    bytecode: Vec<u8>,
    evm_ir: Option<ir::Module>,
}

struct PreparedDeploymentPrefix {
    assembly: PreparedAssembly,
    constructor_arg_offset: Option<DeferredConst>,
    runtime_offset: DeferredConst,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StaticCallStackWord {
    ReturnAddress,
    Argument(usize),
}

#[derive(Clone, Debug)]
struct StackArgRetentionPlan {
    retained: DenseBitSet<usize>,
    drain_ops: Vec<StackOp>,
    shuffle_ops: Vec<StackOp>,
}

#[derive(Clone, Copy, Debug)]
struct SelectorDispatchEntry {
    selector: u32,
    label: Label,
}

#[derive(Clone, Copy, Debug)]
struct MirSwitchEntry {
    value: U256,
    value_id: ValueId,
    target: BlockId,
}

#[derive(Clone, Debug, Default)]
struct StackPhiPlan {
    entries: FxHashMap<BlockId, Vec<ValueId>>,
    edges: FxHashMap<BlockId, StackPhiEdge>,
    edge_sources: FxHashMap<BlockId, DenseBitSet<ValueId>>,
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
        if func.selector.is_none() {
            return Self::default();
        }

        let mut entries = FxHashMap::default();
        let args_by_index: FxHashMap<_, _> = func
            .values
            .iter_enumerated()
            .filter_map(|(value, kind)| match kind {
                crate::mir::Value::Arg { index, .. } => Some((*index, value)),
                _ => None,
            })
            .collect();
        if !(2..=GLOBAL_STACK_MAX_ARGS).contains(&args_by_index.len()) {
            return Self::default();
        }

        let cfg = CfgInfo::new(func);
        if cfg.reachable().count() < GLOBAL_STACK_MIN_BLOCKS {
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
            if !cfg.is_reachable(block_id)
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
        let mut disabled = DenseBitSet::new_empty(func.blocks.len());
        for &block in stack_phi_plan.entries.keys() {
            disabled.insert(block);
        }
        for block_id in func.blocks.indices() {
            if let Some(Terminator::Switch { default, cases, .. }) =
                func.blocks[block_id].terminator.as_ref()
            {
                disabled.insert(*default);
                for &(_, target) in cases {
                    disabled.insert(target);
                }
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
                if disabled.contains(*then_block) || disabled.contains(*else_block) {
                    changed |= disabled.insert(*then_block);
                    changed |= disabled.insert(*else_block);
                }
            }
        }
        entries.retain(|block, _| !disabled.contains(*block));

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
            || (entries.len() * 2 > cfg.reachable().count()
                && cfg.reachable().count() < GLOBAL_STACK_DENSE_AMORTIZATION_BLOCKS)
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
            let mut source_set = DenseBitSet::new_empty(self.func.values.len());
            for &source in &sources {
                source_set.insert(source);
            }
            plan.edge_sources.insert(pred, source_set);
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
            if outer.header == loop_info.header || !outer.blocks.contains(loop_info.header) {
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

    fn value_used_in_blocks(&self, blocks: &DenseBitSet<BlockId>, value: ValueId) -> bool {
        for block_id in blocks {
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
pub struct EvmCodegen<'gcx> {
    gcx: Gcx<'gcx>,
    /// The assembler for bytecode generation.
    asm: Assembler<'gcx>,
    /// Stack scheduler.
    scheduler: StackScheduler,
    /// Block labels.
    block_labels: FxHashMap<BlockId, Label>,
    /// Function labels for direct internal calls.
    function_labels: FxHashMap<FunctionId, Label>,
    /// Functions whose reachable exits all abort. Calls to these functions
    /// make their containing block cold as well.
    cold_functions: DenseBitSet<FunctionId>,
    /// Cold blocks in the function currently being emitted, including blocks
    /// that only forward control to other cold blocks.
    cold_blocks: DenseBitSet<BlockId>,
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
    stack_arg_masks: FxHashMap<FunctionId, DenseBitSet<usize>>,
    /// Whether the current assembly is the runtime (stack-passed arguments
    /// apply). The constructor assembly emits its own copies of internal
    /// functions with the plain frame-store convention.
    runtime_stack_args: bool,
    /// Deferred spill-slot address pushes of the external body being emitted,
    /// keyed by the slot's allocation offset, with their reference counts.
    /// Ranked hottest-first at body end so the most reloaded slots take the
    /// shortest addresses; final addresses wait for global layout.
    spill_addr_consts: FxHashMap<u64, (DeferredConst, usize)>,
    /// Ranked external spill pushes retained until static-allocation layout is
    /// finalized, keyed by entry function.
    external_spill_addr_consts: FxHashMap<FunctionId, Vec<(DeferredConst, usize)>>,
    /// Callees whose internal-call frame can be deallocated after return.
    restorable_internal_frames: DenseBitSet<FunctionId>,
    /// Functions whose frame lives at a compile-time-fixed address (static
    /// frames): internal-convention, non-recursive functions in the runtime
    /// passes. Their arg/local/spill accesses are absolute pushes and their
    /// call sites skip all frame-pointer and free-pointer bookkeeping.
    static_frame_functions: DenseBitSet<FunctionId>,
    /// Interned deferred constants for absolute static-frame addresses, keyed
    /// by (function, byte offset within its frame). Resolved at the end of
    /// the pass, once every body's exact spill size is known.
    static_frame_addr_consts: FxHashMap<(FunctionId, u64), (DeferredConst, usize)>,
    /// Deferred allocations emitted by each external entry.
    pending_static_allocs: FxHashMap<FunctionId, Vec<(DeferredAlloc, u64)>>,
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
    stack_phi_sources: FxHashMap<BlockId, DenseBitSet<ValueId>>,
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
    /// Shared constructor completion reached by ordinary `stop` terminators.
    constructor_exit: Option<Label>,
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
    capture_evm_ir: bool,
    /// Instructions that survive MIR lowering and the word-based backend cannot
    /// emit — an unsupported high-level construct rather than a miscompile. Each
    /// is a `(span, message)` the caller turns into a diagnostic instead of the
    /// backend panicking. Populated after the lowering passes and, when
    /// non-empty, generation is skipped for the affected module.
    unsupported: Vec<(Option<Span>, String)>,
}

impl<'gcx> EvmCodegen<'gcx> {
    /// Creates a new EVM code generator.
    #[must_use]
    pub fn new(gcx: Gcx<'gcx>) -> Self {
        Self {
            gcx,
            asm: Assembler::new(gcx),
            scheduler: StackScheduler::new(),
            block_labels: FxHashMap::default(),
            function_labels: FxHashMap::default(),
            cold_functions: DenseBitSet::new_empty(0),
            cold_blocks: DenseBitSet::new_empty(0),
            function_static_frame_sizes: FxHashMap::default(),
            function_spill_sizes: FxHashMap::default(),
            pending_frame_size_consts: Vec::new(),
            stack_arg_masks: FxHashMap::default(),
            runtime_stack_args: false,
            spill_addr_consts: FxHashMap::default(),
            external_spill_addr_consts: FxHashMap::default(),
            restorable_internal_frames: DenseBitSet::new_empty(0),
            static_frame_functions: DenseBitSet::new_empty(0),
            static_frame_addr_consts: FxHashMap::default(),
            pending_static_allocs: FxHashMap::default(),
            runtime_free_memory_const: None,
            runtime_entry_funcs: Vec::new(),
            current_internal_function: None,
            block_copies: FxHashMap::default(),
            stack_phi_sources: FxHashMap::default(),
            global_stack_active: false,
            global_stack_aliases: FxHashMap::default(),
            runtime_immutable_refs: Vec::new(),
            in_constructor: false,
            constructor_exit: None,
            constructor_param_count: 0,
            in_internal_function: false,
            emitting_dispatch_entry: false,
            capture_evm_ir: false,
            unsupported: Vec::new(),
        }
    }

    /// Drains the unsupported-construct diagnostics collected during lowering.
    /// The caller emits these against its diagnostic context, turning a
    /// construct the backend cannot lower into a clean error rather than a
    /// panic.
    pub fn take_unsupported(&mut self) -> Vec<(Option<Span>, String)> {
        std::mem::take(&mut self.unsupported)
    }

    /// Whether a function is an external interface of its module: an ABI entry,
    /// the constructor, the fallback, or the receive function. A module with
    /// none has no reachable runtime code.
    fn is_module_entry(func: &Function) -> bool {
        func.selector.is_some()
            || func.attributes.is_constructor
            || func.attributes.is_fallback
            || func.attributes.is_receive
    }

    /// Records any instruction that survives MIR lowering but the word-based
    /// backend cannot emit — chiefly logical slices whose aggregate use slice
    /// lowering could not fold. When this finds anything the module is left
    /// ungenerated so the caller reports it instead of the backend panicking.
    ///
    /// Only live instructions — those still in a block — are checked, since the
    /// instruction arena retains folded-away slices the backend never emits.
    fn collect_unsupported(&mut self, module: &Module) {
        'func: for func in module.functions.iter() {
            for block in func.blocks.iter() {
                for &inst_id in &block.instructions {
                    let inst = &func.instructions[inst_id];
                    let message = match inst.kind {
                        InstKind::MakeSlice { .. }
                        | InstKind::SlicePtr(_)
                        | InstKind::SliceLen(_) => {
                            "codegen does not support this calldata-slice usage yet"
                        }
                        _ => continue,
                    };
                    self.unsupported.push((inst.metadata.source_span(), message.to_string()));
                    // One diagnostic per function is enough to explain the bail.
                    continue 'func;
                }
            }
        }
    }

    /// Controls whether generated artifacts include final EVM IR.
    pub fn set_capture_evm_ir(&mut self, capture: bool) {
        self.capture_evm_ir = capture;
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

    /// Generates deployment bytecode for a module.
    /// Returns (deployment_bytecode, runtime_bytecode).
    /// Returns empty bytecodes for interfaces (they have no implementation).
    ///
    /// This runs optimization passes (including DCE) on the module before codegen unless disabled.
    pub fn generate_deployment_bytecode(&mut self, module: &mut Module) -> (Vec<u8>, Vec<u8>) {
        let artifact = self.generate_deployment_artifact(module);
        (artifact.deployment, artifact.runtime)
    }

    #[tracing::instrument(
        name = "evm_codegen",
        level = "debug",
        skip_all,
        fields(module = %module.name),
    )]
    fn generate_deployment_artifact(&mut self, module: &mut Module) -> EvmArtifact {
        // An internal-only library (no external interface) has no reachable
        // runtime code — like `solc`, it produces no bytecode rather than
        // standalone bodies for functions only ever inlined elsewhere.
        if module.is_interface || !module.functions.iter().any(Self::is_module_entry) {
            return EvmArtifact::default();
        }
        if let Some(func) = module.functions.iter().find(|func| func.blocks.is_empty()) {
            panic!("cannot codegen MIR function `{}` without an entry block", func.name);
        }
        self.run_optimization_passes(module);
        if !self.unsupported.is_empty() {
            return EvmArtifact::default();
        }
        // First generate the runtime code
        let mut runtime_code = self.generate_runtime_code(module);
        if let Some(evm_ir) = &mut runtime_code.evm_ir {
            evm_ir.set_name("runtime");
        }
        let runtime_len = runtime_code.bytecode.len();
        let immutable_refs = std::mem::take(&mut self.runtime_immutable_refs);

        // The constructor copies the runtime code to memory and patches the
        // immutable placeholders with the staged scratch words before
        // returning. Copy to offset 0 unless that would overwrite the scratch
        // words before the patch loop reads them.
        let copy_base = if !immutable_refs.is_empty()
            && runtime_len as u64 > EvmMemoryLayout::IMMUTABLE_SCRATCH_BASE
        {
            EvmMemoryLayout::IMMUTABLE_SCRATCH_BASE + module.immutable_data_len() as u64
        } else {
            0
        };

        // Generate constructor initialization and the deployment postlude as
        // one control-flow graph and optimize it once. Constructor arguments
        // are appended after the generated deployment prefix, so their offset
        // and the runtime-code offset depend on its final push widths. Only
        // repeat final assembly while both offsets stabilize.
        let prepared_deploy_code =
            self.prepare_deployment_prefix(module, runtime_len, copy_base, &immutable_refs);
        let mut deploy_code_len = 0usize;
        let mut constructor_arg_offset = runtime_len;
        let mut deploy_code = self.assemble_deployment_prefix(
            &prepared_deploy_code,
            constructor_arg_offset,
            deploy_code_len,
        );
        for _ in 0..8 {
            let next_deploy_code_len = deploy_code.bytecode.len();
            let next_arg_offset = next_deploy_code_len + runtime_len;
            if next_deploy_code_len == deploy_code_len && next_arg_offset == constructor_arg_offset
            {
                break;
            }
            deploy_code_len = next_deploy_code_len;
            constructor_arg_offset = next_arg_offset;
            deploy_code = self.assemble_deployment_prefix(
                &prepared_deploy_code,
                constructor_arg_offset,
                deploy_code_len,
            );
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
        if let Some(evm_ir) = &mut deploy_code.evm_ir {
            evm_ir.set_name("deployment");
        }

        let mut deploy_bytecode = deploy_code.bytecode;
        deploy_bytecode.extend_from_slice(&runtime_code.bytecode);

        let mut deployment_evm_ir = Vec::new();
        if let Some(evm_ir) = deploy_code.evm_ir {
            deployment_evm_ir.push(evm_ir);
        }
        if let Some(evm_ir) = runtime_code.evm_ir.clone() {
            deployment_evm_ir.push(evm_ir);
        }

        // The returned runtime artifact keeps the zero placeholders, like
        // solc's `deployedBytecode` for contracts with immutables.
        EvmArtifact {
            deployment: deploy_bytecode,
            runtime: runtime_code.bytecode,
            deployment_evm_ir,
            runtime_evm_ir: runtime_code.evm_ir,
        }
    }

    fn emit_deployment_postlude(
        &mut self,
        runtime_offset: DeferredConst,
        runtime_len: usize,
        copy_base: u64,
        immutable_refs: &[ImmutableRef],
    ) {
        // Copy runtime code from creation code to memory at `copy_base`.
        self.asm.emit_push(U256::from(runtime_len as u64));
        self.asm.emit_op(op::dup(1));
        self.asm.emit_push_deferred(runtime_offset);
        self.asm.emit_push(U256::from(copy_base));
        self.asm.emit_op(op::CODECOPY);

        // Patch each `PUSH32` placeholder with its staged immutable word.
        // The placeholder data starts one byte after the PUSH32 opcode.
        for r in immutable_refs {
            self.asm
                .emit_push(U256::from(EvmMemoryLayout::IMMUTABLE_SCRATCH_BASE + u64::from(r.id)));
            self.asm.emit_op(op::MLOAD);
            self.asm.emit_push(U256::from(copy_base + r.code_offset as u64 + 1));
            self.asm.emit_op(op::MSTORE);
        }

        // Return the patched runtime code; the DUP'd length is still on the stack.
        self.asm.emit_push(U256::from(copy_base));
        self.asm.emit_op(op::RETURN);
    }

    /// Generates constructor code that runs during deployment.
    /// This includes state variable initializers.
    ///
    /// Constructor arguments are read from the end of the initcode using CODECOPY.
    /// The args are ABI-encoded and appended after the deployment bytecode.
    fn prepare_deployment_prefix(
        &mut self,
        module: &Module,
        runtime_len: usize,
        copy_base: u64,
        immutable_refs: &[ImmutableRef],
    ) -> PreparedDeploymentPrefix {
        self.asm.clear();
        let runtime_offset = self.asm.new_deferred_const();

        // Find constructor function if it exists
        let constructor =
            module.functions.iter_enumerated().find(|(_, f)| f.attributes.is_constructor);

        let constructor_arg_offset = if let Some((ctor_id, ctor)) = constructor {
            // Generate constructor bytecode
            // Clear state and generate function body
            self.block_labels.clear();
            self.block_copies.clear();
            self.function_labels.clear();
            self.cold_functions =
                if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
                    DenseBitSet::new_empty(module.functions.len())
                } else {
                    Self::collect_cold_functions(module)
                };
            self.function_static_frame_sizes.clear();
            self.function_spill_sizes.clear();
            self.pending_frame_size_consts.clear();
            self.restorable_internal_frames = DenseBitSet::new_empty(module.functions.len());
            self.static_frame_functions = DenseBitSet::new_empty(module.functions.len());
            self.stack_arg_masks.clear();
            self.runtime_stack_args = false;
            self.static_frame_addr_consts.clear();
            self.external_spill_addr_consts.clear();
            self.pending_static_allocs.clear();
            self.runtime_free_memory_const = None;
            self.runtime_entry_funcs.clear();
            self.current_internal_function = None;
            self.stack_phi_sources.clear();

            for (func_id, func) in module.functions.iter_enumerated() {
                self.function_static_frame_sizes.insert(func_id, func.internal_frame_size);
                if !func.params.iter().chain(&func.returns).any(|ty| ty.is_memory_reference()) {
                    self.restorable_internal_frames.insert(func_id);
                }
            }

            let call_graph = CallGraphInfo::new(module);
            let internal_targets = call_graph.reachable_callees_from(std::iter::once(ctor_id));
            for func_id in &internal_targets {
                let label = self.new_function_label(func_id);
                self.function_labels.insert(func_id, label);
            }

            // Constructor spill slots are absolute addresses starting at
            // 0x1000. Keep the historical 0x4000 heap start as a floor, but
            // patch it upward after emission if the lazily allocated spill area
            // needs more room.
            let constructor_free_memory_start = self.asm.new_deferred_const();
            let constructor_arg_offset =
                (!ctor.params.is_empty()).then(|| self.asm.new_deferred_const());
            self.asm.emit_push_deferred(constructor_free_memory_start);
            self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
            self.asm.emit_op(op::MSTORE);

            // Set constructor context for LoadArg handling
            self.in_constructor = true;
            self.constructor_param_count = ctor.params.len() as u32;

            // If constructor has parameters, copy the full ABI-encoded argument blob to memory.
            // Constructor args are appended after generated deployment bytecode, so the copy size
            // is `CODESIZE - constructor_arg_offset`.
            if let Some(arg_offset) = constructor_arg_offset {
                self.asm.emit_push_deferred(arg_offset);
                self.asm.emit_op(op::CODESIZE);
                self.asm.emit_op(op::SUB); // size = CODESIZE - arg_offset
                self.asm.emit_push_deferred(arg_offset); // code offset
                self.asm.emit_push(U256::from(EvmMemoryLayout::HEAP_START)); // destOffset in memory
                self.asm.emit_op(op::CODECOPY);
            }

            if !internal_targets.is_empty() {
                let constructor_entry = self.asm.new_label();
                self.asm.emit_push_label(constructor_entry);
                self.asm.emit_op(op::JUMP);

                for (func_id, func) in module.functions.iter_enumerated() {
                    if !internal_targets.contains(func_id) {
                        continue;
                    }
                    let label = self.function_labels[&func_id];
                    self.asm.define_label(label);
                    self.in_internal_function = true;
                    self.generate_function_body(func_id, func);
                    self.in_internal_function = false;
                    self.record_function_spill_size(func_id);
                }

                self.asm.define_label(constructor_entry);
            }

            // Generate the constructor body (which includes SSTORE for
            // initializers). Every ordinary completion jumps to one label so
            // branch layout cannot strand the deployment postlude behind a
            // non-final STOP.
            let constructor_exit = self.asm.new_label();
            self.constructor_exit = Some(constructor_exit);
            self.generate_function_body(ctor_id, ctor);
            let constructor_spill_size = self.record_function_spill_size(ctor_id);
            self.asm.set_deferred_const(
                constructor_free_memory_start,
                U256::from(Self::constructor_free_memory_start(constructor_spill_size)),
            );

            self.resolve_pending_frame_size_consts(module);

            // Reset constructor context
            self.in_constructor = false;
            self.constructor_exit = None;
            self.constructor_param_count = 0;

            self.asm.define_label(constructor_exit);
            constructor_arg_offset
        } else {
            None
        };

        self.emit_deployment_postlude(runtime_offset, runtime_len, copy_base, immutable_refs);
        PreparedDeploymentPrefix {
            assembly: self.asm.prepare(self.capture_evm_ir),
            constructor_arg_offset,
            runtime_offset,
        }
    }

    fn assemble_deployment_prefix(
        &mut self,
        prepared: &PreparedDeploymentPrefix,
        constructor_arg_offset: usize,
        runtime_offset: usize,
    ) -> GeneratedCode {
        let mut deferred_values = Vec::with_capacity(2);
        if let Some(id) = prepared.constructor_arg_offset {
            deferred_values.push((id, U256::from(constructor_arg_offset)));
        }
        deferred_values.push((prepared.runtime_offset, U256::from(runtime_offset)));
        let result = self.asm.assemble_prepared(&prepared.assembly, &deferred_values);
        GeneratedCode { bytecode: result.bytecode, evm_ir: result.evm_ir }
    }

    /// Runs the canonical MIR optimization pipeline on the module.
    fn run_optimization_passes(&mut self, module: &mut Module) {
        run_default_pipeline(self.gcx, module);

        // A logical slice still live in a block after slice lowering is an
        // aggregate use it could not fold — an unsupported pattern, not a
        // miscompile. `LowerEvmShaped` leaves such a module at the
        // `memory-lowered` boundary; record it and stop before emission so the
        // caller reports it rather than the emitter panicking on it.
        self.collect_unsupported(module);
    }

    /// Generates runtime bytecode for a module.
    fn generate_runtime_code(&mut self, module: &Module) -> GeneratedCode {
        self.asm.clear();
        self.block_labels.clear();
        self.function_labels.clear();
        self.cold_functions = if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            DenseBitSet::new_empty(module.functions.len())
        } else {
            Self::collect_cold_functions(module)
        };
        self.function_static_frame_sizes.clear();
        self.function_spill_sizes.clear();
        self.pending_frame_size_consts.clear();
        self.restorable_internal_frames = DenseBitSet::new_empty(module.functions.len());
        self.static_frame_functions = DenseBitSet::new_empty(module.functions.len());
        self.static_frame_addr_consts.clear();
        self.external_spill_addr_consts.clear();
        self.pending_static_allocs.clear();
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

        let result = self.asm.assemble_with_evm_ir(self.capture_evm_ir);
        self.runtime_immutable_refs = result.immutable_refs;
        GeneratedCode { bytecode: result.bytecode, evm_ir: result.evm_ir }
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
        let internal_targets = call_graph.reachable_callees_from(
            module.functions.iter_enumerated().filter_map(|(func_id, func)| {
                (func_id == entry_id || Self::is_external_entry(func)).then_some(func_id)
            }),
        );

        for (func_id, func) in module.functions.iter_enumerated() {
            self.function_static_frame_sizes.insert(func_id, func.internal_frame_size);
            if !func.params.iter().chain(&func.returns).any(|ty| ty.is_memory_reference()) {
                self.restorable_internal_frames.insert(func_id);
            }
            // Non-recursive internal functions get compile-time-fixed frames.
            if func_id != entry_id
                && !Self::is_external_entry(func)
                && Self::is_runtime_function(func)
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
                || (Self::is_runtime_function(func) && internal_targets.contains(func_id));
            if needs_body {
                let label = self.new_function_label(func_id);
                self.function_labels.insert(func_id, label);
            }
        }

        // The MIR entry is the runtime prologue: one shared free-memory
        // store covers every wrapper reached through it.
        self.in_internal_function = false;
        self.emitting_dispatch_entry = true;
        let entry_free = self.emit_external_free_memory_start();
        self.runtime_free_memory_const = Some(entry_free);
        self.generate_function_body(entry_id, &module.functions[entry_id]);
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
            self.generate_function_body(func_id, func);
            self.record_function_spill_size(func_id);
            self.runtime_entry_funcs.push(func_id);
        }

        // Internal-call targets, exactly as in the backend dispatcher path.
        for (func_id, func) in module.functions.iter_enumerated() {
            if func_id == entry_id
                || Self::is_external_entry(func)
                || !Self::is_runtime_function(func)
            {
                continue;
            }
            let Some(&label) = self.function_labels.get(&func_id) else { continue };
            self.asm.define_label(label);
            self.emit_stack_arg_prologue(func_id, func);
            self.in_internal_function = true;
            self.current_internal_function = Some(func_id);
            self.generate_function_body(func_id, func);
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
        let receive_idx = module
            .functions
            .iter_enumerated()
            .find_map(|(func_id, func)| func.attributes.is_receive.then_some(func_id));
        let fallback_idx = module
            .functions
            .iter_enumerated()
            .find_map(|(func_id, func)| func.attributes.is_fallback.then_some(func_id));

        let call_graph = CallGraphInfo::new(module);
        let internal_targets = call_graph.reachable_callees_from(
            module
                .functions
                .iter_enumerated()
                .filter_map(|(func_id, func)| Self::is_external_entry(func).then_some(func_id)),
        );

        for (func_id, func) in module.functions.iter_enumerated() {
            self.function_static_frame_sizes.insert(func_id, func.internal_frame_size);
            if !func.params.iter().chain(&func.returns).any(|ty| ty.is_memory_reference()) {
                self.restorable_internal_frames.insert(func_id);
            }
            // Non-recursive internal functions get compile-time-fixed frames.
            if !Self::is_external_entry(func)
                && Self::is_runtime_function(func)
                && !call_graph.is_recursive(func_id)
            {
                self.static_frame_functions.insert(func_id);
            }
        }
        self.compute_stack_arg_masks(module);

        // Create labels for externally reachable runtime entry points and internal-call targets.
        for (func_id, func) in module.functions.iter_enumerated() {
            let external = Self::is_external_entry(func);
            let needs_body =
                external || (Self::is_runtime_function(func) && internal_targets.contains(func_id));
            let label = needs_body.then(|| self.new_function_label(func_id));
            if let Some(label) = label {
                self.function_labels.insert(func_id, label);
            }
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
                self.asm.emit_push_label(self.function_labels[&recv_idx]);
                self.asm.emit_op(op::JUMP);
            } else if let Some(fb_idx) = fallback_idx {
                self.asm.emit_push_label(self.function_labels[&fb_idx]);
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
            .iter_enumerated()
            .filter_map(|(func_id, func)| {
                if !Self::is_external_entry(func) {
                    return None;
                }
                let selector = func.selector?;
                Some(SelectorDispatchEntry {
                    selector: u32::from_be_bytes(selector),
                    label: self.function_labels[&func_id],
                })
            })
            .collect();
        selectors.sort_by_key(|entry| entry.selector);

        let fallback_label = fallback_idx.map(|idx| self.function_labels[&idx]);
        self.emit_selector_dispatch(&selectors, fallback_label, revert_label);

        // Define external function entry points.
        for (func_id, func) in module.functions.iter_enumerated() {
            if !Self::is_external_entry(func) {
                continue;
            }
            let Some(&label) = self.function_labels.get(&func_id) else { continue };
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
            self.generate_function_body(func_id, func);

            self.record_function_spill_size(func_id);
            self.runtime_entry_funcs.push(func_id);
        }

        // Define internal-call targets once. Calls jump here and return
        // through the stack-passed return address.
        for (func_id, func) in module.functions.iter_enumerated() {
            if Self::is_external_entry(func) || !Self::is_runtime_function(func) {
                continue;
            }
            let Some(&label) = self.function_labels.get(&func_id) else { continue };
            self.asm.define_label(label);
            self.emit_stack_arg_prologue(func_id, func);
            self.in_internal_function = true;
            self.current_internal_function = Some(func_id);
            self.generate_function_body(func_id, func);
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
        Self::is_runtime_function(func)
            && (func.selector.is_some()
                || func.attributes.is_receive
                || func.attributes.is_fallback)
    }

    fn is_runtime_function(func: &Function) -> bool {
        !func.attributes.is_constructor
    }

    fn switch_table_target_width(&self) -> usize {
        if self.in_constructor && self.gcx.sess.opts.evm_version < EvmVersion::Shanghai {
            3
        } else {
            2
        }
    }

    fn emit_selector_dispatch(
        &mut self,
        selectors: &[SelectorDispatchEntry],
        fallback_label: Option<Label>,
        revert_label: Label,
    ) {
        let values: Vec<_> = selectors.iter().map(|entry| U256::from(entry.selector)).collect();
        let default =
            if fallback_label.is_some() { SwitchDefault::Jump } else { SwitchDefault::Revert };
        match select_switch_plan(
            &values,
            self.gcx.sess.opts.optimization,
            self.gcx.sess.opts.evm_version,
            default,
            self.switch_table_target_width(),
        ) {
            SwitchPlan::Linear => {
                self.emit_linear_selector_dispatch(selectors, fallback_label, revert_label);
            }
            SwitchPlan::Binary { leaf_size } => {
                self.emit_binary_selector_dispatch(
                    selectors,
                    fallback_label,
                    revert_label,
                    leaf_size,
                );
            }
            SwitchPlan::Buckets { bucket_count } => {
                self.emit_bucketed_selector_dispatch(
                    selectors,
                    fallback_label,
                    revert_label,
                    bucket_count,
                );
            }
            SwitchPlan::Dense { low, range } => {
                self.emit_dense_selector_dispatch(
                    selectors,
                    fallback_label.unwrap_or(revert_label),
                    low,
                    range,
                );
            }
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
        leaf_size: usize,
    ) {
        if selectors.len() <= leaf_size {
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

        self.emit_binary_selector_dispatch(
            &selectors[mid..],
            fallback_label,
            revert_label,
            leaf_size,
        );

        self.asm.define_label(left_label);
        self.emit_binary_selector_dispatch(
            &selectors[..mid],
            fallback_label,
            revert_label,
            leaf_size,
        );
    }

    fn emit_bucketed_selector_dispatch(
        &mut self,
        selectors: &[SelectorDispatchEntry],
        fallback_label: Option<Label>,
        revert_label: Label,
        bucket_count: usize,
    ) {
        let mut buckets = vec![Vec::new(); bucket_count];
        for &entry in selectors {
            buckets[bucket_index(U256::from(entry.selector), bucket_count)].push(entry);
        }
        let default_label = fallback_label.unwrap_or(revert_label);
        let bucket_labels: Vec<_> = buckets
            .iter()
            .map(|bucket| if bucket.is_empty() { default_label } else { self.asm.new_label() })
            .collect();

        self.asm.emit_op(op::DUP1);
        self.asm.emit_push(U256::from(bucket_count));
        self.asm.emit_op(op::SWAP1);
        self.asm.emit_op(op::MOD);
        self.asm.emit_indexed_jump(bucket_labels.clone());

        for (label, bucket) in bucket_labels.into_iter().zip(buckets) {
            if bucket.is_empty() {
                continue;
            }
            self.asm.define_label(label);
            self.emit_linear_selector_dispatch(&bucket, fallback_label, revert_label);
        }
    }

    fn emit_dense_selector_dispatch(
        &mut self,
        selectors: &[SelectorDispatchEntry],
        default_label: Label,
        low: U256,
        range: usize,
    ) {
        let mut targets = vec![default_label; range];
        for entry in selectors {
            let index = usize::try_from(U256::from(entry.selector) - low)
                .expect("dense selector table index must fit usize");
            targets[index] = entry.label;
        }
        let in_range = self.asm.new_label();

        if !low.is_zero() {
            self.asm.emit_push(low);
            self.asm.emit_op(op::SWAP1);
            self.asm.emit_op(op::SUB);
        }
        self.asm.emit_op(op::DUP1);
        self.asm.emit_push(U256::from(range));
        self.asm.emit_op(op::GT);
        self.asm.emit_push_label(in_range);
        self.asm.emit_op(op::JUMPI);
        self.asm.emit_op(op::POP);
        self.asm.emit_push_label(default_label);
        self.asm.emit_op(op::JUMP);

        self.asm.define_label(in_range);
        self.asm.emit_indexed_jump(targets);
    }

    fn emit_selector_eq_jump(&mut self, entry: SelectorDispatchEntry) {
        self.asm.emit_op(op::dup(1));
        if entry.selector == 0 && self.gcx.sess.opts.optimization != OptimizationMode::None {
            self.asm.emit_op(op::ISZERO);
        } else {
            self.asm.emit_push(U256::from(entry.selector));
            self.asm.emit_op(op::EQ);
        }
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

    /// Generates the body of a function.
    fn generate_function_body(&mut self, func_id: FunctionId, func: &Function) {
        let liveness = self
            .emitting_dispatch_entry
            .then(|| Liveness::compute_block_local_for_codegen(func))
            .flatten()
            .unwrap_or_else(|| Liveness::compute(func));
        let liveness = &liveness;

        // Eliminate phis.
        let phi_result = PhiEliminator::analyze(func);
        let has_phis = !phi_result.phis_to_remove.is_empty();
        for (block_id, copies) in phi_result.block_copies {
            self.block_copies.insert(block_id, copies.copies);
        }
        // Stack-phi planning starts with loop analysis, but cannot produce a
        // plan without a phi. Avoid that analysis for the overwhelmingly
        // common phi-free function.
        let stack_phi_plan =
            if has_phis { StackPhiPlan::analyze(func) } else { StackPhiPlan::default() };
        self.stack_phi_sources = stack_phi_plan.edge_sources.clone();
        let global_stack_plan = GlobalStackPlan::analyze(func, liveness, &stack_phi_plan);
        self.global_stack_active = !global_stack_plan.is_empty();
        self.global_stack_aliases = global_stack_plan.aliases.clone();

        // Reset scheduler
        self.scheduler = StackScheduler::new();
        self.spill_addr_consts.clear();

        self.preallocate_cross_block_spills(func, liveness);

        self.cold_blocks = self.collect_cold_blocks(func);

        // Create labels for each block
        self.block_labels.clear();
        for block_id in func.blocks.indices() {
            let label = self.asm.new_label();
            if self.block_is_cold(block_id) {
                self.asm.mark_label_cold(label);
            }
            self.block_labels.insert(block_id, label);
        }

        // Generate each block.
        let block_order = self.block_layout_order(func);
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
            if !entered_by_preserved_fallthrough && !block.predecessors.is_empty() {
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
                    self.invalidate_carried_phi_spills(func);
                    // Live-ins not on the carried stack still arrive in memory.
                    self.mark_live_in_spills(func, liveness, block_id);
                } else if let Some(entry) = stack_phi_plan.entries.get(&block_id) {
                    self.set_stack_to_values(entry);
                    self.invalidate_carried_phi_spills(func);
                    self.mark_live_in_spills(func, liveness, block_id);
                } else if let Some(entry) = global_stack_plan.entry(block_id) {
                    self.set_stack_to_values(entry);
                    self.invalidate_carried_phi_spills(func);
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
                self.generate_inst(
                    func_id,
                    inst_id,
                    func,
                    &inst.kind,
                    liveness,
                    block_id,
                    inst_idx,
                    result_value,
                );
                if let Some(result) = result_value {
                    self.spill_reserved_result_if_live(func, liveness, block_id, inst_idx, result);
                    // A free-memory-pointer load cannot be rematerialized once
                    // the pointer moves. Park every FMP load at its
                    // definition so later uses reload the original value —
                    // whether the definition crosses a block on a preserved
                    // edge or is re-materialized between two allocations in
                    // its own block.
                    if matches!(
                        inst.kind,
                        InstKind::MLoad(addr)
                            if func.value_u64(addr) == Some(EvmMemoryLayout::FMP_SLOT)
                    ) {
                        self.spill_value_if_needed(func, result);
                    }
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

        self.assign_ranked_spill_addrs(func_id);
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
        let mut live_in_any_target = DenseBitSet::new_empty(func.values.len());
        for target in targets {
            for value in liveness.live_in(target) {
                live_in_any_target.insert(value);
            }
        }
        if carried.iter().any(|&value| !live_in_any_target.contains(value)) {
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

    /// Finds functions whose reachable exits all abort, including chains of
    /// calls to other cold functions.
    fn collect_cold_functions(module: &Module) -> DenseBitSet<FunctionId> {
        let mut cold = DenseBitSet::new_empty(module.functions.len());
        let mut worklist = Vec::new();
        let mut visited = GrowableBitSet::new_empty();
        loop {
            let mut changed = false;
            for (function_id, func) in module.functions.iter_enumerated() {
                if cold.contains(function_id) {
                    continue;
                }
                worklist.clear();
                worklist.push(BlockId::ENTRY);
                visited.clear();
                let mut saw_exit = false;
                let mut all_exits_cold = true;
                while let Some(block_id) = worklist.pop()
                    && all_exits_cold
                {
                    if !visited.insert(block_id) {
                        continue;
                    }
                    let block = &func.blocks[block_id];
                    if block.instructions.iter().any(|&inst_id| {
                        matches!(
                            func.instructions[inst_id].kind,
                            InstKind::InternalCall { function, .. } if cold.contains(function)
                        )
                    }) {
                        saw_exit = true;
                        continue;
                    }
                    let Some(term) = block.terminator.as_ref() else {
                        all_exits_cold = false;
                        continue;
                    };
                    match term {
                        Terminator::Revert { .. } | Terminator::Invalid => {
                            saw_exit = true;
                        }
                        Terminator::TailCall { function, .. } if cold.contains(*function) => {
                            saw_exit = true;
                        }
                        _ => {
                            let successors = term.successors();
                            if successors.is_empty() {
                                all_exits_cold = false;
                            } else {
                                worklist.extend(successors);
                            }
                        }
                    }
                }
                if saw_exit && all_exits_cold {
                    cold.insert(function_id);
                    changed = true;
                }
            }
            if !changed {
                return cold;
            }
        }
    }

    /// Finds blocks that abort directly or can only reach other cold blocks.
    fn collect_cold_blocks(&self, func: &Function) -> DenseBitSet<BlockId> {
        let mut cold = DenseBitSet::new_empty(func.blocks.len());
        let mut worklist = Vec::new();
        for block_id in func.blocks.indices() {
            if self.block_aborts(func, block_id) {
                cold.insert(block_id);
                worklist.push(block_id);
            }
        }
        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            return cold;
        }

        while let Some(block_id) = worklist.pop() {
            for &predecessor in &func.blocks[block_id].predecessors {
                if cold.contains(predecessor) {
                    continue;
                }
                let Some(term) = func.blocks[predecessor].terminator.as_ref() else {
                    continue;
                };
                let successors = term.successors();
                if !successors.is_empty()
                    && successors.iter().all(|&successor| cold.contains(successor))
                {
                    cold.insert(predecessor);
                    worklist.push(predecessor);
                }
            }
        }
        cold
    }

    /// Returns true when a block aborts directly or calls a function whose
    /// reachable exits all abort.
    fn block_aborts(&self, func: &Function, block_id: BlockId) -> bool {
        let block = &func.blocks[block_id];
        matches!(block.terminator, Some(Terminator::Revert { .. } | Terminator::Invalid))
            || matches!(
                block.terminator,
                Some(Terminator::TailCall { function, .. })
                    if self.cold_functions.contains(function)
            )
            || block.instructions.iter().any(|&inst_id| {
                matches!(
                    func.instructions[inst_id].kind,
                    InstKind::InternalCall { function, .. }
                        if self.cold_functions.contains(function)
                )
            })
    }

    fn block_is_cold(&self, block_id: BlockId) -> bool {
        self.cold_blocks.contains(block_id)
    }

    fn new_function_label(&mut self, function: FunctionId) -> Label {
        let label = self.asm.new_label();
        if self.cold_functions.contains(function) {
            self.asm.mark_label_cold(label);
        }
        label
    }

    fn block_layout_order(&self, func: &Function) -> Vec<BlockId> {
        // Layout only initializes reachability; RPO, dominators, and
        // transitive reachability remain unevaluated.
        let cfg = CfgInfo::new(func);
        let reachable = cfg.reachable();
        let mut order = Vec::with_capacity(func.blocks.len());
        let mut placed = DenseBitSet::new_empty(func.blocks.len());

        self.append_layout_chain(func, BlockId::ENTRY, reachable, &mut placed, &mut order);
        for block_id in func.blocks.indices() {
            if reachable.contains(block_id) {
                self.append_layout_chain(func, block_id, reachable, &mut placed, &mut order);
            }
        }

        order
    }

    fn append_layout_chain(
        &self,
        func: &Function,
        mut block_id: BlockId,
        reachable: &DenseBitSet<BlockId>,
        placed: &mut DenseBitSet<BlockId>,
        order: &mut Vec<BlockId>,
    ) {
        loop {
            if !reachable.contains(block_id) || !placed.insert(block_id) {
                return;
            }
            order.push(block_id);

            let target = match func.blocks[block_id].terminator.as_ref() {
                Some(Terminator::Jump(target))
                    if func.blocks[*target].predecessors.as_slice() == [block_id] =>
                {
                    *target
                }
                Some(Terminator::Branch { then_block, else_block, .. })
                    if !matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) =>
                {
                    match (self.block_is_cold(*then_block), self.block_is_cold(*else_block)) {
                        (true, false) => *else_block,
                        (false, true) => *then_block,
                        _ => return,
                    }
                }
                _ => return,
            };
            if placed.contains(target) {
                return;
            }

            block_id = target;
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
        self.stack_phi_sources.get(&block).is_some_and(|sources| sources.contains(value))
    }

    /// Preallocates stable spill slots for values that may cross block boundaries.
    ///
    /// Blocks are emitted in layout order, not necessarily dominance order, so a block can be
    /// emitted before the predecessor that stores one of its live-in values. Reserving the slot up
    /// front lets the later load use a stable memory location; stores still happen only when the
    /// value is actually available on the stack.
    fn preallocate_cross_block_spills(&mut self, func: &Function, liveness: &Liveness) {
        for val in &Self::cross_block_spill_values(func, liveness) {
            self.scheduler.spills.allocate(val);
        }
    }

    fn cross_block_spill_values(func: &Function, liveness: &Liveness) -> DenseBitSet<ValueId> {
        let mut values = DenseBitSet::new_empty(func.values.len());
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
        let seeds: Vec<ValueId> = values.iter().collect();
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
    fn collect_recompute_leaves(func: &Function, val: ValueId, out: &mut DenseBitSet<ValueId>) {
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

        for val in live_out {
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
        let mut exempt_values = DenseBitSet::new_empty(func.values.len());
        for &value in exempt {
            exempt_values.insert(value);
        }
        for val in liveness.live_out(block_id) {
            if !exempt_values.contains(val) {
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

    /// Invalidates the spill bookkeeping of every phi result on a stack
    /// restored from a carried edge. A loop-carried phi is redefined on every
    /// re-entry without a store, so a slot stored during an earlier iteration
    /// holds a stale definition: an exit-path use must spill the carried copy
    /// again before anything reloads the slot. Other carried values are
    /// immutable SSA definitions whose stored slots stay current, and
    /// invalidating those would force later paths to recompute
    /// memory-dependent definitions whose operands may have changed.
    fn invalidate_carried_phi_spills(&mut self, func: &Function) {
        let carried: Vec<ValueId> = self.scheduler.stack.iter().flatten().collect();
        for value in carried {
            if let crate::mir::Value::Inst(inst_id) = func.value(value)
                && matches!(func.instructions[*inst_id].kind, InstKind::Phi(_))
            {
                self.scheduler.spills.invalidate_stored(value);
            }
        }
    }

    fn mark_live_in_spills(&mut self, func: &Function, liveness: &Liveness, block_id: BlockId) {
        // Values already on the stack (carried in from a preserved predecessor
        // edge) are read directly; marking them reloadable would point at a
        // spill slot that may never have been stored.
        for val in liveness.live_in(block_id) {
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

    /// Parks stack-resident operands in their spill slots before an
    /// `emit_value_fresh` sequence. The sequence re-materializes each value,
    /// and definitions such as free-memory-pointer loads cannot be recomputed
    /// once memory has moved on: reaching them through a reload keeps the
    /// original definition.
    fn prepare_fresh_operands(&mut self, func: &Function, operands: &[ValueId]) {
        for &operand in operands {
            self.spill_value_if_needed(func, operand);
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
    #[allow(clippy::too_many_arguments)]
    fn generate_inst(
        &mut self,
        func_id: FunctionId,
        inst_id: InstId,
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
            InstKind::Alloc { size, .. } => {
                debug_assert!(func.instructions[inst_id].metadata.deferred_alloc());
                let size =
                    func.value_u64(*size).expect("deferred allocation must have a constant size");
                let alloc = self.asm.emit_deferred_alloc();
                self.pending_static_allocs.entry(func_id).or_default().push((alloc, size));
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Fmp | InstKind::SetFmp(_) => {
                unreachable!("abstract allocation instruction reached EVM emission")
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
                    self.asm.emit_push(U256::from(
                        EvmMemoryLayout::IMMUTABLE_SCRATCH_BASE + u64::from(*offset),
                    ));
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
            InstKind::AddMod(a, b, n) => self.emit_ternary_op(
                func,
                [*n, *b, *a],
                op::ADDMOD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::MulMod(a, b, n) => self.emit_ternary_op(
                func,
                [*n, *b, *a],
                op::MULMOD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),

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
                self.prepare_fresh_operands(
                    func,
                    &[*gas, *addr, *value, *args_offset, *args_size, *ret_offset, *ret_size],
                );
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
                self.prepare_fresh_operands(
                    func,
                    &[*gas, *addr, *args_offset, *args_size, *ret_offset, *ret_size],
                );
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
                self.prepare_fresh_operands(
                    func,
                    &[*gas, *addr, *args_offset, *args_size, *ret_offset, *ret_size],
                );
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

            InstKind::MappingSlot(_, _)
            | InstKind::MappingSlotMemory(_, _)
            | InstKind::MappingSlotCalldata(_, _) => {
                unreachable!("mapping-slot builtins must be lowered before EVM codegen")
            }

            InstKind::MakeSlice { .. } | InstKind::SlicePtr(_) | InstKind::SliceLen(_) => {
                unreachable!(
                    "slice instructions must be lowered before EVM codegen: {kind:?} in `{}`",
                    func.name
                )
            }

            InstKind::MemoryObjectLen(_, _)
            | InstKind::SetMemoryObjectLen(_, _, _)
            | InstKind::MemoryObjectData(_, _)
            | InstKind::MemoryObjectFieldAddr { .. }
            | InstKind::MemoryObjectElementAddr { .. }
            | InstKind::Keccak256Bytes(_) => {
                unreachable!("memory-object instructions must be lowered before EVM codegen")
            }

            InstKind::AbiEncode { .. } => {
                unreachable!("ABI encoding must be lowered before EVM codegen")
            }

            InstKind::StorageToMemory { .. }
            | InstKind::MemoryToStorage { .. }
            | InstKind::ClearStorage { .. } => {
                unreachable!("aggregate operations must be lowered before EVM codegen")
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
        self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
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
        self.asm.emit_push(U256::from(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT));
        self.scheduler.stack.push_unknown();
        self.asm.emit_op(op::MSTORE);
        self.scheduler.instruction_executed(2, None);
    }

    fn emit_store_new_free_pointer_from_frame_base(&mut self, frame_size: DeferredConst) {
        self.asm.emit_push_deferred(frame_size);
        self.scheduler.stack.push_unknown();
        self.emit_op_with_effect(op::ADD, StackEffect { pops: 2, pushes: 1 }, StackPush::Unknown);
        self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
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
        self.asm.emit_push(U256::from(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT));
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
            && self.static_frame_functions.contains(func_id)
        {
            let addr = self.static_frame_addr(func_id, offset);
            self.asm.emit_push_deferred(addr);
            return;
        }
        if !self.in_internal_function && !self.in_constructor {
            self.asm.emit_push(U256::from(EvmMemoryLayout::HEAP_START + offset));
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
        self.stack_arg_masks.clear();
        if self.static_frame_functions.is_empty() {
            return;
        }

        let mut scores: FxHashMap<FunctionId, Vec<i32>> = FxHashMap::default();
        let mut excluded = DenseBitSet::new_empty(module.functions.len());
        for (caller_id, func) in module.functions.iter_enumerated() {
            let mut has_candidate_call = false;
            for block in func.blocks.iter() {
                if let Some(Terminator::TailCall { function, args }) = &block.terminator
                    && !args.is_empty()
                    && self.static_frame_functions.contains(*function)
                {
                    excluded.insert(*function);
                }
                has_candidate_call |= block.instructions.iter().any(|&inst_id| {
                    matches!(
                        &func.instructions[inst_id].kind,
                        InstKind::InternalCall { function, .. }
                            if self.static_frame_functions.contains(*function)
                    )
                });
            }
            if !has_candidate_call {
                continue;
            }

            let caller_is_entry = Self::is_external_entry(func);
            let caller_static = self.static_frame_functions.contains(caller_id);
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
                for &inst_id in &block.instructions {
                    let InstKind::InternalCall { function, args, .. } =
                        &func.instructions[inst_id].kind
                    else {
                        continue;
                    };
                    if !self.static_frame_functions.contains(*function) {
                        continue;
                    }
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
            self.static_frame_functions.contains(*func_id) && !excluded.contains(*func_id)
        });
        let mut masks = FxHashMap::default();
        for (func_id, score) in scores {
            // The callee prologue pays one store per stack argument.
            let mut mask = DenseBitSet::new_empty(score.len());
            for (index, _) in score.iter().enumerate().filter(|(_, benefit)| **benefit > 4) {
                mask.insert(index);
            }
            if !mask.is_empty() {
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
                    let addr = self.static_frame_addr(
                        func_id,
                        EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                            + u64::from(*index) * EvmMemoryLayout::WORD_SIZE,
                    );
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
        if mask.domain_size() != func.params.len() {
            return;
        }
        for i in (0..mask.domain_size()).rev() {
            if mask.contains(i) {
                let addr = self.static_frame_addr(
                    func_id,
                    EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                        + i as u64 * EvmMemoryLayout::WORD_SIZE,
                );
                self.asm.emit_push_deferred(addr);
                self.asm.emit_op(op::MSTORE);
            }
        }
    }

    /// Plans a bounded rotation that keeps computed arguments on the physical
    /// stack while the rest of the caller stack is drained. The resulting
    /// layout matches the existing stack-argument convention: selected
    /// arguments in descending index order above the return address.
    fn plan_retained_stack_args(
        &self,
        func: &Function,
        args: &[ValueId],
        mask: &DenseBitSet<usize>,
    ) -> Option<StackArgRetentionPlan> {
        let selected = mask.count();
        if mask.domain_size() != args.len()
            || selected == 0
            || selected > STACK_ARG_ROTATION_LIMIT
            || self.scheduler.stack.depth() > STACK_ARG_ROTATION_LIMIT + 1
        {
            return None;
        }

        // One physical word cannot fill two argument positions. Repeated
        // values keep the spill-reload path, which materializes each
        // occurrence independently.
        let mut selected_value_counts = FxHashMap::default();
        for (i, &arg) in args.iter().enumerate() {
            if mask.contains(i) && matches!(func.value(arg), crate::mir::Value::Inst(_)) {
                *selected_value_counts.entry(arg).or_insert(0usize) += 1;
            }
        }
        let candidates: Vec<_> = args
            .iter()
            .enumerate()
            .filter_map(|(i, &arg)| {
                (mask.contains(i)
                    && selected_value_counts.get(&arg) == Some(&1)
                    && self.scheduler.stack.contains(arg))
                .then_some(i)
            })
            .collect();
        if candidates.is_empty() {
            return None;
        }
        self.build_stack_arg_retention_plan(args, mask, &candidates)
    }

    fn build_stack_arg_retention_plan(
        &self,
        args: &[ValueId],
        mask: &DenseBitSet<usize>,
        retained_indices: &[usize],
    ) -> Option<StackArgRetentionPlan> {
        let mut keep = FxHashMap::default();
        for &index in retained_indices {
            keep.insert(args[index], index);
        }

        let mut stack = self.scheduler.stack.as_slice().to_vec();
        let mut drain_ops = Vec::new();
        while stack.len() > keep.len() {
            let depth = stack.iter().position(|word| match word {
                Some(value) if keep.contains_key(value) => {
                    stack.iter().filter(|other| **other == *word).count() > 1
                }
                _ => true,
            })?;
            if depth > STACK_ARG_ROTATION_LIMIT {
                return None;
            }
            if depth != 0 {
                drain_ops.push(StackOp::Swap(depth as u8));
                stack.swap(0, depth);
            }
            drain_ops.push(StackOp::Pop);
            stack.remove(0);
        }

        let mut layout = Vec::with_capacity(mask.count() + 1);
        for word in stack {
            layout.push(StaticCallStackWord::Argument(*keep.get(&word?)?));
        }
        layout.insert(0, StaticCallStackWord::ReturnAddress);
        for i in 0..args.len() {
            if mask.contains(i) && !retained_indices.contains(&i) {
                layout.insert(0, StaticCallStackWord::Argument(i));
            }
        }

        let mut target: Vec<_> = (0..args.len())
            .filter(|&i| mask.contains(i))
            .map(StaticCallStackWord::Argument)
            .collect();
        target.reverse();
        target.push(StaticCallStackWord::ReturnAddress);
        if layout.len() != target.len() || layout.len() > STACK_ARG_ROTATION_LIMIT + 1 {
            return None;
        }

        let mut shuffle_ops = Vec::new();
        for target_depth in (1..layout.len()).rev() {
            if layout[target_depth] == target[target_depth] {
                continue;
            }
            let source_depth =
                layout[..=target_depth].iter().position(|&word| word == target[target_depth])?;
            if source_depth != 0 {
                shuffle_ops.push(StackOp::Swap(source_depth as u8));
                layout.swap(0, source_depth);
            }
            shuffle_ops.push(StackOp::Swap(target_depth as u8));
            layout.swap(0, target_depth);
        }
        debug_assert_eq!(layout, target);

        // Baseline drains every tracked word and reloads each computed stack
        // argument through at least PUSH1+MLOAD. A value without a stored slot
        // also pays at least DUP+PUSH1+MSTORE. Deferred addresses can only make
        // that baseline larger, so this is a conservative byte gate.
        let fresh = retained_indices
            .iter()
            .filter(|&&index| !self.scheduler.spills.is_stored(args[index]))
            .count();
        let baseline_cost = self.scheduler.stack.depth() + retained_indices.len() * 3 + fresh * 4;
        let planned_cost = drain_ops.len() + shuffle_ops.len();
        if planned_cost >= baseline_cost {
            return None;
        }

        let mut retained = DenseBitSet::new_empty(args.len());
        for &index in retained_indices {
            retained.insert(index);
        }
        Some(StackArgRetentionPlan { retained, drain_ops, shuffle_ops })
    }

    fn static_frame_addr(&mut self, func_id: FunctionId, offset: u64) -> DeferredConst {
        if let Some((id, references)) = self.static_frame_addr_consts.get_mut(&(func_id, offset)) {
            *references += 1;
            return *id;
        }
        let id = self.asm.new_deferred_const();
        self.static_frame_addr_consts.insert((func_id, offset), (id, 1));
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
        EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
            + ((func.params.len() + func.returns.len()) as u64) * EvmMemoryLayout::WORD_SIZE
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
    /// constant accounts for its exact spill area and every accepted static
    /// allocation, plus the overlaid helper region when one is referenced.
    fn resolve_static_frames(&mut self, module: &Module) {
        let runtime_entries = std::mem::take(&mut self.runtime_entry_funcs);
        let entry_bases: FxHashMap<FunctionId, u64> = runtime_entries
            .iter()
            .copied()
            .map(|func_id| (func_id, Self::external_spill_base(&module.functions[func_id])))
            .collect();
        let mut entry_ends: FxHashMap<FunctionId, u64> = runtime_entries
            .iter()
            .copied()
            .map(|func_id| {
                let func = &module.functions[func_id];
                let spill = self
                    .function_spill_sizes
                    .get(&func_id)
                    .copied()
                    .unwrap_or_else(|| Self::conservative_spill_frame_size(func));
                (func_id, entry_bases[&func_id] + spill)
            })
            .collect();

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
                if self.static_frame_functions.contains(caller) {
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
        let mut static_span = 0;
        for &func_id in &placed {
            let relative = depth.get(&func_id).copied().unwrap_or(0);
            static_span = static_span.max(relative + self.emitted_frame_size(module, func_id));
        }

        let layout = |max_entry_end: u64| {
            if placed.is_empty() {
                (max_entry_end, max_entry_end)
            } else {
                let start = max_entry_end
                    .max(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT + EvmMemoryLayout::WORD_SIZE);
                (start, start + static_span)
            }
        };

        // Prefer eligible allocations before each entry's exact spill area,
        // then fall back to appending them after spills when only spill pushes
        // prevent the lower placement.
        // Entries overlay because only one dispatcher arm executes per call.
        // Reject any proposal that widens a shared heap/static-frame or
        // ranked-spill push.
        let mut static_alloc_sizes: FxHashMap<FunctionId, u64> = FxHashMap::default();
        let mut post_spill_entries = FxHashSet::default();
        for func_id in runtime_entries {
            let Some(allocations) = self.pending_static_allocs.remove(&func_id) else { continue };
            for (alloc, size) in allocations {
                let current_static_size = static_alloc_sizes.get(&func_id).copied().unwrap_or(0);
                let proposed_static_size = current_static_size + size;
                let current_end = entry_ends[&func_id];
                let proposed_end = current_end + size;
                let before_max = entry_ends.values().copied().max().unwrap_or(0);
                let after_max = entry_ends
                    .iter()
                    .map(|(&entry, &end)| if entry == func_id { proposed_end } else { end })
                    .max()
                    .unwrap_or(proposed_end);
                let (before_start, before_end) = layout(before_max);
                let (after_start, after_end) = layout(after_max);

                let mut addresses = Vec::with_capacity(self.static_frame_addr_consts.len() + 1);
                if self.runtime_free_memory_const.is_some() {
                    addresses.push(RelayoutAddress {
                        before: before_end,
                        after: after_end,
                        references: 1,
                    });
                }
                addresses.extend(self.static_frame_addr_consts.iter().map(
                    |(&(static_func, offset), &(_, references))| {
                        let relative = depth.get(&static_func).copied().unwrap_or(0) + offset;
                        RelayoutAddress {
                            before: before_start + relative,
                            after: after_start + relative,
                            references,
                        }
                    },
                ));
                let global_width_neutral = preserves_push_width(addresses.iter().copied());
                let spills_width_neutral =
                    self.external_spill_addr_consts.get(&func_id).is_none_or(|spills| {
                        let base = entry_bases[&func_id];
                        preserves_push_width(spills.iter().enumerate().map(
                            |(rank, &(_, references))| {
                                let offset = rank as u64 * 32;
                                RelayoutAddress {
                                    before: base + current_static_size + offset,
                                    after: base + proposed_static_size + offset,
                                    references,
                                }
                            },
                        ))
                    });

                if global_width_neutral
                    && spills_width_neutral
                    && !post_spill_entries.contains(&func_id)
                {
                    let static_address = entry_bases[&func_id] + current_static_size;
                    self.asm.set_deferred_alloc_static(alloc, U256::from(static_address));
                    entry_ends.insert(func_id, proposed_end);
                    static_alloc_sizes.insert(func_id, proposed_static_size);
                } else if global_width_neutral {
                    // If inserting before spills would widen one of their
                    // pushes, append after the exact spill area instead. Once
                    // an entry uses this suffix, later allocations must stay
                    // there so already-emitted static addresses never move.
                    self.asm.set_deferred_alloc_static(alloc, U256::from(current_end));
                    entry_ends.insert(func_id, proposed_end);
                    post_spill_entries.insert(func_id);
                } else {
                    self.asm.set_deferred_alloc_dynamic(alloc, U256::from(size));
                }
            }
        }

        // A retained candidate should always belong to an emitted external
        // entry. Lower defensively to the dynamic form if an unusual pipeline
        // shape leaves one behind.
        for (_, allocations) in self.pending_static_allocs.drain() {
            for (alloc, size) in allocations {
                self.asm.set_deferred_alloc_dynamic(alloc, U256::from(size));
            }
        }

        for (func_id, spills) in self.external_spill_addr_consts.drain() {
            let base = Self::external_spill_base(&module.functions[func_id])
                + static_alloc_sizes.get(&func_id).copied().unwrap_or(0);
            for (rank, (id, _)) in spills.into_iter().enumerate() {
                self.asm.set_deferred_const(id, U256::from(base + rank as u64 * 32));
            }
        }

        let max_entry_end = entry_ends.values().copied().max().unwrap_or(0);
        let (region_start, region_end) = layout(max_entry_end);
        for (&(func_id, offset), &(id, _)) in &self.static_frame_addr_consts {
            let relative = depth.get(&func_id).copied().unwrap_or(0) + offset;
            self.asm.set_deferred_const(id, U256::from(region_start + relative));
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
            EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT + EvmMemoryLayout::WORD_SIZE
        } else {
            EvmMemoryLayout::HEAP_START
        };
        low_memory_start + func.internal_frame_size.max(func.external_static_return_size)
    }

    fn constructor_free_memory_start(spill_size: u64) -> u64 {
        EvmMemoryLayout::CONSTRUCTOR_HEAP_FLOOR.max(EvmMemoryLayout::SPILL_BASE + spill_size)
    }

    fn uses_internal_frame_slot(func: &Function) -> bool {
        func.instructions.iter().any(|inst| matches!(inst.kind, InstKind::InternalCall { .. }))
    }

    fn emit_external_free_memory_start(&mut self) -> DeferredConst {
        let id = self.asm.new_deferred_const();
        self.asm.emit_push_deferred(id);
        self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
        self.asm.emit_op(op::MSTORE);
        id
    }

    fn emit_spill_slot_addr(&mut self, func: &Function, slot: SpillSlot) {
        if self.in_internal_function {
            let spill_base = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                + (func.params.len() as u64) * EvmMemoryLayout::WORD_SIZE
                + (func.returns.len() as u64) * EvmMemoryLayout::WORD_SIZE;
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

    /// Ranks the external body's spill slots by reference count, hottest
    /// first, so the most reloaded slots receive the shortest addresses after
    /// final layout. The ranking is a bijection over the same slot area —
    /// every site of a slot goes through one deferred constant — so sizes and
    /// disjointness are unchanged.
    fn assign_ranked_spill_addrs(&mut self, func_id: FunctionId) {
        if self.spill_addr_consts.is_empty() {
            return;
        }
        let mut slots: Vec<(u64, (DeferredConst, usize))> =
            self.spill_addr_consts.drain().collect();
        slots.sort_by(|a, b| b.1.1.cmp(&a.1.1).then(a.0.cmp(&b.0)));
        self.external_spill_addr_consts
            .insert(func_id, slots.into_iter().map(|(_, deferred)| deferred).collect());
    }

    fn emit_internal_arg_load(&mut self, index: u32) {
        self.emit_own_frame_addr(
            EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                + u64::from(index) * EvmMemoryLayout::WORD_SIZE,
        );
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
        if self.static_frame_functions.contains(callee) {
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
        let static_frame_size = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
            + ((args.len() + returns) as u64) * EvmMemoryLayout::WORD_SIZE
            + static_local_frame_size;
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
        self.asm.emit_push(U256::from(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT));
        self.asm.emit_op(op::MLOAD);
        self.scheduler.stack.push_unknown();
        self.emit_internal_frame_store_from_top_preserving_base(32);

        for (i, &arg) in args.iter().enumerate() {
            self.emit_operand(func, arg);
            self.emit_internal_frame_store_from_top_preserving_base(
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    + (i as u64) * EvmMemoryLayout::WORD_SIZE,
            );
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
            self.emit_current_internal_frame_addr(
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    + (args.len() as u64) * EvmMemoryLayout::WORD_SIZE,
            );
            self.asm.emit_op(op::MLOAD);
            self.scheduler.stack.push(result);
            // Store the result to its reserved slot now, while it is on top.
            // Other value-producing instructions do this; internal calls did
            // not, so a reserved result (e.g. a recompute leaf of a live-out
            // cheap value) was never stored. No-op unless reserved and live.
            self.spill_top_value_if_live(func, liveness, block, inst_idx, result);
        }

        // Copy returns 2..N to an ephemeral buffer at the current free-memory
        // pointer. Keep the base below the loop and publish it through the
        // dedicated scratch word afterwards; the first return stays on the
        // stack. This happens before restoring the frame pointer while the
        // callee frame remains addressable.
        if returns > 1 {
            self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
            self.asm.emit_op(op::MLOAD);
            self.asm.emit_push(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT));
            self.asm.emit_op(op::MSTORE);
            for i in 1..returns {
                self.emit_current_internal_frame_addr(
                    EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                        + (args.len() as u64) * EvmMemoryLayout::WORD_SIZE
                        + (i as u64) * EvmMemoryLayout::WORD_SIZE,
                );
                self.asm.emit_op(op::MLOAD);
                self.asm.emit_push(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT));
                self.asm.emit_op(op::MLOAD);
                self.asm.emit_push(U256::from((i as u64) * 32));
                self.asm.emit_op(op::ADD);
                self.asm.emit_op(op::MSTORE);
            }
        }

        // Deallocate the callee frame in strict LIFO order by restoring the
        // free memory pointer to the callee frame base. This must happen before
        // restoring the caller frame pointer because `emit_current_internal_frame_addr`
        // reads the internal-frame pointer slot. Do this only when the callee's declared
        // params/returns contain no memory pointer: memory pointer returns may
        // reference the callee's frame/heap region, and a memory pointer param lets
        // the callee install a fresh pointer into caller-visible memory. Solidity
        // allocation lowering zero-initializes new arrays/bytes/structs, so reclaimed
        // frame bytes need not be wiped.
        if self.restorable_internal_frames.contains(callee) {
            self.emit_current_internal_frame_addr(0);
            self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
            self.asm.emit_op(op::MSTORE);
        }

        // Restore the caller frame pointer. If a result is on the stack, this leaves it there.
        self.emit_current_internal_frame_addr(32);
        self.asm.emit_op(op::MLOAD);
        self.asm.emit_push(U256::from(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT));
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
            if stack_mask.as_ref().is_some_and(|mask| mask.contains(i)) {
                continue;
            }
            self.emit_operand(func, arg);
            let addr = self.static_frame_addr(
                callee,
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    + (i as u64) * EvmMemoryLayout::WORD_SIZE,
            );
            self.asm.emit_push_deferred(addr);
            self.scheduler.stack.push_unknown();
            self.asm.emit_op(op::MSTORE);
            self.scheduler.instruction_executed(2, None);
        }

        let retention_plan =
            stack_mask.as_ref().and_then(|mask| self.plan_retained_stack_args(func, args, mask));

        // A computed argument not retained physically survives the drain in
        // its spill slot and is reloaded raw after it; make sure the slot is
        // written while the value is still reachable.
        if let Some(mask) = &stack_mask {
            for (i, &arg) in args.iter().enumerate() {
                if mask.contains(i)
                    && !retention_plan.as_ref().is_some_and(|plan| plan.retained.contains(i))
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

        if let Some(plan) = &retention_plan {
            for &op in &plan.drain_ops {
                self.emit_stack_op(op);
            }
            debug_assert_eq!(self.scheduler.stack.depth(), plan.retained.count());
        } else {
            self.pop_all_stack_values();
        }
        self.scheduler.clear_stack();

        self.asm.emit_push_label(return_label);
        // Stack-passed arguments ride above the return address, untracked by
        // the model like the return address itself; the callee prologue
        // stores them into its frame before its body runs.
        if let Some(mask) = &stack_mask {
            for (i, &arg) in args.iter().enumerate() {
                if mask.contains(i)
                    && !retention_plan.as_ref().is_some_and(|plan| plan.retained.contains(i))
                {
                    self.emit_raw_stack_arg(func, arg);
                }
            }
        }
        if let Some(plan) = &retention_plan {
            for &op in &plan.shuffle_ops {
                debug_assert!(matches!(op, StackOp::Swap(_)));
                self.asm.emit_op(op.opcode());
            }
        }
        self.asm.emit_push_label(callee_label);
        self.asm.emit_op(op::JUMP);

        self.asm.define_label(return_label);
        self.scheduler.clear_stack();

        if let Some(result) = result
            && returns > 0
        {
            let addr = self.static_frame_addr(
                callee,
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    + (args.len() as u64) * EvmMemoryLayout::WORD_SIZE,
            );
            self.asm.emit_push_deferred(addr);
            self.asm.emit_op(op::MLOAD);
            self.scheduler.stack.push(result);
            self.spill_top_value_if_live(func, liveness, block, inst_idx, result);
        }

        // Copy return values 2..N into the same ephemeral buffer as the
        // dynamic-frame path.
        if returns > 1 {
            self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
            self.asm.emit_op(op::MLOAD);
            self.asm.emit_push(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT));
            self.asm.emit_op(op::MSTORE);
            for i in 1..returns {
                let addr = self.static_frame_addr(
                    callee,
                    EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                        + ((args.len() + i) as u64) * EvmMemoryLayout::WORD_SIZE,
                );
                self.asm.emit_push_deferred(addr);
                self.asm.emit_op(op::MLOAD);
                self.asm.emit_push(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT));
                self.asm.emit_op(op::MLOAD);
                self.asm.emit_push(U256::from((i as u64) * 32));
                self.asm.emit_op(op::ADD);
                self.asm.emit_op(op::MSTORE);
            }
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
                ScheduledOp::LoadArg(index) => {
                    if self.in_internal_function {
                        self.emit_internal_arg_load(index);
                    } else if self.in_constructor {
                        // Constructor args were copied to memory at 0x80
                        // Load from memory: 0x80 + index * 32
                        let offset = EvmMemoryLayout::HEAP_START
                            + (index as u64) * EvmMemoryLayout::WORD_SIZE;
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
                    let offset =
                        EvmMemoryLayout::HEAP_START + (*index as u64) * EvmMemoryLayout::WORD_SIZE;
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
                // A value carried on the live stack is the current definition;
                // duplicate it instead of reloading or recomputing. A preserved
                // edge can carry a value that was never spilled, and
                // recomputing a definition such as an FMP load would observe
                // memory that changed since the definition executed.
                if let Some(depth) = self.scheduler.stack.find(val)
                    && depth < MAX_STACK_ACCESS
                {
                    self.emit_stack_op(StackOp::Dup(depth as u8 + 1));
                    return;
                }
                // For instruction results, we need to check if they're spilled
                // or if they're instruction results that produce fresh values (like GAS, MLOAD)
                if let Some(slot) = self.scheduler.spills.get(val)
                    && self.scheduler.spills.is_stored(val)
                {
                    // Load from spill slot. Reloadable covers slots whose
                    // defining block is emitted later: the definition still
                    // executes before any use at runtime.
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
                                    EvmMemoryLayout::IMMUTABLE_SCRATCH_BASE + u64::from(*offset),
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
                            // Re-reading a constant scratch location is safe, but the
                            // free-memory-pointer word moves: a pointer defined as
                            // `mload(0x40)` must reach this point through its spill
                            // slot. A slot that is reloadable but not yet stored
                            // belongs to a defining block emitted after this point
                            // that still executes first at runtime.
                            if func.value_u64(*offset) == Some(EvmMemoryLayout::FMP_SLOT) {
                                if let Some(slot) = self.scheduler.spills.get(val)
                                    && self.scheduler.spills.is_reloadable(val)
                                {
                                    self.emit_spill_slot_addr(func, slot);
                                    self.asm.emit_op(op::MLOAD);
                                    self.scheduler.stack.push(val);
                                    return;
                                }
                                panic!(
                                    "emit_value_fresh: rematerializing a stale \
                                     free-memory-pointer load: {val:?} in `{}`",
                                    func.name
                                );
                            }
                            self.emit_value_fresh(func, *offset);
                            self.asm.emit_op(op::MLOAD);
                            // Pop offset, push result
                            self.scheduler.stack.pop();
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::CalldataLoad(offset) => {
                            // Calldata is immutable, so re-reading it is
                            // always safe once the address rematerializes.
                            self.emit_value_fresh(func, *offset);
                            self.asm.emit_op(op::CALLDATALOAD);
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

    /// Emits a ternary operation with liveness awareness.
    #[allow(clippy::too_many_arguments)]
    fn emit_ternary_op(
        &mut self,
        func: &Function,
        operands: [ValueId; 3],
        opcode: u8,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        for (i, &operand) in operands.iter().enumerate() {
            if i == 0 {
                self.emit_value(func, operand);
            } else {
                self.emit_operand(func, operand);
            }
            let seen = operands[..=i].iter().filter(|&&op| op == operand).count();
            if !self.block_local_copy_survives(liveness, block, operand, seen) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, operand);
            }
        }
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
        let return_base = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
            + (func.params.len() as u64) * EvmMemoryLayout::WORD_SIZE;
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

    fn emit_external_stop(&mut self) {
        if let Some(exit) = self.constructor_exit {
            self.asm.emit_push_label(exit);
            self.asm.emit_op(op::JUMP);
        } else {
            self.asm.emit_op(op::STOP);
        }
    }

    fn constant_switch_entries(
        &self,
        func: &Function,
        cases: &[(ValueId, BlockId)],
    ) -> Option<(Vec<U256>, Vec<MirSwitchEntry>)> {
        let mut entries = cases
            .iter()
            .map(|&(value_id, target)| {
                let value = func.value(value_id).as_immediate()?.as_u256()?;
                Some(MirSwitchEntry { value, value_id, target })
            })
            .collect::<Option<Vec<_>>>()?;
        let linear_values = entries.iter().map(|entry| entry.value).collect();
        entries.sort_unstable_by_key(|entry| entry.value);
        if entries.windows(2).any(|entries| entries[0].value == entries[1].value) {
            return None;
        }
        Some((linear_values, entries))
    }

    fn emit_linear_mir_switch(&mut self, func: &Function, cases: &[(ValueId, BlockId)]) {
        for &(value_id, target) in cases {
            let value = func.value(value_id).as_immediate().and_then(|value| value.as_u256());
            self.emit_mir_switch_eq_jump(func, value_id, value, target);
        }
    }

    fn emit_binary_mir_switch(
        &mut self,
        func: &Function,
        entries: &[MirSwitchEntry],
        default: BlockId,
        can_fallthrough: bool,
        leaf_size: usize,
    ) {
        if entries.len() <= leaf_size {
            for entry in entries {
                self.emit_mir_switch_eq_jump(func, entry.value_id, Some(entry.value), entry.target);
            }
            self.emit_mir_switch_default(default, can_fallthrough);
            return;
        }

        let mid = entries.len() / 2;
        let left_label = self.asm.new_label();
        let entry_stack = self.scheduler.stack.clone();

        // With the pivot on top, GT computes `pivot > selector`.
        self.asm.emit_op(op::DUP1);
        self.scheduler.stack.dup(1);
        self.emit_operand(func, entries[mid].value_id);
        self.asm.emit_op(op::GT);
        self.scheduler.instruction_executed_untracked(2);
        self.asm.emit_push_label(left_label);
        self.asm.emit_op(op::JUMPI);
        self.scheduler.instruction_executed(1, None);

        self.emit_binary_mir_switch(func, &entries[mid..], default, false, leaf_size);

        self.asm.define_label(left_label);
        self.scheduler.stack = entry_stack;
        self.emit_binary_mir_switch(func, &entries[..mid], default, can_fallthrough, leaf_size);
    }

    fn emit_bucketed_mir_switch(
        &mut self,
        func: &Function,
        entries: &[MirSwitchEntry],
        default: BlockId,
        can_fallthrough: bool,
        bucket_count: usize,
    ) {
        let mut buckets = vec![Vec::new(); bucket_count];
        for &entry in entries {
            buckets[bucket_index(entry.value, bucket_count)].push(entry);
        }
        let default_label = self.block_labels[&default];
        let empty_label = (!self.emitting_dispatch_entry
            && buckets.iter().any(|bucket| bucket.is_empty()))
        .then(|| self.asm.new_label());
        let bucket_labels: Vec<_> = buckets
            .iter()
            .map(|bucket| {
                if bucket.is_empty() {
                    empty_label.unwrap_or(default_label)
                } else {
                    self.asm.new_label()
                }
            })
            .collect();

        self.asm.emit_op(op::DUP1);
        self.scheduler.stack.dup(1);
        self.asm.emit_push(U256::from(bucket_count));
        self.scheduler.stack.push_unknown();
        self.asm.emit_op(op::SWAP1);
        self.scheduler.stack_swapped();
        self.asm.emit_op(op::MOD);
        self.scheduler.instruction_executed_untracked(2);
        self.asm.emit_indexed_jump(bucket_labels.clone());
        self.scheduler.stack.pop();

        let entry_stack = self.scheduler.stack.clone();
        if let Some(empty_label) = empty_label {
            self.asm.define_label(empty_label);
            self.scheduler.stack = entry_stack.clone();
            self.emit_mir_switch_default(default, false);
        }
        let last_bucket = buckets.iter().rposition(|bucket| !bucket.is_empty()).unwrap();
        for (index, (label, bucket)) in bucket_labels.into_iter().zip(buckets).enumerate() {
            if bucket.is_empty() {
                continue;
            }
            self.asm.define_label(label);
            self.scheduler.stack = entry_stack.clone();
            for entry in bucket {
                self.emit_mir_switch_eq_jump(func, entry.value_id, Some(entry.value), entry.target);
            }
            self.emit_mir_switch_default(default, can_fallthrough && index == last_bucket);
        }
    }

    fn emit_dense_mir_switch(
        &mut self,
        entries: &[MirSwitchEntry],
        default: BlockId,
        low: U256,
        range: usize,
    ) {
        let mut targets = vec![self.block_labels[&default]; range];
        for entry in entries {
            let index = usize::try_from(entry.value - low)
                .expect("dense switch table index must fit usize");
            targets[index] = self.block_labels[&entry.target];
        }
        let in_range = self.asm.new_label();

        if !low.is_zero() {
            self.asm.emit_push(low);
            self.scheduler.stack.push_unknown();
            self.asm.emit_op(op::SWAP1);
            self.scheduler.stack_swapped();
            self.asm.emit_op(op::SUB);
            self.scheduler.instruction_executed_untracked(2);
        }
        self.asm.emit_op(op::DUP1);
        self.scheduler.stack.dup(1);
        self.asm.emit_push(U256::from(range));
        self.scheduler.stack.push_unknown();
        self.asm.emit_op(op::GT);
        self.scheduler.instruction_executed_untracked(2);
        self.asm.emit_push_label(in_range);
        self.asm.emit_op(op::JUMPI);
        self.scheduler.instruction_executed(1, None);

        let indexed_stack = self.scheduler.stack.clone();
        self.asm.emit_op(op::POP);
        self.scheduler.stack.pop();
        self.asm.emit_push_label(self.block_labels[&default]);
        self.asm.emit_op(op::JUMP);

        self.asm.define_label(in_range);
        self.scheduler.stack = indexed_stack;
        self.asm.emit_indexed_jump(targets);
        self.scheduler.stack.pop();
    }

    fn emit_mir_switch_eq_jump(
        &mut self,
        func: &Function,
        value_id: ValueId,
        value: Option<U256>,
        target: BlockId,
    ) {
        self.asm.emit_op(op::DUP1);
        self.scheduler.stack.dup(1);
        if value.is_some_and(|value| value.is_zero())
            && self.gcx.sess.opts.optimization != OptimizationMode::None
        {
            self.asm.emit_op(op::ISZERO);
            self.scheduler.instruction_executed_untracked(1);
        } else {
            self.emit_operand(func, value_id);
            self.asm.emit_op(op::EQ);
            self.scheduler.instruction_executed_untracked(2);
        }
        self.asm.emit_push_label(self.block_labels[&target]);
        self.asm.emit_op(op::JUMPI);
        self.scheduler.instruction_executed(1, None);
    }

    fn emit_mir_switch_default(&mut self, default: BlockId, can_fallthrough: bool) {
        if !self.emitting_dispatch_entry {
            self.asm.emit_op(op::POP);
            self.scheduler.stack.pop();
        }
        if !can_fallthrough {
            self.asm.emit_push_label(self.block_labels[&default]);
            self.asm.emit_op(op::JUMP);
        }
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
                        self.static_frame_functions.contains(*function),
                        "argument-carrying tail call to a non-static-frame callee"
                    );
                    for (i, &arg) in args.iter().enumerate() {
                        self.emit_operand(func, arg);
                        let addr = self.static_frame_addr(
                            *function,
                            EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                                + (i as u64) * EvmMemoryLayout::WORD_SIZE,
                        );
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
                        if self.block_is_cold(*then_block) && !self.block_is_cold(*else_block) {
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
                let constant_entries = self.constant_switch_entries(func, cases);
                let plan = constant_entries.as_ref().map_or(
                    SwitchPlan::Linear,
                    |(linear_values, entries)| {
                        let values: Vec<_> = entries.iter().map(|entry| entry.value).collect();
                        let default =
                            match (self.emitting_dispatch_entry, fallthrough == Some(*default)) {
                                (true, true) => SwitchDefault::Fallthrough,
                                (true, false) => SwitchDefault::Jump,
                                (false, true) => SwitchDefault::CleanupFallthrough,
                                (false, false) => SwitchDefault::CleanupJump,
                            };
                        select_switch_plan_with_linear_values(
                            &values,
                            linear_values,
                            self.gcx.sess.opts.optimization,
                            self.gcx.sess.opts.evm_version,
                            default,
                            self.switch_table_target_width(),
                        )
                    },
                );
                let constant_entries = constant_entries.map(|(_, entries)| entries);

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

                match (plan, constant_entries) {
                    (SwitchPlan::Binary { leaf_size }, Some(entries)) => {
                        self.emit_binary_mir_switch(
                            func,
                            &entries,
                            *default,
                            fallthrough == Some(*default),
                            leaf_size,
                        );
                    }
                    (SwitchPlan::Buckets { bucket_count }, Some(entries)) => {
                        self.emit_bucketed_mir_switch(
                            func,
                            &entries,
                            *default,
                            fallthrough == Some(*default),
                            bucket_count,
                        );
                    }
                    (SwitchPlan::Dense { low, range }, Some(entries)) => {
                        self.emit_dense_mir_switch(&entries, *default, low, range);
                    }
                    _ => {
                        self.emit_linear_mir_switch(func, cases);
                        self.emit_mir_switch_default(*default, fallthrough == Some(*default));
                    }
                }
            }

            Terminator::Return { values } => {
                if self.in_internal_function {
                    self.emit_internal_return(func, values);
                    return;
                }

                assert!(values.is_empty(), "external ABI returns with values must use ReturnData");
                self.emit_external_stop();
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
                    self.emit_external_stop();
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

/// The artifact produced by the EVM backend.
#[derive(Clone, Debug, Default)]
pub struct EvmArtifact {
    /// Deployment (init) bytecode that, when run, returns the runtime code.
    pub deployment: Vec<u8>,
    /// Runtime bytecode, i.e. the code stored on-chain.
    pub runtime: Vec<u8>,
    /// Final creation-code EVM IR segments in bytecode order.
    pub deployment_evm_ir: Vec<ir::Module>,
    /// Final runtime EVM IR immediately before byte emission.
    pub runtime_evm_ir: Option<ir::Module>,
}

impl crate::backend::Backend for EvmCodegen<'_> {
    type Output = EvmArtifact;

    fn lower_module(&mut self, module: &mut Module) -> EvmArtifact {
        self.generate_deployment_artifact(module)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backend::evm::test_utils::disassemble,
        lower,
        mir::{FunctionBuilder, MirType},
    };
    use snapbox::assert_data_eq;
    use solar_config::{CompileOpts, UnstableOpts};
    use solar_interface::{Ident, Session, kw, sym};
    use solar_sema::{Compiler, hir::Visibility};
    use std::{ops::ControlFlow, path::PathBuf};

    fn with_codegen<T: Send>(opts: CompileOpts, f: impl FnOnce(EvmCodegen<'_>) -> T + Send) -> T {
        let compiler = Compiler::new(Session::builder().opts(opts).build());
        compiler.enter(|c| f(EvmCodegen::new(c.gcx())))
    }

    #[test]
    fn constructor_success_jumps_to_deployment_postlude() {
        with_codegen(CompileOpts::default(), |mut codegen| {
            let mut module = Module::new(Ident::with_dummy_span(sym::Test));
            let mut constructor = Function::new(Ident::with_dummy_span(kw::Constructor));
            constructor.attributes.is_constructor = true;
            {
                let mut builder = FunctionBuilder::new(&mut constructor);
                let condition = builder.imm_u64(1);
                let revert = builder.create_block();
                let success = builder.create_block();
                builder.branch(condition, revert, success);

                builder.switch_to_block(revert);
                let zero = builder.imm_u64(0);
                builder.revert(zero, zero);

                builder.switch_to_block(success);
                builder.stop();
            }
            module.add_function(constructor);
            let prepared = codegen.prepare_deployment_prefix(&module, 0, 0, &[]);
            let mut deploy_code_len = 0;
            let deployment = loop {
                let code = codegen.assemble_deployment_prefix(&prepared, 0, deploy_code_len);
                if code.bytecode.len() == deploy_code_len {
                    break code;
                }
                deploy_code_len = code.bytecode.len();
            };

            assert_data_eq!(
                disassemble(&deployment.bytecode),
                snapbox::str![[r#"
PUSH2 0x4000
PUSH1 0x40
MSTORE
PUSH1 0x01
PUSH1 0x13
JUMPI
PUSH0
DUP1
PUSH1 0x17
PUSH0
CODECOPY
PUSH0
RETURN
JUMPDEST
PUSH0
DUP1
REVERT

"#]]
            );
        });
    }

    #[test]
    fn empty_external_return_falls_off_end() {
        with_codegen(CompileOpts::default(), |mut codegen| {
            let mut function = Function::new(Ident::with_dummy_span(sym::Test));
            function.attributes.visibility = Visibility::External;
            FunctionBuilder::new(&mut function).ret(Vec::new());
            codegen.generate_function_body(FunctionId::from_usize(0), &function);

            assert!(codegen.asm.assemble().bytecode.is_empty());
        });
    }

    #[test]
    fn cold_forwarder_selects_hot_fallthrough() {
        let (module, caller_id, entry, cold_forwarder, cold_block, hot_block) =
            with_codegen(CompileOpts::default(), |mut codegen| {
                let mut module = Module::new(Ident::with_dummy_span(sym::Test));

                let mut cold_func = Function::new(Ident::with_dummy_span(sym::__revert_error));
                {
                    let mut builder = FunctionBuilder::new(&mut cold_func);
                    let zero = builder.imm_u64(0);
                    builder.revert(zero, zero);
                }
                let cold_func = module.add_function(cold_func);

                let mut cold_wrapper = Function::new(Ident::with_dummy_span(sym::Test));
                {
                    let mut builder = FunctionBuilder::new(&mut cold_wrapper);
                    builder.internal_call_void(cold_func, Vec::new(), 0);
                    builder.ret(Vec::new());
                }
                let cold_wrapper = module.add_function(cold_wrapper);

                let mut caller = Function::new(Ident::with_dummy_span(sym::Test));
                let entry = BlockId::ENTRY;
                let (cold_forwarder, cold_block, hot_block);
                {
                    let mut builder = FunctionBuilder::new(&mut caller);
                    let condition = builder.add_param(MirType::Bool);
                    cold_forwarder = builder.create_block();
                    cold_block = builder.create_block();
                    hot_block = builder.create_block();
                    builder.branch(condition, cold_forwarder, hot_block);

                    builder.switch_to_block(cold_forwarder);
                    builder.jump(cold_block);

                    builder.switch_to_block(cold_block);
                    builder.tail_call(cold_wrapper, Vec::new());

                    builder.switch_to_block(hot_block);
                    builder.ret(Vec::new());
                }
                let caller_id = module.add_function(caller);
                codegen.cold_functions = EvmCodegen::collect_cold_functions(&module);
                let caller = &module.functions[caller_id];
                codegen.cold_blocks = codegen.collect_cold_blocks(caller);

                assert!(codegen.cold_functions.contains(cold_func));
                assert!(codegen.cold_functions.contains(cold_wrapper));
                assert!(codegen.block_aborts(caller, cold_block));
                assert!(!codegen.block_aborts(caller, cold_forwarder));
                assert!(codegen.block_is_cold(cold_forwarder));
                assert_eq!(
                    codegen.block_layout_order(caller),
                    [entry, hot_block, cold_forwarder, cold_block]
                );

                (module, caller_id, entry, cold_forwarder, cold_block, hot_block)
            });

        let opts = CompileOpts { optimization: OptimizationMode::None, ..Default::default() };
        with_codegen(opts, |mut codegen| {
            codegen.cold_functions = EvmCodegen::collect_cold_functions(&module);
            let caller = &module.functions[caller_id];
            codegen.cold_blocks = codegen.collect_cold_blocks(caller);
            assert_eq!(
                codegen.block_layout_order(caller),
                [entry, cold_forwarder, cold_block, hot_block]
            );
        });
    }

    /// Helper to compile Solidity source to bytecode, returning Result.
    fn compile_source(source: &str) -> Result<Vec<u8>, String> {
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
                    let mut codegen = EvmCodegen::new(gcx);
                    let bytecode = codegen.generate_deployment_artifact(&mut module).runtime;
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

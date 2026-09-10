//! EVM bytecode generation from MIR.
//!
//! This module generates EVM bytecode from MIR using:
//! - Liveness analysis to know when values die
//! - Phi elimination to convert SSA to parallel copies
//! - Stack scheduling to generate DUP/SWAP sequences
//! - EVM IR optimization, relocation, and byte encoding
//!
//! Deployment and runtime modules coordinate artifact emission. Function,
//! instruction, value, and terminator modules emit scheduled EVM IR. Internal
//! calling conventions live in `calls`; physical memory placement lives in
//! `frames`. The private `stack` subtree owns operand scheduling, CFG layout
//! planning, edge transitions, and spilling.

use self::{
    memory_contract::MemoryCheckedEmitter,
    stack::{
        MAX_STACK_ACCESS, OperandCostModel, OperandPlan, ScheduleCost, ScheduledOp, SpillSlot,
        StackScheduler, TargetSlot, cross_block_values, is_cross_block_recomputable_kind,
        is_rematerializable_leaf,
        layout::{
            GlobalStackPlan, StackPhiBranch, StackPhiEdge, StackPhiPlan, planned_entry_carries,
        },
        rematerializable_nullary_opcode, rematerializable_nullary_value,
    },
    switch::MAX_GAS_CODE_GROWTH,
};
use super::{
    DebugFunction, DebugFunctionExit, DebugInstruction, ir,
    layout::{RelayoutAddress, preserves_push_width},
    op::{self, WORD_BYTES},
};
use crate::{
    backend::assembler::{
        ArtifactKind, Assembler, DeferredAlloc, DeferredConst, ImmutableRef, Label,
    },
    mir::{
        ArgIdx, BlockId, EffectKind, Function, FunctionId, ImmutableEncoding, ImmutableId, InstId,
        InstKind, MemoryRegion, MirPhase, MirType, Module, Terminator, Value, ValueId,
        analysis::{
            AliasAnalysis, CallGraphInfo, CfgInfo, CopyDest, CopySource, Liveness, Loop,
            LoopAnalyzer, MemoryBase, ParallelCopy, PhiEliminator,
        },
        immutable::{
            immutable_push_type_size, immutable_staging_addr, immutable_staging_base,
            immutable_staging_end,
        },
        memory::EvmMemoryLayout,
        pass::run_pipeline,
    },
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_config::OptimizationMode;
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet},
};
use solar_sema::Gcx;
use std::{cell::OnceCell, collections::hash_map::Entry as StdEntry, rc::Rc};

mod stack;
pub(super) use stack::{
    MAX_STACK_DEPTH, StackModel, StackOp, lowered_stack_cost, resynthesize_physical_ops,
};

mod switch;

mod calls;
mod deployment;
mod frames;
mod function;
mod instructions;
mod memory_contract;
mod runtime;
mod terminator;
mod values;

const STACK_PHI_LAYOUT_LIMIT: usize = 8;
/// Bounds profitability search; physical layouts use the EVM stack-access limit.
const GLOBAL_STACK_LAYOUT_LIMIT: usize = 8;

#[derive(Default)]
struct GeneratedCode {
    bytecode: Vec<u8>,
    evm_ir: Option<ir::Module>,
    debug_info: Option<Vec<DebugInstruction>>,
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

/// Stack arguments whose static-frame stores are delayed until their first instruction use.
///
/// `args` follows physical stack order, highest argument index first. Values in `frame_values` are
/// used again and therefore receive a store immediately before that use; the others die on the
/// stack without ever occupying their declared frame slot.
#[derive(Clone, Debug)]
struct LazyStackArgPlan {
    args: Vec<(ArgIdx, ValueId)>,
    frame_values: DenseBitSet<ValueId>,
}

type CanonicalArgValues = IndexVec<ArgIdx, Option<ValueId>>;

struct StackArgUseInfo {
    use_counts: FxHashMap<ValueId, usize>,
    non_entry_uses: DenseBitSet<ValueId>,
    call_uses: DenseBitSet<ValueId>,
    entry_first_uses: FxHashMap<ValueId, usize>,
    first_entry_call: Option<usize>,
}

#[derive(Clone)]
struct SpillStore {
    value: ValueId,
    slot: SpillSlot,
    block: ir::BlockId,
    range: std::ops::Range<usize>,
}

/// A single-use call gas operand rebuilt at the call site.
struct LateGasOperand {
    subtracted: Option<U256>,
}

impl LazyStackArgPlan {
    fn values(&self) -> impl Iterator<Item = ValueId> + '_ {
        self.args.iter().map(|&(_, value)| value)
    }
}

/// A profitable static-call layout whose caller words stay below the
/// untracked return address until control returns.
#[derive(Clone, Debug)]
struct StaticCallStackPlan {
    prepare_ops: Vec<StackOp>,
    caller_stack: StackModel,
}

/// Stack-native exit signature for an internal callee.
#[derive(Clone, Copy, Debug)]
struct StackReturnPlan {
    /// Number of result words left on the physical stack.
    arity: usize,
    /// First local/spill byte in the original MIR frame layout.
    local_base: u64,
    /// Whether the call occurs within assembly that may retain source scratch contents.
    preserve_source_scratch: bool,
}

/// Caller-side binding of stack-returned tuple words to their multi-return
/// protocol loads.
struct StackResultProjection {
    /// The buffer-pointer read, its offset additions, and the extra-return
    /// loads; all skipped during emission.
    elided: Vec<InstId>,
    /// The adopted load result for each extra return index `1..arity`.
    extras: Vec<Option<ValueId>>,
}

/// Subset-invariant analyses shared by one resident-layout subset search.
struct ResidentSearchContext {
    /// Planned stack-phi edges, present when the function has phis.
    phi_plan: Option<Rc<StackPhiPlan>>,
    /// CFG facts whose memoized dominators persist across candidates.
    cfg: CfgInfo,
    /// Operand occurrences per candidate value across the whole function.
    value_uses: FxHashMap<ValueId, usize>,
}

/// Complete stack calling convention selected for one internal callee.
#[derive(Clone, Debug)]
struct StaticCallAbi {
    /// Recursive activations use only stack arguments, locals, and results.
    recursive_stack: bool,
    /// Argument positions delivered above the return address. Arguments not selected here keep
    /// their static-frame homes, which is the conservative per-word spill fallback.
    stack_args: DenseBitSet<usize>,
    /// Unused parameters whose argument values need no delivery or frame home.
    ignored_args: DenseBitSet<usize>,
    /// How the callee adopts the incoming argument tuple.
    entry: StaticCallEntry,
    /// Complete tuple returned above the preserved caller prefix, when profitable.
    returns: Option<StackReturnPlan>,
}

impl StaticCallAbi {
    fn new(arg_count: usize) -> Self {
        Self {
            stack_args: DenseBitSet::new_empty(arg_count),
            recursive_stack: false,
            ignored_args: DenseBitSet::new_empty(arg_count),
            entry: StaticCallEntry::Stored,
            returns: None,
        }
    }
}

/// Callee-side realization of a [`StaticCallAbi`] entry signature.
#[derive(Clone, Debug, Default)]
enum StaticCallEntry {
    /// Store incoming stack arguments into their ordinary static-frame slots.
    #[default]
    Stored,
    /// Consume every incoming stack argument directly in the entry block.
    Direct(Vec<ValueId>),
    /// Keep a profitable subset resident through the complete callee CFG.
    Resident { values: Vec<ValueId>, layout: GlobalStackPlan },
    /// Consume the first use directly and materialize only values used again.
    Lazy(LazyStackArgPlan),
}

#[derive(Clone, Copy, Debug)]
struct ICallStackEdge {
    caller: FunctionId,
    callee: FunctionId,
    preserved_words: usize,
    argument_words: usize,
}

/// EVM code generator.
pub struct EvmCodegen<'gcx> {
    gcx: Gcx<'gcx>,
    /// The assembler for bytecode generation.
    asm: MemoryCheckedEmitter<'gcx>,
    /// Stack scheduler.
    scheduler: StackScheduler,
    /// Block labels.
    block_labels: FxHashMap<BlockId, Label>,
    /// Function labels for direct internal calls.
    function_labels: FxHashMap<FunctionId, Label>,
    /// Functions whose reachable exits all abort. Calls to these functions
    /// make their containing block cold as well.
    cold_functions: DenseBitSet<FunctionId>,
    /// Functions consisting only of an empty block terminated by `stop`.
    empty_stop_functions: DenseBitSet<FunctionId>,
    /// Cold blocks in the function currently being emitted, including blocks
    /// that only forward control to other cold blocks.
    cold_blocks: DenseBitSet<BlockId>,
    /// Exact per-function spill area sizes, in bytes, recorded after emission.
    function_spill_sizes: FxHashMap<FunctionId, u64>,
    /// Internal-call frame-size constants waiting for exact callee spill sizes.
    pending_frame_size_consts: Vec<(DeferredConst, FunctionId)>,
    /// Per-function entry/exit stack signatures for non-recursive static calls. An absent plan, or
    /// an argument not selected by a plan, uses the existing static-memory convention.
    static_call_abis: FxHashMap<FunctionId, StaticCallAbi>,
    /// Functions whose stack-only argument convention had to materialize a frame fallback during
    /// emission. They stay on the ordinary stack-argument convention on the regenerated runtime.
    disabled_stack_only_functions: DenseBitSet<FunctionId>,
    /// Whether stack-native return tuples may be selected. Cleared when the
    /// whole-program stack proof fails even without preserved prefixes or
    /// stack arguments, falling back to the frame-backed return convention.
    stack_returns_enabled: bool,
    /// Enables the optional caller-prefix convention for this emission. If
    /// post-emission stack validation rejects it, runtime codegen reruns once
    /// with this disabled.
    preserve_caller_stack: bool,
    /// Functions reached from a recursive activation. Their incoming physical
    /// prefix is unbounded, so preserving another caller prefix would change
    /// the recursion limit.
    recursive_stack_functions: DenseBitSet<FunctionId>,
    /// Functions that are themselves members of a recursive call cycle. A
    /// nested activation reuses their static scratch frame only after the
    /// suspended activation's live words have moved to the EVM stack.
    recursive_frame_functions: DenseBitSet<FunctionId>,
    /// Call edges within a recursive static-frame component. The caller state
    /// must survive the entire callee activation because it can re-enter and
    /// overwrite the caller's fixed frame.
    recursive_frame_edges: FxHashSet<(FunctionId, FunctionId)>,
    /// Functions that are recursive or can reach recursion. A preserved
    /// prefix must not be carried into an unbounded descendant.
    recursion_reaching_functions: DenseBitSet<FunctionId>,
    /// High-water mark of the modeled stack above each function's inherited
    /// untracked prefix.
    function_stack_peaks: FxHashMap<FunctionId, usize>,
    /// Runtime internal-call edges and the caller words retained at each site.
    icall_stack_edges: Vec<ICallStackEdge>,
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
    /// Per-external-entry free-memory-pointer constants, resolved after static-frame placement.
    /// Entries that never use dynamic memory omit the initialization entirely.
    runtime_free_memory_consts: FxHashMap<FunctionId, DeferredConst>,
    /// Internal functions reachable from each entry that initializes the free-memory pointer.
    runtime_entry_reachability: FxHashMap<FunctionId, DenseBitSet<FunctionId>>,
    /// Every external body emitted this pass, for sizing the heap floor.
    runtime_entry_funcs: Vec<FunctionId>,
    /// The internal-convention function currently being emitted.
    current_internal_function: Option<FunctionId>,
    /// Copies to insert at block exits (from phi elimination).
    block_copies: FxHashMap<BlockId, Vec<ParallelCopy>>,
    /// Values carried by planned stack-resident edges, keyed by predecessor block.
    stack_phi_sources: FxHashMap<BlockId, Vec<ValueId>>,
    /// Spill stores available on the current block's path at the current
    /// emission point (`None` outside block emission or when no emitted
    /// forward predecessor constrains it). Stores and clobbers in the block
    /// update the set before it propagates to successors.
    spill_available: Option<FxHashSet<ValueId>>,
    /// Multi-return protocol instructions satisfied directly from adopted
    /// stack-return words; the emission loop skips them.
    elided_insts: FxHashSet<InstId>,
    late_gas_operands: FxHashMap<ValueId, LateGasOperand>,
    spill_stores: Vec<SpillStore>,
    spill_loads: Vec<(SpillSlot, ir::BlockId, usize)>,
    early_spill_removals: Vec<(ir::BlockId, std::ops::Range<usize>)>,
    /// Stack-phi plans by function, shared by the resident-argument search and body emission.
    /// A plan depends only on the function, its whole-function liveness, and the module's cold
    /// functions, so one analysis per function serves both.
    stack_phi_plans: FxHashMap<FunctionId, Rc<StackPhiPlan>>,
    function_ir_block_start: usize,
    /// Whole-calldata-forwarding clobbers (`calldatacopy(0, 0, calldatasize())`
    /// in a proxy) whose write reaches the compiler spill area. Values live
    /// across one are kept stack-resident instead of reloaded from the
    /// overwritten slot. Empty for every function without such a forward.
    spill_hazard_insts: FxHashSet<InstId>,
    /// Cross-block values whose spill homes would overlap a forwarding buffer.
    spill_hazard_values: DenseBitSet<ValueId>,
    /// Functions that can share a call's memory with a source-level `msize` observation.
    msize_observed_functions: GrowableBitSet<FunctionId>,
    /// Functions that can overwrite their caller's low-memory frame.
    spill_clobber_functions: GrowableBitSet<FunctionId>,
    /// Functions sharing a call context with assembly that permits arbitrary memory access.
    unrestricted_memory_functions: GrowableBitSet<FunctionId>,
    /// Heap-pointer arguments sufficient to keep a helper's writes out of caller spills.
    spill_clobber_args: FxHashMap<FunctionId, DenseBitSet<ArgIdx>>,
    /// Whether deep forwarding recovery must avoid expanding memory in this function.
    forwarding_scratch_observable: bool,
    /// Leaf helpers whose sole returned word is derived from the free-memory pointer.
    /// Their callers may safely use the result as a dynamic forwarding-buffer base.
    heap_pointer_return_functions: DenseBitSet<FunctionId>,
    /// Whether the current function has canonical cross-block argument layouts.
    global_stack_active: bool,
    /// Calldata words physically identical to arguments in the active global
    /// layout, adopted after their final validation use.
    global_stack_aliases: FxHashMap<ValueId, ValueId>,
    /// Immutable `PUSH<N>` placeholders in the last assembled runtime code.
    runtime_immutable_refs: Vec<ImmutableRef>,
    /// Backend encodings derived from the current module's immutable declarations.
    immutable_encodings: IndexVec<ImmutableId, ImmutableEncoding>,
    /// First constructor-memory word reserved for immutable staging.
    immutable_staging_base: u64,
    /// Deferred absolute base of the copied constructor ABI argument blob.
    constructor_args_base_const: Option<DeferredConst>,
    /// Deferred code offset of the copied constructor ABI argument blob.
    constructor_args_offset_const: Option<DeferredConst>,
    /// Whether we're currently generating constructor code.
    /// When true, arguments load from the copied deployment ABI blob.
    in_constructor: bool,
    /// Shared constructor completion reached by ordinary `stop` terminators.
    constructor_exit: Option<Label>,
    /// Number of constructor parameters (used for CODECOPY offset calculation).
    constructor_param_count: u32,
    /// Whether we're emitting an internal function body.
    in_internal_function: bool,
    /// Whether we're emitting the MIR `entry` function. Its switch
    /// keeps the selector on the physical stack through the case chain and
    /// leaves it inert below the taken arm. This is only sound for `entry`: it
    /// runs once and every arm terminates externally, so the leftover word can
    /// neither accumulate nor disturb an internal return.
    emitting_entry: bool,
    /// Gas-mode switch growth still available in the current deployment artifact.
    switch_gas_code_growth_remaining: usize,
    capture_mir: bool,
    capture_evm_ir: bool,
    capture_debug_info: bool,
}

impl<'gcx> EvmCodegen<'gcx> {
    /// Creates a new EVM code generator.
    #[must_use]
    pub fn new(gcx: Gcx<'gcx>) -> Self {
        let switch_gas_code_growth_remaining = Self::switch_gas_code_growth_limit(gcx);
        Self {
            gcx,
            asm: MemoryCheckedEmitter::new(gcx),
            scheduler: StackScheduler::for_evm_version(gcx.sess.opts.evm_version),
            block_labels: FxHashMap::default(),
            function_labels: FxHashMap::default(),
            cold_functions: DenseBitSet::new_empty(0),
            empty_stop_functions: DenseBitSet::new_empty(0),
            cold_blocks: DenseBitSet::new_empty(0),
            function_spill_sizes: FxHashMap::default(),
            pending_frame_size_consts: Vec::new(),
            static_call_abis: FxHashMap::default(),
            disabled_stack_only_functions: DenseBitSet::new_empty(0),
            stack_returns_enabled: true,
            preserve_caller_stack: false,
            recursive_stack_functions: DenseBitSet::new_empty(0),
            recursive_frame_functions: DenseBitSet::new_empty(0),
            recursive_frame_edges: FxHashSet::default(),
            recursion_reaching_functions: DenseBitSet::new_empty(0),
            function_stack_peaks: FxHashMap::default(),
            icall_stack_edges: Vec::new(),
            runtime_stack_args: false,
            spill_addr_consts: FxHashMap::default(),
            external_spill_addr_consts: FxHashMap::default(),
            restorable_internal_frames: DenseBitSet::new_empty(0),
            static_frame_functions: DenseBitSet::new_empty(0),
            static_frame_addr_consts: FxHashMap::default(),
            pending_static_allocs: FxHashMap::default(),
            runtime_free_memory_consts: FxHashMap::default(),
            runtime_entry_reachability: FxHashMap::default(),
            runtime_entry_funcs: Vec::new(),
            current_internal_function: None,
            block_copies: FxHashMap::default(),
            stack_phi_sources: FxHashMap::default(),
            spill_available: None,
            elided_insts: FxHashSet::default(),
            late_gas_operands: FxHashMap::default(),
            spill_stores: Vec::new(),
            spill_loads: Vec::new(),
            early_spill_removals: Vec::new(),
            stack_phi_plans: FxHashMap::default(),
            function_ir_block_start: 0,
            spill_hazard_insts: FxHashSet::default(),
            spill_hazard_values: DenseBitSet::new_empty(0),
            msize_observed_functions: GrowableBitSet::new_empty(),
            spill_clobber_functions: GrowableBitSet::new_empty(),
            unrestricted_memory_functions: GrowableBitSet::new_empty(),
            spill_clobber_args: FxHashMap::default(),
            forwarding_scratch_observable: false,
            heap_pointer_return_functions: DenseBitSet::new_empty(0),
            global_stack_active: false,
            global_stack_aliases: FxHashMap::default(),
            runtime_immutable_refs: Vec::new(),
            immutable_encodings: IndexVec::new(),
            immutable_staging_base: EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT
                + EvmMemoryLayout::WORD_SIZE,
            constructor_args_base_const: None,
            constructor_args_offset_const: None,
            in_constructor: false,
            constructor_exit: None,
            constructor_param_count: 0,
            in_internal_function: false,
            emitting_entry: false,
            switch_gas_code_growth_remaining,
            capture_mir: false,
            capture_evm_ir: false,
            capture_debug_info: false,
        }
    }

    /// Clears state that belongs to one lowered MIR module.
    fn reset_for_module(&mut self, module: &Module) {
        self.asm.clear();
        self.scheduler.reset();
        self.block_labels.clear();
        self.function_labels.clear();
        self.cold_functions.clear_to(module.functions.len());
        self.empty_stop_functions.clear_to(module.functions.len());
        self.cold_blocks.clear_to(0);
        self.function_spill_sizes.clear();
        self.pending_frame_size_consts.clear();
        self.static_call_abis.clear();
        self.disabled_stack_only_functions.clear_to(module.functions.len());
        self.stack_returns_enabled = true;
        self.preserve_caller_stack = false;
        self.recursive_stack_functions.clear_to(module.functions.len());
        self.recursive_frame_functions.clear_to(module.functions.len());
        self.recursive_frame_edges.clear();
        self.recursion_reaching_functions.clear_to(module.functions.len());
        self.function_stack_peaks.clear();
        self.icall_stack_edges.clear();
        self.runtime_stack_args = false;
        self.spill_addr_consts.clear();
        self.external_spill_addr_consts.clear();
        self.restorable_internal_frames.clear_to(module.functions.len());
        self.static_frame_functions.clear_to(module.functions.len());
        self.static_frame_addr_consts.clear();
        self.pending_static_allocs.clear();
        self.runtime_free_memory_consts.clear();
        self.runtime_entry_reachability.clear();
        self.runtime_entry_funcs.clear();
        self.current_internal_function = None;
        self.block_copies.clear();
        self.stack_phi_sources.clear();
        self.spill_available = None;
        self.elided_insts.clear();
        self.late_gas_operands.clear();
        self.stack_phi_plans.clear();
        self.spill_clobber_functions.clear();
        self.unrestricted_memory_functions.clear();
        self.spill_clobber_args.clear();
        self.spill_hazard_insts.clear();
        self.spill_hazard_values.clear();
        self.heap_pointer_return_functions.clear_to(module.functions.len());
        self.global_stack_active = false;
        self.global_stack_aliases.clear();
        self.runtime_immutable_refs.clear();
        self.immutable_encodings.clear();
        self.immutable_staging_base =
            EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT + EvmMemoryLayout::WORD_SIZE;
        self.constructor_args_base_const = None;
        self.constructor_args_offset_const = None;
        self.in_constructor = false;
        self.constructor_exit = None;
        self.constructor_param_count = 0;
        self.in_internal_function = false;
        self.emitting_entry = false;
        self.reset_switch_gas_code_growth();
    }

    fn reset_switch_gas_code_growth(&mut self) {
        self.switch_gas_code_growth_remaining = Self::switch_gas_code_growth_limit(self.gcx);
    }

    fn switch_gas_code_growth_limit(gcx: Gcx<'_>) -> usize {
        gcx.sess.opts.unstable.switch_max_gas_code_growth.unwrap_or(MAX_GAS_CODE_GROWTH)
    }

    /// Reports MIR constructs the backend cannot emit yet.
    ///
    /// This includes argument-taking fallbacks and logical slices whose
    /// aggregate use slice lowering could not fold.
    ///
    /// Only live instructions — those still in a block — are checked, since the
    /// instruction arena retains folded-away slices the backend never emits.
    #[must_use]
    fn emit_unsupported(&self, module: &Module) -> bool {
        if module
            .functions
            .iter()
            .any(|func| func.attributes.is_fallback && !func.params.is_empty())
        {
            self.gcx
                .dcx()
                .err("codegen does not support `fallback(bytes) returns (bytes)` yet")
                .span(module.name.span)
                .emit();
            return true;
        }

        let mut emitted = false;
        'func: for func in module.functions.iter() {
            for inst_id in func.instructions() {
                let inst = func.inst(inst_id);
                let message = match inst.kind {
                    InstKind::MakeSlice { .. } | InstKind::SlicePtr(_) | InstKind::SliceLen(_) => {
                        "codegen does not support this calldata-slice usage yet"
                    }
                    InstKind::StoreImmutable(..) => {
                        "immutable assignments must be lowered before EVM codegen"
                    }
                    _ => continue,
                };
                let span = inst.metadata.source_span().unwrap_or(module.name.span);
                self.gcx
                    .dcx()
                    .err(message)
                    .span(span)
                    .note(format!("remaining MIR slice is in function `{}`", func.name))
                    .emit();
                emitted = true;
                // One diagnostic per function is enough to explain the bail.
                continue 'func;
            }
        }
        emitted
    }

    /// Controls whether generated artifacts include final EVM IR.
    pub fn set_capture_evm_ir(&mut self, capture: bool) {
        self.capture_evm_ir = capture;
    }

    /// Controls whether generated artifacts include final instruction locations.
    pub fn set_capture_debug_info(&mut self, capture: bool) {
        self.capture_debug_info = capture;
    }

    /// Controls whether modules without an external entry still run the MIR pipeline.
    pub(crate) fn set_capture_mir(&mut self, capture: bool) {
        self.capture_mir = capture;
    }
}

/// The artifact produced by the EVM backend.
#[derive(Clone, Debug, Default)]
pub struct EvmArtifact {
    /// Deployment (init) bytecode that, when run, returns the runtime code.
    pub deployment: Vec<u8>,
    /// Runtime bytecode, i.e. the code stored on-chain.
    pub runtime: Vec<u8>,
    /// Immutable placeholders in the runtime bytecode.
    pub(crate) immutable_references: Vec<ImmutableRef>,
    /// Final deployment-prefix EVM IR immediately before byte emission.
    pub deployment_evm_ir: Option<ir::Module>,
    /// Final runtime EVM IR immediately before byte emission.
    pub runtime_evm_ir: Option<ir::Module>,
    /// Final deployment-prefix instruction locations.
    pub deployment_debug_info: Option<Vec<DebugInstruction>>,
    /// Final runtime instruction locations.
    pub runtime_debug_info: Option<Vec<DebugInstruction>>,
}

impl crate::backend::Backend for EvmCodegen<'_> {
    type Output = EvmArtifact;

    fn lower_module(&mut self, module: &mut Module) -> EvmArtifact {
        self.generate_deployment_artifact(module)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        stack::spills::{SpillColor, SpillLiveRange},
        *,
    };
    use crate::mir::{
        DataRef, FunctionBuilder, Immediate, Instruction, MirType, TypeSize, Value,
        utils as mir_utils,
    };
    use solar_config::{CompileOpts, EvmVersion};
    use solar_interface::{Ident, Session, sym};
    use solar_sema::{Compiler, hir::Visibility};

    #[test]
    fn constructor_memory_regions_do_not_overlap() {
        let mut module = Module::new(Ident::with_dummy_span(sym::Test));
        let mut constructor = Function::new(Ident::with_dummy_span(sym::Test));
        constructor.attributes.is_constructor = true;
        constructor.internal_frame_size = 0x3000;
        module.add_function(constructor);
        let id = module.add_immutable(
            Ident::with_dummy_span(sym::x),
            MirType::UInt(TypeSize::new_int_bits(8)),
            None,
        );
        let staging_base = immutable_staging_base(&module);
        assert_eq!(staging_base, 0x3080);
        let runtime_len = staging_base as usize;

        let full_word = ImmutableRef {
            id,
            code_offset: runtime_len - 33,
            type_size: TypeSize::new_int_bits(256),
        };
        assert_eq!(EvmCodegen::runtime_copy_base(&module, runtime_len, &[full_word]), 0);

        let short =
            ImmutableRef { id, code_offset: runtime_len - 2, type_size: TypeSize::new_int_bits(8) };
        assert_eq!(EvmCodegen::runtime_copy_base(&module, runtime_len, &[short]), 0);

        let short = ImmutableRef {
            id,
            code_offset: runtime_len - 3,
            type_size: TypeSize::new_int_bits(16),
        };
        assert_eq!(
            EvmCodegen::runtime_copy_base(&module, runtime_len, &[short]),
            immutable_staging_end(staging_base, 1)
        );

        with_codegen(CompileOpts::default(), |mut codegen| {
            codegen.immutable_staging_base = staging_base;
            assert_eq!(codegen.constructor_spill_base(0), staging_base);
            assert_eq!(codegen.constructor_spill_base(1), immutable_staging_end(staging_base, 1));
            assert_eq!(
                codegen.constructor_fixed_memory_end(257, 0),
                immutable_staging_end(staging_base, 257)
            );
            assert_eq!(
                codegen.constructor_fixed_memory_end(1, 0x2000),
                immutable_staging_end(staging_base, 1) + 0x2000
            );
        });
    }

    fn with_codegen<T: Send>(opts: CompileOpts, f: impl FnOnce(EvmCodegen<'_>) -> T + Send) -> T {
        let compiler = Compiler::new(Session::builder().opts(opts).build());
        compiler.enter(|c| f(EvmCodegen::new(c.gcx())))
    }

    #[test]
    fn codegen_reuses_module_state() {
        with_codegen(CompileOpts::default(), |mut codegen| {
            let mut module = Module::new(Ident::DUMMY);
            let mut entry = Function::new(Ident::DUMMY);
            FunctionBuilder::new(&mut entry).stop();
            let entry = module.add_function(entry);
            module.set_dispatch_entry(entry);
            module.advance_phase(MirPhase::EvmShaped);

            let mut first_module = module.clone();
            let first = codegen.generate_deployment_bytecode(&mut first_module);
            let mut second_module = module.clone();
            let second = codegen.generate_deployment_bytecode(&mut second_module);

            assert_eq!(second, first);
        });
    }

    #[test]
    fn static_frames_reject_explicit_signature_addresses() {
        let make_function = |offset| {
            let mut function = Function::new(Ident::DUMMY);
            function.alloc_param(MirType::uint256());
            function.internal_frame_size = EvmMemoryLayout::WORD_SIZE;
            let (inst, _) = function.alloc_value_inst(Instruction::new(
                InstKind::InternalFrameAddr(offset),
                Some(MirType::MemPtr),
            ));
            function.blocks[BlockId::ENTRY].instructions.push(inst);
            function
        };

        assert!(!EvmCodegen::static_frame_offsets_are_local(&make_function(0)));
        assert!(!EvmCodegen::static_frame_offsets_are_local(&make_function(
            EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
        )));
        assert!(EvmCodegen::static_frame_offsets_are_local(&make_function(
            EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE + EvmMemoryLayout::WORD_SIZE
        )));
        assert!(!EvmCodegen::static_frame_offsets_are_local(&make_function(u64::MAX)));
    }

    #[test]
    fn data_copy_reaches_destination_before_relocation_push() {
        with_codegen(CompileOpts::default(), |mut codegen| {
            let mut module = Module::new(Ident::DUMMY);
            module.phase = MirPhase::EvmShaped;
            let data = module.add_data(vec![0; WORD_BYTES].into(), None);

            let mut function = Function::new(Ident::DUMMY);
            let mut builder = FunctionBuilder::new(&mut function);
            let one = builder.imm(1);
            let dest = builder.add(one, one);
            let size = builder.imm(WORD_BYTES as u64);
            builder.data_copy(DataRef::new(data, 0), dest, size);
            builder.stop();
            let function = module.add_function(function);

            codegen.asm.load_data(&module);
            let function = &module.functions[function];
            let liveness = Liveness::compute(function);
            codegen.scheduler.stack.push(dest);
            for _ in 0..MAX_STACK_ACCESS - 2 {
                codegen.scheduler.stack.push_unknown();
            }
            assert_eq!(codegen.scheduler.stack.find(dest), Some(MAX_STACK_ACCESS - 2));

            codegen.emit_data_copy(
                function,
                DataRef::new(data, 0),
                dest,
                size,
                &liveness,
                BlockId::ENTRY,
                1,
            );

            assert_eq!(codegen.scheduler.stack.find(dest), Some(MAX_STACK_ACCESS - 2));
        });
    }

    #[test]
    fn data_copy_participates_in_memory_analysis() {
        let data = DataRef::new(crate::mir::DataId::from_usize(0), 0);

        let mut constant = Function::new(Ident::DUMMY);
        let dest = constant.alloc_value(Value::Immediate(Immediate::uint256(U256::from(0x40))));
        let size = constant.alloc_value(Value::Immediate(Immediate::uint256(U256::from(0x20))));
        let inst =
            constant.alloc_inst(Instruction::new(InstKind::DataCopy(data, dest, size), None));
        constant.blocks[BlockId::ENTRY].instructions.push(inst);
        assert_eq!(EvmCodegen::constant_memory_high_water_mark(&constant), 0x60);
        assert!(EvmCodegen::function_may_observe_free_memory_slot(&constant));
        assert!(mir_utils::is_memory_inst(&constant.inst(inst).kind));

        let mut dynamic = Function::new(Ident::DUMMY);
        let dest = dynamic.alloc_param(MirType::MemPtr);
        let size = dynamic.alloc_param(MirType::uint256());
        let inst = dynamic.alloc_inst(Instruction::new(InstKind::DataCopy(data, dest, size), None));
        dynamic.blocks[BlockId::ENTRY].instructions.push(inst);
        assert_eq!(EvmCodegen::dynamic_spill_write_dest(&dynamic, inst), Some(dest));
    }

    #[test]
    fn caller_stack_prefix_validation_rejects_overflow() {
        with_codegen(CompileOpts::default(), |mut codegen| {
            let mut module = Module::new(Ident::DUMMY);
            let entry = module.add_function(Function::new(Ident::with_dummy_span(sym::entry)));
            module.set_dispatch_entry(entry);
            let callee = module.add_function(Function::new(Ident::with_dummy_span(sym::Test)));
            let mut constructor = Function::new(Ident::DUMMY);
            constructor.attributes.is_constructor = true;
            let constructor = module.add_function(constructor);

            codegen.recursive_stack_functions = DenseBitSet::new_empty(module.functions.len());
            codegen.function_stack_peaks.insert(entry, 1);
            codegen.function_stack_peaks.insert(callee, MAX_STACK_DEPTH - 1);
            codegen.icall_stack_edges.push(ICallStackEdge {
                caller: entry,
                callee,
                preserved_words: 1,
                argument_words: 0,
            });
            assert!(!codegen.caller_stack_prefixes_fit(&module, MAX_STACK_DEPTH));

            codegen.function_stack_peaks.insert(callee, MAX_STACK_DEPTH - 2);
            assert!(codegen.caller_stack_prefixes_fit(&module, MAX_STACK_DEPTH));

            // The transient argument tuple and target label must be budgeted even
            // when the preserved prefix and callee peak fit on their own.
            codegen.icall_stack_edges[0].preserved_words = MAX_STACK_DEPTH - 3;
            codegen.icall_stack_edges[0].argument_words = 2;
            codegen.function_stack_peaks.insert(callee, 2);
            assert!(!codegen.caller_stack_prefixes_fit(&module, MAX_STACK_DEPTH));

            codegen.icall_stack_edges[0].preserved_words = 0;
            codegen.icall_stack_edges[0].argument_words = MAX_STACK_DEPTH;
            codegen.function_stack_peaks.insert(callee, 0);
            assert!(!codegen.caller_stack_prefixes_fit(&module, MAX_STACK_DEPTH));

            codegen.icall_stack_edges[0] = ICallStackEdge {
                caller: constructor,
                callee,
                preserved_words: 1,
                argument_words: 0,
            };
            codegen.function_stack_peaks.insert(callee, MAX_STACK_DEPTH - 1);
            assert!(!codegen.stack_prefixes_fit_from(&module, constructor, MAX_STACK_DEPTH));
        });
    }

    #[test]
    fn label_push_extends_the_scheduler_peak() {
        with_codegen(CompileOpts::default(), |mut codegen| {
            for _ in 0..MAX_STACK_DEPTH {
                codegen.scheduler.stack.push_unknown();
            }

            let label = codegen.asm.new_label();
            codegen.emit_push_label(label);

            assert_eq!(codegen.scheduler.stack.max_depth(), MAX_STACK_DEPTH + 1);
        });
    }

    #[test]
    fn removing_instructions_keeps_label_relocations() {
        with_codegen(CompileOpts::default(), |mut codegen| {
            let label = codegen.asm.new_label();
            let (block, start) = codegen.asm.next_instruction_position();
            codegen.asm.emit_op(op::ADD);
            codegen.emit_push_label(label);
            codegen.asm.remove_instructions(&mut [(block, start..start + 1)]);
            codegen.asm.define_label(label);

            let (module, _) = codegen.asm.finish_evm_ir().unwrap();
            assert_eq!(
                module.blocks[ir::BlockId::ENTRY].instructions[0].pushed_block(),
                Some(ir::BlockId::from_usize(1))
            );
        });
    }

    #[test]
    fn irreducible_runtime_stack_overflow_terminates() {
        with_codegen(CompileOpts::default(), |mut codegen| {
            let mut module = Module::new(Ident::DUMMY);
            for index in 0..=MAX_STACK_DEPTH {
                let mut function = Function::new(Ident::DUMMY);
                let mut builder = FunctionBuilder::new(&mut function);
                if index < MAX_STACK_DEPTH {
                    builder.icall_void(FunctionId::from_usize(index + 1), Vec::new(), 0);
                }
                builder.stop();
                let function = module.add_function(function);
                if index == 0 {
                    module.set_dispatch_entry(function);
                }
            }
            module.advance_phase(MirPhase::EvmShaped);
            let call_graph = CallGraphInfo::new(&module);
            codegen.cold_functions = DenseBitSet::new_empty(module.functions.len());

            let _ = codegen.generate_runtime_code(&module, &call_graph);

            assert!(!codegen.stack_returns_enabled);
            assert!(codegen.gcx.dcx().has_errors().is_err());
        });
    }

    #[test]
    fn dynamic_frame_stack_args_allow_raw_values() {
        let mut function = Function::new(Ident::DUMMY);
        let argument = function.alloc_param(MirType::uint256());
        let immediate = function.alloc_value(Value::Immediate(Immediate::uint256(U256::from(1))));
        let (_, computed) = function.alloc_value_inst(Instruction::new(
            InstKind::Add(argument, immediate),
            Some(MirType::uint256()),
        ));
        let (_, calldata_size) = function
            .alloc_value_inst(Instruction::new(InstKind::CalldataSize, Some(MirType::uint256())));

        assert!(EvmCodegen::stack_arg_site_eligible(&function, false, immediate));
        assert!(!EvmCodegen::stack_arg_site_eligible(&function, false, argument));
        assert!(EvmCodegen::stack_arg_site_eligible(&function, false, computed));
        assert!(EvmCodegen::raw_arg_emittable(&function, false, calldata_size));
        assert!(EvmCodegen::stack_arg_site_eligible(&function, true, argument));
        assert!(EvmCodegen::stack_arg_site_eligible(&function, true, computed));

        with_codegen(CompileOpts::default(), |mut codegen| {
            codegen.emit_raw_stack_arg(&function, calldata_size, None, None, 0);
            assert_eq!(codegen.asm.assemble().bytecode, [op::CALLDATASIZE]);
        });
    }

    #[test]
    fn spill_elision_requires_uniform_successor_residency() {
        let mut function = Function::new(Ident::DUMMY);
        let condition = function.alloc_value(Value::Immediate(Immediate::bool(true)));
        let first = function.alloc_value(Value::Immediate(Immediate::uint256(U256::from(1))));
        let second = function.alloc_value(Value::Immediate(Immediate::uint256(U256::from(2))));
        let then_block = function.alloc_block();
        let else_block = function.alloc_block();
        let term = Terminator::Branch { condition, then_block, else_block };
        let mut plan = GlobalStackPlan {
            entries: FxHashMap::from_iter([
                (then_block, vec![first, second]),
                (else_block, vec![first]),
            ]),
            aliases: FxHashMap::default(),
            terminal_sensitive: true,
            layout_limit: MAX_STACK_ACCESS,
        };

        assert_eq!(plan.uniformly_carried_values(&function, &term), [first]);
        plan.entries.insert(else_block, vec![first, second]);
        assert_eq!(plan.uniformly_carried_values(&function, &term), [first, second]);

        // Switch layouts intersect across the default and every case target.
        let case_block = function.alloc_block();
        let switch = Terminator::Switch {
            value: condition,
            default: else_block,
            cases: vec![(condition, case_block)],
        };
        plan.entries.insert(case_block, vec![second]);
        assert_eq!(plan.uniformly_carried_values(&function, &switch), [second]);
        plan.entries.insert(case_block, vec![first, second]);
        assert_eq!(plan.uniformly_carried_values(&function, &switch), [first, second]);
    }

    #[test]
    fn icall_headroom_includes_return_label() {
        let value = ValueId::from_usize(0);
        let call = InstKind::ICall {
            function: FunctionId::from_usize(0),
            args: vec![value; MAX_STACK_ACCESS].into(),
            returns: 0,
        };
        assert_eq!(
            EvmCodegen::instruction_transient_growth(&call, MAX_STACK_ACCESS),
            MAX_STACK_ACCESS
        );

        let add = InstKind::Add(value, value);
        assert_eq!(EvmCodegen::instruction_transient_growth(&add, 2), 1);
    }

    #[test]
    fn one_operand_terminators_check_stack_arg_reach() {
        let value = ValueId::from_usize(0);
        let branch = Terminator::Branch {
            condition: value,
            then_block: BlockId::from_usize(1),
            else_block: BlockId::from_usize(2),
        };
        assert_eq!(EvmCodegen::terminator_transient_growth(&branch), 1);

        let return_value = Terminator::Return { values: smallvec::smallvec![value] };
        assert_eq!(EvmCodegen::terminator_transient_growth(&return_value), 1);
        assert_eq!(EvmCodegen::terminator_transient_growth(&Terminator::Stop), 0);
    }

    #[test]
    fn resident_phi_merge_rejects_inaccessible_layout() {
        let mut function = Function::new(Ident::DUMMY);
        let join = function.alloc_block();
        let mut phi = StackPhiPlan::default();
        phi.entries.insert(join, (0..MAX_STACK_ACCESS).map(ValueId::from_usize).collect());
        let resident = GlobalStackPlan {
            entries: FxHashMap::from_iter([(join, vec![ValueId::from_usize(MAX_STACK_ACCESS)])]),
            aliases: FxHashMap::default(),
            terminal_sensitive: true,
            layout_limit: MAX_STACK_ACCESS,
        };

        assert!(!phi.merge_resident(&function, &resident));
        assert_eq!(phi.entries[&join].len(), MAX_STACK_ACCESS);
    }

    #[test]
    fn materialized_stack_only_args_use_frame_on_retry() {
        let opts = CompileOpts { optimization: OptimizationMode::Gas, ..Default::default() };
        with_codegen(opts, |mut codegen| {
            let mut module = Module::new(Ident::DUMMY);
            let mut function = Function::new(Ident::DUMMY);
            let argument = function.alloc_param(MirType::uint256());
            let function = module.add_function(function);

            codegen.static_call_abi_mut(function, 1).stack_args.insert(0);
            codegen.static_frame_functions = DenseBitSet::new_empty(module.functions.len());
            codegen.static_frame_functions.insert(function);
            codegen.disabled_stack_only_functions = DenseBitSet::new_empty(module.functions.len());
            codegen.disabled_stack_only_functions.insert(function);

            let arg_values = FxHashMap::from_iter([(
                function,
                CanonicalArgValues::from_vec(vec![Some(argument)]),
            )]);
            let use_info = FxHashMap::from_iter([(
                function,
                StackArgUseInfo {
                    use_counts: FxHashMap::from_iter([(argument, 1)]),
                    non_entry_uses: DenseBitSet::new_empty(1),
                    call_uses: DenseBitSet::new_empty(1),
                    entry_first_uses: FxHashMap::from_iter([(argument, 0)]),
                    first_entry_call: None,
                },
            )]);

            codegen.compute_lazy_stack_args(&module, &arg_values, &use_info);
            codegen.compute_direct_stack_args(&module, &arg_values, &use_info);
            assert!(codegen.lazy_stack_args(function).is_none());
            assert!(codegen.direct_stack_args(function).is_none());
        });
    }

    #[test]
    fn stack_return_compacts_offsets_after_stack_arg_fallback() {
        let opts = CompileOpts { optimization: OptimizationMode::Gas, ..Default::default() };
        with_codegen(opts, |mut codegen| {
            let mut module = Module::new(Ident::DUMMY);
            let mut function = Function::new(Ident::with_dummy_span(sym::Test));
            function.internal_frame_size = EvmMemoryLayout::WORD_SIZE;
            let mut builder = FunctionBuilder::new(&mut function);
            let argument = builder.add_param(MirType::uint256());
            builder.add_return(MirType::uint256());
            builder.ret([argument]);
            let function = module.add_function(function);

            codegen.static_frame_functions = DenseBitSet::new_empty(module.functions.len());
            codegen.static_frame_functions.insert(function);
            codegen.disabled_stack_only_functions = DenseBitSet::new_empty(module.functions.len());
            codegen.recursive_frame_functions = DenseBitSet::new_empty(module.functions.len());
            codegen.function_spill_sizes.insert(function, 0);
            codegen.runtime_stack_args = false;
            codegen.compute_stack_return_plans(&module);

            let local =
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE + 2 * EvmMemoryLayout::WORD_SIZE;
            assert!(codegen.stack_return_plan(function).is_some());
            assert_eq!(
                codegen.compact_static_frame_offset(function, local),
                local - EvmMemoryLayout::WORD_SIZE
            );
            assert_eq!(codegen.emitted_frame_size(&module, function), local);
        });
    }

    #[test]
    fn direct_stack_args_reject_switch_terminators() {
        let opts = CompileOpts { optimization: OptimizationMode::Gas, ..Default::default() };
        with_codegen(opts, |mut codegen| {
            let mut module = Module::new(Ident::DUMMY);
            let mut function = Function::new(Ident::with_dummy_span(sym::Test));
            let mut builder = FunctionBuilder::new(&mut function);
            let argument = builder.add_param(MirType::uint256());
            let one = builder.imm(1);
            let _unrelated = builder.add(one, one);
            let _use = builder.add(argument, one);
            let default = builder.create_block();
            let case = builder.create_block();
            builder.switch(argument, default, vec![(one, case)]);
            builder.switch_to_block(default);
            builder.stop();
            builder.switch_to_block(case);
            builder.stop();
            let function = module.add_function(function);

            codegen.static_call_abi_mut(function, 1).stack_args.insert(0);
            codegen.disabled_stack_only_functions = DenseBitSet::new_empty(module.functions.len());
            let arg_values = codegen.collect_canonical_stack_arg_values(&module);
            let use_info = codegen.collect_stack_arg_uses(&module);
            codegen.compute_direct_stack_args(&module, &arg_values, &use_info);

            assert!(codegen.direct_stack_args(function).is_none());
        });
    }

    #[test]
    fn resident_layout_selection_is_pinned_across_runs() {
        // `select_resident_layout` weighs runtime gas against deploy bytes
        // through `optimizer_runs`. No current cost shape flips the choice
        // (an eligible stack-riding value dominates the frame convention in
        // both dimensions), so this pins the selection at both extremes:
        // a cost-model change that silently alters layout choices, or makes
        // them run-count-unstable, must show up here as an intentional edit.
        let select = |runs: u64| {
            let opts = CompileOpts {
                optimization: OptimizationMode::Gas,
                optimizer_runs: Some(runs),
                ..Default::default()
            };
            with_codegen(opts, |codegen| {
                let mut function = Function::new(Ident::DUMMY);
                let argument = function.alloc_param(MirType::uint256());
                let mut builder = FunctionBuilder::new(&mut function);
                let one = builder.imm(1);
                let blocks: Vec<_> = (0..5).map(|_| builder.create_block()).collect();
                builder.jump(blocks[0]);
                for (index, &block) in blocks.iter().enumerate() {
                    builder.switch_to_block(block);
                    if let Some(&next) = blocks.get(index + 1) {
                        builder.jump(next);
                    }
                }
                let acc = builder.add(argument, one);
                builder.ret([acc]);
                let liveness = Liveness::compute(&function);
                codegen
                    .select_resident_layout(&function, &liveness, &[argument], false, None)
                    .map(|(values, _)| values)
            })
        };

        let deploy_dominated = select(1);
        let runtime_dominated = select(200_000);
        assert_eq!(deploy_dominated, select(1));
        assert_eq!(runtime_dominated, select(200_000));
        assert_eq!(deploy_dominated.as_deref(), Some(&[ValueId::from_usize(0)][..]));
        assert_eq!(runtime_dominated.as_deref(), Some(&[ValueId::from_usize(0)][..]));
    }

    #[test]
    fn free_memory_slot_overlap_is_conservative() {
        let overlaps = EvmCodegen::constant_memory_range_may_overlap_fmp;

        assert!(!overlaps(Some(0x20), Some(0x20)));
        assert!(!overlaps(Some(0x3f), Some(1)));
        assert!(overlaps(Some(0x3f), Some(2)));
        assert!(overlaps(Some(0x40), Some(0x20)));
        assert!(overlaps(Some(0x5f), Some(1)));
        assert!(!overlaps(Some(0x60), None));
        assert!(!overlaps(None, Some(0)));
        assert!(overlaps(None, Some(1)));
        assert!(overlaps(Some(0), None));
        assert!(overlaps(Some(0x20), Some(u64::MAX)));
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
    fn nullary_reads_have_expected_rematerialization_opcodes() {
        let mut function = Function::new(Ident::with_dummy_span(sym::Test));
        for (kind, expected_op) in [
            (InstKind::CalldataSize, op::CALLDATASIZE),
            (InstKind::CodeSize, op::CODESIZE),
            (InstKind::Caller, op::CALLER),
            (InstKind::CallValue, op::CALLVALUE),
            (InstKind::Address, op::ADDRESS),
            (InstKind::Origin, op::ORIGIN),
            (InstKind::GasPrice, op::GASPRICE),
            (InstKind::Coinbase, op::COINBASE),
            (InstKind::Timestamp, op::TIMESTAMP),
            (InstKind::BlockNumber, op::NUMBER),
            (InstKind::PrevRandao, op::PREVRANDAO),
            (InstKind::GasLimit, op::GASLIMIT),
            (InstKind::SlotNum, op::SLOTNUM),
            (InstKind::ChainId, op::CHAINID),
            (InstKind::BaseFee, op::BASEFEE),
            (InstKind::BlobBaseFee, op::BLOBBASEFEE),
        ] {
            let (_, value) =
                function.alloc_value_inst(Instruction::new(kind, Some(MirType::uint256())));
            assert_eq!(EvmCodegen::always_rematerializable_op(&function, value), Some(expected_op));
            assert!(!EvmCodegen::can_own_spill_slot(&function, value));
        }

        for kind in
            [InstKind::MSize, InstKind::ReturnDataSize, InstKind::SelfBalance, InstKind::Gas]
        {
            let (_, value) =
                function.alloc_value_inst(Instruction::new(kind, Some(MirType::uint256())));
            assert_eq!(EvmCodegen::always_rematerializable_op(&function, value), None);
            assert!(EvmCodegen::can_own_spill_slot(&function, value));
        }
    }

    #[test]
    fn deep_spill_exposes_one_word_at_each_target_limit() {
        for evm_version in [EvmVersion::Osaka, EvmVersion::Amsterdam] {
            let opts = CompileOpts { evm_version, ..Default::default() };
            with_codegen(opts, |mut codegen| {
                let mut function = Function::new(Ident::with_dummy_span(sym::Test));
                let lhs = function.alloc_value(Value::Immediate(Immediate::uint256(U256::ZERO)));
                let rhs = function.alloc_value(Value::Immediate(Immediate::uint256(U256::ONE)));
                let (_, target) = function.alloc_value_inst(Instruction::new(
                    InstKind::Add(lhs, rhs),
                    Some(MirType::uint256()),
                ));
                codegen.scheduler.stack.push(target);
                for _ in 0..evm_version.reachable_stack_depth() {
                    let (_, filler) = function.alloc_value_inst(Instruction::new(
                        InstKind::Add(lhs, rhs),
                        Some(MirType::uint256()),
                    ));
                    codegen.scheduler.stack.push(filler);
                }
                let before = codegen.scheduler.stack.as_slice().to_vec();

                codegen.spill_value_if_needed(&function, target);

                assert_eq!(codegen.scheduler.stack.as_slice(), before);
                assert!(codegen.scheduler.spills.is_stored(target));
                assert_eq!(codegen.scheduler.spills.spill_area_size(), 64);
            });
        }
    }

    #[test]
    fn unreachable_phi_copies_do_not_leak_between_functions() {
        with_codegen(CompileOpts::default(), |mut codegen| {
            let mut first = Function::new(Ident::with_dummy_span(sym::Test));
            let mut builder = FunctionBuilder::new(&mut first);
            let unreachable_pred = builder.create_block();
            let unreachable_merge = builder.create_block();
            builder.stop();
            builder.switch_to_block(unreachable_pred);
            let value = builder.imm(1);
            builder.jump(unreachable_merge);
            builder.switch_to_block(unreachable_merge);
            let value = builder.phi(vec![(unreachable_pred, value)]);
            builder.ret([value]);

            codegen.generate_function_body(FunctionId::from_usize(0), &first);
            assert!(codegen.block_copies.contains_key(&unreachable_pred));

            let mut second = Function::new(Ident::with_dummy_span(sym::Test));
            FunctionBuilder::new(&mut second).stop();
            codegen.generate_function_body(FunctionId::from_usize(1), &second);

            assert!(codegen.block_copies.is_empty());
        });
    }

    #[test]
    fn cross_block_reload_excludes_phi_edge_uses() {
        let mut function = Function::new(Ident::DUMMY);
        let immediate = function.alloc_value(Value::Immediate(Immediate::uint256(U256::from(1))));
        let (edge_inst, edge_value) = function.alloc_value_inst(Instruction::new(
            InstKind::Add(immediate, immediate),
            Some(MirType::uint256()),
        ));
        let (direct_inst, direct_value) = function.alloc_value_inst(Instruction::new(
            InstKind::Mul(immediate, immediate),
            Some(MirType::uint256()),
        ));
        function.blocks[BlockId::ENTRY].instructions.extend([edge_inst, direct_inst]);

        let phi_block = function.alloc_block();
        let (phi_inst, _) = function.alloc_value_inst(Instruction::new(
            InstKind::Phi(vec![(BlockId::ENTRY, edge_value)]),
            Some(MirType::uint256()),
        ));
        function.blocks[phi_block].instructions.push(phi_inst);

        let direct_block = function.alloc_block();
        let (use_inst, _) = function.alloc_value_inst(Instruction::new(
            InstKind::Add(direct_value, immediate),
            Some(MirType::uint256()),
        ));
        function.blocks[direct_block].instructions.push(use_inst);

        let reloaded = EvmCodegen::cross_block_reload_values(&function);
        assert!(!reloaded.contains(edge_value));
        assert!(reloaded.contains(direct_value));
    }

    #[test]
    fn spill_color_accepts_only_disjoint_ranges() {
        let block0 = BlockId::from_usize(0);
        let block1 = BlockId::from_usize(1);
        let value0 = ValueId::from_usize(0);
        let value1 = ValueId::from_usize(1);
        let interferences = FxHashMap::default();
        let mut color = SpillColor::new(2);
        color
            .insert(value0, &FxHashMap::from_iter([(block0, SpillLiveRange { start: 2, end: 4 })]));

        assert!(color.accepts(
            value1,
            &FxHashMap::from_iter([(block0, SpillLiveRange { start: 5, end: 7 })]),
            &interferences,
        ));
        assert!(!color.accepts(
            value1,
            &FxHashMap::from_iter([(block0, SpillLiveRange { start: 4, end: 7 })]),
            &interferences,
        ));
        assert!(color.accepts(
            value1,
            &FxHashMap::from_iter([(block1, SpillLiveRange { start: 2, end: 4 })]),
            &interferences,
        ));
    }

    #[test]
    fn parallel_phi_interference_follows_copy_order() {
        let source0 = ValueId::from_usize(0);
        let destination0 = ValueId::from_usize(1);
        let source1 = ValueId::from_usize(2);
        let destination1 = ValueId::from_usize(3);
        let mut colorable = DenseBitSet::new_empty(4);
        colorable.insert_all();
        let block_copies = FxHashMap::from_iter([(
            BlockId::ENTRY,
            vec![
                ParallelCopy {
                    src: CopySource::Value(source0),
                    dst: CopyDest::Value(destination0),
                    ty: MirType::uint256(),
                },
                ParallelCopy {
                    src: CopySource::Value(source1),
                    dst: CopyDest::Value(destination1),
                    ty: MirType::uint256(),
                },
            ],
        )]);

        let function = Function::new(Ident::DUMMY);
        let liveness = Liveness::compute(&function);
        let interferences =
            EvmCodegen::parallel_phi_interferences(&function, &liveness, &colorable, &block_copies);
        assert!(interferences[&destination0].contains(&destination1));
        assert!(interferences[&destination0].contains(&source1));
        assert!(!interferences[&destination0].contains(&source0));
        assert!(!interferences[&destination1].contains(&source0));
    }
}

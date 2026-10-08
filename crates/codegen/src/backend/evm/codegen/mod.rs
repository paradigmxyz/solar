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
    stack::{
        OperandCostModel, OperandPlan, ScheduleCost, ScheduledOp, SpillSlot, StackScheduler,
        TargetSlot, cross_block_values, is_cross_block_recomputable_kind, is_rematerializable_leaf,
        layout::{
            GlobalStackPlan, StackPhiBranch, StackPhiEdge, StackPhiPlan, planned_entry_carries,
        },
        rematerializable_nullary_value,
    },
    switch::MAX_GAS_CODE_GROWTH,
};
use super::{
    DebugFunction, DebugFunctionExit, DebugInfo, ir,
    layout::{RelayoutAddress, preserves_push_width},
    op::{self, WORD_BYTES},
};
use crate::{
    backend::assembler::{
        ArtifactKind, Assembler, DeferredAlloc, DeferredConst, ImmutableRef, Label,
    },
    link::{EmbeddedBytecodes, LibraryRelocation, LibraryTable},
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
use std::{cell::OnceCell, sync::Arc};

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
mod planning;
mod runtime;
pub(crate) mod select;
mod terminator;
mod values;

const STACK_PHI_LAYOUT_LIMIT: usize = 8;
const GLOBAL_STACK_LAYOUT_LIMIT: usize = 8;

#[derive(Default)]
struct GeneratedCode {
    bytecode: Vec<u8>,
    library_relocations: Vec<LibraryRelocation>,
    evm_ir: Option<ir::Module>,
    debug_info: Option<DebugInfo>,
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

/// Stack-native exit signature for a non-recursive static callee.
#[derive(Clone, Copy, Debug)]
struct StackReturnPlan {
    /// Number of result words left on the physical stack.
    arity: usize,
    /// First local/spill byte in the original MIR frame layout.
    local_base: u64,
}

/// Caller-side binding of stack-returned tuple words to their multi-return
/// protocol loads.
struct StackResultProjection {
    /// The buffer-pointer read, its offset additions, and the extra-return
    /// loads; all skipped during emission.
    elided: Vec<InstId>,
    /// The adopted load result for each extra return index `1..arity`.
    extras: Vec<ValueId>,
}

/// Subset-invariant analyses shared by one resident-layout subset search.
struct ResidentSearchContext {
    /// Planned stack-phi edges, present when the function has phis.
    phi_plan: Option<Arc<StackPhiPlan>>,
    /// CFG facts whose memoized dominators persist across candidates.
    cfg: CfgInfo,
    /// Operand occurrences per candidate value across the whole function.
    value_uses: FxHashMap<ValueId, usize>,
    /// Arguments live where computed values exhaust direct stack access.
    frame_required: DenseBitSet<ValueId>,
}

/// Complete stack calling convention selected for one non-recursive static callee.
#[derive(Clone, Debug)]
struct StaticCallAbi {
    /// Argument positions delivered above the return address. Arguments not selected here keep
    /// their static-frame homes, which is the conservative per-word spill fallback.
    stack_args: DenseBitSet<usize>,
    /// How the callee adopts the incoming argument tuple.
    entry: StaticCallEntry,
    /// Complete tuple returned above the preserved caller prefix, when profitable.
    returns: Option<StackReturnPlan>,
}

impl StaticCallAbi {
    fn new(arg_count: usize) -> Self {
        Self {
            stack_args: DenseBitSet::new_empty(arg_count),
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
    asm: Assembler<'gcx>,
    /// Stack scheduler.
    scheduler: StackScheduler,
    /// Block labels.
    block_labels: FxHashMap<BlockId, Label>,
    /// Function labels for direct internal calls.
    function_labels: FxHashMap<FunctionId, Label>,
    /// Return arities inferred from the final lowered function signatures.
    function_return_counts: IndexVec<FunctionId, usize>,
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
    /// Final packed sizes of scalar static frames after unused references are deleted.
    packed_static_frame_sizes: FxHashMap<FunctionId, u64>,
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
    stack_phi_plans: FxHashMap<FunctionId, Arc<StackPhiPlan>>,
    /// Whole-function liveness by function, shared the same way as `stack_phi_plans`.
    function_liveness: FxHashMap<FunctionId, Arc<Liveness>>,
    function_ir_block_start: usize,
    /// Whole-calldata-forwarding clobbers (`calldatacopy(0, 0, calldatasize())`
    /// in a proxy) whose write reaches the compiler spill area. Values live
    /// across one are kept stack-resident instead of reloaded from the
    /// overwritten slot. Empty for every function without such a forward.
    spill_hazard_insts: FxHashSet<InstId>,
    /// Leaf helpers whose sole returned word is derived from the free-memory pointer.
    /// Their callers may safely use the result as a dynamic forwarding-buffer base.
    heap_pointer_return_functions: DenseBitSet<FunctionId>,
    /// Runtime code of a scheduled module, waiting for embedded bytecode to be linked in.
    pending_runtime: Option<PendingRuntime>,
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
    /// Shared constructor completion reached by ordinary empty returns.
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
            asm: Assembler::new(gcx),
            scheduler: StackScheduler::for_evm_version(gcx.sess.opts.evm_version)
                .with_wide_permutation_search(gcx.sess.opts.optimization.is_gas()),
            block_labels: FxHashMap::default(),
            function_labels: FxHashMap::default(),
            function_return_counts: IndexVec::new(),
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
            packed_static_frame_sizes: FxHashMap::default(),
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
            function_liveness: FxHashMap::default(),
            function_ir_block_start: 0,
            spill_hazard_insts: FxHashSet::default(),
            heap_pointer_return_functions: DenseBitSet::new_empty(0),
            pending_runtime: None,
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
        self.packed_static_frame_sizes.clear();
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
        self.function_liveness.clear();
        self.spill_hazard_insts.clear();
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

/// Runtime code whose EVM IR pipeline has run in the assembler, waiting for embedded bytecode.
struct PendingRuntime {
    call_graph: CallGraphInfo,
}

/// The artifact produced by the EVM backend.
#[derive(Clone, Debug, Default)]
pub struct EvmArtifact {
    /// Library identities referenced by this artifact.
    pub libraries: crate::link::LibraryTable,
    /// Deployment (init) bytecode that, when run, returns the runtime code.
    pub deployment: Vec<u8>,
    /// Runtime bytecode, i.e. the code stored on-chain.
    pub runtime: Vec<u8>,
    /// Library address offsets in the deployment bytecode.
    pub deployment_library_relocations: Vec<LibraryRelocation>,
    /// Library address offsets in the runtime bytecode.
    pub runtime_library_relocations: Vec<LibraryRelocation>,
    /// Immutable placeholders in the runtime bytecode.
    pub(crate) immutable_references: Vec<ImmutableRef>,
    /// Textual input to an optional code generation backend.
    pub backend_ir: Option<String>,
    /// Final deployment-prefix EVM IR immediately before byte emission.
    pub deployment_evm_ir: Option<ir::Module>,
    /// Final runtime EVM IR immediately before byte emission.
    pub runtime_evm_ir: Option<ir::Module>,
    /// Final deployment-prefix instruction locations.
    pub deployment_debug_info: Option<DebugInfo>,
    /// Final runtime instruction locations.
    pub runtime_debug_info: Option<DebugInfo>,
}

impl crate::backend::Backend for EvmCodegen<'_> {
    type Output = EvmArtifact;

    fn lower_module(&mut self, module: &mut Module, bytecodes: &EmbeddedBytecodes) -> EvmArtifact {
        if !self.schedule_module(module) {
            return EvmArtifact::default();
        }
        self.finish_module(module, bytecodes)
    }
}

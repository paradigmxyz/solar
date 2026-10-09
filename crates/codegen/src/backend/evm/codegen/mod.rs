//! EVM bytecode generation from MIR.
//!
//! The runtime, and separately the constructor with the helpers it calls, go
//! through the stack-resident lowering in `stackify`. It plans every function's
//! stack layouts ahead of emission, keeps values and return addresses on the
//! stack, and emits EVM IR, which the assembler then optimizes, relocates, and
//! encodes.
//!
//! Deployment and runtime modules coordinate artifact emission. `select` picks
//! single opcodes, `switch` lowers switches, and `function`, `terminator`, and
//! `values` hold shared emission helpers. Physical memory placement lives in
//! `frames`; the private `stack` subtree holds the symbolic stack and the
//! stack-operation run solver that EVM IR passes use.

use self::switch::MAX_GAS_CODE_GROWTH;
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
        ArgIdx, BlockId, Function, FunctionId, ImmutableEncoding, ImmutableId, InstId, InstKind,
        MemoryRegion, MirPhase, MirType, Module, Terminator, Value, ValueId,
        analysis::{AliasAnalysis, CallGraphInfo, CfgInfo, Liveness, LoopAnalyzer, MemoryBase},
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
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet},
};
use solar_sema::Gcx;

mod stack;
pub(super) use stack::{
    MAX_STACK_DEPTH, StackModel, StackOp, lowered_stack_cost, resynthesize_physical_ops,
};

mod switch;

mod deployment;
mod frames;
mod function;
mod runtime;
pub(crate) mod select;
mod stackify;
mod terminator;
mod values;

#[derive(Default)]
struct GeneratedCode {
    bytecode: Vec<u8>,
    library_relocations: Vec<LibraryRelocation>,
    evm_ir: Option<ir::Module>,
    debug_info: Option<DebugInfo>,
}

/// EVM code generator.
pub struct EvmCodegen<'gcx> {
    gcx: Gcx<'gcx>,
    /// The assembler for bytecode generation.
    asm: Assembler<'gcx>,
    /// Spill slots of the body being emitted, as word offsets into its spill area.
    spill_slots: FxHashMap<ValueId, u32>,
    /// The spill slot of the body's return address, when it leaves the stack.
    ret_spill_slot: Option<u32>,
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
    /// Functions that are themselves members of a recursive call cycle. A
    /// nested activation reuses their static scratch frame only after the
    /// suspended activation's live words have moved to the EVM stack.
    recursive_frame_functions: DenseBitSet<FunctionId>,
    /// Deferred spill-slot address pushes of the external body being emitted,
    /// keyed by the slot, with their reference counts. Ranked hottest-first at
    /// body end so the most reloaded slots take the shortest addresses; final
    /// addresses wait for global layout.
    spill_addr_consts: FxHashMap<u32, (DeferredConst, usize)>,
    /// Ranked external spill pushes retained until static-allocation layout is
    /// finalized, keyed by entry function.
    external_spill_addr_consts: FxHashMap<FunctionId, Vec<(DeferredConst, usize)>>,
    /// Functions whose frame lives at a compile-time-fixed address: every
    /// internal function and the constructor. Their frame objects and spill
    /// slots are absolute pushes.
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
    /// Heap floors, by the function that pushes each one. Each resolves to the highest initial
    /// free memory pointer of the entries reaching the function.
    fmp_floor_consts: Vec<(FunctionId, DeferredConst)>,
    /// Internal functions reachable from each entry that initializes the free-memory pointer.
    runtime_entry_reachability: FxHashMap<FunctionId, DenseBitSet<FunctionId>>,
    /// Every external body emitted this pass, for sizing the heap floor.
    runtime_entry_funcs: Vec<FunctionId>,
    /// The body being emitted.
    body: Body,
    /// Leaf helpers whose sole returned word is derived from the free-memory pointer.
    /// Their callers may safely use the result as a dynamic forwarding-buffer base.
    heap_pointer_return_functions: DenseBitSet<FunctionId>,
    /// Memory-reference arguments that every internal call site passes a heap pointer.
    heap_pointer_args: IndexVec<FunctionId, DenseBitSet<ArgIdx>>,
    /// Runtime code of a scheduled module, waiting for embedded bytecode to be linked in.
    pending_runtime: Option<PendingRuntime>,
    /// Immutable `PUSH<N>` placeholders in the last assembled runtime code.
    runtime_immutable_refs: Vec<ImmutableRef>,
    /// Backend encodings derived from the current module's immutable declarations.
    immutable_encodings: IndexVec<ImmutableId, ImmutableEncoding>,
    /// First constructor-memory word reserved for immutable staging.
    immutable_staging_base: u64,
    /// Deferred absolute base of the copied constructor ABI argument blob.
    constructor_args_base_const: Option<DeferredConst>,
    /// Code offset of the constructor ABI argument blob: the end of the runtime code data.
    constructor_args_offset: Option<ir::DataRef>,
    /// The deferred end of the constructor's fixed compiler-owned memory and the heap prefix
    /// guard, from which the constructor derives its initial free memory pointer.
    constructor_heap_start: Option<(DeferredConst, u64)>,
    /// Whether we're currently generating constructor code.
    /// When true, arguments load from the copied deployment ABI blob.
    in_constructor: bool,
    /// Shared constructor completion reached by ordinary empty returns.
    constructor_exit: Option<Label>,
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
            spill_slots: FxHashMap::default(),
            ret_spill_slot: None,
            block_labels: FxHashMap::default(),
            function_labels: FxHashMap::default(),
            cold_functions: DenseBitSet::new_empty(0),
            empty_stop_functions: DenseBitSet::new_empty(0),
            cold_blocks: DenseBitSet::new_empty(0),
            function_spill_sizes: FxHashMap::default(),
            recursive_frame_functions: DenseBitSet::new_empty(0),
            spill_addr_consts: FxHashMap::default(),
            external_spill_addr_consts: FxHashMap::default(),
            static_frame_functions: DenseBitSet::new_empty(0),
            static_frame_addr_consts: FxHashMap::default(),
            packed_static_frame_sizes: FxHashMap::default(),
            pending_static_allocs: FxHashMap::default(),
            runtime_free_memory_consts: FxHashMap::default(),
            fmp_floor_consts: Vec::new(),
            runtime_entry_reachability: FxHashMap::default(),
            runtime_entry_funcs: Vec::new(),
            body: Body::External,
            heap_pointer_return_functions: DenseBitSet::new_empty(0),
            heap_pointer_args: IndexVec::new(),
            pending_runtime: None,
            runtime_immutable_refs: Vec::new(),
            immutable_encodings: IndexVec::new(),
            immutable_staging_base: EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT
                + EvmMemoryLayout::WORD_SIZE,
            constructor_args_base_const: None,
            constructor_args_offset: None,
            constructor_heap_start: None,
            in_constructor: false,
            constructor_exit: None,
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
        self.reset_artifact(module);
        self.cold_functions.clear_to(module.functions.len());
        self.empty_stop_functions.clear_to(module.functions.len());
        self.heap_pointer_return_functions.clear_to(module.functions.len());
        self.heap_pointer_args.clear();
        self.runtime_immutable_refs.clear();
        self.immutable_encodings.clear();
        self.immutable_staging_base =
            EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT + EvmMemoryLayout::WORD_SIZE;
        self.constructor_args_base_const = None;
        self.constructor_args_offset = None;
        self.constructor_heap_start = None;
        self.in_constructor = false;
        self.constructor_exit = None;
        self.reset_switch_gas_code_growth();
    }

    /// Clears the emission state of one artifact: labels, frames, and spill areas.
    fn reset_artifact(&mut self, module: &Module) {
        let functions = module.functions.len();
        self.spill_slots.clear();
        self.ret_spill_slot = None;
        self.block_labels.clear();
        self.function_labels.clear();
        self.cold_blocks.clear_to(0);
        self.function_spill_sizes.clear();
        self.recursive_frame_functions.clear_to(functions);
        self.spill_addr_consts.clear();
        self.external_spill_addr_consts.clear();
        self.static_frame_functions.clear_to(functions);
        self.static_frame_addr_consts.clear();
        self.packed_static_frame_sizes.clear();
        self.pending_static_allocs.clear();
        self.runtime_free_memory_consts.clear();
        self.fmp_floor_consts.clear();
        self.runtime_entry_reachability.clear();
        self.runtime_entry_funcs.clear();
        self.body = Body::External;
        self.emitting_entry = false;
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

/// The kind of body being emitted, which decides where its frame and spill slots live.
#[derive(Clone, Copy, Debug)]
enum Body {
    /// The dispatch entry or an external entry, whose frame objects sit at the bottom of the
    /// heap.
    External,
    /// An internal function, with a fixed frame that also holds its spill slots.
    Internal(FunctionId),
    /// The constructor, with a fixed frame and a spill area of its own.
    Constructor(FunctionId),
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

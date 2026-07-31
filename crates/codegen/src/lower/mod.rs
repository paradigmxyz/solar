//! HIR to MIR lowering.
//!
//! This module transforms the high-level IR from solar-sema into MIR.

mod abi_encode;
mod abi_packed;
mod bytes;
mod call;
mod checked_arith;
mod expr;
mod index;
mod stmt;
mod storage;
mod type_query;

use crate::{
    memory::EvmMemoryLayout,
    mir::{
        AbiLayout, BlockId, Function, FunctionAttributes, FunctionBuilder, FunctionId, ImmutableId,
        MemoryObjectKind, MirType, Module, SliceLocation, StorageLayoutRef, TypeSize, ValueId,
    },
};
use alloy_primitives::{Bytes, U256};
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    map::{FxHashMap, FxHashSet},
    smallvec::SmallVec,
};
use solar_interface::{
    Ident, Span,
    diagnostics::{DiagMsg, ErrorGuaranteed},
    kw, sym,
};
use solar_sema::{
    hir::{self, ContractId, ElementaryType, FunctionId as HirFunctionId, VariableId},
    ty::{CallableParamSource, Gcx, Ty, TyKind},
};
use std::collections::hash_map::Entry;

use self::storage::StorageLocation;

/// Minimum contiguous zero-word count where bulk zeroing beats individual stores.
const MIN_BULK_ZERO_MEMORY_WORDS: u64 = 4;

/// Context for a loop (tracks break/continue targets).
#[derive(Clone, Copy)]
pub(crate) struct LoopContext {
    /// Block to jump to on `break`.
    pub break_target: BlockId,
    /// Block to jump to on `continue`.
    pub continue_target: BlockId,
}

/// Clean-word validator for a value type decoded from ABI calldata.
#[derive(Clone, Copy)]
enum AbiWordValidator {
    /// The word must equal itself masked with the given mask.
    Mask(U256),
    /// The word must equal `signextend(byte_index, word)`.
    SignExtend(u64),
    /// The word must equal `iszero(iszero(word))`.
    Bool,
    /// The word must be less than the member count.
    EnumRange(u64),
}

impl AbiWordValidator {
    /// Builds the condition that accepts a canonical ABI word.
    fn condition(self, builder: &mut FunctionBuilder<'_>, word: ValueId) -> ValueId {
        match self {
            Self::Mask(mask) => {
                let mask = builder.imm_u256(mask);
                let canonical = builder.and(word, mask);
                builder.eq(word, canonical)
            }
            Self::SignExtend(byte_index) => {
                let byte_index = builder.imm_u64(byte_index);
                let canonical = builder.signextend(byte_index, word);
                builder.eq(word, canonical)
            }
            Self::Bool => {
                let is_zero = builder.iszero(word);
                let canonical = builder.iszero(is_zero);
                builder.eq(word, canonical)
            }
            Self::EnumRange(count) => {
                let count = builder.imm_u64(count);
                builder.lt(word, count)
            }
        }
    }
}

enum ConstructorArguments {
    Resolving,
    Resolved(SmallVec<[ValueId; 4]>),
}

#[derive(Clone, Copy)]
enum AbiParamSource {
    ExternalCalldata,
    ConstructorMemory,
}

impl AbiParamSource {
    fn load(
        self,
        lowerer: &mut Lowerer<'_>,
        builder: &mut FunctionBuilder<'_>,
        arg_index: u64,
    ) -> ValueId {
        match self {
            Self::ExternalCalldata => {
                // Runtime ABI encoding: selector (4 bytes) + one head word per parameter.
                let offset = builder.imm_u64(4 + arg_index * EvmMemoryLayout::WORD_SIZE);
                builder.calldataload(offset)
            }
            Self::ConstructorMemory => {
                let base = lowerer.constructor_args_base(builder);
                let offset = builder.imm_u64(arg_index * EvmMemoryLayout::WORD_SIZE);
                let address = builder.add(base, offset);
                builder.mload(address)
            }
        }
    }
}

/// Where an inlined callee's `return` statements deliver their values: each
/// value is stored into the matching return variable's local slot, then control
/// jumps to `exit_block`, where the call site reads the slots back.
#[derive(Clone)]
struct InlineReturnCtx {
    /// Join block the call site continues from after the inlined body.
    exit_block: BlockId,
    /// The callee's return variables, in declaration order. Each has a local
    /// slot allocated before the body is lowered.
    return_vars: Vec<VariableId>,
}

type InternalFunctionPointerShape = (Vec<MirType>, Vec<MirType>);

/// Lowering context for converting HIR to MIR.
pub(crate) struct Lowerer<'gcx> {
    /// The global context.
    gcx: Gcx<'gcx>,
    /// The current module being built.
    module: Module,
    /// The most-derived contract this module is being built for.
    contract_id: Option<ContractId>,
    /// The current contract being lowered.
    current_contract_id: Option<ContractId>,
    /// Mapping from HIR variable IDs to storage slots.
    storage_slots: FxHashMap<VariableId, U256>,
    /// Mapping from HIR variable IDs to full storage locations.
    storage_locations: FxHashMap<VariableId, StorageLocation>,
    /// Next available storage slot.
    next_storage_slot: U256,
    /// Next available byte offset in `next_storage_slot` for packed variables.
    next_storage_offset: u8,
    /// Mapping from HIR immutable variable IDs to MIR immutable IDs.
    immutable_ids: FxHashMap<VariableId, ImmutableId>,
    /// Mapping from HIR variable IDs to MIR values (for local variables).
    /// For SSA-style immutable variables (function params and non-mutated locals).
    locals: FxHashMap<VariableId, ValueId>,
    /// Mapping from HIR variable IDs to memory offsets (for mutable locals).
    /// Memory layout: starts at offset 0x80 (after scratch space).
    local_memory_slots: FxHashMap<VariableId, u64>,
    /// Reassignable calldata `bytes`/`string`/array locals whose two-word
    /// local slot holds the logical slice as `[ptr][len]`. Rebinding stores
    /// both words, so every CFG join reads one merged representation while
    /// the value stays a lazy slice.
    slice_slot_locals: FxHashSet<VariableId>,
    /// Active inline-return target. While a callee body is being inlined at a
    /// call site, an explicit `return` stores its values into the callee's
    /// return-variable slots and jumps here, instead of terminating the
    /// enclosing MIR function.
    inline_returns: Option<InlineReturnCtx>,
    /// Return values of the most recently inlined multi-return callee whose
    /// returns cannot ride the one-word-per-value multi-return buffer
    /// (calldata slices). Destructuring consumes them directly.
    pending_inline_returns: Option<Vec<ValueId>>,
    /// Next available memory offset for locals.
    next_local_memory_offset: u64,
    /// Bytecodes of other contracts (for `new` expressions).
    contract_bytecodes: FxHashMap<ContractId, Bytes>,
    /// Stack of loop contexts for nested loops.
    loop_stack: Vec<LoopContext>,
    /// Variables that are assigned after declaration (need memory storage).
    /// Variables not in this set can be kept as SSA values.
    assigned_vars: GrowableBitSet<VariableId>,
    /// Invalid event declarations whose topic-count error has already been emitted.
    invalid_event_topics: GrowableBitSet<hir::EventId>,
    /// Whether the next expression is an error-checking boundary.
    check_expr_errors: bool,
    /// Whether HIR contained errors before codegen started.
    hir_has_errors: bool,
    /// Local variables that are storage references (pointers). Their value in
    /// `locals` or a local memory slot is a storage *slot*, so `r.field` reads
    /// `sload(slot + offset)` and `r.field = v` writes `sstore(slot + offset, v)`,
    /// rather than treating the value as a memory pointer.
    storage_ref_locals: GrowableBitSet<VariableId>,
    /// Stack of function IDs currently being inlined (for cycle detection).
    inline_stack: Vec<HirFunctionId>,
    /// Expression error-checking states suspended at inline function boundaries.
    inline_expr_error_checks: u32,
    /// HIR functions already lowered into this MIR module.
    hir_to_mir_functions: FxHashMap<HirFunctionId, FunctionId>,
    /// Internal-convention copies of public functions, lowered on demand so that
    /// public functions can be called internally/recursively via `internal_call`.
    hir_to_internal_mir_functions: FxHashMap<HirFunctionId, FunctionId>,
    /// Cache of whether a function is (directly) self-recursive.
    recursive_functions: FxHashMap<HirFunctionId, bool>,
    /// Functions currently being lowered on demand.
    lowering_functions: GrowableBitSet<HirFunctionId>,
    /// Functions whose declarations are used as internal function values.
    internal_function_pointer_targets: GrowableBitSet<HirFunctionId>,
    /// Shared internal function-pointer dispatchers keyed by MIR parameter and return types.
    internal_function_pointer_dispatchers: FxHashMap<InternalFunctionPointerShape, FunctionId>,
    /// Whether the current function body is constructor code.
    lowering_constructor: bool,
    /// Shared base value for constructor ABI argument accesses.
    constructor_args_base: Option<ValueId>,
    /// Whether local memory slots should be addressed through the internal-call frame.
    lowering_internal_function: bool,
    /// The module's shared `Error(string)` revert helper, synthesized on first
    /// use: constant short revert messages call it instead of materializing
    /// and ABI-encoding the string at every site.
    revert_error_helper: Option<FunctionId>,
    /// The module's shared storage-`bytes`/`string` load helper: decodes the
    /// packed short/long form into a fresh `[length][data...]` memory copy.
    storage_bytes_helper: Option<FunctionId>,
    /// Guards helper synthesis against routing through itself.
    synthesizing_helper: bool,
    /// Whether arithmetic should use wrapping Solidity `unchecked` semantics.
    in_unchecked_block: bool,
    /// Sema return types of the function currently being lowered (one per declared
    /// return), used to ABI-encode external returns.
    current_return_tys: Vec<Ty<'gcx>>,
    /// Mapping from struct state variable ID to base storage slot.
    pub(crate) struct_storage_base_slots: FxHashMap<VariableId, U256>,
    /// Cached struct field slot offsets: (struct_type_id, field_index) -> slot offset from base.
    pub(crate) struct_field_offsets: FxHashMap<(hir::StructId, usize), u64>,
    /// Interned semantic memory/storage layout for each lowered struct type.
    struct_storage_layouts: FxHashMap<hir::StructId, StorageLayoutRef>,
}

impl<'gcx> Lowerer<'gcx> {
    /// Returns an existing error guarantee or emits a codegen diagnostic when
    /// lowering encounters invalid HIR without a prior error.
    pub(super) fn recovery_error(
        &self,
        span: Option<Span>,
        msg: impl Into<DiagMsg>,
    ) -> ErrorGuaranteed {
        match self.gcx.dcx().has_errors() {
            Err(guar) => guar,
            Ok(()) => {
                let diag = self.gcx.dcx().err(msg);
                if let Some(span) = span { diag.span(span).emit() } else { diag.emit() }
            }
        }
    }

    /// Reports a lowering error and returns the error sentinel value carrying
    /// the emitted diagnostic's guarantee, mirroring HIR's error types.
    pub(super) fn err_value(
        &self,
        builder: &mut FunctionBuilder<'_>,
        span: Span,
        msg: impl Into<DiagMsg>,
    ) -> ValueId {
        let guar = self.gcx.dcx().err(msg).span(span).emit();
        builder.error_value(guar)
    }

    /// Creates a new lowerer.
    pub(crate) fn new(gcx: Gcx<'gcx>, name: Ident) -> Self {
        if !gcx.has_typeck_results() {
            gcx.dcx().emit_err(
                name.span,
                "tried to lower contract without typeck results; likely missing -Zcodegen",
            );
        }
        let hir_has_errors = gcx.dcx().has_errors().is_err();
        Self {
            gcx,
            module: Module::new(name),
            contract_id: None,
            current_contract_id: None,
            storage_slots: FxHashMap::default(),
            storage_locations: FxHashMap::default(),
            next_storage_slot: U256::ZERO,
            next_storage_offset: 0,
            immutable_ids: FxHashMap::default(),
            locals: FxHashMap::default(),
            local_memory_slots: FxHashMap::default(),
            slice_slot_locals: FxHashSet::default(),
            inline_returns: None,
            pending_inline_returns: None,
            next_local_memory_offset: EvmMemoryLayout::HEAP_START,
            contract_bytecodes: FxHashMap::default(),
            loop_stack: Vec::new(),
            assigned_vars: GrowableBitSet::new_empty(),
            invalid_event_topics: GrowableBitSet::new_empty(),
            check_expr_errors: hir_has_errors,
            hir_has_errors,
            storage_ref_locals: GrowableBitSet::new_empty(),
            inline_stack: Vec::new(),
            inline_expr_error_checks: 0,
            hir_to_mir_functions: FxHashMap::default(),
            hir_to_internal_mir_functions: FxHashMap::default(),
            recursive_functions: FxHashMap::default(),
            lowering_functions: GrowableBitSet::new_empty(),
            internal_function_pointer_targets: GrowableBitSet::new_empty(),
            internal_function_pointer_dispatchers: FxHashMap::default(),
            lowering_constructor: false,
            constructor_args_base: None,
            lowering_internal_function: false,
            revert_error_helper: None,
            storage_bytes_helper: None,
            synthesizing_helper: false,
            in_unchecked_block: false,
            current_return_tys: Vec::new(),
            struct_storage_base_slots: FxHashMap::default(),
            struct_field_offsets: FxHashMap::default(),
            struct_storage_layouts: FxHashMap::default(),
        }
    }

    /// Pushes a loop context onto the stack.
    pub(crate) fn push_loop(&mut self, ctx: LoopContext) {
        self.loop_stack.push(ctx);
    }

    /// Pops a loop context from the stack.
    pub(crate) fn pop_loop(&mut self) {
        self.loop_stack.pop();
    }

    /// Gets the current loop context, if any.
    pub(crate) fn current_loop(&self) -> Option<&LoopContext> {
        self.loop_stack.last()
    }

    /// Maximum inline depth to prevent excessive recursion.
    const MAX_INLINE_DEPTH: usize = 32;
    /// Historical base used by local memory slots in external function bodies.
    /// Attempts to enter inlining for a function. Returns false if a cycle is detected
    /// or the max inline depth is exceeded.
    fn try_enter_inline(&mut self, func_id: HirFunctionId) -> bool {
        // Check for cycle
        if self.inline_stack.contains(&func_id) {
            return false;
        }
        // Check depth limit
        if self.inline_stack.len() >= Self::MAX_INLINE_DEPTH {
            return false;
        }
        self.inline_stack.push(func_id);
        self.inline_expr_error_checks <<= 1;
        self.inline_expr_error_checks |=
            u32::from(std::mem::replace(&mut self.check_expr_errors, self.hir_has_errors));
        true
    }

    /// Exits inlining for a function.
    fn exit_inline(&mut self) {
        let popped = self.inline_stack.pop();
        debug_assert!(popped.is_some());
        self.check_expr_errors = self.inline_expr_error_checks & 1 != 0;
        self.inline_expr_error_checks >>= 1;
    }

    /// Allocates a memory slot for a local variable.
    /// Returns the memory offset.
    pub(crate) fn alloc_local_memory(&mut self, var_id: VariableId) -> u64 {
        let offset = self.next_local_memory_offset;
        self.next_local_memory_offset += EvmMemoryLayout::WORD_SIZE;
        self.local_memory_slots.insert(var_id, offset);
        offset
    }

    /// Allocates a two-word memory slot holding a logical slice as
    /// `[ptr][len]` and returns the base offset.
    pub(crate) fn alloc_local_slice_memory(&mut self, var_id: VariableId) -> u64 {
        let offset = self.next_local_memory_offset;
        self.next_local_memory_offset += 2 * EvmMemoryLayout::WORD_SIZE;
        self.local_memory_slots.insert(var_id, offset);
        self.slice_slot_locals.insert(var_id);
        offset
    }

    /// Whether `var_id` is a reassignable local whose slot holds a slice.
    pub(crate) fn is_slice_slot_local(&self, var_id: &VariableId) -> bool {
        self.slice_slot_locals.contains(var_id)
    }

    /// Stores a logical slice into its two-word local slot.
    pub(crate) fn store_slice_slot(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        offset: u64,
        slice: ValueId,
    ) {
        let ptr = builder.slice_ptr(slice);
        let len = builder.slice_len(slice);
        let ptr_addr = self.local_memory_addr(builder, offset);
        builder.mstore(ptr_addr, ptr);
        let len_addr = self.local_memory_addr(builder, offset + EvmMemoryLayout::WORD_SIZE);
        builder.mstore(len_addr, len);
    }

    /// Initializes a two-word local slice slot to the empty slice.
    pub(crate) fn init_empty_slice_slot(&mut self, builder: &mut FunctionBuilder<'_>, offset: u64) {
        let ptr_addr = self.local_memory_addr(builder, offset);
        let ptr = builder.imm_u64(0);
        builder.mstore(ptr_addr, ptr);
        let len_addr = self.local_memory_addr(builder, offset + EvmMemoryLayout::WORD_SIZE);
        let len = builder.imm_u64(0);
        builder.mstore(len_addr, len);
    }

    /// Reloads a logical slice from its two-word local slot.
    pub(crate) fn load_slice_slot(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        offset: u64,
        location: crate::mir::SliceLocation,
    ) -> ValueId {
        let ptr_addr = self.local_memory_addr(builder, offset);
        let ptr = builder.mload(ptr_addr);
        let len_addr = self.local_memory_addr(builder, offset + EvmMemoryLayout::WORD_SIZE);
        let len = builder.mload(len_addr);
        builder.make_slice(ptr, len, location)
    }

    /// Gets the memory offset for a local variable, if it's stored in memory.
    pub(crate) fn get_local_memory_offset(&self, var_id: &VariableId) -> Option<u64> {
        self.local_memory_slots.get(var_id).copied()
    }

    /// Returns the address for a local memory slot in the current lowering context.
    pub(crate) fn local_memory_addr(
        &self,
        builder: &mut FunctionBuilder<'_>,
        offset: u64,
    ) -> ValueId {
        if self.lowering_internal_function {
            let header_size = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE;
            let arg_size = (builder.func().params.len() as u64) * EvmMemoryLayout::WORD_SIZE;
            let return_size = (builder.func().returns.len() as u64) * EvmMemoryLayout::WORD_SIZE;
            let local_offset = offset.saturating_sub(EvmMemoryLayout::HEAP_START);
            builder.internal_frame_addr(header_size + arg_size + return_size + local_offset)
        } else {
            builder.imm_u64(offset)
        }
    }

    fn constructor_args_base(&mut self, builder: &mut FunctionBuilder<'_>) -> ValueId {
        if let Some(base) = self.constructor_args_base {
            return base;
        }
        let base = builder.constructor_args_base();
        self.constructor_args_base = Some(base);
        base
    }

    /// Loads an immutable value.
    pub(crate) fn load_immutable_value(
        &self,
        builder: &mut FunctionBuilder<'_>,
        id: ImmutableId,
    ) -> ValueId {
        builder.load_immutable(id, self.module.immutable_type(id))
    }

    /// Registers a contract's bytecode for use in `new` expressions.
    pub(crate) fn register_contract_bytecode(&mut self, contract_id: ContractId, bytecode: Bytes) {
        self.contract_bytecodes.insert(contract_id, bytecode);
    }

    /// Lowers a contract to MIR.
    pub(crate) fn lower_contract(&mut self, contract_id: ContractId) {
        let contract = self.gcx.hir.contract(contract_id);
        self.contract_id = Some(contract_id);

        // Track the current contract for using directive resolution.
        self.current_contract_id = Some(contract_id);

        // Mark interfaces - they don't generate deployable bytecode.
        if contract.kind == hir::ContractKind::Interface {
            self.module.is_interface = true;
        }

        self.allocate_storage(contract_id);

        // Generate a constructor for inherited construction/state-variable
        // initialization when the current contract does not declare one.
        if contract.ctor.is_none() {
            self.generate_synthetic_constructor(contract_id);
        }

        if self.gcx.sess.opts.unstable.codegen_all_functions || self.hir_has_errors {
            for function in self.collect_unpruned_functions(contract_id) {
                self.ensure_function_lowered(function);
            }
        } else {
            self.lower_reachable_function_roots(contract_id);
        }

        self.current_contract_id = None;
    }

    /// Lowers reachable function roots in inheritance order.
    fn lower_reachable_function_roots(&mut self, contract_id: ContractId) {
        let contract = self.gcx.hir.contract(contract_id);
        let reachable = self.gcx.contract_reachable_functions(contract_id);
        let mut interface = DenseBitSet::new_empty(self.gcx.hir.function_ids().len());
        for function in self.gcx.interface_functions(contract_id) {
            interface.insert(function.id);
        }

        for &base_id in contract.linearized_bases {
            for function_id in self.gcx.hir.contract(base_id).all_functions() {
                if !reachable.contains(function_id) {
                    continue;
                }

                let function = self.gcx.hir.function(function_id);
                let selected = match function.kind {
                    hir::FunctionKind::Constructor => contract.ctor == Some(function_id),
                    hir::FunctionKind::Fallback => contract.fallback == Some(function_id),
                    hir::FunctionKind::Receive => contract.receive == Some(function_id),
                    hir::FunctionKind::Function
                        if function.visibility >= hir::Visibility::Public =>
                    {
                        interface.contains(function_id)
                    }
                    hir::FunctionKind::Function | hir::FunctionKind::Modifier => {
                        base_id == contract_id || function.visibility != hir::Visibility::Private
                    }
                };
                if selected {
                    self.ensure_function_lowered(function_id);
                }
            }
        }
    }

    /// Collects function roots without callgraph reachability filtering.
    fn collect_unpruned_functions(&self, contract_id: ContractId) -> Vec<HirFunctionId> {
        let contract = self.gcx.hir.contract(contract_id);
        let linearized_bases = contract.linearized_bases;

        let mut seen_selectors: FxHashSet<[u8; 4]> = FxHashSet::default();
        let mut has_constructor = false;
        let mut has_fallback = false;
        let mut has_receive = false;
        let mut functions = Vec::new();

        // Iterate from most-derived (index 0) to most-base (last index).
        // The first function with a given selector wins (override behavior).
        for &base_id in linearized_bases.iter() {
            let base_contract = self.gcx.hir.contract(base_id);

            for func_id in base_contract.all_functions() {
                let func = self.gcx.hir.function(func_id);

                // Handle special functions by kind
                match func.kind {
                    hir::FunctionKind::Constructor => {
                        // Constructors are not inherited. Base constructors
                        // are called from the current contract's constructor
                        // prelude instead.
                        if base_id == contract_id && !has_constructor {
                            has_constructor = true;
                            functions.push(func_id);
                        }
                    }
                    hir::FunctionKind::Fallback => {
                        if !has_fallback {
                            has_fallback = true;
                            functions.push(func_id);
                        }
                    }
                    hir::FunctionKind::Receive => {
                        if !has_receive {
                            has_receive = true;
                            functions.push(func_id);
                        }
                    }
                    hir::FunctionKind::Function | hir::FunctionKind::Modifier => {
                        // Skip private functions from base contracts - they're not inherited
                        if base_id != contract_id && func.visibility == hir::Visibility::Private {
                            continue;
                        }

                        // For regular functions, use selector to determine uniqueness.
                        // Only external/public functions have selectors.
                        let is_external_abi = matches!(
                            func.visibility,
                            hir::Visibility::External | hir::Visibility::Public
                        );
                        if is_external_abi {
                            let selector = self.function_selector(func_id);
                            if seen_selectors.insert(selector) {
                                functions.push(func_id);
                            }
                        } else {
                            // Include internal functions from every base by identity; they have no
                            // selector.
                            functions.push(func_id);
                        }
                    }
                }
            }
        }

        functions
    }

    /// Generates a synthetic constructor to initialize state variables and run
    /// inherited constructors when the current contract does not declare one.
    fn generate_synthetic_constructor(&mut self, contract_id: ContractId) {
        let contract = self.gcx.hir.contract(contract_id);
        let linearized_bases = contract.linearized_bases;

        let has_state_initializers = linearized_bases.iter().any(|&base_id| {
            self.gcx.hir.contract(base_id).variables().any(|var_id| {
                let var = self.gcx.hir.variable(var_id);
                var.is_state_variable() && !var.is_constant() && var.initializer.is_some()
            })
        });
        let has_base_constructors = linearized_bases.iter().any(|&base_id| {
            base_id != contract_id && self.gcx.hir.contract(base_id).ctor.is_some()
        });

        if !has_state_initializers && !has_base_constructors {
            return;
        }

        // Create constructor function
        let ctor_name = Ident::new(kw::Constructor, Span::DUMMY);
        let mut mir_func = Function::new(ctor_name);
        mir_func.attributes = FunctionAttributes {
            visibility: hir::Visibility::Public,
            state_mutability: hir::StateMutability::NonPayable,
            is_constructor: true,
            is_fallback: false,
            is_receive: false,
            is_dispatch_entry: false,
            no_inline: false,
        };

        {
            let mut builder = FunctionBuilder::new(&mut mir_func);
            let saved_locals = std::mem::take(&mut self.locals);
            let saved_local_memory_slots = std::mem::take(&mut self.local_memory_slots);
            let saved_slice_slot_locals = std::mem::take(&mut self.slice_slot_locals);
            let saved_next_local_memory_offset = self.next_local_memory_offset;
            let saved_assigned_vars = std::mem::take(&mut self.assigned_vars);
            let saved_inline_returns = self.inline_returns.take();
            let saved_pending_inline_returns = self.pending_inline_returns.take();
            let saved_lowering_constructor = self.lowering_constructor;
            let saved_constructor_args_base = self.constructor_args_base;
            let saved_lowering_internal_function = self.lowering_internal_function;
            let saved_in_unchecked_block = self.in_unchecked_block;
            let saved_current_return_tys = std::mem::take(&mut self.current_return_tys);
            self.next_local_memory_offset = EvmMemoryLayout::HEAP_START;
            self.lowering_constructor = true;
            self.constructor_args_base = None;
            self.lowering_internal_function = false;
            self.in_unchecked_block = false;

            self.lower_constructor_prelude(&mut builder, contract_id);
            builder.stop();
            builder.func_mut().internal_frame_size =
                self.next_local_memory_offset.saturating_sub(EvmMemoryLayout::HEAP_START);
            self.locals = saved_locals;
            self.local_memory_slots = saved_local_memory_slots;
            self.slice_slot_locals = saved_slice_slot_locals;
            self.next_local_memory_offset = saved_next_local_memory_offset;
            self.assigned_vars = saved_assigned_vars;
            self.inline_returns = saved_inline_returns;
            self.pending_inline_returns = saved_pending_inline_returns;
            self.lowering_constructor = saved_lowering_constructor;
            self.constructor_args_base = saved_constructor_args_base;
            self.lowering_internal_function = saved_lowering_internal_function;
            self.in_unchecked_block = saved_in_unchecked_block;
            self.current_return_tys = saved_current_return_tys;
        }

        self.module.add_function(mir_func);
    }

    /// Allocates storage slots for state variables.
    ///
    /// For inheritance, state variables are allocated starting from the most base contract
    /// (last in linearized_bases) to the most derived (first in linearized_bases).
    /// This ensures parent storage comes before child storage in the layout.
    fn allocate_storage(&mut self, contract_id: ContractId) {
        let contract = self.gcx.hir.contract(contract_id);
        let linearized_bases = contract.linearized_bases;

        // Iterate in reverse order (most base first) to get correct storage layout.
        // Skip index 0 since that's the contract itself - we handle it last.
        for &base_id in linearized_bases.iter().rev() {
            let base_contract = self.gcx.hir.contract(base_id);
            for var_id in base_contract.variables() {
                // Skip if we already allocated this variable (shouldn't happen, but safety check)
                if self.storage_slots.contains_key(&var_id) {
                    continue;
                }

                let var = self.gcx.hir.variable(var_id);
                // Constants are inlined. Immutables are patched into typed
                // runtime-code `PUSH<N>` placeholders at deploy time.
                if var.is_state_variable() && var.is_immutable() {
                    let Some(name) = var.name else {
                        self.recovery_error(Some(var.span), "state immutable must be named");
                        continue;
                    };
                    let ty = self.lower_type_from_var(var_id);
                    let id = self.module.add_immutable(name, ty, Some(var_id));
                    self.immutable_ids.insert(var_id, id);
                } else if var.is_state_variable() && !var.is_constant() {
                    let var_ty = self.gcx.type_of_item(var_id.into());
                    let location = self.allocate_storage_location(var_ty, var.ty.span);
                    let base_slot = location.slot;

                    // Track struct base slots for field access
                    if matches!(var_ty.peel_refs().kind, TyKind::Struct(_)) {
                        self.struct_storage_base_slots.insert(var_id, base_slot);
                    }

                    self.storage_slots.insert(var_id, base_slot);
                    self.storage_locations.insert(var_id, location);
                }
            }
        }
    }

    /// The calldata slice for a `calldata` struct member, read from the copy's
    /// trailing position word.
    ///
    /// Reads of a member go through the rebuilt copy; this exists only for the
    /// one use the copy cannot serve — handing the member to a `calldata`
    /// parameter, whose callee expects a slice rather than an object. A
    /// dynamically encoded struct puts each member's head word at its own base,
    /// and a dynamic member's head word is the offset of its tail relative to
    /// that base.
    pub(super) fn calldata_member_slice(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> Option<ValueId> {
        let hir::ExprKind::Member(base, member) = &expr.kind else { return None };
        let ty = self.get_expr_type(base)?;
        if !matches!(ty.kind, TyKind::Ref(_, solar_ast::DataLocation::Calldata)) {
            return None;
        }
        let TyKind::Struct(struct_id) = ty.peel_refs().kind else { return None };
        let (_, index) = self.get_memory_struct_field_info(base, *member)?;
        let field_tys = self.gcx.struct_field_types(struct_id).to_vec();
        let field_ty = field_tys.get(index)?.peel_refs();
        // Only a member that *is* a slice: an array, or `bytes`/`string`. A
        // nested struct member is dynamic too, but it is its own copy, which
        // carries its own base and answers its own members.
        if !matches!(
            field_ty.kind,
            TyKind::DynArray(_)
                | TyKind::Slice(_)
                | TyKind::Elementary(
                    solar_ast::ElementaryType::Bytes | solar_ast::ElementaryType::String
                )
        ) {
            return None;
        }

        let ptr = self.lower_value_expr(builder, base);
        let struct_base = self.calldata_base_of_copy(builder, ptr, field_tys.len() as u64);
        let head_offset = match self
            .abi_head_size_sum(field_tys[..index].iter().map(|&field| field.peel_refs()))
        {
            Ok(size) => size,
            Err(guar) => return Some(builder.error_value(guar)),
        };
        let head_pos = self.offset_ptr(builder, struct_base, head_offset);
        let tail_offset = builder.calldataload(head_pos);
        let len_pos = builder.add(struct_base, tail_offset);
        let len = builder.calldataload(len_pos);
        let word = builder.imm_u64(EvmMemoryLayout::WORD_SIZE);
        let data = builder.add(len_pos, word);
        Some(builder.make_slice(data, len, SliceLocation::Calldata))
    }

    /// Returns the type and constant length of a fixed-size array parameter
    /// whose elements are single ABI words. Other parameter shapes return
    /// `None`.
    fn fixed_word_array_param(&self, param_id: VariableId) -> Option<(Ty<'gcx>, u64)> {
        let TyKind::Array(elem, len) = self.gcx.type_of_item(param_id.into()).peel_refs().kind
        else {
            return None;
        };
        (self.abi_is_word_element(elem) && len <= U256::from(u16::MAX))
            .then(|| (elem, len.to::<u64>()))
    }

    /// Whether a parameter is a memory-located dynamic array of single-word elements, which
    /// the prologue decodes from calldata into Solidity's `[length][data...]` memory layout.
    fn is_dyn_word_array_memory_param(&self, param_id: VariableId) -> bool {
        let param = self.gcx.hir.variable(param_id);
        if param.data_location != Some(solar_ast::DataLocation::Memory) {
            return false;
        }
        match self.gcx.type_of_item(param_id.into()).peel_refs().kind {
            TyKind::DynArray(elem) => self.abi_is_word_element(elem),
            _ => false,
        }
    }

    /// Lowers a function to MIR.
    pub(super) fn ensure_function_lowered(&mut self, func_id: hir::FunctionId) -> FunctionId {
        if let Some(&mir_id) = self.hir_to_mir_functions.get(&func_id) {
            return mir_id;
        }

        if self.lowering_functions.contains(func_id) {
            return self
                .module
                .add_function(Function::new(Ident::new(sym::_recursive_internal, Span::DUMMY)));
        }

        let saved_locals = std::mem::take(&mut self.locals);
        let saved_local_memory_slots = std::mem::take(&mut self.local_memory_slots);
        let saved_slice_slot_locals = std::mem::take(&mut self.slice_slot_locals);
        let saved_next_local_memory_offset = self.next_local_memory_offset;
        let saved_assigned_vars = std::mem::take(&mut self.assigned_vars);
        let saved_inline_returns = self.inline_returns.take();
        let saved_pending_inline_returns = self.pending_inline_returns.take();
        let saved_current_contract_id = self.current_contract_id;
        let saved_lowering_constructor = self.lowering_constructor;
        let saved_constructor_args_base = self.constructor_args_base;
        let saved_lowering_internal_function = self.lowering_internal_function;
        let saved_in_unchecked_block = self.in_unchecked_block;
        let saved_current_return_tys = std::mem::take(&mut self.current_return_tys);

        self.lowering_functions.insert(func_id);
        self.current_contract_id = self.gcx.hir.function(func_id).contract;
        self.in_unchecked_block = false;
        let mir_id = self.lower_function(func_id, false);
        self.lowering_functions.remove(func_id);

        self.locals = saved_locals;
        self.local_memory_slots = saved_local_memory_slots;
        self.slice_slot_locals = saved_slice_slot_locals;
        self.next_local_memory_offset = saved_next_local_memory_offset;
        self.assigned_vars = saved_assigned_vars;
        self.inline_returns = saved_inline_returns;
        self.pending_inline_returns = saved_pending_inline_returns;
        self.current_contract_id = saved_current_contract_id;
        self.lowering_constructor = saved_lowering_constructor;
        self.constructor_args_base = saved_constructor_args_base;
        self.lowering_internal_function = saved_lowering_internal_function;
        self.in_unchecked_block = saved_in_unchecked_block;
        self.current_return_tys = saved_current_return_tys;
        mir_id
    }

    /// Returns the module's shared `Error(string)` revert helper, synthesizing
    /// it on first use.
    ///
    /// The helper takes the message length (1..=32) and its bytes left-aligned
    /// in one word, and reverts with the standard `Error(string)` encoding:
    /// selector, head offset, length, one padded data word — 100 bytes of
    /// revert data, matching what the generic in-line path produces for short
    /// messages. Sharing this cold path saves the string materialization and
    /// ABI-encode boilerplate (~60-90 bytes) at every `require`/`revert` site
    /// with a constant short message.
    pub(super) fn ensure_revert_error_helper(&mut self) -> FunctionId {
        let Self { revert_error_helper, module, .. } = self;
        *revert_error_helper.get_or_insert_with(|| {
            let name = Ident::new(sym::__revert_error, Span::DUMMY);
            let mut func = Function::new(name);
            func.attributes.no_inline = true;
            {
                let mut builder = FunctionBuilder::new(&mut func);
                let len = builder.add_param(MirType::uint256());
                let data = builder.add_param(MirType::uint256());
                let selector = builder.imm_u256(U256::from(0x08c3_79a0u64) << 224);
                let zero = builder.imm_u64(0);
                builder.mstore(zero, selector);
                let selector_size = builder.imm_u64(4);
                let head_offset = builder.imm_u64(32);
                builder.mstore(selector_size, head_offset);
                let len_offset = builder.imm_u64(36);
                builder.mstore(len_offset, len);
                let data_offset = builder.imm_u64(68);
                builder.mstore(data_offset, data);
                let size = builder.imm_u64(100);
                builder.revert(zero, size);
            }
            module.add_function(func)
        })
    }

    /// Returns the module's shared storage-`bytes`/`string` load helper,
    /// synthesizing it on first use: takes the slot, decodes the packed
    /// short/long form, and returns a fresh `[length][data...]` memory copy.
    /// Marked `no_inline` — the whole point is existing once per module.
    pub(super) fn ensure_load_storage_bytes_helper(&mut self) -> FunctionId {
        if let Some(id) = self.storage_bytes_helper {
            return id;
        }
        let name = Ident::new(sym::__load_storage_bytes, Span::DUMMY);
        let mut func = Function::new(name);
        func.attributes.no_inline = true;
        {
            let mut builder = FunctionBuilder::new(&mut func);
            let slot = builder.add_param(MirType::uint256());
            builder.add_return(MirType::MemoryObject(MemoryObjectKind::Bytes));
            self.synthesizing_helper = true;
            let ptr = self.materialize_storage_bytes_inline(&mut builder, slot);
            self.synthesizing_helper = false;
            builder.ret([ptr]);
        }
        let id = self.module.add_function(func);
        self.storage_bytes_helper = Some(id);
        id
    }

    /// Lowers a public function with the internal-frame calling convention so it
    /// can be called via `internal_call` (e.g. recursion). The result is cached
    /// separately from the external entry; the id is registered before the body
    /// is lowered so the copy's own recursive call resolves to itself.
    pub(super) fn ensure_internal_mir_function(&mut self, func_id: hir::FunctionId) -> FunctionId {
        if let Some(&mir_id) = self.hir_to_internal_mir_functions.get(&func_id) {
            return mir_id;
        }

        let saved_locals = std::mem::take(&mut self.locals);
        let saved_local_memory_slots = std::mem::take(&mut self.local_memory_slots);
        let saved_slice_slot_locals = std::mem::take(&mut self.slice_slot_locals);
        let saved_next_local_memory_offset = self.next_local_memory_offset;
        let saved_assigned_vars = std::mem::take(&mut self.assigned_vars);
        let saved_inline_returns = self.inline_returns.take();
        let saved_pending_inline_returns = self.pending_inline_returns.take();
        let saved_current_contract_id = self.current_contract_id;
        let saved_lowering_constructor = self.lowering_constructor;
        let saved_constructor_args_base = self.constructor_args_base;
        let saved_lowering_internal_function = self.lowering_internal_function;
        let saved_in_unchecked_block = self.in_unchecked_block;
        let saved_current_return_tys = std::mem::take(&mut self.current_return_tys);

        self.current_contract_id = self.gcx.hir.function(func_id).contract;
        self.in_unchecked_block = false;
        let mir_id = self.lower_function(func_id, true);

        self.locals = saved_locals;
        self.local_memory_slots = saved_local_memory_slots;
        self.slice_slot_locals = saved_slice_slot_locals;
        self.next_local_memory_offset = saved_next_local_memory_offset;
        self.assigned_vars = saved_assigned_vars;
        self.inline_returns = saved_inline_returns;
        self.pending_inline_returns = saved_pending_inline_returns;
        self.current_contract_id = saved_current_contract_id;
        self.lowering_constructor = saved_lowering_constructor;
        self.constructor_args_base = saved_constructor_args_base;
        self.lowering_internal_function = saved_lowering_internal_function;
        self.in_unchecked_block = saved_in_unchecked_block;
        self.current_return_tys = saved_current_return_tys;
        mir_id
    }

    /// Lowers a function to MIR. When `force_internal` is set, the function is
    /// lowered with the internal-frame convention (no selector) regardless of its
    /// visibility, and registered in `hir_to_internal_mir_functions`.
    fn lower_function(&mut self, func_id: hir::FunctionId, force_internal: bool) -> FunctionId {
        let check_expr_errors = std::mem::replace(&mut self.check_expr_errors, self.hir_has_errors);
        let hir_func = self.gcx.hir.function(func_id);

        let func_name = hir_func.name.unwrap_or_else(|| Ident::new(sym::_anonymous, Span::DUMMY));

        // Reserve and register the MIR id before lowering the body so recursive
        // self-calls can resolve to this function.
        let mir_id = self.module.add_function(Function::new(func_name));
        if force_internal {
            self.hir_to_internal_mir_functions.insert(func_id, mir_id);
        } else {
            self.hir_to_mir_functions.insert(func_id, mir_id);
        }

        let mut mir_func = Function::new(func_name);

        mir_func.attributes = FunctionAttributes {
            visibility: hir_func.visibility,
            state_mutability: hir_func.state_mutability,
            is_constructor: hir_func.kind == hir::FunctionKind::Constructor,
            is_fallback: hir_func.kind == hir::FunctionKind::Fallback,
            is_receive: hir_func.kind == hir::FunctionKind::Receive,
            is_dispatch_entry: false,
            no_inline: false,
        };

        // Only regular public/external functions get selectors. An internal copy
        // (force_internal) uses the internal-frame convention with no selector.
        // Constructor, receive, and fallback don't have selectors.
        let is_special = mir_func.attributes.is_constructor
            || mir_func.attributes.is_receive
            || mir_func.attributes.is_fallback;
        let uses_external_abi = mir_func.is_public() && !is_special && !force_internal;
        let decodes_abi_params = uses_external_abi || mir_func.attributes.is_constructor;
        if uses_external_abi {
            mir_func.selector = Some(self.function_selector(func_id));
        }
        let uses_internal_frame = !uses_external_abi && !is_special;
        let current_return_tys =
            hir_func.returns.iter().map(|&id| self.gcx.type_of_item(id.into())).collect::<Vec<_>>();

        let external_arg_head_size = if uses_external_abi {
            self.abi_head_size_sum(
                hir_func.parameters.iter().map(|&id| self.gcx.type_of_item(id.into())),
            )
        } else {
            Ok(0)
        };
        let external_static_return_size =
            if uses_external_abi && !current_return_tys.iter().any(|&ty| self.abi_is_dynamic(ty)) {
                self.abi_head_size_sum(current_return_tys.iter().copied())
            } else {
                Ok(0)
            };
        let abi_return_types = if uses_external_abi {
            current_return_tys
                .iter()
                .map(|&ty| self.abi_type(ty, false).ok_or_else(|| self.abi_type_error()))
                .collect::<Result<Vec<_>, _>>()
        } else {
            Ok(Vec::new())
        };
        let (external_arg_head_size, external_static_return_size, abi_return_types) =
            match (external_arg_head_size, external_static_return_size, abi_return_types) {
                (Ok(arg_size), Ok(return_size), Ok(types)) => (arg_size, return_size, types),
                (Err(guar), _, _) | (_, Err(guar), _) | (_, _, Err(guar)) => {
                    let mut builder = FunctionBuilder::new(&mut mir_func);
                    builder.error_value(guar);
                    builder.invalid();
                    mir_func.name = self.module.function(mir_id).name;
                    *self.module.function_mut(mir_id) = mir_func;
                    self.check_expr_errors = check_expr_errors;
                    return mir_id;
                }
            };

        self.locals.clear();
        self.local_memory_slots.clear();
        self.slice_slot_locals.clear();
        self.next_local_memory_offset = EvmMemoryLayout::HEAP_START;
        self.assigned_vars.clear();
        self.lowering_constructor = hir_func.kind == hir::FunctionKind::Constructor;
        self.constructor_args_base = None;
        self.lowering_internal_function = uses_internal_frame;
        self.in_unchecked_block = false;
        self.current_return_tys = current_return_tys;
        if !abi_return_types.is_empty() {
            mir_func.abi_returns =
                Some(self.module.intern_abi_layout(AbiLayout::new(abi_return_types)));
        }

        // Pre-analyze function body to find variables that are assigned after declaration.
        // Variables that are only initialized (never reassigned) can stay as SSA values.
        if let Some(body) = &hir_func.body {
            self.collect_assigned_vars_block(body);
        }

        {
            let mut builder = FunctionBuilder::new(&mut mir_func);

            if uses_external_abi {
                Self::emit_external_calldata_head_size_check(&mut builder, external_arg_head_size);
            }

            // Register the return types before binding parameters. A
            // reassigned parameter's slot address goes through
            // `local_memory_addr`, which spans the complete return area, so a
            // later return registration would shift the address its own reads
            // resolve to.
            for &ret_id in hir_func.returns {
                let ty = self.lower_type_from_var(ret_id);
                builder.add_return(ty);
            }

            let mut deferred_param_slots: Vec<(u64, ValueId)> = Vec::new();
            for &param_id in hir_func.parameters {
                let param = self.gcx.hir.variable(param_id);
                let param_ty = self.gcx.type_of_item(param_id.into());
                let ty = if Self::calldata_dynamic_var_kind(param).is_some() {
                    MirType::Slice(SliceLocation::Calldata)
                } else {
                    self.lower_type_from_var(param_id)
                };

                // Check if this is a struct parameter that needs special handling
                let abi_param_source = if self.lowering_constructor {
                    AbiParamSource::ConstructorMemory
                } else {
                    AbiParamSource::ExternalCalldata
                };

                // Storage-reference parameters (a `mapping`, or a struct/array in
                // `storage` — legal for library functions) travel as their slot:
                // one plain word, never field-expanded from calldata.
                if decodes_abi_params
                    && !self.param_is_storage_ref(param_id)
                    && matches!(param_ty.peel_refs().kind, TyKind::Struct(_))
                    && self.abi_is_dynamic(param_ty)
                {
                    // A struct with a dynamic member is dynamically encoded:
                    // its single head slot holds the offset from the args
                    // start, and every field — including nested dynamic
                    // offsets relative to the struct's own base — lives in
                    // the tail. Rebuild it recursively. Runtime calls read
                    // calldata after the selector; constructors read the
                    // argument blob CODECOPY'd into its backend-owned region.
                    let (source, args_base) = if self.lowering_constructor {
                        (bytes::AbiSource::Memory, self.constructor_args_base(&mut builder))
                    } else {
                        (bytes::AbiSource::Calldata, builder.imm_u64(4))
                    };
                    let offset = builder.add_param(MirType::uint256());
                    let limit = builder.imm_u64(0xffff_ffff_ffff_ffff);
                    let out_of_range = builder.gt(offset, limit);
                    self.emit_abi_decode_revert_if(&mut builder, out_of_range);
                    let base = builder.add(args_base, offset);
                    let struct_ptr =
                        self.materialize_calldata_value_at(&mut builder, source, param_ty, base);
                    self.bind_param_value_deferred(param_id, struct_ptr, &mut deferred_param_slots);
                } else if decodes_abi_params
                    && !self.param_is_storage_ref(param_id)
                    && let TyKind::Struct(struct_id) = param_ty.peel_refs().kind
                {
                    // Struct parameters: copy fields from calldata to memory
                    let strukt = self.gcx.hir.strukt(struct_id);
                    let field_ids = strukt.fields;
                    let num_fields = field_ids.len();
                    let field_tys = self.gcx.struct_field_types(struct_id);

                    // Runtime calls read the inline head after the selector;
                    // constructors read the backend-owned argument blob.
                    let (agg_source, constructor_args_base) = if self.lowering_constructor {
                        (bytes::AbiSource::Memory, Some(self.constructor_args_base(&mut builder)))
                    } else {
                        (bytes::AbiSource::Calldata, None)
                    };

                    // Rebuild every field into its ordinary memory
                    // representation. Dynamic members are memory objects, so
                    // later member and element reads can reuse this copy.
                    let struct_size = num_fields as u64 * EvmMemoryLayout::WORD_SIZE;
                    let struct_size_val = builder.imm_u64(struct_size);
                    let struct_ptr = builder.alloc_object(
                        struct_size_val,
                        crate::mir::MemoryObjectLayout::structure(num_fields as u64),
                        crate::mir::AllocationSemantics::INTERNAL,
                    );

                    // Add MIR params for each struct field (they come from calldata)
                    for (field_idx, &field_id) in field_ids.iter().enumerate() {
                        let Some(&sema_field_ty) = field_tys.get(field_idx) else {
                            self.recovery_error(
                                Some(self.gcx.hir.variable(field_id).span),
                                "codegen cannot determine this struct field's type",
                            );
                            continue;
                        };

                        // A nested static aggregate (struct or fixed array)
                        // occupies several inline head words and is stored as a
                        // pointer to its own allocation. Consume its head words
                        // so following fields slot correctly and rebuild it
                        // recursively from the head region.
                        if matches!(
                            sema_field_ty.peel_refs().kind,
                            TyKind::Struct(_) | TyKind::Array(..) | TyKind::Tuple(_)
                        ) {
                            // Struct field types carry a storage location ref;
                            // peel it so head sizing sees the value type
                            // instead of collapsing to one slot.
                            let field_ty = sema_field_ty.peel_refs();
                            let first_word = builder.func().params.len() as u64;
                            let head_words = match self.abi_head_size(field_ty) {
                                Ok(size) => size / EvmMemoryLayout::WORD_SIZE,
                                Err(guar) => {
                                    builder.error_value(guar);
                                    continue;
                                }
                            };
                            for _ in 0..head_words {
                                builder.add_param(MirType::uint256());
                            }
                            let offset = first_word * EvmMemoryLayout::WORD_SIZE;
                            let pos = if let Some(args_base) = constructor_args_base {
                                let offset = builder.imm_u64(offset);
                                builder.add(args_base, offset)
                            } else {
                                builder.imm_u64(4 + offset)
                            };
                            let field_ptr = self.materialize_calldata_value_at(
                                &mut builder,
                                agg_source,
                                field_ty,
                                pos,
                            );
                            let field_addr = builder.memory_object_field_addr(
                                struct_ptr,
                                crate::mir::MemoryObjectLayout::structure(num_fields as u64),
                                field_idx as u64,
                            );
                            builder.mstore(field_addr, field_ptr);
                            continue;
                        }

                        let arg_index = builder.func().params.len() as u64;
                        let field_ty = MirType::uint256();
                        let field_val = builder.add_param(field_ty);
                        self.emit_abi_param_validation(
                            &mut builder,
                            arg_index,
                            sema_field_ty,
                            abi_param_source,
                        );

                        // A dynamic array/bytes field's head word is the tail
                        // offset relative to the args start: materialize the
                        // `[len][data...]` tail into fresh memory so the body
                        // sees an ordinary memory array/bytes. (A raw word
                        // would be a caller-memory pointer, meaningless here.)
                        let stored_val = match self.linked_field_kind(sema_field_ty) {
                            Some(
                                kind @ (call::LinkedFieldKind::DynArray
                                | call::LinkedFieldKind::DynBytes),
                            ) => {
                                let four = builder.imm_u64(4);
                                let pos = builder.add(four, field_val);
                                let len = builder.calldataload(pos);
                                let word = builder.imm_u64(32);
                                let byte_len = kind.data_size(&mut builder, len, word);
                                let alloc = builder.add(word, byte_len);
                                let object_layout = kind.memory_object_layout();
                                let ptr = builder.alloc_object(
                                    alloc,
                                    object_layout,
                                    crate::mir::AllocationSemantics::INTERNAL,
                                );
                                builder.set_memory_object_len(ptr, len, object_layout.kind());
                                let dst = builder.memory_object_data(ptr, object_layout.kind());
                                let src = builder.add(pos, word);
                                builder.calldatacopy(dst, src, byte_len);
                                ptr
                            }
                            _ => field_val,
                        };

                        // Store the field value into the struct memory
                        let field_addr = builder.memory_object_field_addr(
                            struct_ptr,
                            crate::mir::MemoryObjectLayout::structure(num_fields as u64),
                            field_idx as u64,
                        );
                        builder.mstore(field_addr, stored_val);
                    }

                    // Store the memory pointer as the local (not the Arg value)
                    self.bind_param_value_deferred(param_id, struct_ptr, &mut deferred_param_slots);
                } else if decodes_abi_params
                    && !self.param_is_storage_ref(param_id)
                    && let Some((elem_ty, len)) = self.fixed_word_array_param(param_id)
                {
                    // Fixed-size array of word elements (memory or calldata):
                    // the ABI head is `len` inline words. Add one MIR param per
                    // element and copy them to memory, like struct params.
                    let array_ptr = self.allocate_memory_object(
                        &mut builder,
                        len * 32,
                        MemoryObjectKind::FixedArray,
                    );
                    for elem_idx in 0..len {
                        let arg_index = builder.func().params.len() as u64;
                        let elem_val = builder.add_param(MirType::uint256());
                        self.emit_abi_param_validation(
                            &mut builder,
                            arg_index,
                            elem_ty,
                            abi_param_source,
                        );
                        let elem_index = builder.imm_u64(elem_idx);
                        let elem_addr = builder.memory_object_element_addr(
                            array_ptr,
                            crate::mir::MemoryObjectLayout::word_fixed_array(len),
                            elem_index,
                        );
                        builder.mstore(elem_addr, elem_val);
                    }
                    self.bind_param_value_deferred(param_id, array_ptr, &mut deferred_param_slots);
                } else if decodes_abi_params && self.is_dyn_word_array_memory_param(param_id) {
                    // Dynamic array of word elements in memory: the ABI head is
                    // an offset to `[length][elements...]` in the ABI argument
                    // blob. Runtime calls read it from calldata after the
                    // selector; constructors read it from the copied argument
                    // blob in its backend-owned region.
                    let head = builder.add_param(ty);
                    let abi_base = if self.lowering_constructor {
                        self.constructor_args_base(&mut builder)
                    } else {
                        builder.imm_u64(4)
                    };
                    let len_pos = builder.add(abi_base, head);
                    let len = if self.lowering_constructor {
                        builder.mload(len_pos)
                    } else {
                        builder.calldataload(len_pos)
                    };
                    let word = builder.imm_u64(32);
                    let data_bytes = builder.mul(len, word);
                    let total_bytes = builder.add(data_bytes, word);
                    let array_ptr = builder.alloc_object(
                        total_bytes,
                        crate::mir::MemoryObjectLayout::DynamicArray { element_words: 1 },
                        crate::mir::AllocationSemantics::INTERNAL,
                    );
                    builder.set_memory_object_len(array_ptr, len, MemoryObjectKind::DynamicArray);
                    let dst = builder.memory_object_data(array_ptr, MemoryObjectKind::DynamicArray);
                    let src = builder.add(len_pos, word);
                    if self.lowering_constructor {
                        builder.mcopy(dst, src, data_bytes);
                    } else {
                        builder.calldatacopy(dst, src, data_bytes);
                    }
                    self.bind_param_value_deferred(param_id, array_ptr, &mut deferred_param_slots);
                } else if decodes_abi_params
                    && param.data_location == Some(solar_ast::DataLocation::Memory)
                    && matches!(
                        param_ty.peel_refs().kind,
                        TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
                    )
                {
                    // `bytes`/`string` memory parameter: the ABI head word is
                    // the payload's offset relative to the start of the ABI
                    // arguments. Runtime calls read it from calldata after the
                    // selector; constructors read it from the copied argument
                    // blob in its backend-owned region.
                    let head = builder.add_param(ty);
                    let abi_base = if self.lowering_constructor {
                        self.constructor_args_base(&mut builder)
                    } else {
                        builder.imm_u64(4)
                    };
                    let len_pos = builder.add(abi_base, head);
                    let len = if self.lowering_constructor {
                        builder.mload(len_pos)
                    } else {
                        builder.calldataload(len_pos)
                    };
                    let thirty_one = builder.imm_u64(31);
                    let rounded = builder.add(len, thirty_one);
                    let mask = builder.not(thirty_one);
                    let padded = builder.and(rounded, mask);
                    let word = builder.imm_u64(32);
                    let total = builder.add(padded, word);
                    let ptr = self.allocate_memory_object_dynamic(
                        &mut builder,
                        total,
                        MemoryObjectKind::Bytes,
                    );
                    builder.set_memory_object_len(ptr, len, MemoryObjectKind::Bytes);
                    let data_ptr = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);
                    let src = builder.add(len_pos, word);
                    if self.lowering_constructor {
                        builder.mcopy(data_ptr, src, len);
                    } else {
                        builder.calldatacopy(data_ptr, src, len);
                    }
                    self.bind_param_value_deferred(param_id, ptr, &mut deferred_param_slots);
                } else {
                    // Non-struct parameters: use normal Arg handling
                    let arg_index = builder.func().params.len() as u64;
                    let head_or_value = builder.add_param(ty);
                    if decodes_abi_params {
                        self.emit_abi_param_validation(
                            &mut builder,
                            arg_index,
                            param_ty,
                            abi_param_source,
                        );
                    }
                    let is_reassigned = self.is_var_assigned(&param_id);
                    let is_storage_ref = self.param_is_storage_ref(param_id);
                    if Self::calldata_dynamic_var_kind(param).is_some() && is_reassigned {
                        // A rebindable calldata slice needs one representation
                        // on every CFG path: give it a two-word slot instead
                        // of a lexical SSA binding.
                        let offset = self.alloc_local_slice_memory(param_id);
                        self.store_slice_slot(&mut builder, offset, head_or_value);
                    } else if is_storage_ref && is_reassigned {
                        let offset = self.alloc_local_memory(param_id);
                        deferred_param_slots.push((offset, head_or_value));
                    } else {
                        self.bind_param_value_deferred(
                            param_id,
                            head_or_value,
                            &mut deferred_param_slots,
                        );
                    }
                    // A storage-reference parameter (`mapping`/`storage`) is passed
                    // by slot: its value *is* the base slot, so mark it so mapping
                    // indexing and struct/array reads through it use storage, and
                    // so passing it onward resolves back to the slot.
                    if is_storage_ref {
                        self.storage_ref_locals.insert(param_id);
                    }
                }
            }

            // Every parameter is registered now, so a staged slot address
            // resolves the same way the body's reads will.
            for (offset, value) in std::mem::take(&mut deferred_param_slots) {
                let addr = self.local_memory_addr(&mut builder, offset);
                builder.mstore(addr, value);
            }

            // Initialize named-return slots only after the complete return
            // prefix and parameter area have been registered.
            for &ret_id in hir_func.returns {
                let ret_var = self.gcx.hir.variable(ret_id);
                // An unnamed return cannot be assigned or read by the body.
                // Keep it absent and materialize its default only if control
                // actually reaches the implicit-return epilogue.
                if ret_var.name.is_none() {
                    continue;
                }
                // Allocate memory for return variables so they can be assigned to
                // within the function body (e.g., `liquidity = 1` in if/else branches)
                if Self::calldata_dynamic_var_kind(ret_var).is_some() {
                    let offset = self.alloc_local_slice_memory(ret_id);
                    self.init_empty_slice_slot(&mut builder, offset);
                    continue;
                }

                let offset = self.alloc_local_memory(ret_id);
                let offset_val = self.local_memory_addr(&mut builder, offset);
                if let Some(value) = self.lower_bulk_zero_return_struct(&mut builder, ret_id) {
                    builder.mstore(offset_val, value);
                } else if let Some(value) = self.lower_default_variable_value(&mut builder, ret_id)
                {
                    builder.mstore(offset_val, value);
                }
            }

            if hir_func.kind == hir::FunctionKind::Constructor
                && let Some(contract_id) = hir_func.contract
            {
                self.lower_constructor_prelude(&mut builder, contract_id);
            }

            if let Some(body) = &hir_func.body {
                self.lower_block(&mut builder, body);
            }

            if !builder.func().block(builder.current_block()).is_terminated() {
                if builder.func().returns.is_empty() {
                    builder.stop();
                } else {
                    // Load each return variable's word (the value for value types,
                    // a memory pointer for reference types).
                    let mut items: Vec<(ValueId, Ty<'gcx>)> = Vec::new();
                    for &ret_id in hir_func.returns {
                        let ret_var = self.gcx.hir.variable(ret_id);
                        let ret_val = if let Some(offset) = self.get_local_memory_offset(&ret_id) {
                            if self.is_slice_slot_local(&ret_id) {
                                self.load_slice_slot(
                                    &mut builder,
                                    offset,
                                    crate::mir::SliceLocation::Calldata,
                                )
                            } else {
                                let offset_val = self.local_memory_addr(&mut builder, offset);
                                builder.mload(offset_val)
                            }
                        } else if let Some(value) =
                            self.lower_default_variable_value(&mut builder, ret_id)
                        {
                            value
                        } else {
                            self.err_value(
                                &mut builder,
                                ret_var.span,
                                "codegen is missing a return variable slot",
                            )
                        };
                        items.push((ret_val, self.gcx.type_of_item(ret_id.into())));
                    }
                    self.finish_return(&mut builder, items);
                }
            }
        }

        self.lowering_constructor = false;
        self.lowering_internal_function = false;
        mir_func.internal_frame_size =
            self.next_local_memory_offset.saturating_sub(EvmMemoryLayout::HEAP_START);
        mir_func.external_static_return_size = external_static_return_size;

        mir_func.name = self.module.function(mir_id).name;
        *self.module.function_mut(mir_id) = mir_func;
        self.check_expr_errors = check_expr_errors;
        mir_id
    }

    /// Reverts when calldata does not contain the complete ABI head.
    ///
    /// `calldataload` returns zero for missing bytes, so this guard must run
    /// before parameter validation or short calldata can be accepted as a
    /// canonical zero argument.
    fn emit_external_calldata_head_size_check(builder: &mut FunctionBuilder<'_>, head_size: u64) {
        if head_size == 0 {
            return;
        }
        let calldatasize = builder.calldatasize();
        let selector_size = builder.imm_u64(4);
        let payload_size = builder.sub(calldatasize, selector_size);
        let required_size = builder.imm_u64(head_size);
        let is_short = builder.slt(payload_size, required_size);
        Self::emit_revert_if(builder, is_short);
    }

    /// Validates the ABI encoding of a value-type external parameter.
    ///
    /// Solc via-ir reverts with empty revert data when the calldata word of a
    /// value-type parameter is not its canonical encoding, and downstream code
    /// (including our checked-arithmetic shapes) relies on arguments being
    /// canonical. We mirror solc's `validator_revert_t_*` semantics:
    /// - `uintN` (N < 256): high bits must be zero
    /// - `intN` (N < 256): the word must equal its sign extension
    /// - `address` / contract types: top 96 bits must be zero
    /// - `bool`: the word must be 0 or 1
    /// - `bytesN` (N < 32): low `32 - N` bytes must be zero
    /// - enums: the value must be less than the member count
    ///
    /// Reference and dynamic types are not validated here.
    ///
    /// The check reads the raw word with an explicit `calldataload` instead of
    /// reusing the `Arg` value: optimization passes are allowed to assume that
    /// `Arg` values of external functions are canonical (this validation is
    /// what establishes that invariant), so the validator itself must read the
    /// unvalidated word opaquely or it would be folded away.
    /// Selects the clean-word validator for a value type decoded from ABI
    /// calldata, if it has one. Value types narrower than a word must equal
    /// their canonical form; wider or reference types have no word validator.
    fn abi_word_validator(&self, ty: Ty<'gcx>) -> Option<AbiWordValidator> {
        let ty = match ty.kind {
            TyKind::Udvt(underlying, _) => underlying,
            _ => ty,
        };
        Some(match ty.kind {
            TyKind::Elementary(elem) => match elem {
                ElementaryType::UInt(size) => {
                    let bits = size.bits();
                    if bits >= 256 {
                        return None;
                    }
                    AbiWordValidator::Mask(U256::MAX >> (256 - usize::from(bits)))
                }
                ElementaryType::Int(size) => {
                    let bits = size.bits();
                    if bits >= 256 {
                        return None;
                    }
                    AbiWordValidator::SignExtend(u64::from(bits / 8) - 1)
                }
                ElementaryType::Address(_) => AbiWordValidator::Mask(U256::MAX >> 96),
                ElementaryType::Bool => AbiWordValidator::Bool,
                ElementaryType::FixedBytes(size) => {
                    let bytes = size.bytes();
                    if bytes >= 32 {
                        return None;
                    }
                    AbiWordValidator::Mask(U256::MAX << (256 - 8 * usize::from(bytes)))
                }
                _ => return None,
            },
            TyKind::Contract(_) => AbiWordValidator::Mask(U256::MAX >> 96),
            TyKind::Enum(enum_id) => {
                AbiWordValidator::EnumRange(self.gcx.hir.enumm(enum_id).variants.len() as u64)
            }
            _ => return None,
        })
    }

    /// Reverts when `word` is not the canonical encoding for `validator`.
    fn emit_abi_word_clean_check(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        word: ValueId,
        validator: AbiWordValidator,
    ) {
        let ok = validator.condition(builder, word);
        Self::emit_revert_unless(builder, ok);
    }

    /// Validates a value-typed field decoded from ABI calldata at `word`. A
    /// dirty narrow value reverts, matching solc's decode.
    pub(super) fn emit_abi_field_clean_check(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: Ty<'gcx>,
        word: ValueId,
    ) {
        if let Some(validator) = self.abi_word_validator(ty) {
            self.emit_abi_word_clean_check(builder, word, validator);
        }
    }

    fn emit_abi_param_validation(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        arg_index: u64,
        ty: Ty<'gcx>,
        source: AbiParamSource,
    ) {
        let Some(validator) = self.abi_word_validator(ty) else { return };

        let word = source.load(self, builder, arg_index);
        self.emit_abi_word_clean_check(builder, word, validator);
    }

    /// Branches to a plain `revert(0, 0)` when `cond` is zero, then continues
    /// lowering in the fallthrough block.
    fn emit_revert_unless(builder: &mut FunctionBuilder<'_>, cond: ValueId) {
        let revert_block = builder.create_block();
        let continue_block = builder.create_block();
        builder.branch(cond, continue_block, revert_block);

        builder.switch_to_block(revert_block);
        let zero = builder.imm_u64(0);
        builder.revert(zero, zero);

        builder.switch_to_block(continue_block);
    }

    /// Reverts with empty data when `cond` is true, continuing otherwise.
    /// Branching directly on the condition avoids an `iszero` polarity flip.
    fn emit_revert_if(builder: &mut FunctionBuilder<'_>, cond: ValueId) {
        let revert_block = builder.create_block();
        let continue_block = builder.create_block();
        builder.branch(cond, revert_block, continue_block);

        builder.switch_to_block(revert_block);
        let zero = builder.imm_u64(0);
        builder.revert(zero, zero);

        builder.switch_to_block(continue_block);
    }

    /// Lowers state-variable initializers and base constructors for an explicit constructor.
    fn lower_constructor_prelude(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        contract_id: ContractId,
    ) {
        let contract = self.gcx.hir.contract(contract_id);
        let mut constructor_args = FxHashMap::default();

        let construction_order = contract.linearized_bases;

        // State variables are initialized from the most base contract to the
        // most derived contract before constructor argument expressions run.
        for &base_id in construction_order.iter().rev() {
            let base_contract = self.gcx.hir.contract(base_id);
            for var_id in base_contract.variables() {
                let var = self.gcx.hir.variable(var_id);
                if var.is_state_variable()
                    && !var.is_constant()
                    && let Some(init) = var.initializer
                {
                    let init_val = self.lower_value_expr(builder, init);
                    if let Some(&id) = self.immutable_ids.get(&var_id) {
                        builder.store_immutable(id, init_val);
                    } else if let Some(&location) = self.storage_locations.get(&var_id) {
                        self.store_storage_location(builder, location, init_val);
                    }
                }
            }
        }

        // Base constructor arguments are evaluated in the derived contract's
        // linearized order, independently of constructor body execution.
        for &base_id in construction_order.iter().skip(1) {
            if self.gcx.hir.contract(base_id).ctor.is_some()
                && self
                    .lower_base_constructor_arguments(
                        builder,
                        contract_id,
                        base_id,
                        &mut constructor_args,
                    )
                    .is_err()
            {
                return;
            }
        }

        // Argument expressions for an indirect base may refer to the
        // constructor parameters of the contract which supplied them. Those
        // bindings are only needed while resolving the full argument chain.
        for &base_id in constructor_args.keys() {
            if let Some(ctor_id) = self.gcx.hir.contract(base_id).ctor {
                for &param_id in self.gcx.hir.function(ctor_id).parameters {
                    self.locals.remove(&param_id);
                }
            }
        }

        // Constructor bodies execute from the most base contract to the most
        // derived. The current contract's body is lowered by the caller.
        for &base_id in construction_order.iter().rev() {
            if base_id != contract_id
                && let Some(ctor_id) = self.gcx.hir.contract(base_id).ctor
            {
                let Some(ConstructorArguments::Resolved(arg_values)) =
                    constructor_args.get(&base_id)
                else {
                    self.recovery_error(
                        Some(self.gcx.hir.contract(base_id).span),
                        "base constructor arguments were not resolved",
                    );
                    return;
                };
                self.lower_base_constructor_call(builder, ctor_id, arg_values);
            }
        }
    }

    fn lower_base_constructor_arguments(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        contract_id: ContractId,
        base_id: ContractId,
        values: &mut FxHashMap<ContractId, ConstructorArguments>,
    ) -> Result<(), ErrorGuaranteed> {
        match values.entry(base_id) {
            Entry::Occupied(entry) => match entry.get() {
                ConstructorArguments::Resolved(_) => return Ok(()),
                ConstructorArguments::Resolving => {
                    return Err(self
                        .gcx
                        .dcx()
                        .err("cyclic base constructor arguments during codegen")
                        .span(self.gcx.hir.contract(base_id).span)
                        .emit());
                }
            },
            Entry::Vacant(entry) => {
                entry.insert(ConstructorArguments::Resolving);
            }
        }

        let linearized_bases = self.gcx.hir.contract(contract_id).linearized_bases;
        let provider = linearized_bases.iter().copied().find_map(|declaring_id| {
            let modifier = {
                let declaring = self.gcx.hir.contract(declaring_id);
                declaring
                    .linearized_bases
                    .iter()
                    .skip(1)
                    .copied()
                    .zip(declaring.linearized_bases_args.iter().copied())
                    .find_map(|(candidate_id, modifier)| {
                        (candidate_id == base_id).then_some(modifier).flatten()
                    })
            };
            modifier
                .filter(|modifier| !modifier.args.is_dummy())
                .map(|modifier| (declaring_id, modifier))
        });

        let Some((declaring_id, modifier)) = provider else {
            let base = self.gcx.hir.contract(base_id);
            let parameters =
                base.ctor.map_or(&[][..], |ctor_id| self.gcx.hir.function(ctor_id).parameters);
            if parameters.is_empty() {
                values.insert(base_id, ConstructorArguments::Resolved(SmallVec::new()));
                return Ok(());
            }
            return Err(self
                .gcx
                .dcx()
                .err(format!("could not resolve arguments for base constructor `{}`", base.name))
                .span(self.gcx.hir.contract(contract_id).span)
                .emit());
        };

        if declaring_id != contract_id && self.gcx.hir.contract(declaring_id).ctor.is_some() {
            self.lower_base_constructor_arguments(builder, contract_id, declaring_id, values)?;
        }

        let Some(ctor_id) = self.gcx.hir.contract(base_id).ctor else {
            return Err(self.recovery_error(
                Some(modifier.span),
                "base constructor arguments require a constructor",
            ));
        };
        let parameters = self.gcx.hir.function(ctor_id).parameters;
        if modifier.args.len() != parameters.len() {
            return Err(self
                .gcx
                .dcx()
                .err("could not resolve base constructor arguments during codegen")
                .span(modifier.span)
                .emit());
        }

        let mut arg_values = SmallVec::new();
        let arguments = self.ordered_args_for(
            &modifier.args,
            Some(CallableParamSource::Function { id: ctor_id, skips_receiver: false }),
        )?;
        for (&param_id, argument) in parameters.iter().zip(arguments) {
            let param = self.gcx.hir.variable(param_id);
            let value = self.lower_constructor_arg(builder, argument, &param.ty);
            self.locals.insert(param_id, value);
            arg_values.push(value);
        }
        values.insert(base_id, ConstructorArguments::Resolved(arg_values));

        Ok(())
    }

    fn function_selector(&self, func_id: HirFunctionId) -> [u8; 4] {
        self.gcx.function_selector(func_id).0
    }

    /// Returns the nonzero runtime discriminator for an internal function.
    fn internal_function_pointer_id(func_id: HirFunctionId) -> u64 {
        u64::try_from(func_id.index()).expect("function index does not fit in u64") + 1
    }

    /// Lowers a type from a variable declaration.
    fn lower_type_from_var(&self, var_id: VariableId) -> MirType {
        self.lower_type_from_ty(self.gcx.type_of_item(var_id.into()))
    }

    /// Lowers a type-checked Solidity type to MIR's coarse value type.
    fn lower_type_from_ty(&self, ty: Ty<'gcx>) -> MirType {
        match ty.peel_refs().kind {
            TyKind::Elementary(elem) => match elem {
                ElementaryType::Bool => MirType::Bool,
                ElementaryType::Address(_) => MirType::Address,
                ElementaryType::Int(size) => MirType::Int(TypeSize::new_int_bits(size.bits())),
                ElementaryType::UInt(size) => MirType::UInt(TypeSize::new_int_bits(size.bits())),
                ElementaryType::Fixed(size, _) => MirType::Int(TypeSize::new_int_bits(size.bits())),
                ElementaryType::UFixed(size, _) => {
                    MirType::UInt(TypeSize::new_int_bits(size.bits()))
                }
                ElementaryType::FixedBytes(size) => MirType::FixedBytes(size),
                ElementaryType::String | ElementaryType::Bytes => {
                    MirType::MemoryObject(MemoryObjectKind::Bytes)
                }
            },
            TyKind::Mapping(_, _) => MirType::StoragePtr,
            TyKind::DynArray(_) | TyKind::Slice(_) => {
                MirType::MemoryObject(MemoryObjectKind::DynamicArray)
            }
            TyKind::Array(_, _) => MirType::MemoryObject(MemoryObjectKind::FixedArray),
            TyKind::Fn(_) => MirType::Function,
            TyKind::Struct(_) => MirType::MemoryObject(MemoryObjectKind::Struct),
            TyKind::Enum(_) => MirType::UInt(TypeSize::new_int_bits(8)),
            TyKind::Udvt(underlying, _) => self.lower_type_from_ty(underlying),
            TyKind::Contract(_) | TyKind::Super(_) => MirType::Address,
            TyKind::StringLiteral(_, _)
            | TyKind::IntLiteral(_, _, _)
            | TyKind::Tuple(_)
            | TyKind::Variadic
            | TyKind::Error(_, _)
            | TyKind::Event(_, _)
            | _ => MirType::uint256(),
        }
    }

    /// Returns the completed module.
    #[must_use]
    pub(crate) fn finish(mut self) -> Module {
        self.generate_internal_function_pointer_dispatchers();
        self.module
    }

    /// Collects variables that are assigned after declaration in a block.
    fn collect_assigned_vars_block(&mut self, block: &hir::Block<'_>) {
        for stmt in block.stmts {
            self.collect_assigned_vars_stmt(stmt);
        }
    }

    /// Collects variables that are assigned after declaration in a statement.
    fn collect_assigned_vars_stmt(&mut self, stmt: &hir::Stmt<'_>) {
        use hir::StmtKind;
        match &stmt.kind {
            StmtKind::Expr(expr) => self.collect_assigned_vars_expr(expr),
            StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => {
                self.collect_assigned_vars_block(block)
            }
            StmtKind::If(cond, then_stmt, else_stmt) => {
                self.collect_assigned_vars_expr(cond);
                self.collect_assigned_vars_stmt(then_stmt);
                if let Some(else_s) = else_stmt {
                    self.collect_assigned_vars_stmt(else_s);
                }
            }
            StmtKind::Loop(block, _) => self.collect_assigned_vars_block(block),
            StmtKind::Switch(switch) => {
                self.collect_assigned_vars_expr(switch.selector);
                for case in switch.cases {
                    self.collect_assigned_vars_block(&case.body);
                }
            }
            StmtKind::Return(Some(expr)) | StmtKind::Revert(expr) | StmtKind::Emit(expr) => {
                self.collect_assigned_vars_expr(expr)
            }
            StmtKind::Try(try_stmt) => {
                self.collect_assigned_vars_expr(&try_stmt.expr);
                for clause in try_stmt.clauses {
                    self.collect_assigned_vars_block(&clause.block);
                }
            }
            StmtKind::AssemblyBlock(block) => self.collect_assigned_vars_block(block),
            StmtKind::DeclSingle(_)
            | StmtKind::DeclMulti(_, _)
            | StmtKind::Return(None)
            | StmtKind::Continue
            | StmtKind::Break
            | StmtKind::Placeholder
            | StmtKind::Err(_) => {}
        }
    }

    /// Collects variables that are assigned in an expression.
    fn collect_assigned_vars_expr(&mut self, expr: &hir::Expr<'_>) {
        use hir::ExprKind;
        match &expr.kind {
            ExprKind::Assign(lhs, _, rhs) => {
                // Record assignment targets
                self.mark_assigned_var(lhs);
                self.collect_assigned_vars_expr(rhs);
            }
            ExprKind::Binary(lhs, _, rhs) => {
                self.collect_assigned_vars_expr(lhs);
                self.collect_assigned_vars_expr(rhs);
            }
            ExprKind::Unary(op, operand) => {
                // ++x, x++, --x, x-- are unary ops that mutate the operand
                use solar_ast::UnOpKind;
                if matches!(
                    op.kind,
                    UnOpKind::PreInc | UnOpKind::PostInc | UnOpKind::PreDec | UnOpKind::PostDec
                ) {
                    self.mark_assigned_var(operand);
                }
                self.collect_assigned_vars_expr(operand);
            }
            ExprKind::Ternary(cond, true_val, false_val) => {
                self.collect_assigned_vars_expr(cond);
                self.collect_assigned_vars_expr(true_val);
                self.collect_assigned_vars_expr(false_val);
            }
            ExprKind::Call(callee, args, _) => {
                self.collect_assigned_vars_expr(callee);
                for arg in args.kind.exprs() {
                    self.collect_assigned_vars_expr(arg);
                }
            }
            ExprKind::Index(base, idx) => {
                self.collect_assigned_vars_expr(base);
                if let Some(i) = idx {
                    self.collect_assigned_vars_expr(i);
                }
            }
            ExprKind::Slice(base, start, end) => {
                self.collect_assigned_vars_expr(base);
                if let Some(s) = start {
                    self.collect_assigned_vars_expr(s);
                }
                if let Some(e) = end {
                    self.collect_assigned_vars_expr(e);
                }
            }
            ExprKind::Member(base, _) | ExprKind::YulMember(base, _) => {
                self.collect_assigned_vars_expr(base)
            }
            ExprKind::Array(elems) => {
                for elem in elems.iter() {
                    self.collect_assigned_vars_expr(elem);
                }
            }
            ExprKind::Tuple(elems) => {
                for elem in elems.iter().flatten() {
                    self.collect_assigned_vars_expr(elem);
                }
            }
            ExprKind::Payable(inner) | ExprKind::Delete(inner) => {
                self.collect_assigned_vars_expr(inner)
            }
            ExprKind::New(_)
            | ExprKind::TypeCall(_)
            | ExprKind::Lit(_)
            | ExprKind::Ident(_)
            | ExprKind::Type(_)
            | ExprKind::Err(_) => {}
        }
    }

    /// Marks a variable as being assigned (needs memory storage).
    fn mark_assigned_var(&mut self, expr: &hir::Expr<'_>) {
        // A tuple assignment `(a, b) = ...` assigns every element; missing
        // them here kept the variables SSA-tracked, so a value assigned in
        // one branch arm leaked into the sibling arm's lowering.
        if let hir::ExprKind::Tuple(elements) = &expr.kind {
            for element in elements.iter().copied().flatten() {
                self.mark_assigned_var(element);
            }
            return;
        }
        // A Yul component assignment (`s.offset := ...`, `s.length := ...`,
        // `p.slot := ...`) mutates the base variable, so it too must be tracked
        // as reassigned; otherwise a slice rebuilt in one branch would leak into
        // a sibling arm instead of merging through the variable's slot.
        if let hir::ExprKind::YulMember(base, _) = &expr.kind {
            self.mark_assigned_var(base);
            return;
        }
        if let Some(var_id) = self.gcx.resolved_variable(expr) {
            self.assigned_vars.insert(var_id);
        }
    }

    /// Returns true if a variable is assigned after declaration.
    pub(crate) fn is_var_assigned(&self, var_id: &VariableId) -> bool {
        self.assigned_vars.contains(*var_id)
    }

    /// Binds a parameter's lowered value, mirroring local-declaration lowering:
    /// a reassigned parameter gets a memory slot, everything else stays an SSA
    /// value.
    ///
    /// A parameter reassigned in the body — including in inline assembly, as in
    /// `subject := add(subject, 1)` — needs one representation on every path
    /// and across a loop back edge. A plain SSA binding only updates within a
    /// block, so a sibling branch or the next iteration would read a definition
    /// that cannot reach it. A storage-reference parameter is excluded: its
    /// value *is* a slot, and its uses resolve through `storage_ref_locals`
    /// rather than a memory read.
    /// Like [`Self::bind_param_value`], but records the slot store to emit once
    /// every parameter is registered.
    ///
    /// `local_memory_addr` derives a frame address from the parameter and return
    /// counts, so an address computed while parameters are still being added
    /// resolves differently from the reads that follow. Callers inside the
    /// parameter loop stage their stores and flush them afterwards.
    fn bind_param_value_deferred(
        &mut self,
        param_id: hir::VariableId,
        value: ValueId,
        deferred: &mut Vec<(u64, ValueId)>,
    ) {
        if self.is_var_assigned(&param_id) && !self.param_is_storage_ref(param_id) {
            deferred.push((self.alloc_local_memory(param_id), value));
            return;
        }
        self.locals.insert(param_id, value);
    }

    pub(super) fn bind_param_value(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        param_id: hir::VariableId,
        value: ValueId,
    ) {
        if self.is_var_assigned(&param_id) && !self.param_is_storage_ref(param_id) {
            let offset = self.alloc_local_memory(param_id);
            let addr = self.local_memory_addr(builder, offset);
            builder.mstore(addr, value);
            return;
        }
        self.locals.insert(param_id, value);
    }

    /// Checks if an expression contains an external call.
    /// External calls write their return data to shared memory at offset 0,
    /// so variables initialized from them must be stored in memory to preserve the value
    /// across subsequent calls.
    pub(crate) fn has_external_call(&self, expr: &hir::Expr<'_>) -> bool {
        use hir::ExprKind;
        match &expr.kind {
            ExprKind::Call(callee, args, _) => {
                // Check if this is an external call (method call on a contract)
                if self.is_external_call(callee) {
                    return true;
                }
                // Check callee and arguments for nested external calls
                if self.has_external_call(callee) {
                    return true;
                }
                for arg in args.kind.exprs() {
                    if self.has_external_call(arg) {
                        return true;
                    }
                }
                false
            }
            ExprKind::Member(base, _) | ExprKind::YulMember(base, _) => {
                // Member access itself doesn't contain external calls
                // but the base might
                self.has_external_call(base)
            }
            ExprKind::Binary(lhs, _, rhs) => {
                self.has_external_call(lhs) || self.has_external_call(rhs)
            }
            ExprKind::Unary(_, operand) => self.has_external_call(operand),
            ExprKind::Ternary(cond, true_val, false_val) => {
                self.has_external_call(cond)
                    || self.has_external_call(true_val)
                    || self.has_external_call(false_val)
            }
            ExprKind::Index(base, idx) => {
                self.has_external_call(base) || idx.is_some_and(|i| self.has_external_call(i))
            }
            ExprKind::Array(elems) => elems.iter().any(|e| self.has_external_call(e)),
            ExprKind::Tuple(elems) => {
                elems.iter().any(|e| e.is_some_and(|expr| self.has_external_call(expr)))
            }
            ExprKind::Payable(inner) | ExprKind::Delete(inner) => self.has_external_call(inner),
            ExprKind::Slice(base, start, end) => {
                self.has_external_call(base)
                    || start.is_some_and(|s| self.has_external_call(s))
                    || end.is_some_and(|e| self.has_external_call(e))
            }
            ExprKind::Assign(lhs, _, rhs) => {
                self.has_external_call(lhs) || self.has_external_call(rhs)
            }
            ExprKind::New(_)
            | ExprKind::TypeCall(_)
            | ExprKind::Lit(_)
            | ExprKind::Ident(_)
            | ExprKind::Type(_)
            | ExprKind::Err(_) => false,
        }
    }

    /// Checks if a call expression is an external call (method on a contract).
    fn is_external_call(&self, callee: &hir::Expr<'_>) -> bool {
        // External calls are Member expressions where the base is a contract
        if let hir::ExprKind::Member(base, _) = &callee.kind
            && let Some(var_id) = self.gcx.resolved_variable(base)
        {
            let var = self.gcx.hir.variable(var_id);
            // This scan tracks declaration-level contract values; struct fields are lowered as
            // member expressions.
            if !var.is_struct_member()
                && matches!(var.ty.kind, hir::TypeKind::Custom(hir::ItemId::Contract(_)))
            {
                return true;
            }
        }
        false
    }
}

/// Lowers a contract from HIR to MIR.
pub fn lower_contract(gcx: Gcx<'_>, contract_id: ContractId) -> Module {
    lower_contract_with_bytecodes(gcx, contract_id, &FxHashMap::default())
}

/// Lowers a contract from HIR to MIR with pre-compiled bytecodes available for `new` expressions.
#[tracing::instrument(name = "mir_lower_contract", level = "debug", skip_all, fields(?contract_id))]
pub fn lower_contract_with_bytecodes(
    gcx: Gcx<'_>,
    contract_id: ContractId,
    child_bytecodes: &FxHashMap<ContractId, Bytes>,
) -> Module {
    let contract = gcx.hir.contract(contract_id);
    let mut lowerer = Lowerer::new(gcx, contract.name);

    // Register all child contract bytecodes
    for (&child_id, bytecode) in child_bytecodes {
        lowerer.register_contract_bytecode(child_id, bytecode.clone());
    }

    lowerer.lower_contract(contract_id);
    lowerer.finish()
}

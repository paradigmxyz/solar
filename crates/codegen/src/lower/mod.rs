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
mod type_query;

use crate::{
    IMMUTABLE_SCRATCH_BASE,
    mir::{
        BlockId, Function, FunctionAttributes, FunctionBuilder, FunctionId, ImmutableSlot, MirType,
        Module, StorageSlot, ValueId,
    },
};
use alloy_primitives::U256;
use solar_data_structures::{
    Never,
    map::{FxHashMap, FxHashSet},
};
use solar_interface::{Ident, Span};
use solar_sema::{
    hir::{self, ContractId, ElementaryType, FunctionId as HirFunctionId, VariableId, Visit},
    ty::{Gcx, Ty, TyKind},
};
use std::ops::ControlFlow;

/// Context for a loop (tracks break/continue targets).
#[derive(Clone, Copy)]
pub struct LoopContext {
    /// Block to jump to on `break`.
    pub break_target: BlockId,
    /// Block to jump to on `continue`.
    pub continue_target: BlockId,
}

#[derive(Clone, Copy)]
enum AbiParamSource {
    ExternalCalldata,
    ConstructorMemory,
}

/// Lowering context for converting HIR to MIR.
pub struct Lowerer<'gcx> {
    /// The global context.
    gcx: Gcx<'gcx>,
    /// The current module being built.
    module: Module,
    /// The current contract being lowered.
    current_contract_id: Option<ContractId>,
    /// Mapping from HIR variable IDs to storage slots.
    storage_slots: FxHashMap<VariableId, u64>,
    /// Next available storage slot.
    next_storage_slot: u64,
    /// Mapping from HIR immutable variable IDs to runtime immutable byte offsets.
    immutable_slots: FxHashMap<VariableId, u64>,
    /// Next available immutable byte offset.
    next_immutable_offset: u64,
    /// Mapping from HIR variable IDs to MIR values (for local variables).
    /// For SSA-style immutable variables (function params and non-mutated locals).
    locals: FxHashMap<VariableId, ValueId>,
    /// Mapping from HIR variable IDs to memory offsets (for mutable locals).
    /// Memory layout: starts at offset 0x80 (after scratch space).
    local_memory_slots: FxHashMap<VariableId, u64>,
    /// Next available memory offset for locals.
    next_local_memory_offset: u64,
    /// Bytecodes of other contracts (for `new` expressions).
    /// Maps contract ID to (deployment_bytecode, data_segment_index).
    contract_bytecodes: FxHashMap<ContractId, (Vec<u8>, usize)>,
    /// Stack of loop contexts for nested loops.
    loop_stack: Vec<LoopContext>,
    /// Variables that are assigned after declaration (need memory storage).
    /// Variables not in this set can be kept as SSA values.
    assigned_vars: FxHashSet<VariableId>,
    /// Local variables that are storage references (pointers). Their value in
    /// `locals` is a storage *slot*, so `r.field` reads `sload(slot + offset)`
    /// and `r.field = v` writes `sstore(slot + offset, v)`, rather than treating
    /// the value as a memory pointer.
    storage_ref_locals: FxHashSet<VariableId>,
    /// Stack of function IDs currently being inlined (for cycle detection).
    inline_stack: Vec<HirFunctionId>,
    /// HIR functions already lowered into this MIR module.
    hir_to_mir_functions: FxHashMap<HirFunctionId, FunctionId>,
    /// Internal-convention copies of public functions, lowered on demand so that
    /// public functions can be called internally/recursively via `internal_call`.
    hir_to_internal_mir_functions: FxHashMap<HirFunctionId, FunctionId>,
    /// Cache of whether a function is (directly) self-recursive.
    recursive_functions: FxHashMap<HirFunctionId, bool>,
    /// Functions currently being lowered on demand.
    lowering_functions: FxHashSet<HirFunctionId>,
    /// Whether the current function body is constructor code.
    lowering_constructor: bool,
    /// Whether local memory slots should be addressed through the internal-call frame.
    lowering_internal_function: bool,
    /// Whether arithmetic should use wrapping Solidity `unchecked` semantics.
    in_unchecked_block: bool,
    /// Sema return types of the function currently being lowered (one per declared
    /// return), used to ABI-encode external returns.
    current_return_tys: Vec<Ty<'gcx>>,
    /// Mapping from struct state variable ID to base storage slot.
    pub struct_storage_base_slots: FxHashMap<VariableId, u64>,
    /// Cached struct field slot offsets: (struct_type_id, field_index) -> slot offset from base.
    pub struct_field_offsets: FxHashMap<(hir::StructId, usize), u64>,
    /// Cached struct field memory offsets: (struct_type_id, field_index) -> byte offset from base.
    pub struct_field_memory_offsets: FxHashMap<(hir::StructId, usize), u64>,
}

impl<'gcx> Lowerer<'gcx> {
    /// Creates a new lowerer.
    pub fn new(gcx: Gcx<'gcx>, name: Ident) -> Self {
        Self {
            gcx,
            module: Module::new(name),
            current_contract_id: None,
            storage_slots: FxHashMap::default(),
            next_storage_slot: 0,
            immutable_slots: FxHashMap::default(),
            next_immutable_offset: 0,
            locals: FxHashMap::default(),
            local_memory_slots: FxHashMap::default(),
            next_local_memory_offset: 0x80, // Start after Solidity's scratch space
            contract_bytecodes: FxHashMap::default(),
            loop_stack: Vec::new(),
            assigned_vars: FxHashSet::default(),
            storage_ref_locals: FxHashSet::default(),
            inline_stack: Vec::new(),
            hir_to_mir_functions: FxHashMap::default(),
            hir_to_internal_mir_functions: FxHashMap::default(),
            recursive_functions: FxHashMap::default(),
            lowering_functions: FxHashSet::default(),
            lowering_constructor: false,
            lowering_internal_function: false,
            in_unchecked_block: false,
            current_return_tys: Vec::new(),
            struct_storage_base_slots: FxHashMap::default(),
            struct_field_offsets: FxHashMap::default(),
            struct_field_memory_offsets: FxHashMap::default(),
        }
    }

    /// Pushes a loop context onto the stack.
    pub fn push_loop(&mut self, ctx: LoopContext) {
        self.loop_stack.push(ctx);
    }

    /// Pops a loop context from the stack.
    pub fn pop_loop(&mut self) {
        self.loop_stack.pop();
    }

    /// Gets the current loop context, if any.
    pub fn current_loop(&self) -> Option<&LoopContext> {
        self.loop_stack.last()
    }

    /// Maximum inline depth to prevent excessive recursion.
    const MAX_INLINE_DEPTH: usize = 32;
    /// Historical base used by local memory slots in external function bodies.
    const LOCAL_MEMORY_BASE: u64 = 0x80;
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
        true
    }

    /// Exits inlining for a function.
    fn exit_inline(&mut self) {
        self.inline_stack.pop();
    }

    /// Allocates a memory slot for a local variable.
    /// Returns the memory offset.
    pub fn alloc_local_memory(&mut self, var_id: VariableId) -> u64 {
        let offset = self.next_local_memory_offset;
        self.next_local_memory_offset += 32; // Each slot is 32 bytes
        self.local_memory_slots.insert(var_id, offset);
        offset
    }

    /// Gets the memory offset for a local variable, if it's stored in memory.
    pub fn get_local_memory_offset(&self, var_id: &VariableId) -> Option<u64> {
        self.local_memory_slots.get(var_id).copied()
    }

    /// Returns the address for a local memory slot in the current lowering context.
    pub fn local_memory_addr(&self, builder: &mut FunctionBuilder<'_>, offset: u64) -> ValueId {
        if self.lowering_internal_function {
            let header_size = 64;
            let arg_size = (builder.func().params.len() as u64) * 32;
            let return_size = (builder.func().returns.len() as u64) * 32;
            let local_offset = offset.saturating_sub(Self::LOCAL_MEMORY_BASE);
            builder.internal_frame_addr(header_size + arg_size + return_size + local_offset)
        } else {
            builder.imm_u64(offset)
        }
    }

    /// Returns the constructor scratch address for an immutable word.
    pub fn immutable_scratch_addr(offset: u64) -> u64 {
        IMMUTABLE_SCRATCH_BASE + offset
    }

    /// Stages an immutable word in constructor memory.
    pub fn store_immutable_value(
        &self,
        builder: &mut FunctionBuilder<'_>,
        offset: u64,
        value: ValueId,
    ) {
        let addr = builder.imm_u64(Self::immutable_scratch_addr(offset));
        builder.mstore(addr, value);
    }

    /// Loads an immutable word.
    ///
    /// Runtime code reads a `PUSH32` placeholder that the constructor patches
    /// with the staged value before returning the runtime code. The running
    /// constructor's own placeholders are never patched, so constructor-context
    /// reads load the staged scratch word instead.
    pub fn load_immutable_value(&self, builder: &mut FunctionBuilder<'_>, offset: u64) -> ValueId {
        if self.lowering_constructor {
            let addr = builder.imm_u64(Self::immutable_scratch_addr(offset));
            builder.mload(addr)
        } else {
            builder.load_immutable(offset)
        }
    }

    /// Registers a contract's bytecode for use in `new` expressions.
    pub fn register_contract_bytecode(&mut self, contract_id: ContractId, bytecode: Vec<u8>) {
        let segment_idx = self.module.add_data_segment(bytecode.clone());
        self.contract_bytecodes.insert(contract_id, (bytecode, segment_idx));
    }

    /// Gets the bytecode for a contract, if registered.
    pub fn get_contract_bytecode(&self, contract_id: ContractId) -> Option<&(Vec<u8>, usize)> {
        self.contract_bytecodes.get(&contract_id)
    }

    /// Lowers a contract to MIR.
    pub fn lower_contract(&mut self, contract_id: ContractId) {
        let contract = self.gcx.hir.contract(contract_id);

        // Track the current contract for using directive resolution.
        self.current_contract_id = Some(contract_id);

        // Mark interfaces - they don't generate deployable bytecode.
        if contract.kind == hir::ContractKind::Interface {
            self.module.is_interface = true;
        }

        self.allocate_storage(contract_id);

        // Collect all functions from the inheritance chain, handling overrides.
        // Functions are collected from most-derived to most-base, so if a function
        // with the same selector already exists, we skip the base version.
        let functions = self.collect_inherited_functions(contract_id);

        // Generate a constructor for inherited construction/state-variable
        // initialization when the current contract does not declare one.
        if contract.ctor.is_none() {
            self.generate_synthetic_constructor(contract_id);
        }

        for func_id in functions {
            self.ensure_function_lowered(func_id);
        }

        self.current_contract_id = None;
    }

    /// Collects all functions from the inheritance chain, handling overrides.
    ///
    /// Functions from more-derived contracts take precedence over base contracts.
    /// For regular functions, we use the selector to determine uniqueness.
    /// For constructor/fallback/receive, we use the function kind.
    fn collect_inherited_functions(&self, contract_id: ContractId) -> Vec<HirFunctionId> {
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
                            // Internal functions: use function identity
                            // For simplicity, we include internal functions from all bases
                            // (they won't have selectors in the dispatcher anyway)
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
        let ctor_name = Ident::new(
            solar_interface::Symbol::intern("constructor"),
            solar_interface::Span::DUMMY,
        );
        let mut mir_func = Function::new(ctor_name);
        mir_func.attributes = FunctionAttributes {
            visibility: hir::Visibility::Public,
            state_mutability: hir::StateMutability::NonPayable,
            is_constructor: true,
            is_fallback: false,
            is_receive: false,
        };

        {
            let mut builder = FunctionBuilder::new(&mut mir_func);
            let saved_lowering_constructor = self.lowering_constructor;
            let saved_lowering_internal_function = self.lowering_internal_function;
            let saved_in_unchecked_block = self.in_unchecked_block;
            let saved_current_return_tys = std::mem::take(&mut self.current_return_tys);
            self.lowering_constructor = true;
            self.lowering_internal_function = false;
            self.in_unchecked_block = false;

            self.lower_constructor_prelude(&mut builder, contract_id);
            builder.stop();
            self.lowering_constructor = saved_lowering_constructor;
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
                // Constants are inlined. Immutables are patched into the
                // runtime code's `PUSH32` placeholders at deploy time.
                if var.is_state_variable() && var.is_immutable() {
                    let offset = self.next_immutable_offset;
                    self.next_immutable_offset += 32;
                    self.immutable_slots.insert(var_id, offset);

                    let mir_ty = self.lower_type_from_var(var);
                    self.module.add_immutable_slot(ImmutableSlot {
                        offset,
                        ty: mir_ty,
                        name: var.name,
                    });
                } else if var.is_state_variable() && !var.is_constant() {
                    let base_slot = self.next_storage_slot;

                    // Calculate how many slots this variable needs
                    let num_slots = self.calculate_storage_slots_for_type(&var.ty);
                    self.next_storage_slot += num_slots;

                    // Track struct base slots for field access
                    if let hir::TypeKind::Custom(hir::ItemId::Struct(_)) = &var.ty.kind {
                        self.struct_storage_base_slots.insert(var_id, base_slot);
                    }

                    self.storage_slots.insert(var_id, base_slot);

                    let mir_ty = self.lower_type_from_var(var);
                    self.module.add_storage_slot(StorageSlot {
                        slot: base_slot,
                        offset: 0,
                        ty: mir_ty,
                        name: var.name,
                    });
                }
            }
        }
    }

    /// Calculates the number of storage slots needed for a type.
    fn calculate_storage_slots_for_type(&self, ty: &hir::Type<'_>) -> u64 {
        match &ty.kind {
            hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) => {
                let strukt = self.gcx.hir.strukt(*struct_id);
                let mut total = 0u64;
                for &field_id in strukt.fields {
                    let field = self.gcx.hir.variable(field_id);
                    total += self.calculate_storage_slots_for_type(&field.ty);
                }
                total.max(1)
            }
            // Fixed-size arrays occupy one slot per element (no packing),
            // starting at the base slot. Dynamic arrays keep one length slot.
            hir::TypeKind::Array(arr) if arr.size.is_some() => {
                if let solar_sema::ty::TyKind::Array(_, len) =
                    self.gcx.type_of_hir_ty(ty).peel_refs().kind
                {
                    let elem_slots = self.calculate_storage_slots_for_type(&arr.element);
                    match u64::try_from(len).ok().and_then(|len| len.checked_mul(elem_slots)) {
                        Some(slots) => slots.max(1),
                        None => {
                            self.gcx
                                .dcx()
                                .err("fixed-size storage arrays this large are not supported")
                                .span(ty.span)
                                .emit();
                            1
                        }
                    }
                } else {
                    1
                }
            }
            _ => 1,
        }
    }

    /// Returns the constant length of a fixed-size array parameter whose elements are single
    /// ABI words, for prologue decoding. Other parameter shapes return `None`.
    fn fixed_word_array_param_len(&self, param: &hir::Variable<'_>) -> Option<u64> {
        let hir::TypeKind::Array(arr) = &param.ty.kind else { return None };
        arr.size.as_ref()?;
        let solar_sema::ty::TyKind::Array(elem, len) =
            self.gcx.type_of_hir_ty(&param.ty).peel_refs().kind
        else {
            return None;
        };
        (self.abi_is_word_element(elem) && len <= alloy_primitives::U256::from(u16::MAX))
            .then(|| len.to::<u64>())
    }

    /// Whether a parameter is a memory-located dynamic array of single-word elements, which
    /// the prologue decodes from calldata into Solidity's `[length][data...]` memory layout.
    fn is_dyn_word_array_memory_param(&self, param: &hir::Variable<'_>) -> bool {
        if param.data_location != Some(solar_ast::DataLocation::Memory) {
            return false;
        }
        let hir::TypeKind::Array(arr) = &param.ty.kind else { return false };
        if arr.size.is_some() {
            return false;
        }
        match self.gcx.type_of_hir_ty(&param.ty).peel_refs().kind {
            solar_sema::ty::TyKind::DynArray(elem) => self.abi_is_word_element(elem),
            _ => false,
        }
    }

    /// Gets the storage slot offset for a struct field.
    pub fn get_struct_field_slot_offset(
        &mut self,
        struct_id: hir::StructId,
        field_index: usize,
    ) -> u64 {
        if let Some(&offset) = self.struct_field_offsets.get(&(struct_id, field_index)) {
            return offset;
        }

        let strukt = self.gcx.hir.strukt(struct_id);
        let mut offset = 0u64;
        for (i, &field_id) in strukt.fields.iter().enumerate() {
            if i == field_index {
                break;
            }
            let field = self.gcx.hir.variable(field_id);
            offset += self.calculate_storage_slots_for_type(&field.ty);
        }

        self.struct_field_offsets.insert((struct_id, field_index), offset);
        offset
    }

    /// Calculates the number of 32-byte memory words needed for a type (flattened for structs).
    pub fn calculate_memory_words_for_type(&self, ty: &hir::Type<'_>) -> u64 {
        match &ty.kind {
            hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) => {
                let strukt = self.gcx.hir.strukt(*struct_id);
                let mut total = 0u64;
                for &field_id in strukt.fields {
                    let field = self.gcx.hir.variable(field_id);
                    total += self.calculate_memory_words_for_type(&field.ty);
                }
                total.max(1)
            }
            _ => 1,
        }
    }

    /// Gets the memory byte offset for a struct field.
    pub fn get_struct_field_memory_offset(
        &mut self,
        struct_id: hir::StructId,
        field_index: usize,
    ) -> u64 {
        if let Some(&offset) = self.struct_field_memory_offsets.get(&(struct_id, field_index)) {
            return offset;
        }

        let offset = (field_index as u64) * 32;

        self.struct_field_memory_offsets.insert((struct_id, field_index), offset);
        offset
    }

    /// Recursively copies a struct from storage to memory.
    /// Handles nested structs by flattening them into contiguous memory.
    /// Returns the next memory offset after all fields are copied.
    pub fn copy_storage_to_memory(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        struct_id: hir::StructId,
        base_slot: u64,
        mem_ptr: ValueId,
        mem_offset: u64,
    ) -> u64 {
        let strukt = self.gcx.hir.strukt(struct_id);
        let mut current_slot_offset = 0u64;
        let mut current_mem_offset = mem_offset;

        for &field_id in strukt.fields {
            let field = self.gcx.hir.variable(field_id);

            if let hir::TypeKind::Custom(hir::ItemId::Struct(inner_struct_id)) = &field.ty.kind {
                // Recursively copy nested struct
                current_mem_offset = self.copy_storage_to_memory(
                    builder,
                    *inner_struct_id,
                    base_slot + current_slot_offset,
                    mem_ptr,
                    current_mem_offset,
                );
                current_slot_offset += self.calculate_storage_slots_for_type(&field.ty);
            } else {
                // Copy scalar field: SLOAD from storage, MSTORE to memory
                let slot = base_slot + current_slot_offset;
                let slot_val = builder.imm_u64(slot);
                let field_val = builder.sload(slot_val);

                if current_mem_offset == 0 {
                    builder.mstore(mem_ptr, field_val);
                } else {
                    let offset_val = builder.imm_u64(current_mem_offset);
                    let field_addr = builder.add(mem_ptr, offset_val);
                    builder.mstore(field_addr, field_val);
                }

                current_slot_offset += 1;
                current_mem_offset += 32;
            }
        }

        current_mem_offset
    }

    /// Recursively copies a struct from memory to storage.
    /// Handles nested structs by reading from flattened memory layout.
    /// Returns the next memory offset after all fields are read.
    pub fn copy_memory_to_storage(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        struct_id: hir::StructId,
        base_slot: u64,
        mem_ptr: ValueId,
        mem_offset: u64,
    ) -> u64 {
        let strukt = self.gcx.hir.strukt(struct_id);
        let mut current_slot_offset = 0u64;
        let mut current_mem_offset = mem_offset;

        for &field_id in strukt.fields {
            let field = self.gcx.hir.variable(field_id);

            if let hir::TypeKind::Custom(hir::ItemId::Struct(inner_struct_id)) = &field.ty.kind {
                // Recursively copy nested struct
                current_mem_offset = self.copy_memory_to_storage(
                    builder,
                    *inner_struct_id,
                    base_slot + current_slot_offset,
                    mem_ptr,
                    current_mem_offset,
                );
                current_slot_offset += self.calculate_storage_slots_for_type(&field.ty);
            } else {
                // Copy scalar field: MLOAD from memory, SSTORE to storage
                let slot = base_slot + current_slot_offset;
                let slot_val = builder.imm_u64(slot);

                let field_val = if current_mem_offset == 0 {
                    builder.mload(mem_ptr)
                } else {
                    let offset_val = builder.imm_u64(current_mem_offset);
                    let field_addr = builder.add(mem_ptr, offset_val);
                    builder.mload(field_addr)
                };

                builder.sstore(slot_val, field_val);

                current_slot_offset += 1;
                current_mem_offset += 32;
            }
        }

        current_mem_offset
    }

    /// Lowers a function to MIR.
    pub(super) fn ensure_function_lowered(&mut self, func_id: hir::FunctionId) -> FunctionId {
        if let Some(&mir_id) = self.hir_to_mir_functions.get(&func_id) {
            return mir_id;
        }

        if self.lowering_functions.contains(&func_id) {
            return self.module.add_function(Function::new(Ident::new(
                solar_interface::Symbol::intern("_recursive_internal"),
                solar_interface::Span::DUMMY,
            )));
        }

        let saved_locals = std::mem::take(&mut self.locals);
        let saved_local_memory_slots = std::mem::take(&mut self.local_memory_slots);
        let saved_next_local_memory_offset = self.next_local_memory_offset;
        let saved_assigned_vars = std::mem::take(&mut self.assigned_vars);
        let saved_current_contract_id = self.current_contract_id;
        let saved_lowering_constructor = self.lowering_constructor;
        let saved_lowering_internal_function = self.lowering_internal_function;
        let saved_in_unchecked_block = self.in_unchecked_block;
        let saved_current_return_tys = std::mem::take(&mut self.current_return_tys);

        self.lowering_functions.insert(func_id);
        self.current_contract_id = self.gcx.hir.function(func_id).contract;
        self.in_unchecked_block = false;
        let mir_id = self.lower_function(func_id, false);
        self.lowering_functions.remove(&func_id);

        self.locals = saved_locals;
        self.local_memory_slots = saved_local_memory_slots;
        self.next_local_memory_offset = saved_next_local_memory_offset;
        self.assigned_vars = saved_assigned_vars;
        self.current_contract_id = saved_current_contract_id;
        self.lowering_constructor = saved_lowering_constructor;
        self.lowering_internal_function = saved_lowering_internal_function;
        self.in_unchecked_block = saved_in_unchecked_block;
        self.current_return_tys = saved_current_return_tys;

        mir_id
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
        let saved_next_local_memory_offset = self.next_local_memory_offset;
        let saved_assigned_vars = std::mem::take(&mut self.assigned_vars);
        let saved_current_contract_id = self.current_contract_id;
        let saved_lowering_constructor = self.lowering_constructor;
        let saved_lowering_internal_function = self.lowering_internal_function;
        let saved_in_unchecked_block = self.in_unchecked_block;
        let saved_current_return_tys = std::mem::take(&mut self.current_return_tys);

        self.current_contract_id = self.gcx.hir.function(func_id).contract;
        self.in_unchecked_block = false;
        let mir_id = self.lower_function(func_id, true);

        self.locals = saved_locals;
        self.local_memory_slots = saved_local_memory_slots;
        self.next_local_memory_offset = saved_next_local_memory_offset;
        self.assigned_vars = saved_assigned_vars;
        self.current_contract_id = saved_current_contract_id;
        self.lowering_constructor = saved_lowering_constructor;
        self.lowering_internal_function = saved_lowering_internal_function;
        self.in_unchecked_block = saved_in_unchecked_block;
        self.current_return_tys = saved_current_return_tys;

        mir_id
    }

    /// Lowers a function to MIR. When `force_internal` is set, the function is
    /// lowered with the internal-frame convention (no selector) regardless of its
    /// visibility, and registered in `hir_to_internal_mir_functions`.
    fn lower_function(&mut self, func_id: hir::FunctionId, force_internal: bool) -> FunctionId {
        let hir_func = self.gcx.hir.function(func_id);

        let func_name = hir_func.name.unwrap_or_else(|| {
            Ident::new(solar_interface::Symbol::intern("_anonymous"), solar_interface::Span::DUMMY)
        });

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

        self.locals.clear();
        self.local_memory_slots.clear();
        self.next_local_memory_offset = 0x80;
        self.assigned_vars.clear();
        self.lowering_constructor = hir_func.kind == hir::FunctionKind::Constructor;
        self.lowering_internal_function = uses_internal_frame;
        self.in_unchecked_block = false;
        self.current_return_tys = hir_func
            .returns
            .iter()
            .map(|&id| self.gcx.type_of_hir_ty(&self.gcx.hir.variable(id).ty))
            .collect();

        // Pre-analyze function body to find variables that are assigned after declaration.
        // Variables that are only initialized (never reassigned) can stay as SSA values.
        if let Some(body) = &hir_func.body {
            self.collect_assigned_vars_block(body);
        }

        let external_arg_head_size = if uses_external_abi {
            hir_func
                .parameters
                .iter()
                .map(|&id| {
                    let param = self.gcx.hir.variable(id);
                    let ty = self.gcx.type_of_hir_ty(&param.ty);
                    self.abi_head_size(ty)
                })
                .sum()
        } else {
            0
        };

        {
            let mut builder = FunctionBuilder::new(&mut mir_func);

            if uses_external_abi {
                Self::emit_external_calldata_head_size_check(&mut builder, external_arg_head_size);
            }

            for &param_id in hir_func.parameters {
                let param = self.gcx.hir.variable(param_id);
                let ty = self.lower_type_from_var(param);

                // Check if this is a struct parameter that needs special handling
                let abi_param_source = if self.lowering_constructor {
                    AbiParamSource::ConstructorMemory
                } else {
                    AbiParamSource::ExternalCalldata
                };

                if decodes_abi_params
                    && let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &param.ty.kind
                {
                    // Struct parameters: copy fields from calldata to memory
                    let strukt = self.gcx.hir.strukt(*struct_id);
                    let field_ids = strukt.fields;
                    let num_fields = field_ids.len();

                    // Allocate memory for the struct
                    let struct_size = (num_fields as u64) * 32;
                    let struct_ptr = self.allocate_memory(&mut builder, struct_size);

                    // Add MIR params for each struct field (they come from calldata)
                    for (field_idx, &field_id) in field_ids.iter().enumerate() {
                        let arg_index = builder.func().params.len() as u64;
                        let field_ty = MirType::uint256();
                        let field_val = builder.add_param(field_ty);
                        let field_var = self.gcx.hir.variable(field_id);
                        self.emit_abi_param_validation(
                            &mut builder,
                            arg_index,
                            &field_var.ty,
                            abi_param_source,
                        );

                        // Store the field value into the struct memory
                        let field_offset = (field_idx as u64) * 32;
                        if field_offset == 0 {
                            builder.mstore(struct_ptr, field_val);
                        } else {
                            let offset_val = builder.imm_u64(field_offset);
                            let field_addr = builder.add(struct_ptr, offset_val);
                            builder.mstore(field_addr, field_val);
                        }
                    }

                    // Store the memory pointer as the local (not the Arg value)
                    self.locals.insert(param_id, struct_ptr);
                } else if decodes_abi_params
                    && let Some(len) = self.fixed_word_array_param_len(param)
                {
                    // Fixed-size array of word elements (memory or calldata):
                    // the ABI head is `len` inline words. Add one MIR param per
                    // element and copy them to memory, like struct params.
                    let array_ptr = self.allocate_memory(&mut builder, len * 32);
                    let elem_hir_ty = match &param.ty.kind {
                        hir::TypeKind::Array(array) => &array.element,
                        _ => &param.ty,
                    };
                    for elem_idx in 0..len {
                        let arg_index = builder.func().params.len() as u64;
                        let elem_val = builder.add_param(MirType::uint256());
                        self.emit_abi_param_validation(
                            &mut builder,
                            arg_index,
                            elem_hir_ty,
                            abi_param_source,
                        );
                        if elem_idx == 0 {
                            builder.mstore(array_ptr, elem_val);
                        } else {
                            let offset_val = builder.imm_u64(elem_idx * 32);
                            let elem_addr = builder.add(array_ptr, offset_val);
                            builder.mstore(elem_addr, elem_val);
                        }
                    }
                    self.locals.insert(param_id, array_ptr);
                } else if decodes_abi_params && self.is_dyn_word_array_memory_param(param) {
                    // Dynamic array of word elements in memory: the ABI head is
                    // an offset to `[length][elements...]` in the ABI argument
                    // blob. Runtime calls read it from calldata after the
                    // selector; constructors read it from the copied argument
                    // blob at memory 0x80.
                    let head = builder.add_param(ty);
                    let abi_base =
                        builder.imm_u64(if self.lowering_constructor { 0x80 } else { 4 });
                    let len_pos = builder.add(abi_base, head);
                    let len = if self.lowering_constructor {
                        builder.mload(len_pos)
                    } else {
                        builder.calldataload(len_pos)
                    };
                    let word = builder.imm_u64(32);
                    let data_bytes = builder.mul(len, word);
                    let total_bytes = builder.add(data_bytes, word);
                    let free_ptr_addr = builder.imm_u64(0x40);
                    let array_ptr = builder.mload(free_ptr_addr);
                    let new_free_ptr = builder.add(array_ptr, total_bytes);
                    let free_ptr_addr = builder.imm_u64(0x40);
                    builder.mstore(free_ptr_addr, new_free_ptr);
                    builder.mstore(array_ptr, len);
                    let dst = builder.add(array_ptr, word);
                    let src = builder.add(len_pos, word);
                    if self.lowering_constructor {
                        self.mcopy(&mut builder, dst, src, data_bytes, None);
                    } else {
                        builder.calldatacopy(dst, src, data_bytes);
                    }
                    self.locals.insert(param_id, array_ptr);
                } else if decodes_abi_params
                    && param.data_location == Some(solar_ast::DataLocation::Memory)
                    && matches!(
                        param.ty.kind,
                        hir::TypeKind::Elementary(
                            hir::ElementaryType::Bytes | hir::ElementaryType::String
                        )
                    )
                {
                    // `bytes`/`string` memory parameter: the ABI head word is
                    // the payload's offset relative to the start of the ABI
                    // arguments. Runtime calls read it from calldata after the
                    // selector; constructors read it from the copied argument
                    // blob at memory 0x80.
                    let head = builder.add_param(ty);
                    let abi_base =
                        builder.imm_u64(if self.lowering_constructor { 0x80 } else { 4 });
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
                    let ptr = self.allocate_memory_dynamic(&mut builder, total);
                    builder.mstore(ptr, len);
                    let data_ptr = builder.add(ptr, word);
                    let src = builder.add(len_pos, word);
                    if self.lowering_constructor {
                        self.mcopy(&mut builder, data_ptr, src, len, None);
                    } else {
                        builder.calldatacopy(data_ptr, src, len);
                    }
                    self.locals.insert(param_id, ptr);
                } else {
                    // Non-struct parameters: use normal Arg handling
                    let arg_index = builder.func().params.len() as u64;
                    let val = builder.add_param(ty);
                    if decodes_abi_params {
                        self.emit_abi_param_validation(
                            &mut builder,
                            arg_index,
                            &param.ty,
                            abi_param_source,
                        );
                    }
                    self.locals.insert(param_id, val);
                }
            }

            for &ret_id in hir_func.returns {
                let ret_var = self.gcx.hir.variable(ret_id);
                let ty = self.lower_type_from_var(ret_var);
                builder.add_return(ty);
                // Allocate memory for return variables so they can be assigned to
                // within the function body (e.g., `liquidity = 1` in if/else branches)
                let offset = self.alloc_local_memory(ret_id);
                let offset_val = self.local_memory_addr(&mut builder, offset);
                if let hir::TypeKind::Custom(hir::ItemId::Struct(_)) = &ret_var.ty.kind {
                    let struct_size = self.calculate_memory_words_for_type(&ret_var.ty) * 32;
                    let struct_ptr = self.allocate_memory(&mut builder, struct_size);
                    builder.mstore(offset_val, struct_ptr);
                } else if self.is_fixed_memory_array_type(&ret_var.ty, ret_var.data_location)
                    && let Some(array_ptr) =
                        self.allocate_zeroed_fixed_memory_array(&mut builder, &ret_var.ty)
                {
                    // A named fixed-array return must point at real zeroed
                    // memory like a local declaration; a zero pointer aliases
                    // the scratch space.
                    builder.mstore(offset_val, array_ptr);
                } else {
                    let zero = builder.imm_u256(U256::ZERO);
                    builder.mstore(offset_val, zero);
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
                            let offset_val = self.local_memory_addr(&mut builder, offset);
                            builder.mload(offset_val)
                        } else {
                            builder.imm_u256(U256::ZERO)
                        };
                        items.push((ret_val, self.gcx.type_of_hir_ty(&ret_var.ty)));
                    }
                    self.finish_external_or_internal_return(&mut builder, items, uses_external_abi);
                }
            }
        }

        self.lowering_constructor = false;
        self.lowering_internal_function = false;
        mir_func.internal_frame_size =
            self.next_local_memory_offset.saturating_sub(Self::LOCAL_MEMORY_BASE);
        if uses_external_abi && !self.current_return_tys.iter().any(|&ty| self.abi_is_dynamic(ty)) {
            mir_func.external_static_return_size =
                self.current_return_tys.iter().map(|&ty| self.abi_head_size(ty)).sum();
        }

        *self.module.function_mut(mir_id) = mir_func;
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
    fn emit_abi_param_validation(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        arg_index: u64,
        hir_ty: &hir::Type<'_>,
        source: AbiParamSource,
    ) {
        enum Validator {
            /// The word must equal itself masked with the given mask.
            Mask(U256),
            /// The word must equal `signextend(byte_index, word)`.
            SignExtend(u64),
            /// The word must equal `iszero(iszero(word))`.
            Bool,
            /// The word must be less than the member count.
            EnumRange(u64),
        }

        let mut ty = self.gcx.type_of_hir_ty(hir_ty);
        if let TyKind::Udvt(underlying, _) = ty.kind {
            ty = underlying;
        }
        let validator = match ty.kind {
            TyKind::Elementary(elem) => match elem {
                ElementaryType::UInt(size) => {
                    let bits = size.bits();
                    if bits >= 256 {
                        return;
                    }
                    Validator::Mask(U256::MAX >> (256 - usize::from(bits)))
                }
                ElementaryType::Int(size) => {
                    let bits = size.bits();
                    if bits >= 256 {
                        return;
                    }
                    Validator::SignExtend(u64::from(bits / 8) - 1)
                }
                ElementaryType::Address(_) => Validator::Mask(U256::MAX >> 96),
                ElementaryType::Bool => Validator::Bool,
                ElementaryType::FixedBytes(size) => {
                    let bytes = size.bytes();
                    if bytes >= 32 {
                        return;
                    }
                    Validator::Mask(U256::MAX << (256 - 8 * usize::from(bytes)))
                }
                _ => return,
            },
            TyKind::Contract(_) => Validator::Mask(U256::MAX >> 96),
            TyKind::Enum(enum_id) => {
                Validator::EnumRange(self.gcx.hir.enumm(enum_id).variants.len() as u64)
            }
            _ => return,
        };

        let word = match source {
            AbiParamSource::ExternalCalldata => {
                // Runtime ABI encoding: selector (4 bytes) + one head word per parameter.
                let offset = builder.imm_u64(4 + arg_index * 32);
                builder.calldataload(offset)
            }
            AbiParamSource::ConstructorMemory => {
                // Constructor ABI arguments are copied to memory at 0x80 by the backend.
                let offset = builder.imm_u64(0x80 + arg_index * 32);
                builder.mload(offset)
            }
        };
        let ok = match validator {
            Validator::Mask(mask) => {
                let mask = builder.imm_u256(mask);
                let canonical = builder.and(word, mask);
                builder.eq(word, canonical)
            }
            Validator::SignExtend(byte_index) => {
                let byte_index = builder.imm_u64(byte_index);
                let canonical = builder.signextend(byte_index, word);
                builder.eq(word, canonical)
            }
            Validator::Bool => {
                let is_zero = builder.iszero(word);
                let canonical = builder.iszero(is_zero);
                builder.eq(word, canonical)
            }
            Validator::EnumRange(count) => {
                let count = builder.imm_u64(count);
                builder.lt(word, count)
            }
        };
        Self::emit_revert_unless(builder, ok);
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

        // Solidity runs construction from base to derived. For each contract in that order,
        // initialize its state variables, then run its constructor body. The current contract's
        // own constructor body is lowered by the caller after this prelude.
        let construction_order: Vec<_> = contract
            .linearized_bases
            .iter()
            .enumerate()
            .map(|(idx, &base_id)| {
                let args = idx.checked_sub(1).and_then(|arg_idx| {
                    contract.linearized_bases_args.get(arg_idx).and_then(|m| *m)
                });
                (base_id, args)
            })
            .collect();

        for (base_id, args) in construction_order.into_iter().rev() {
            let base_contract = self.gcx.hir.contract(base_id);
            for var_id in base_contract.variables() {
                let var = self.gcx.hir.variable(var_id);
                if var.is_state_variable()
                    && !var.is_constant()
                    && let Some(init) = var.initializer
                {
                    let init_val = self.lower_expr(builder, init);
                    if let Some(&offset) = self.immutable_slots.get(&var_id) {
                        self.store_immutable_value(builder, offset, init_val);
                    } else if let Some(&slot) = self.storage_slots.get(&var_id) {
                        let slot_val = builder.imm_u64(slot);
                        builder.sstore(slot_val, init_val);
                    }
                }
            }

            if base_id != contract_id
                && let Some(ctor_id) = base_contract.ctor
            {
                self.lower_base_constructor_call(builder, ctor_id, args);
            }
        }
    }

    fn function_selector(&self, func_id: HirFunctionId) -> [u8; 4] {
        self.gcx.function_selector(func_id).0
    }

    pub(super) fn mcopy(
        &self,
        builder: &mut FunctionBuilder<'_>,
        dest: ValueId,
        src: ValueId,
        len: ValueId,
        span: Option<Span>,
    ) {
        if self.gcx.sess.opts.evm_version.has_mcopy() {
            builder.mcopy(dest, src, len);
        } else {
            let err = self.gcx.dcx().err("codegen requires Cancun-compatible EVM for memory copy");
            let err = if let Some(span) = span { err.span(span) } else { err };
            err.help("compile with `--evm-version cancun` or newer").emit();
        }
    }

    /// Lowers a type from a variable declaration.
    fn lower_type_from_var(&self, var: &hir::Variable<'_>) -> MirType {
        self.lower_type_kind(&var.ty.kind)
    }

    /// Lowers a TypeKind to MirType.
    fn lower_type_kind(&self, kind: &hir::TypeKind<'_>) -> MirType {
        match kind {
            hir::TypeKind::Elementary(elem) => match elem {
                hir::ElementaryType::Bool => MirType::Bool,
                hir::ElementaryType::Address(_) => MirType::Address,
                hir::ElementaryType::Int(bits) => MirType::Int(bits.bits()),
                hir::ElementaryType::UInt(bits) => MirType::UInt(bits.bits()),
                hir::ElementaryType::Fixed(_, _) => MirType::Int(256),
                hir::ElementaryType::UFixed(_, _) => MirType::UInt(256),
                hir::ElementaryType::FixedBytes(n) => MirType::FixedBytes(n.bytes()),
                hir::ElementaryType::String => MirType::MemPtr,
                hir::ElementaryType::Bytes => MirType::MemPtr,
            },
            hir::TypeKind::Mapping(_) => MirType::StoragePtr,
            hir::TypeKind::Array(_) => MirType::MemPtr,
            hir::TypeKind::Function(_) => MirType::Function,
            hir::TypeKind::Custom(item_id) => match item_id {
                hir::ItemId::Struct(_) => MirType::MemPtr,
                hir::ItemId::Enum(_) => MirType::UInt(8),
                hir::ItemId::Contract(_) => MirType::Address,
                _ => MirType::uint256(),
            },
            hir::TypeKind::Err(_) => MirType::uint256(),
        }
    }

    /// Returns the completed module.
    #[must_use]
    pub fn finish(self) -> Module {
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
        if let hir::ExprKind::Ident(res_slice) = &expr.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            self.assigned_vars.insert(*var_id);
        }
    }

    /// Returns true if a variable is assigned after declaration.
    pub fn is_var_assigned(&self, var_id: &VariableId) -> bool {
        self.assigned_vars.contains(var_id)
    }

    /// Checks if an expression contains an external call.
    /// External calls write their return data to shared memory at offset 0,
    /// so variables initialized from them must be stored in memory to preserve the value
    /// across subsequent calls.
    pub fn has_external_call(&self, expr: &hir::Expr<'_>) -> bool {
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
            && let hir::ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            // Contract type variables are external call targets
            if matches!(var.ty.kind, hir::TypeKind::Custom(hir::ItemId::Contract(_))) {
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

/// Returns contracts whose creation bytecode is referenced by `contract_id`.
pub fn contract_bytecode_dependencies(
    gcx: Gcx<'_>,
    contract_id: ContractId,
) -> FxHashSet<ContractId> {
    let mut deps = FxHashSet::default();
    collect_contract_bytecode_dependencies(gcx, contract_id, &mut deps);
    deps.remove(&contract_id);
    deps
}

fn collect_contract_bytecode_dependencies(
    gcx: Gcx<'_>,
    contract_id: ContractId,
    deps: &mut FxHashSet<ContractId>,
) {
    let contract = gcx.hir.contract(contract_id);
    let mut collector = BytecodeDependencyCollector { gcx, deps };

    for modifier in contract.linearized_bases_args.iter().flatten() {
        let ControlFlow::Continue(()) = collector.visit_modifier(modifier);
    }

    for &base_id in contract.linearized_bases {
        let base = gcx.hir.contract(base_id);

        for var_id in base.variables() {
            let ControlFlow::Continue(()) = collector.visit_nested_var(var_id);
        }

        for func_id in base.all_functions() {
            let func = gcx.hir.function(func_id);

            for modifier in func.modifiers {
                let ControlFlow::Continue(()) = collector.visit_modifier(modifier);
            }

            if let Some(body) = func.body {
                for stmt in body.stmts {
                    let ControlFlow::Continue(()) = collector.visit_stmt(stmt);
                }
            }
        }
    }
}

struct BytecodeDependencyCollector<'a, 'gcx> {
    gcx: Gcx<'gcx>,
    deps: &'a mut FxHashSet<ContractId>,
}

impl<'a, 'gcx> BytecodeDependencyCollector<'a, 'gcx> {
    fn collect_type(&mut self, ty: &hir::Type<'gcx>) {
        if let hir::TypeKind::Custom(hir::ItemId::Contract(contract_id)) = &ty.kind {
            self.deps.insert(*contract_id);
        }
    }
}

impl<'gcx> Visit<'gcx> for BytecodeDependencyCollector<'_, 'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        match &expr.kind {
            hir::ExprKind::New(ty) => self.collect_type(ty),
            hir::ExprKind::Member(base, member)
                if matches!(member.name.as_str(), "creationCode" | "runtimeCode") =>
            {
                if let hir::ExprKind::TypeCall(ty) = &base.kind {
                    self.collect_type(ty);
                }
            }
            _ => {}
        }

        self.walk_expr(expr)
    }
}

/// Lowers a contract from HIR to MIR with pre-compiled bytecodes available for `new` expressions.
pub fn lower_contract_with_bytecodes(
    gcx: Gcx<'_>,
    contract_id: ContractId,
    child_bytecodes: &FxHashMap<ContractId, Vec<u8>>,
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

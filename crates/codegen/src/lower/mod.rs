//! HIR to MIR lowering.
//!
//! This module transforms the high-level IR from solar-sema into MIR.

mod expr;
mod stmt;

use crate::mir::{
    BlockId, Function, FunctionAttributes, FunctionBuilder, FunctionId, MirType, Module,
    StorageSlot, ValueId,
};
use alloy_primitives::U256;
use rustc_hash::{FxHashMap, FxHashSet};
use solar_interface::Ident;
use solar_sema::{
    hir::{self, ContractId, FunctionId as HirFunctionId, VariableId},
    ty::Gcx,
};

/// Context for a loop (tracks break/continue targets).
#[derive(Clone, Copy)]
pub struct LoopContext {
    /// Block to jump to on `break`.
    pub break_target: BlockId,
    /// Block to jump to on `continue`.
    pub continue_target: BlockId,
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
    /// Stack of function IDs currently being inlined (for cycle detection).
    inline_stack: Vec<HirFunctionId>,
    /// HIR functions already lowered into this MIR module.
    hir_to_mir_functions: FxHashMap<HirFunctionId, FunctionId>,
    /// Functions currently being lowered on demand.
    lowering_functions: FxHashSet<HirFunctionId>,
    /// Whether the current function body is constructor code.
    lowering_constructor: bool,
    /// Whether local memory slots should be addressed through the internal-call frame.
    lowering_internal_function: bool,
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
            locals: FxHashMap::default(),
            local_memory_slots: FxHashMap::default(),
            next_local_memory_offset: 0x80, // Start after Solidity's scratch space
            contract_bytecodes: FxHashMap::default(),
            loop_stack: Vec::new(),
            assigned_vars: FxHashSet::default(),
            inline_stack: Vec::new(),
            hir_to_mir_functions: FxHashMap::default(),
            lowering_functions: FxHashSet::default(),
            lowering_constructor: false,
            lowering_internal_function: false,
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

        // Track the current contract for using directive resolution
        self.current_contract_id = Some(contract_id);

        // Mark interfaces - they don't generate deployable bytecode
        if contract.kind == hir::ContractKind::Interface {
            self.module.is_interface = true;
        }

        self.allocate_storage(contract_id);

        // Collect all functions from the inheritance chain, handling overrides.
        // Functions are collected from most-derived to most-base, so if a function
        // with the same selector already exists, we skip the base version.
        let functions = self.collect_inherited_functions(contract_id);

        // Check if contract has an explicit constructor
        let has_constructor = functions
            .iter()
            .any(|&f| self.gcx.hir.function(f).kind == hir::FunctionKind::Constructor);

        // Generate synthetic constructor for state variable initialization if needed
        if !has_constructor {
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
                        // Only include constructor from the most-derived contract
                        if !has_constructor {
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
                            let selector = self.compute_selector(func);
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

    /// Generates a synthetic constructor to initialize state variables with initializers.
    ///
    /// For inheritance, this initializes state variables from all base contracts in the
    /// correct order (most base first, most derived last).
    fn generate_synthetic_constructor(&mut self, contract_id: ContractId) {
        let contract = self.gcx.hir.contract(contract_id);
        let linearized_bases = contract.linearized_bases;

        // Collect state variables with initializers from all base contracts
        // in reverse order (most base first) for proper initialization order.
        let mut vars_with_init: Vec<VariableId> = Vec::new();
        for &base_id in linearized_bases.iter().rev() {
            let base_contract = self.gcx.hir.contract(base_id);
            for var_id in base_contract.variables() {
                let var = self.gcx.hir.variable(var_id);
                // Skip constant variables - they don't have storage slots
                if var.is_state_variable() && !var.is_constant() && var.initializer.is_some() {
                    vars_with_init.push(var_id);
                }
            }
        }

        if vars_with_init.is_empty() {
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
            self.lowering_constructor = true;
            self.lowering_internal_function = false;

            // Initialize each state variable
            for var_id in vars_with_init {
                let var = self.gcx.hir.variable(var_id);
                if let Some(init) = var.initializer {
                    // Get the storage slot for this variable
                    if let Some(&slot) = self.storage_slots.get(&var_id) {
                        // Lower the initializer expression
                        let init_val = self.lower_expr(&mut builder, init);
                        // Store to the slot
                        let slot_val = builder.imm_u64(slot);
                        builder.sstore(slot_val, init_val);
                    }
                }
            }

            builder.stop();
            self.lowering_constructor = saved_lowering_constructor;
            self.lowering_internal_function = saved_lowering_internal_function;
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
                // Skip constant variables - they are inlined and don't use storage
                if var.is_state_variable() && !var.is_constant() {
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
            _ => 1,
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

    /// Loads a memory struct as flattened ABI return words.
    pub(super) fn load_struct_return_values(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        struct_id: hir::StructId,
        struct_ptr: ValueId,
    ) -> Vec<ValueId> {
        let mut values = Vec::new();
        self.load_struct_return_values_at(builder, struct_id, struct_ptr, 0, &mut values);
        values
    }

    fn load_struct_return_values_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        struct_id: hir::StructId,
        struct_ptr: ValueId,
        base_offset: u64,
        values: &mut Vec<ValueId>,
    ) -> u64 {
        let strukt = self.gcx.hir.strukt(struct_id);
        let mut offset = base_offset;

        for &field_id in strukt.fields {
            let field = self.gcx.hir.variable(field_id);
            if let hir::TypeKind::Custom(hir::ItemId::Struct(inner_struct_id)) = &field.ty.kind {
                offset = self.load_struct_return_values_at(
                    builder,
                    *inner_struct_id,
                    struct_ptr,
                    offset,
                    values,
                );
            } else {
                let field_ptr = if offset == 0 {
                    struct_ptr
                } else {
                    let offset_val = builder.imm_u64(offset);
                    builder.add(struct_ptr, offset_val)
                };
                values.push(builder.mload(field_ptr));
                offset += 32;
            }
        }

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

        self.lowering_functions.insert(func_id);
        self.current_contract_id = self.gcx.hir.function(func_id).contract;
        let mir_id = self.lower_function(func_id);
        self.lowering_functions.remove(&func_id);

        self.locals = saved_locals;
        self.local_memory_slots = saved_local_memory_slots;
        self.next_local_memory_offset = saved_next_local_memory_offset;
        self.assigned_vars = saved_assigned_vars;
        self.current_contract_id = saved_current_contract_id;
        self.lowering_constructor = saved_lowering_constructor;
        self.lowering_internal_function = saved_lowering_internal_function;

        mir_id
    }

    fn lower_function(&mut self, func_id: hir::FunctionId) -> FunctionId {
        let hir_func = self.gcx.hir.function(func_id);

        let func_name = hir_func.name.unwrap_or_else(|| {
            Ident::new(solar_interface::Symbol::intern("_anonymous"), solar_interface::Span::DUMMY)
        });

        let mut mir_func = Function::new(func_name);

        mir_func.attributes = FunctionAttributes {
            visibility: hir_func.visibility,
            state_mutability: hir_func.state_mutability,
            is_constructor: hir_func.kind == hir::FunctionKind::Constructor,
            is_fallback: hir_func.kind == hir::FunctionKind::Fallback,
            is_receive: hir_func.kind == hir::FunctionKind::Receive,
        };

        // Only regular public/external functions get selectors.
        // Constructor, receive, and fallback don't have selectors.
        let is_special = mir_func.attributes.is_constructor
            || mir_func.attributes.is_receive
            || mir_func.attributes.is_fallback;
        if mir_func.is_public() && !is_special {
            mir_func.selector = Some(self.compute_selector(hir_func));
        }
        let uses_external_abi = mir_func.is_public() && !is_special;
        let uses_internal_frame = !uses_external_abi && !is_special;

        self.locals.clear();
        self.local_memory_slots.clear();
        self.next_local_memory_offset = 0x80;
        self.assigned_vars.clear();
        self.lowering_constructor = hir_func.kind == hir::FunctionKind::Constructor;
        self.lowering_internal_function = uses_internal_frame;

        // Pre-analyze function body to find variables that are assigned after declaration.
        // Variables that are only initialized (never reassigned) can stay as SSA values.
        if let Some(body) = &hir_func.body {
            self.collect_assigned_vars_block(body);
        }

        {
            let mut builder = FunctionBuilder::new(&mut mir_func);

            for &param_id in hir_func.parameters {
                let param = self.gcx.hir.variable(param_id);
                let ty = self.lower_type_from_var(param);

                // Check if this is a struct parameter that needs special handling
                if uses_external_abi
                    && let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &param.ty.kind
                {
                    // Struct parameters: copy fields from calldata to memory
                    let strukt = self.gcx.hir.strukt(*struct_id);
                    let num_fields = strukt.fields.len();

                    // Allocate memory for the struct
                    let struct_size = (num_fields as u64) * 32;
                    let struct_ptr = self.allocate_memory(&mut builder, struct_size);

                    // Add MIR params for each struct field (they come from calldata)
                    for field_idx in 0..num_fields {
                        let field_ty = MirType::uint256();
                        let field_val = builder.add_param(field_ty);

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
                } else {
                    // Non-struct parameters: use normal Arg handling
                    let val = builder.add_param(ty);
                    self.locals.insert(param_id, val);
                }
            }

            for &ret_id in hir_func.returns {
                let ret_var = self.gcx.hir.variable(ret_id);
                let ty = self.lower_type_from_var(ret_var);
                builder.add_return(ty);
                let offset = self.alloc_local_memory(ret_id);
                if ret_var.name.is_none() {
                    continue;
                }

                // Named return variables are in-scope locals initialized to zero.
                let offset_val = self.local_memory_addr(&mut builder, offset);
                if let hir::TypeKind::Custom(hir::ItemId::Struct(_)) = &ret_var.ty.kind {
                    let struct_size = self.calculate_memory_words_for_type(&ret_var.ty) * 32;
                    let struct_ptr = self.allocate_memory(&mut builder, struct_size);
                    builder.mstore(offset_val, struct_ptr);
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
                    // Load return values from memory (where they were stored during execution)
                    let mut ret_vals = Vec::new();
                    for &ret_id in hir_func.returns {
                        let ret_var = self.gcx.hir.variable(ret_id);
                        let ret_val = if let Some(offset) = self.get_local_memory_offset(&ret_id) {
                            let offset_val = self.local_memory_addr(&mut builder, offset);
                            builder.mload(offset_val)
                        } else {
                            builder.imm_u256(U256::ZERO)
                        };

                        if uses_external_abi
                            && let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) =
                                &ret_var.ty.kind
                        {
                            ret_vals.extend(self.load_struct_return_values(
                                &mut builder,
                                *struct_id,
                                ret_val,
                            ));
                        } else {
                            ret_vals.push(ret_val);
                        }
                    }
                    builder.ret(ret_vals);
                }
            }
        }

        self.lowering_constructor = false;
        self.lowering_internal_function = false;
        if uses_internal_frame {
            mir_func.internal_frame_size =
                self.next_local_memory_offset.saturating_sub(Self::LOCAL_MEMORY_BASE);
        }

        let mir_id = self.module.add_function(mir_func);
        self.hir_to_mir_functions.insert(func_id, mir_id);
        mir_id
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
                    && let Some(&slot) = self.storage_slots.get(&var_id)
                {
                    let init_val = self.lower_expr(builder, init);
                    let slot_val = builder.imm_u64(slot);
                    builder.sstore(slot_val, init_val);
                }
            }

            if base_id != contract_id
                && let Some(ctor_id) = base_contract.ctor
            {
                self.lower_base_constructor_call(builder, ctor_id, args);
            }
        }
    }

    /// Computes the 4-byte function selector.
    fn compute_selector(&self, func: &hir::Function<'_>) -> [u8; 4] {
        use alloy_primitives::keccak256;

        let name = func.name.map(|n| n.to_string()).unwrap_or_default();
        let mut sig = name;
        sig.push('(');
        for (i, &param_id) in func.parameters.iter().enumerate() {
            if i > 0 {
                sig.push(',');
            }
            let param = self.gcx.hir.variable(param_id);
            sig.push_str(&self.type_canonical_name(param));
        }
        sig.push(')');

        let hash = keccak256(sig.as_bytes());
        [hash[0], hash[1], hash[2], hash[3]]
    }

    /// Gets the canonical name of a type for selector computation.
    fn type_canonical_name(&self, var: &hir::Variable<'_>) -> String {
        let ty = &var.ty;
        self.type_kind_canonical_name(&ty.kind)
    }

    /// Gets the canonical name from a TypeKind.
    fn type_kind_canonical_name(&self, kind: &hir::TypeKind<'_>) -> String {
        match kind {
            hir::TypeKind::Elementary(elem) => elem.to_abi_str().into_owned(),
            hir::TypeKind::Array(arr) => {
                let elem_name = self.type_kind_canonical_name(&arr.element.kind);
                format!("{elem_name}[]")
            }
            hir::TypeKind::Mapping(_) => "mapping".to_string(),
            hir::TypeKind::Function(_) => "function".to_string(),
            hir::TypeKind::Custom(item_id) => match item_id {
                hir::ItemId::Struct(struct_id) => {
                    // Structs are encoded as tuples in ABI signatures
                    let s = self.gcx.hir.strukt(*struct_id);
                    let mut tuple = String::from("(");
                    for (i, &field_id) in s.fields.iter().enumerate() {
                        if i > 0 {
                            tuple.push(',');
                        }
                        let field = self.gcx.hir.variable(field_id);
                        tuple.push_str(&self.type_kind_canonical_name(&field.ty.kind));
                    }
                    tuple.push(')');
                    tuple
                }
                hir::ItemId::Enum(_) => {
                    // Enums are represented as uint8 in ABI encoding
                    "uint8".to_string()
                }
                hir::ItemId::Contract(_) => "address".to_string(),
                _ => "unknown".to_string(),
            },
            hir::TypeKind::Err(_) => "error".to_string(),
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
            StmtKind::Return(Some(expr)) | StmtKind::Revert(expr) | StmtKind::Emit(expr) => {
                self.collect_assigned_vars_expr(expr)
            }
            StmtKind::Try(try_stmt) => {
                self.collect_assigned_vars_expr(&try_stmt.expr);
                for clause in try_stmt.clauses {
                    self.collect_assigned_vars_block(&clause.block);
                }
            }
            StmtKind::Assembly(assembly) => {
                self.collect_assigned_vars_yul_block(&assembly.block);
            }
            StmtKind::DeclSingle(_)
            | StmtKind::DeclMulti(_, _)
            | StmtKind::Return(None)
            | StmtKind::Continue
            | StmtKind::Break
            | StmtKind::Placeholder
            | StmtKind::Err(_) => {}
        }
    }

    fn collect_assigned_vars_yul_block(&mut self, block: &hir::yul::Block<'_>) {
        for stmt in block.stmts {
            self.collect_assigned_vars_yul_stmt(stmt);
        }
    }

    fn collect_assigned_vars_yul_stmt(&mut self, stmt: &hir::yul::Stmt<'_>) {
        use hir::yul::StmtKind;
        match &stmt.kind {
            StmtKind::Block(block) => self.collect_assigned_vars_yul_block(block),
            StmtKind::AssignSingle(path, _) => self.mark_assigned_yul_path(path),
            StmtKind::AssignMulti(paths, _) => {
                for path in *paths {
                    self.mark_assigned_yul_path(path);
                }
            }
            StmtKind::If(_, block) => self.collect_assigned_vars_yul_block(block),
            StmtKind::For(for_stmt) => {
                self.collect_assigned_vars_yul_block(&for_stmt.init);
                self.collect_assigned_vars_yul_block(&for_stmt.step);
                self.collect_assigned_vars_yul_block(&for_stmt.body);
            }
            StmtKind::Switch(switch) => {
                for case in switch.cases {
                    self.collect_assigned_vars_yul_block(&case.body);
                }
            }
            StmtKind::FunctionDef(_)
            | StmtKind::VarDecl(_, _)
            | StmtKind::Expr(_)
            | StmtKind::Leave
            | StmtKind::Break
            | StmtKind::Continue => {}
        }
    }

    fn mark_assigned_yul_path(&mut self, path: &hir::yul::Path<'_>) {
        if let hir::yul::PathRes::SolidityVariable(var_id) = path.res {
            self.assigned_vars.insert(var_id);
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
            ExprKind::Member(base, _) => self.collect_assigned_vars_expr(base),
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
            ExprKind::Member(base, _) => {
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

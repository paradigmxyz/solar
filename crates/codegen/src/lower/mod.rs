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
    /// Mapping from struct state variable ID to base storage slot.
    pub struct_storage_base_slots: FxHashMap<VariableId, u64>,
    /// Cached struct field slot offsets: (struct_type_id, field_index) -> slot offset from base.
    pub struct_field_offsets: FxHashMap<(hir::StructId, usize), u64>,
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
            struct_storage_base_slots: FxHashMap::default(),
            struct_field_offsets: FxHashMap::default(),
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
            self.lower_function(func_id);
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

        eprintln!(
            "DEBUG allocate_storage: contract={:?}, linearized_bases={:?}",
            contract.name, linearized_bases
        );

        // Iterate in reverse order (most base first) to get correct storage layout.
        // Skip index 0 since that's the contract itself - we handle it last.
        for &base_id in linearized_bases.iter().rev() {
            let base_contract = self.gcx.hir.contract(base_id);
            eprintln!("DEBUG   processing base_contract={:?}", base_contract.name);
            for var_id in base_contract.variables() {
                // Skip if we already allocated this variable (shouldn't happen, but safety check)
                if self.storage_slots.contains_key(&var_id) {
                    continue;
                }

                let var = self.gcx.hir.variable(var_id);
                eprintln!(
                    "DEBUG     var {:?} ({:?}): is_state={}, is_const={}",
                    var.name,
                    var_id,
                    var.is_state_variable(),
                    var.is_constant()
                );
                // Skip constant variables - they are inlined and don't use storage
                if var.is_state_variable() && !var.is_constant() {
                    let base_slot = self.next_storage_slot;
                    eprintln!("DEBUG       -> assigned slot {base_slot}");

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

    /// Lowers a function to MIR.
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

        self.locals.clear();
        self.local_memory_slots.clear();
        self.next_local_memory_offset = 0x80;
        self.assigned_vars.clear();

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
                if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &param.ty.kind {
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
                // Allocate memory for return variables so they can be assigned to
                // within the function body (e.g., `liquidity = 1` in if/else branches)
                self.alloc_local_memory(ret_id);
            }

            if let Some(body) = &hir_func.body {
                self.lower_block(&mut builder, body);
            }

            if !builder.func().block(builder.current_block()).is_terminated() {
                if builder.func().returns.is_empty() {
                    builder.stop();
                } else {
                    // Load return values from memory (where they were stored during execution)
                    let ret_vals: Vec<ValueId> = hir_func
                        .returns
                        .iter()
                        .map(|&ret_id| {
                            if let Some(offset) = self.get_local_memory_offset(&ret_id) {
                                let offset_val = builder.imm_u64(offset);
                                builder.mload(offset_val)
                            } else {
                                builder.imm_u256(U256::ZERO)
                            }
                        })
                        .collect();
                    builder.ret(ret_vals);
                }
            }
        }

        self.module.add_function(mir_func)
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
                hir::ItemId::Contract(contract_id) => {
                    let c = self.gcx.hir.contract(*contract_id);
                    c.name.to_string()
                }
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

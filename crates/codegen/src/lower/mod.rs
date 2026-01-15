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
use rustc_hash::FxHashMap;
use solar_interface::Ident;
use solar_sema::{
    hir::{self, ContractId, VariableId},
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
    /// Mapping from HIR variable IDs to storage slots.
    storage_slots: FxHashMap<VariableId, u64>,
    /// Next available storage slot.
    next_storage_slot: u64,
    /// Mapping from HIR variable IDs to MIR values (for local variables).
    /// For SSA-style immutable variables (function params).
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
}

impl<'gcx> Lowerer<'gcx> {
    /// Creates a new lowerer.
    pub fn new(gcx: Gcx<'gcx>, name: Ident) -> Self {
        Self {
            gcx,
            module: Module::new(name),
            storage_slots: FxHashMap::default(),
            next_storage_slot: 0,
            locals: FxHashMap::default(),
            local_memory_slots: FxHashMap::default(),
            next_local_memory_offset: 0x80, // Start after Solidity's scratch space
            contract_bytecodes: FxHashMap::default(),
            loop_stack: Vec::new(),
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

        // Mark interfaces - they don't generate deployable bytecode
        if contract.kind == hir::ContractKind::Interface {
            self.module.is_interface = true;
        }

        self.allocate_storage(contract);

        // Check if contract has an explicit constructor
        let has_constructor = contract
            .all_functions()
            .any(|f| self.gcx.hir.function(f).kind == hir::FunctionKind::Constructor);

        // Generate synthetic constructor for state variable initialization if needed
        if !has_constructor {
            self.generate_synthetic_constructor(contract);
        }

        for func_id in contract.all_functions() {
            self.lower_function(func_id);
        }
    }

    /// Generates a synthetic constructor to initialize state variables with initializers.
    fn generate_synthetic_constructor(&mut self, contract: &hir::Contract<'_>) {
        // Collect state variables with initializers
        let vars_with_init: Vec<_> = contract
            .variables()
            .filter(|var_id| {
                let var = self.gcx.hir.variable(*var_id);
                var.is_state_variable() && var.initializer.is_some()
            })
            .collect();

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
    fn allocate_storage(&mut self, contract: &hir::Contract<'_>) {
        for var_id in contract.variables() {
            let var = self.gcx.hir.variable(var_id);
            if var.is_state_variable() {
                let slot = self.next_storage_slot;
                self.next_storage_slot += 1;

                self.storage_slots.insert(var_id, slot);

                let mir_ty = self.lower_type_from_var(var);
                self.module.add_storage_slot(StorageSlot {
                    slot,
                    offset: 0,
                    ty: mir_ty,
                    name: var.name,
                });
            }
        }
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

        if mir_func.is_public() && !mir_func.attributes.is_constructor {
            mir_func.selector = Some(self.compute_selector(hir_func));
        }

        self.locals.clear();
        self.local_memory_slots.clear();
        self.next_local_memory_offset = 0x80;

        {
            let mut builder = FunctionBuilder::new(&mut mir_func);

            for &param_id in hir_func.parameters {
                let param = self.gcx.hir.variable(param_id);
                let ty = self.lower_type_from_var(param);
                let val = builder.add_param(ty);
                self.locals.insert(param_id, val);
            }

            for &ret_id in hir_func.returns {
                let ret_var = self.gcx.hir.variable(ret_id);
                let ty = self.lower_type_from_var(ret_var);
                builder.add_return(ty);
            }

            if let Some(body) = &hir_func.body {
                self.lower_block(&mut builder, body);
            }

            if !builder.func().block(builder.current_block()).is_terminated() {
                if builder.func().returns.is_empty() {
                    builder.stop();
                } else {
                    let zero = builder.imm_u256(U256::ZERO);
                    builder.ret([zero]);
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
                    let s = self.gcx.hir.strukt(*struct_id);
                    s.name.to_string()
                }
                hir::ItemId::Enum(enum_id) => {
                    let e = self.gcx.hir.enumm(*enum_id);
                    e.name.to_string()
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

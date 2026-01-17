//! EVM bytecode generation from MIR.
//!
//! This module generates EVM bytecode from MIR using:
//! - Liveness analysis to know when values die
//! - Phi elimination to convert SSA to parallel copies
//! - Stack scheduling to generate DUP/SWAP sequences
//! - Two-pass assembly for label resolution

use crate::{
    analysis::{CopyDest, CopySource, Liveness, ParallelCopy, eliminate_phis},
    codegen::{
        assembler::{Assembler, Label, opcodes},
        stack::{ScheduledOp, StackScheduler},
    },
    mir::{BlockId, Function, InstKind, Module, Terminator, ValueId},
    transform::DeadCodeEliminator,
};
use alloy_primitives::U256;
use rustc_hash::FxHashMap;

/// EVM code generator.
pub struct EvmCodegen {
    /// The assembler for bytecode generation.
    asm: Assembler,
    /// Stack scheduler.
    scheduler: StackScheduler,
    /// Block labels.
    block_labels: FxHashMap<BlockId, Label>,
    /// Copies to insert at block exits (from phi elimination).
    block_copies: FxHashMap<BlockId, Vec<ParallelCopy>>,
    /// Whether we're currently generating constructor code.
    /// When true, LoadArg uses CODECOPY from the end of code instead of CALLDATALOAD.
    in_constructor: bool,
    /// Number of constructor parameters (used for CODECOPY offset calculation).
    constructor_param_count: u32,
}

impl EvmCodegen {
    /// Creates a new EVM code generator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            asm: Assembler::new(),
            scheduler: StackScheduler::new(),
            block_labels: FxHashMap::default(),
            block_copies: FxHashMap::default(),
            in_constructor: false,
            constructor_param_count: 0,
        }
    }

    /// Generates bytecode for a module (runtime code only).
    /// Returns empty bytecode for interfaces (they have no implementation).
    ///
    /// This runs optimization passes (including DCE) on the module before codegen.
    pub fn generate_module(&mut self, module: &mut Module) -> Vec<u8> {
        if module.is_interface {
            return Vec::new();
        }
        self.run_optimization_passes(module);
        self.generate_runtime_code(module)
    }

    /// Generates deployment bytecode for a module.
    /// Returns (deployment_bytecode, runtime_bytecode).
    /// Returns empty bytecodes for interfaces (they have no implementation).
    ///
    /// This runs optimization passes (including DCE) on the module before codegen.
    pub fn generate_deployment_bytecode(&mut self, module: &mut Module) -> (Vec<u8>, Vec<u8>) {
        if module.is_interface {
            return (Vec::new(), Vec::new());
        }
        self.run_optimization_passes(module);
        // First generate the runtime code
        let runtime_code = self.generate_runtime_code(module);
        let runtime_len = runtime_code.len();

        // Generate constructor initialization code (if any)
        let constructor_code = self.generate_constructor_code(module);

        // Deploy code structure:
        // [constructor_code]    ; run constructor (SSTOREs for initializers)
        // PUSH<n> runtime_len   ; size to return
        // DUP1                  ; duplicate for CODECOPY
        // PUSH<n> offset        ; where runtime starts
        // PUSH0                 ; memory destination = 0
        // CODECOPY              ; copy runtime to memory
        // PUSH0                 ; memory offset = 0
        // RETURN                ; return the runtime code

        // Calculate push sizes
        let len_push_size = if runtime_len == 0 {
            1 // PUSH0
        } else if runtime_len <= 255 {
            2 // PUSH1
        } else {
            3 // PUSH2
        };

        // Calculate the deploy code length (excluding constructor)
        let return_code_len = len_push_size + 1 + 2 + 1 + 1 + 1 + 1; // 9 bytes for PUSH+DUP+PUSH+PUSH0+CODECOPY+PUSH0+RETURN

        // Total deploy code = constructor + return code
        let deploy_code_len = constructor_code.len() + return_code_len;

        // Build the deployment bytecode
        let mut deploy_bytecode = Vec::new();

        // Add constructor code first
        deploy_bytecode.extend_from_slice(&constructor_code);

        // PUSH runtime_len
        Self::emit_push_raw(&mut deploy_bytecode, runtime_len as u64);
        // DUP1
        deploy_bytecode.push(opcodes::dup(1));
        // PUSH deploy_code_len (offset to runtime)
        Self::emit_push_raw(&mut deploy_bytecode, deploy_code_len as u64);
        // PUSH0 (memory destination)
        deploy_bytecode.push(opcodes::PUSH0);
        // CODECOPY
        deploy_bytecode.push(opcodes::CODECOPY);
        // PUSH0 (return offset)
        deploy_bytecode.push(opcodes::PUSH0);
        // RETURN
        deploy_bytecode.push(opcodes::RETURN);

        // Append runtime code
        deploy_bytecode.extend_from_slice(&runtime_code);

        (deploy_bytecode, runtime_code)
    }

    /// Generates constructor code that runs during deployment.
    /// This includes state variable initializers.
    ///
    /// Constructor arguments are read from the end of the initcode using CODECOPY.
    /// The args are ABI-encoded and appended after the deployment bytecode.
    fn generate_constructor_code(&mut self, module: &Module) -> Vec<u8> {
        // Find constructor function if it exists
        let constructor = module.functions.iter().find(|f| f.attributes.is_constructor);

        if let Some(ctor) = constructor {
            // Generate constructor bytecode
            let mut asm = Assembler::new();
            std::mem::swap(&mut self.asm, &mut asm);

            // Clear state and generate function body
            self.block_labels.clear();
            self.block_copies.clear();

            // Initialize free memory pointer: MSTORE(0x40, 0x80)
            self.asm.emit_push(U256::from(0x80));
            self.asm.emit_push(U256::from(0x40));
            self.asm.emit_op(opcodes::MSTORE);

            // Set constructor context for LoadArg handling
            self.in_constructor = true;
            self.constructor_param_count = ctor.params.len() as u32;

            // If constructor has parameters, emit code to copy args from code end to memory
            // Constructor args are appended after bytecode, read via CODECOPY
            if !ctor.params.is_empty() {
                let args_size = ctor.params.len() * 32;
                // CODESIZE - args_size = offset where args start
                // CODECOPY(destOffset=0x80, offset=CODESIZE-args_size, size=args_size)
                // We use memory starting at 0x80 (after Solidity's scratch space)
                self.asm.emit_push(U256::from(args_size)); // size
                self.asm.emit_push(U256::from(args_size)); // for subtraction
                self.asm.emit_op(opcodes::CODESIZE);
                self.asm.emit_op(opcodes::SUB); // CODESIZE - args_size = offset
                self.asm.emit_push(U256::from(0x80)); // destOffset in memory
                self.asm.emit_op(opcodes::CODECOPY);
            }

            // Generate the constructor body (which includes SSTORE for initializers)
            self.generate_function_body(ctor);

            // Reset constructor context
            self.in_constructor = false;
            self.constructor_param_count = 0;

            std::mem::swap(&mut self.asm, &mut asm);
            let mut bytecode = asm.assemble().bytecode;

            // Remove trailing STOP (0x00) if present - we want to fall through to CODECOPY/RETURN
            if bytecode.last() == Some(&opcodes::STOP) {
                bytecode.pop();
            }

            bytecode
        } else {
            Vec::new()
        }
    }

    /// Emit a PUSH instruction with the optimal size for the value.
    fn emit_push_raw(bytecode: &mut Vec<u8>, value: u64) {
        // PUSH0 = 0x5f, PUSH1 = 0x60, PUSH2 = 0x61, etc.
        if value == 0 {
            bytecode.push(0x5f); // PUSH0
        } else if value <= 0xFF {
            bytecode.push(0x60); // PUSH1
            bytecode.push(value as u8);
        } else if value <= 0xFFFF {
            bytecode.push(0x61); // PUSH2
            bytecode.push((value >> 8) as u8);
            bytecode.push(value as u8);
        } else if value <= 0xFFFFFF {
            bytecode.push(0x62); // PUSH3
            bytecode.push((value >> 16) as u8);
            bytecode.push((value >> 8) as u8);
            bytecode.push(value as u8);
        } else {
            // For larger values, use the minimum bytes needed
            let bytes = value.to_be_bytes();
            let first_non_zero = bytes.iter().position(|&b| b != 0).unwrap_or(7);
            let num_bytes = 8 - first_non_zero;
            bytecode.push(0x5f + num_bytes as u8); // PUSH1 = 0x60, PUSH2 = 0x61, etc.
            bytecode.extend_from_slice(&bytes[first_non_zero..]);
        }
    }

    /// Runs optimization passes on all functions in the module.
    fn run_optimization_passes(&mut self, module: &mut Module) {
        let mut dce = DeadCodeEliminator::new();
        for func in module.functions.iter_mut() {
            dce.run_to_fixpoint(func);
        }
    }

    /// Generates runtime bytecode for a module.
    fn generate_runtime_code(&mut self, module: &Module) -> Vec<u8> {
        self.asm = Assembler::new();
        self.block_labels.clear();
        self.block_copies.clear();

        // Initialize free memory pointer: MSTORE(0x40, 0x80)
        self.asm.emit_push(U256::from(0x80));
        self.asm.emit_push(U256::from(0x40));
        self.asm.emit_op(opcodes::MSTORE);

        if !module.functions.is_empty() {
            // The dispatcher generates function bodies inline
            self.generate_dispatcher(module);
        }

        let result = std::mem::take(&mut self.asm).assemble();
        result.bytecode
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
        // Find receive and fallback functions
        let receive_idx = module.functions.iter().position(|f| f.attributes.is_receive);
        let fallback_idx = module.functions.iter().position(|f| f.attributes.is_fallback);

        // Create labels for each function
        let mut func_labels: Vec<Label> = Vec::new();
        for _ in module.functions.iter() {
            func_labels.push(self.asm.new_label());
        }
        let revert_label = self.asm.new_label();
        let has_calldata_label = self.asm.new_label();

        // Check if calldatasize == 0
        self.asm.emit_op(opcodes::CALLDATASIZE);
        self.asm.emit_push_label(has_calldata_label);
        self.asm.emit_op(opcodes::JUMPI);

        // calldatasize == 0: Handle receive/fallback
        // Solidity semantics: if receive exists, call it; else if fallback exists, call it; else
        // revert
        if let Some(recv_idx) = receive_idx {
            self.asm.emit_push_label(func_labels[recv_idx]);
            self.asm.emit_op(opcodes::JUMP);
        } else if let Some(fb_idx) = fallback_idx {
            self.asm.emit_push_label(func_labels[fb_idx]);
            self.asm.emit_op(opcodes::JUMP);
        } else {
            self.asm.emit_push_label(revert_label);
            self.asm.emit_op(opcodes::JUMP);
        }

        // calldatasize > 0: Load selector and match
        self.asm.define_label(has_calldata_label);
        self.asm.emit_op(opcodes::JUMPDEST);

        // Load selector from calldata
        self.asm.emit_push(U256::ZERO);
        self.asm.emit_op(opcodes::CALLDATALOAD);
        self.asm.emit_push(U256::from(0xe0));
        self.asm.emit_op(opcodes::SHR);

        // Compare against each function's selector
        for (i, func) in module.functions.iter().enumerate() {
            if let Some(selector) = func.selector {
                self.asm.emit_op(opcodes::dup(1));
                self.asm.emit_push(U256::from_be_slice(&selector));
                self.asm.emit_op(opcodes::EQ);
                self.asm.emit_push_label(func_labels[i]);
                self.asm.emit_op(opcodes::JUMPI);
            }
        }

        // No selector match - jump to fallback or revert
        if let Some(fb_idx) = fallback_idx {
            // Pop selector and jump to fallback
            self.asm.emit_op(opcodes::POP);
            self.asm.emit_push_label(func_labels[fb_idx]);
            self.asm.emit_op(opcodes::JUMP);
        } else {
            self.asm.emit_push_label(revert_label);
            self.asm.emit_op(opcodes::JUMP);
        }

        // Define function entry points
        for (i, func) in module.functions.iter().enumerate() {
            self.asm.define_label(func_labels[i]);
            self.asm.emit_op(opcodes::JUMPDEST);

            // Pop the selector for regular functions (receive/fallback don't have it on stack)
            if func.selector.is_some() {
                self.asm.emit_op(opcodes::POP);
            }

            // Emit payable check for non-payable functions
            self.emit_payable_check(func);

            // Generate function body
            self.generate_function_body(func);
        }

        // Revert label
        self.asm.define_label(revert_label);
        self.asm.emit_op(opcodes::JUMPDEST);
        self.asm.emit_push(U256::ZERO);
        self.asm.emit_push(U256::ZERO);
        self.asm.emit_op(opcodes::REVERT);
    }

    /// Emits a payable check for non-payable functions.
    /// Non-payable, view, and pure functions revert if called with value.
    fn emit_payable_check(&mut self, func: &Function) {
        use solar_sema::hir::StateMutability;

        match func.attributes.state_mutability {
            StateMutability::Payable => {
                // Payable functions accept ETH - no check needed
            }
            StateMutability::NonPayable | StateMutability::View | StateMutability::Pure => {
                // CALLVALUE ISZERO ok JUMPI PUSH0 PUSH0 REVERT ok: JUMPDEST
                let ok_label = self.asm.new_label();

                self.asm.emit_op(opcodes::CALLVALUE);
                self.asm.emit_op(opcodes::ISZERO);
                self.asm.emit_push_label(ok_label);
                self.asm.emit_op(opcodes::JUMPI);
                // Revert with empty data
                self.asm.emit_push(U256::ZERO);
                self.asm.emit_push(U256::ZERO);
                self.asm.emit_op(opcodes::REVERT);

                self.asm.define_label(ok_label);
                self.asm.emit_op(opcodes::JUMPDEST);
            }
        }
    }

    /// Generates bytecode for a function.
    #[allow(dead_code)]
    fn generate_function(&mut self, func: &Function) {
        self.generate_function_body(func);
    }

    /// Generates the body of a function.
    fn generate_function_body(&mut self, func: &Function) {
        // Compute liveness
        let liveness = Liveness::compute(func);

        // Eliminate phis
        let phi_result = eliminate_phis(func);
        for (block_id, copies) in phi_result.block_copies {
            self.block_copies.insert(block_id, copies.copies);
        }

        // Reset scheduler
        self.scheduler = StackScheduler::new();

        // Create labels for each block
        self.block_labels.clear();
        for block_id in func.blocks.indices() {
            self.block_labels.insert(block_id, self.asm.new_label());
        }

        // Generate each block
        for (block_id, block) in func.blocks.iter_enumerated() {
            // Define block label
            self.asm.define_label(self.block_labels[&block_id]);
            self.asm.emit_op(opcodes::JUMPDEST);

            // Reset stack at block entry (simplified - real impl needs canonical shapes)
            self.scheduler.clear_stack();

            // Generate instructions
            for (inst_idx, &inst_id) in block.instructions.iter().enumerate() {
                let inst = &func.instructions[inst_id];

                // Skip phi instructions (they're handled by copies)
                if matches!(inst.kind, InstKind::Phi(_)) {
                    continue;
                }

                // Find the value ID that corresponds to this instruction (if any)
                let result_value = func
                    .values
                    .iter_enumerated()
                    .find(|(_, v)| matches!(v, crate::mir::Value::Inst(id) if *id == inst_id))
                    .map(|(vid, _)| vid);

                // Generate the instruction
                self.generate_inst(func, &inst.kind, &liveness, block_id, inst_idx, result_value);
            }

            // Insert phi copies before terminator
            if let Some(copies) = self.block_copies.remove(&block_id) {
                let mut temps = FxHashMap::default();
                for copy in &copies {
                    self.generate_copy(func, copy, &mut temps);
                }
            }

            // Generate terminator
            if let Some(term) = &block.terminator {
                self.generate_terminator(func, term);
            }
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
        match kind {
            // Binary arithmetic operations
            InstKind::Add(a, b) => {
                self.emit_binary_op_with_result(func, *a, *b, opcodes::ADD, result_value)
            }
            InstKind::Sub(a, b) => {
                self.emit_binary_op_with_result(func, *a, *b, opcodes::SUB, result_value)
            }
            InstKind::Mul(a, b) => {
                self.emit_binary_op_with_result(func, *a, *b, opcodes::MUL, result_value)
            }
            InstKind::Div(a, b) => {
                self.emit_binary_op_with_result(func, *a, *b, opcodes::DIV, result_value)
            }
            InstKind::SDiv(a, b) => {
                self.emit_binary_op_with_result(func, *a, *b, opcodes::SDIV, result_value)
            }
            InstKind::Mod(a, b) => {
                self.emit_binary_op_with_result(func, *a, *b, opcodes::MOD, result_value)
            }
            InstKind::SMod(a, b) => {
                self.emit_binary_op_with_result(func, *a, *b, opcodes::SMOD, result_value)
            }
            InstKind::Exp(a, b) => {
                self.emit_binary_op_with_result(func, *a, *b, opcodes::EXP, result_value)
            }

            // Bitwise operations
            InstKind::And(a, b) => {
                self.emit_binary_op_with_result(func, *a, *b, opcodes::AND, result_value)
            }
            InstKind::Or(a, b) => {
                self.emit_binary_op_with_result(func, *a, *b, opcodes::OR, result_value)
            }
            InstKind::Xor(a, b) => {
                self.emit_binary_op_with_result(func, *a, *b, opcodes::XOR, result_value)
            }
            InstKind::Not(a) => {
                self.emit_unary_op_with_result(func, *a, opcodes::NOT, result_value)
            }
            InstKind::Shl(shift, val) => {
                self.emit_binary_op_with_result(func, *shift, *val, opcodes::SHL, result_value)
            }
            InstKind::Shr(shift, val) => {
                self.emit_binary_op_with_result(func, *shift, *val, opcodes::SHR, result_value)
            }
            InstKind::Sar(shift, val) => {
                self.emit_binary_op_with_result(func, *shift, *val, opcodes::SAR, result_value)
            }
            InstKind::Byte(i, x) => {
                self.emit_binary_op_with_result(func, *i, *x, opcodes::BYTE, result_value)
            }

            // Comparison operations - track results for branch conditions and Select
            InstKind::Lt(a, b) => {
                self.emit_binary_op_with_result(func, *a, *b, opcodes::LT, result_value)
            }
            InstKind::Gt(a, b) => {
                self.emit_binary_op_with_result(func, *a, *b, opcodes::GT, result_value)
            }
            InstKind::SLt(a, b) => {
                self.emit_binary_op_with_result(func, *a, *b, opcodes::SLT, result_value)
            }
            InstKind::SGt(a, b) => {
                self.emit_binary_op_with_result(func, *a, *b, opcodes::SGT, result_value)
            }
            InstKind::Eq(a, b) => {
                self.emit_binary_op_with_result(func, *a, *b, opcodes::EQ, result_value)
            }
            InstKind::IsZero(a) => {
                self.emit_unary_op_with_result(func, *a, opcodes::ISZERO, result_value)
            }

            // Memory operations
            // Track MLOAD results so they can be used as operands in subsequent instructions.
            // This is essential for nested external calls where the return value from one call
            // becomes an argument to another call.
            InstKind::MLoad(addr) => {
                self.emit_unary_op_with_result(func, *addr, opcodes::MLOAD, result_value)
            }
            InstKind::MStore(addr, val) => self.emit_store_op(func, *addr, *val, opcodes::MSTORE),
            InstKind::MStore8(addr, val) => self.emit_store_op(func, *addr, *val, opcodes::MSTORE8),
            InstKind::MSize => {
                self.asm.emit_op(opcodes::MSIZE);
                self.scheduler.instruction_executed(0, result_value);
            }

            // Storage operations
            InstKind::SLoad(slot) => {
                self.emit_unary_op_with_result(func, *slot, opcodes::SLOAD, result_value)
            }
            InstKind::SStore(slot, val) => self.emit_store_op_live_aware(
                func,
                *slot,
                *val,
                opcodes::SSTORE,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::TLoad(slot) => {
                self.emit_unary_op_with_result(func, *slot, opcodes::TLOAD, result_value)
            }
            InstKind::TStore(slot, val) => self.emit_store_op(func, *slot, *val, opcodes::TSTORE),

            // Calldata operations
            InstKind::CalldataLoad(off) => {
                self.emit_unary_op_with_result(func, *off, opcodes::CALLDATALOAD, result_value)
            }
            InstKind::CalldataSize => {
                self.asm.emit_op(opcodes::CALLDATASIZE);
                self.scheduler.instruction_executed(0, result_value);
            }

            // Hash operations
            InstKind::Keccak256(off, len) => {
                self.emit_binary_op_with_result(func, *off, *len, opcodes::KECCAK256, result_value)
            }

            // Environment operations
            InstKind::Caller => {
                self.asm.emit_op(opcodes::CALLER);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::CallValue => {
                self.asm.emit_op(opcodes::CALLVALUE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Address => {
                self.asm.emit_op(opcodes::ADDRESS);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Origin => {
                self.asm.emit_op(opcodes::ORIGIN);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::GasPrice => {
                self.asm.emit_op(opcodes::GASPRICE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Gas => {
                self.asm.emit_op(opcodes::GAS);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Timestamp => {
                self.asm.emit_op(opcodes::TIMESTAMP);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::BlockNumber => {
                self.asm.emit_op(opcodes::NUMBER);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Coinbase => {
                self.asm.emit_op(opcodes::COINBASE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::ChainId => {
                self.asm.emit_op(opcodes::CHAINID);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::SelfBalance => {
                self.asm.emit_op(opcodes::SELFBALANCE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::BaseFee => {
                self.asm.emit_op(opcodes::BASEFEE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::BlobBaseFee => {
                self.asm.emit_op(opcodes::BLOBBASEFEE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::GasLimit => {
                self.asm.emit_op(opcodes::GASLIMIT);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::PrevRandao => {
                self.asm.emit_op(opcodes::PREVRANDAO);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Balance(addr) => {
                self.emit_unary_op_with_result(func, *addr, opcodes::BALANCE, result_value)
            }
            InstKind::BlockHash(num) => {
                self.emit_unary_op_with_result(func, *num, opcodes::BLOCKHASH, result_value)
            }
            InstKind::BlobHash(idx) => {
                self.emit_unary_op_with_result(func, *idx, opcodes::BLOBHASH, result_value)
            }
            InstKind::ExtCodeSize(addr) => {
                self.emit_unary_op_with_result(func, *addr, opcodes::EXTCODESIZE, result_value)
            }
            InstKind::ExtCodeHash(addr) => {
                self.emit_unary_op_with_result(func, *addr, opcodes::EXTCODEHASH, result_value)
            }
            InstKind::CodeSize => {
                self.asm.emit_op(opcodes::CODESIZE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::ReturnDataSize => {
                self.asm.emit_op(opcodes::RETURNDATASIZE);
                self.scheduler.instruction_executed(0, result_value);
            }

            // Ternary operations
            InstKind::AddMod(a, b, n) => self.emit_ternary_op(func, *a, *b, *n, opcodes::ADDMOD),
            InstKind::MulMod(a, b, n) => self.emit_ternary_op(func, *a, *b, *n, opcodes::MULMOD),

            // Select is like a ternary conditional
            InstKind::Select(cond, true_val, false_val) => {
                // select(cond, t, f) = f + cond * (t - f)
                //
                // We emit all three values to the stack, then do inline computation.
                // After emitting, the scheduler thinks [f, t, cond] are on stack.
                // We do: DUP3 SUB MUL ADD to compute the result.
                // Then we tell the scheduler we consumed 3 and produced 1.

                self.emit_value(func, *false_val); // Stack: [f]
                self.emit_value(func, *true_val); // Stack: [f, t]
                self.emit_value(func, *cond); // Stack: [f, t, cond]

                // Now compute: f + cond * (t - f)
                // Stack is [f, t, cond] with cond on top
                // Step 1: Get t-f onto stack
                self.asm.emit_op(opcodes::dup(2)); // [f, t, cond, t]
                self.asm.emit_op(opcodes::dup(4)); // [f, t, cond, t, f]
                self.asm.emit_op(opcodes::SUB); // [f, t, cond, t-f]
                // Step 2: Multiply by cond
                self.asm.emit_op(opcodes::MUL); // [f, t, cond*(t-f)]
                // Step 3: Add f (which is now at depth 2)
                self.asm.emit_op(opcodes::swap(2)); // [cond*(t-f), t, f]
                self.asm.emit_op(opcodes::POP); // [cond*(t-f), f]
                self.asm.emit_op(opcodes::ADD); // [result]

                // Tell scheduler: consumed 3, produced 1
                self.scheduler.instruction_executed(3, result_value);
            }

            // Sign extend
            InstKind::SignExtend(b, x) => {
                self.emit_binary_op_with_result(func, *b, *x, opcodes::SIGNEXTEND, result_value)
            }

            // Phi nodes are skipped (handled by copies)
            InstKind::Phi(_) => {}

            // Contract creation
            InstKind::Create(value, offset, size) => {
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.emit_value(func, *value);
                self.asm.emit_op(opcodes::CREATE);
                // CREATE consumes 3 values and produces 1 (new contract address)
                self.scheduler.instruction_executed(3, result_value);
            }

            InstKind::Create2(value, offset, size, salt) => {
                self.emit_value(func, *salt);
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.emit_value(func, *value);
                self.asm.emit_op(opcodes::CREATE2);
                // CREATE2 consumes 4 values and produces 1 (new contract address)
                self.scheduler.instruction_executed(4, result_value);
            }

            // External calls
            InstKind::Call { gas, addr, value, args_offset, args_size, ret_offset, ret_size } => {
                // CALL(gas, addr, value, argsOffset, argsSize, retOffset, retSize)
                // Stack needs (top to bottom): gas, addr, value, argsOffset, argsSize, retOffset,
                // retSize
                //
                // NOTE: Due to how MIR is structured, `gas` and `addr` are instruction results
                // that were emitted immediately before this CALL. They are already on the stack.
                // We only need to emit the immediate values in the right positions.
                //
                // Stack state before this: [..., addr_value, gas_value]
                // We need: [gas, addr, value, argsOffset, argsSize, retOffset, retSize]
                //
                // Since gas and addr are on top, we need to:
                // 1. Push all the immediate values
                // 2. Then swap to get the order right
                //
                // For simplicity, emit all as immediates - the computed values (gas, addr)
                // will be pushed fresh since they're immediate in our lowering.

                self.emit_value(func, *ret_size);
                self.emit_value(func, *ret_offset);
                self.emit_value(func, *args_size);
                self.emit_value(func, *args_offset);
                self.emit_value(func, *value);
                self.emit_value(func, *addr);
                self.emit_value(func, *gas);

                self.asm.emit_op(opcodes::CALL);
                // CALL consumes 7 values and produces 1 (success bool)
                self.scheduler.instruction_executed(7, result_value);
            }

            InstKind::StaticCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                // STATICCALL(gas, addr, argsOffset, argsSize, retOffset, retSize)
                self.emit_value(func, *ret_size);
                self.emit_value(func, *ret_offset);
                self.emit_value(func, *args_size);
                self.emit_value(func, *args_offset);
                self.emit_value(func, *addr);
                self.emit_value(func, *gas);
                self.asm.emit_op(opcodes::STATICCALL);
                // STATICCALL consumes 6 values and produces 1 (success bool)
                self.scheduler.instruction_executed(6, result_value);
            }

            InstKind::DelegateCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                // DELEGATECALL(gas, addr, argsOffset, argsSize, retOffset, retSize)
                self.emit_value(func, *ret_size);
                self.emit_value(func, *ret_offset);
                self.emit_value(func, *args_size);
                self.emit_value(func, *args_offset);
                self.emit_value(func, *addr);
                self.emit_value(func, *gas);
                self.asm.emit_op(opcodes::DELEGATECALL);
                // DELEGATECALL consumes 6 values and produces 1 (success bool)
                self.scheduler.instruction_executed(6, result_value);
            }

            // Log operations
            InstKind::Log0(offset, size) => {
                // LOG0(offset, size) - stack order: offset on top, then size
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.asm.emit_op(opcodes::LOG0);
                self.scheduler.instruction_executed(2, None);
            }
            InstKind::Log1(offset, size, topic1) => {
                // LOG1(offset, size, topic1) - stack order: offset, size, topic1
                self.emit_value(func, *topic1);
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.asm.emit_op(opcodes::LOG1);
                self.scheduler.instruction_executed(3, None);
            }
            InstKind::Log2(offset, size, topic1, topic2) => {
                // LOG2(offset, size, topic1, topic2) - stack order: offset, size, topic1, topic2
                self.emit_value(func, *topic2);
                self.emit_value(func, *topic1);
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.asm.emit_op(opcodes::LOG2);
                self.scheduler.instruction_executed(4, None);
            }
            InstKind::Log3(offset, size, topic1, topic2, topic3) => {
                // LOG3(offset, size, topic1, topic2, topic3)
                self.emit_value(func, *topic3);
                self.emit_value(func, *topic2);
                self.emit_value(func, *topic1);
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.asm.emit_op(opcodes::LOG3);
                self.scheduler.instruction_executed(5, None);
            }
            InstKind::Log4(offset, size, topic1, topic2, topic3, topic4) => {
                // LOG4(offset, size, topic1, topic2, topic3, topic4)
                self.emit_value(func, *topic4);
                self.emit_value(func, *topic3);
                self.emit_value(func, *topic2);
                self.emit_value(func, *topic1);
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.asm.emit_op(opcodes::LOG4);
                self.scheduler.instruction_executed(6, None);
            }

            // Memory copy operations
            InstKind::CalldataCopy(dest, offset, size) => {
                // CALLDATACOPY(destOffset, offset, size)
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.emit_value(func, *dest);
                self.asm.emit_op(opcodes::CALLDATACOPY);
                self.scheduler.instruction_executed(3, None);
            }

            InstKind::CodeCopy(dest, offset, size) => {
                // CODECOPY(destOffset, offset, size)
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.emit_value(func, *dest);
                self.asm.emit_op(opcodes::CODECOPY);
                self.scheduler.instruction_executed(3, None);
            }

            InstKind::ReturnDataCopy(dest, offset, size) => {
                // RETURNDATACOPY(destOffset, offset, size)
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.emit_value(func, *dest);
                self.asm.emit_op(opcodes::RETURNDATACOPY);
                self.scheduler.instruction_executed(3, None);
            }

            InstKind::MCopy(dest, src, size) => {
                // MCOPY(destOffset, srcOffset, size)
                self.emit_value(func, *size);
                self.emit_value(func, *src);
                self.emit_value(func, *dest);
                self.asm.emit_op(opcodes::MCOPY);
                self.scheduler.instruction_executed(3, None);
            }

            InstKind::ExtCodeCopy(addr, dest, offset, size) => {
                // EXTCODECOPY(address, destOffset, offset, size)
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.emit_value(func, *dest);
                self.emit_value(func, *addr);
                self.asm.emit_op(opcodes::EXTCODECOPY);
                self.scheduler.instruction_executed(4, None);
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

    /// Emits a value to the stack.
    fn emit_value(&mut self, func: &Function, val: ValueId) {
        let ops = self.scheduler.ensure_on_top(val, func);
        for op in ops {
            match op {
                ScheduledOp::Stack(stack_op) => {
                    self.asm.emit_op(stack_op.opcode());
                }
                ScheduledOp::PushImmediate(imm) => {
                    self.asm.emit_push(*imm);
                }
                ScheduledOp::LoadSpill(slot) => {
                    // PUSH slot_offset, MLOAD
                    self.asm.emit_push(U256::from(slot.byte_offset()));
                    self.asm.emit_op(opcodes::MLOAD);
                }
                ScheduledOp::SaveSpill(slot) => {
                    // PUSH slot_offset, MSTORE
                    self.asm.emit_push(U256::from(slot.byte_offset()));
                    self.asm.emit_op(opcodes::MSTORE);
                }
                ScheduledOp::LoadArg(index) => {
                    if self.in_constructor {
                        // Constructor args were copied to memory at 0x80
                        // Load from memory: 0x80 + index * 32
                        let offset = 0x80 + (*index as u64) * 32;
                        self.asm.emit_push(U256::from(offset));
                        self.asm.emit_op(opcodes::MLOAD);
                    } else {
                        // Runtime function: load from calldata
                        // ABI encoding: selector (4 bytes) + args (32 bytes each)
                        // Offset = 4 + index * 32
                        let offset = 4 + (*index as u64) * 32;
                        self.asm.emit_push(U256::from(offset));
                        self.asm.emit_op(opcodes::CALLDATALOAD);
                    }
                }
            }
        }
    }

    /// Emits a binary operation with result tracking.
    fn emit_binary_op_with_result(
        &mut self,
        func: &Function,
        a: ValueId,
        b: ValueId,
        opcode: u8,
        result: Option<ValueId>,
    ) {
        // Special case: same operand used twice (e.g., a - a, a % a)
        // Need to emit the value once, then DUP1 to have two copies
        if a == b {
            self.emit_value(func, a);
            self.asm.emit_op(opcodes::DUP1);
            self.scheduler.stack.dup(1);
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
            self.asm.emit_op(opcodes::SWAP1);
            self.scheduler.stack_swapped();
        } else if a_can_emit && !b_can_emit && has_untracked {
            // b is an untracked value on top of stack, emit a on top
            self.emit_value(func, a);
        } else if !a_can_emit && b_can_emit && has_untracked_at_1 {
            // a is an untracked value at depth 1, b is tracked on top
            // Stack is [b, a_untracked], need [a, b]
            // Just SWAP1 to get correct order
            self.asm.emit_op(opcodes::SWAP1);
            self.scheduler.stack_swapped();
        } else {
            // Normal case: emit b first (bottom), then a (top)
            self.emit_value(func, b);
            self.emit_value(func, a);
        }

        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(2, result);
    }

    /// Emits a unary operation with result tracking.
    fn emit_unary_op_with_result(
        &mut self,
        func: &Function,
        a: ValueId,
        opcode: u8,
        result: Option<ValueId>,
    ) {
        self.emit_value(func, a);
        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(1, result);
    }

    /// Emits a store operation (consumes both operands, no result).
    fn emit_store_op(&mut self, func: &Function, addr: ValueId, val: ValueId, opcode: u8) {
        self.emit_value(func, val);
        self.emit_value(func, addr);
        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(2, None);
    }

    /// Emits a store operation with liveness awareness.
    /// If the value operand is still live after this instruction, we DUP it first
    /// to preserve it on the stack for later use.
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
        // Check if val is still live after this instruction
        let val_is_live = !liveness.is_dead_after(val, block, inst_idx);
        // Check if addr is still live after this instruction
        let addr_is_live = !liveness.is_dead_after(addr, block, inst_idx);

        // Emit val
        self.emit_value(func, val);

        // If val is still live and is on top of the stack, we need to DUP it
        // before it gets consumed by the store operation
        if val_is_live && self.scheduler.stack.is_on_top(val) {
            self.asm.emit_op(opcodes::dup(1));
            self.scheduler.stack.dup(1);
        }

        // Emit addr
        self.emit_value(func, addr);

        // If addr is still live, DUP it too (rare but possible)
        if addr_is_live && self.scheduler.stack.is_on_top(addr) {
            self.asm.emit_op(opcodes::dup(1));
            self.scheduler.stack.dup(1);
        }

        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(2, None);
    }

    /// Emits a ternary operation.
    fn emit_ternary_op(&mut self, func: &Function, a: ValueId, b: ValueId, c: ValueId, opcode: u8) {
        self.emit_value(func, c);
        self.emit_value(func, b);
        self.emit_value(func, a);
        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(3, None);
    }

    /// Generates a parallel copy.
    fn generate_copy(
        &mut self,
        func: &Function,
        copy: &ParallelCopy,
        temps: &mut FxHashMap<u32, ()>,
    ) {
        // Handle source: either a MIR value or a temporary
        match &copy.src {
            CopySource::Value(val) => {
                self.emit_value(func, *val);
            }
            CopySource::Temp(temp_id) => {
                // Temporaries are kept on stack - this should DUP from the temp's position
                // For now, we assume temps are managed by the stack scheduler
                // This is a simplified implementation
                if !temps.contains_key(temp_id) {
                    // First reference to this temp - it should already be on stack
                    // from a prior copy to CopyDest::Temp
                }
            }
        }

        // Handle destination: either a MIR value or a temporary
        match &copy.dst {
            CopyDest::Value(_val) => {
                // The value is now on stack representing the destination
                // In a real implementation, we'd track that this stack slot
                // now represents the destination ValueId
            }
            CopyDest::Temp(temp_id) => {
                // Mark this temporary as defined - it's now on the stack
                temps.insert(*temp_id, ());
            }
        }
    }

    /// Pops all remaining values from the stack.
    /// This ensures the stack is empty before control flow transfer to another block.
    fn pop_all_stack_values(&mut self) {
        while self.scheduler.stack_depth() > 0 {
            self.asm.emit_op(opcodes::POP);
            self.scheduler.stack.pop();
        }
    }

    /// Generates bytecode for a terminator.
    fn generate_terminator(&mut self, func: &Function, term: &Terminator) {
        match term {
            Terminator::Jump(target) => {
                // Pop any remaining values from the stack before jumping.
                // Each block starts with an empty stack, so we must ensure the stack is
                // clean before jumping to another block (especially important for loops).
                self.pop_all_stack_values();
                self.asm.emit_push_label(self.block_labels[target]);
                self.asm.emit_op(opcodes::JUMP);
            }

            Terminator::Branch { condition, then_block, else_block } => {
                // Emit the condition first (before popping other values)
                self.emit_value(func, *condition);
                // Pop any remaining values EXCEPT the condition we just emitted
                // The condition is now on top, so we need to preserve it
                // Actually, after emit_value the condition is on top. We need to pop
                // everything underneath it, then use the condition.
                // For simplicity, we'll just emit and use immediately.
                self.asm.emit_push_label(self.block_labels[then_block]);
                self.asm.emit_op(opcodes::JUMPI);

                self.asm.emit_push_label(self.block_labels[else_block]);
                self.asm.emit_op(opcodes::JUMP);
            }

            Terminator::Switch { value: _, default, cases } => {
                for (case_val, target) in cases {
                    // DUP the value, compare, jump if equal
                    self.asm.emit_op(opcodes::dup(1));
                    self.emit_value(func, *case_val);
                    self.asm.emit_op(opcodes::EQ);
                    self.asm.emit_push_label(self.block_labels[target]);
                    self.asm.emit_op(opcodes::JUMPI);
                }

                // Pop the value and jump to default
                self.asm.emit_op(opcodes::POP);
                self.asm.emit_push_label(self.block_labels[default]);
                self.asm.emit_op(opcodes::JUMP);
            }

            Terminator::Return { values } => {
                if values.is_empty() {
                    self.asm.emit_push(U256::ZERO);
                    self.asm.emit_push(U256::ZERO);
                } else if values.len() == 1 {
                    // Single return value - simple case
                    self.emit_value(func, values[0]);
                    self.asm.emit_push(U256::ZERO);
                    self.asm.emit_op(opcodes::MSTORE);
                    self.asm.emit_push(U256::from(32));
                    self.asm.emit_push(U256::ZERO);
                } else {
                    // For multiple return values, we need to emit each value and store it
                    // at the correct memory offset (0, 32, 64, etc.).
                    //
                    // The tricky part is that emit_value uses DUP to get values onto the
                    // stack, and we need to properly track the scheduler state so each
                    // subsequent emit_value finds its value at the correct position.
                    let n = values.len();

                    // Emit each value and immediately store it. After each MSTORE,
                    // update the scheduler to reflect that we consumed the value.
                    for (i, &value) in values.iter().enumerate() {
                        self.emit_value(func, value);
                        self.asm.emit_push(U256::from(i * 32));
                        self.asm.emit_op(opcodes::MSTORE);
                        // The PUSH added an unknown value, and MSTORE consumed 2.
                        // Since emit_value used DUP, the original is still in the scheduler model.
                        // We need to tell scheduler that 1 value was consumed (the DUP'd copy).
                        self.scheduler.instruction_executed(1, None);
                    }

                    // Return size and offset
                    self.asm.emit_push(U256::from(n * 32));
                    self.asm.emit_push(U256::ZERO);
                }
                self.asm.emit_op(opcodes::RETURN);
            }

            Terminator::Revert { offset, size } => {
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.asm.emit_op(opcodes::REVERT);
            }

            Terminator::Stop => {
                self.asm.emit_op(opcodes::STOP);
            }

            Terminator::SelfDestruct { recipient } => {
                self.emit_value(func, *recipient);
                self.asm.emit_op(opcodes::SELFDESTRUCT);
            }

            Terminator::Invalid => {
                self.asm.emit_op(opcodes::INVALID);
            }
        }
    }
}

impl Default for EvmCodegen {
    fn default() -> Self {
        Self::new()
    }
}

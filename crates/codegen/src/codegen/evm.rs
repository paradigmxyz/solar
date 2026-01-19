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
    transform::{DeadCodeEliminator, JumpThreader},
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
        } else if runtime_len <= 0xFF {
            2 // PUSH1
        } else if runtime_len <= 0xFFFF {
            3 // PUSH2
        } else {
            4 // PUSH3
        };

        // We need to calculate deploy_code_len, but it depends on the push size for
        // deploy_code_len itself (circular). We solve by computing iteratively:
        // - Start with minimum offset_push_size = 2 (PUSH1 for offset up to 255)
        // - Compute deploy_code_len
        // - Adjust offset_push_size if needed
        //
        // Return code structure:
        // 1. PUSH runtime_len     (len_push_size bytes)
        // 2. DUP1                 (1 byte)
        // 3. PUSH deploy_code_len (offset_push_size bytes)
        // 4. PUSH0                (1 byte - memory destination)
        // 5. CODECOPY             (1 byte)
        // 6. PUSH0                (1 byte - return offset)
        // 7. RETURN               (1 byte)
        // Total: len_push_size + offset_push_size + 5

        // First estimate with PUSH1 for offset (2 bytes)
        let mut offset_push_size = 2;
        let mut deploy_code_len = constructor_code.len() + len_push_size + offset_push_size + 5;

        // Check if we need PUSH2 for the offset
        if deploy_code_len > 255 {
            offset_push_size = 3; // PUSH2
            deploy_code_len = constructor_code.len() + len_push_size + offset_push_size + 5;
        }

        // Check if we need PUSH3 for the offset (very large contracts)
        if deploy_code_len > 65535 {
            offset_push_size = 4; // PUSH3
            deploy_code_len = constructor_code.len() + len_push_size + offset_push_size + 5;
        }

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
        let mut jump_threader = JumpThreader::new();
        for func in module.functions.iter_mut() {
            // Run jump threading to eliminate unnecessary jumps (saves 8 gas per threaded jump)
            jump_threader.run_to_fixpoint(func);
            // Run DCE to remove dead code (including unreachable blocks after threading)
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

            // Reset stack at block entry - all cross-block values should be in spill slots
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

            // Spill all live-out values before the terminator so they can be reloaded
            // in successor blocks
            self.spill_live_out_values(func, &liveness, block_id);

            // Generate terminator
            if let Some(term) = &block.terminator {
                self.generate_terminator(func, term);
            }
        }
    }

    /// Spills all live-out values that are currently on the stack to memory.
    /// This ensures values that need to be accessed in successor blocks can be reloaded.
    fn spill_live_out_values(&mut self, func: &Function, liveness: &Liveness, block_id: BlockId) {
        let live_out = liveness.live_out(block_id);

        for val in live_out.iter() {
            self.spill_value_if_needed(func, val);
        }
    }

    /// Spills a single value to memory if it's on the stack and not already spilled.
    /// Skips immediates and args since they can be re-emitted without spilling.
    fn spill_value_if_needed(&mut self, func: &Function, val: ValueId) {
        // Skip immediates and args - they can be re-emitted without spilling
        match func.value(val) {
            crate::mir::Value::Immediate(_) | crate::mir::Value::Arg { .. } => return,
            _ => {}
        }

        // If the value is not already spilled, spill it
        if !self.scheduler.spills.is_spilled(val) {
            // Check if value is on the stack
            if let Some(depth) = self.scheduler.stack.find(val) {
                // Allocate a spill slot
                let slot = self.scheduler.spills.allocate(val);

                // DUP the value to top of stack for storing.
                // We need to DUP (not just use ensure_on_top) because:
                // 1. If value is on top, ensure_on_top does nothing but we need a copy
                // 2. MSTORE will consume the value, and we want to preserve the original
                let dup_n = (depth + 1) as u8;
                self.asm.emit_op(opcodes::dup(dup_n));
                self.scheduler.stack.dup(dup_n);

                // Store to spill slot: PUSH offset, MSTORE
                // The PUSH creates an untracked stack entry, so we track it as unknown
                self.asm.emit_push(U256::from(slot.byte_offset()));
                self.scheduler.stack.push_unknown();

                self.asm.emit_op(opcodes::MSTORE);
                // MSTORE consumes 2 values: the untracked offset and the DUP'd value
                self.scheduler.stack.pop(); // pop the untracked offset
                self.scheduler.stack.pop(); // pop the DUP'd value (original remains)
            }
        }
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
            if live_out.contains(op) {
                self.spill_value_if_needed(func, op);
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
        // Spill any operands that are live-out before they get consumed.
        // This ensures cross-block values are preserved in memory.
        let operands = kind.operands();
        self.spill_live_out_operands(func, liveness, block, &operands);

        match kind {
            // Binary arithmetic operations
            InstKind::Add(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::ADD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Sub(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::SUB,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Mul(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::MUL,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Div(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::DIV,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SDiv(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::SDIV,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Mod(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::MOD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SMod(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::SMOD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Exp(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::EXP,
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
                opcodes::AND,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Or(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::OR,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Xor(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::XOR,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Not(a) => self.emit_unary_op_with_result(
                func,
                *a,
                opcodes::NOT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Shl(shift, val) => self.emit_binary_op_with_result(
                func,
                *shift,
                *val,
                opcodes::SHL,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Shr(shift, val) => self.emit_binary_op_with_result(
                func,
                *shift,
                *val,
                opcodes::SHR,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Sar(shift, val) => self.emit_binary_op_with_result(
                func,
                *shift,
                *val,
                opcodes::SAR,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Byte(i, x) => self.emit_binary_op_with_result(
                func,
                *i,
                *x,
                opcodes::BYTE,
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
                opcodes::LT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Gt(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::GT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SLt(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::SLT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SGt(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::SGT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Eq(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                opcodes::EQ,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::IsZero(a) => self.emit_unary_op_with_result(
                func,
                *a,
                opcodes::ISZERO,
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
                opcodes::MLOAD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::MStore(addr, val) => self.emit_store_op_live_aware(
                func,
                *addr,
                *val,
                opcodes::MSTORE,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::MStore8(addr, val) => self.emit_store_op_live_aware(
                func,
                *addr,
                *val,
                opcodes::MSTORE8,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::MSize => {
                self.asm.emit_op(opcodes::MSIZE);
                self.scheduler.instruction_executed(0, result_value);
            }

            // Storage operations
            InstKind::SLoad(slot) => self.emit_unary_op_with_result(
                func,
                *slot,
                opcodes::SLOAD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SStore(slot, val) => self.emit_store_op_live_aware(
                func,
                *slot,
                *val,
                opcodes::SSTORE,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::TLoad(slot) => self.emit_unary_op_with_result(
                func,
                *slot,
                opcodes::TLOAD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::TStore(slot, val) => self.emit_store_op(func, *slot, *val, opcodes::TSTORE),

            // Calldata operations
            InstKind::CalldataLoad(off) => self.emit_unary_op_with_result(
                func,
                *off,
                opcodes::CALLDATALOAD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::CalldataSize => {
                self.asm.emit_op(opcodes::CALLDATASIZE);
                self.scheduler.instruction_executed(0, result_value);
            }

            // Hash operations
            InstKind::Keccak256(off, len) => self.emit_binary_op_with_result(
                func,
                *off,
                *len,
                opcodes::KECCAK256,
                result_value,
                liveness,
                block,
                inst_idx,
            ),

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
            InstKind::Balance(addr) => self.emit_unary_op_with_result(
                func,
                *addr,
                opcodes::BALANCE,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::BlockHash(num) => self.emit_unary_op_with_result(
                func,
                *num,
                opcodes::BLOCKHASH,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::BlobHash(idx) => self.emit_unary_op_with_result(
                func,
                *idx,
                opcodes::BLOBHASH,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::ExtCodeSize(addr) => self.emit_unary_op_with_result(
                func,
                *addr,
                opcodes::EXTCODESIZE,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::ExtCodeHash(addr) => self.emit_unary_op_with_result(
                func,
                *addr,
                opcodes::EXTCODEHASH,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
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
            InstKind::SignExtend(b, x) => self.emit_binary_op_with_result(
                func,
                *b,
                *x,
                opcodes::SIGNEXTEND,
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
                self.emit_value(func, *offset);
                self.emit_value(func, *value);
                self.asm.emit_op(opcodes::CREATE);
                // CREATE consumes 3 values and produces 1 (new contract address)
                self.scheduler.instruction_executed(3, result_value);
            }

            InstKind::Create2(value, offset, size, salt) => {
                // CREATE2 expects stack (top to bottom): salt, size, offset, value
                // So we push in reverse order: value first (goes deepest), then offset, size, salt
                self.emit_value(func, *value);
                self.emit_value(func, *offset);
                self.emit_value(func, *size);
                self.emit_value(func, *salt);
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
        let b_is_live = !liveness.is_dead_after(b, block, inst_idx);

        // Helper to check if a value can be re-emitted (immediates and args don't need DUP)
        let can_reemit = |v: ValueId| {
            matches!(func.value(v), crate::mir::Value::Immediate(_) | crate::mir::Value::Arg { .. })
        };

        // Special case: same operand used twice (e.g., a + a, a - a)
        if a == b {
            self.emit_value(func, a);
            // If the value is still live after this instruction and only appears once,
            // DUP it to preserve (if it appears more than once, a copy already exists).
            // Skip DUP for immediates/args since they can be re-emitted.
            if a_is_live && self.scheduler.stack.count(a) == 1 && !can_reemit(a) {
                self.asm.emit_op(opcodes::dup(1));
                self.scheduler.stack.dup(1);
            }
            // Now DUP for the second operand
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
            // DUP b if still live and this is the only copy.
            // Skip DUP for immediates/args since they can be re-emitted.
            if b_is_live && self.scheduler.stack.count(b) == 1 && !can_reemit(b) {
                self.asm.emit_op(opcodes::dup(1));
                self.scheduler.stack.dup(1);
            }
            self.asm.emit_op(opcodes::SWAP1);
            self.scheduler.stack_swapped();
        } else if a_can_emit && !b_can_emit && has_untracked {
            // b is an untracked value on top of stack, emit a on top
            self.emit_value(func, a);
            // DUP a if still live and this is the only copy.
            // Skip DUP for immediates/args since they can be re-emitted.
            if a_is_live && self.scheduler.stack.count(a) == 1 && !can_reemit(a) {
                self.asm.emit_op(opcodes::dup(1));
                self.scheduler.stack.dup(1);
            }
        } else if !a_can_emit && b_can_emit && has_untracked_at_1 {
            // a is an untracked value at depth 1, b is tracked on top
            // Stack is [b, a_untracked], need [a, b]
            // Just SWAP1 to get correct order
            self.asm.emit_op(opcodes::SWAP1);
            self.scheduler.stack_swapped();
        } else {
            // Normal case: emit b first (bottom), then a (top)
            self.emit_value(func, b);
            // DUP b if still live and this is the only copy.
            // Skip DUP for immediates/args since they can be re-emitted.
            if b_is_live && self.scheduler.stack.count(b) == 1 && !can_reemit(b) {
                self.asm.emit_op(opcodes::dup(1));
                self.scheduler.stack.dup(1);
            }
            self.emit_value(func, a);
            // DUP a if still live and this is the only copy.
            // Skip DUP for immediates/args since they can be re-emitted.
            if a_is_live && self.scheduler.stack.count(a) == 1 && !can_reemit(a) {
                self.asm.emit_op(opcodes::dup(1));
                self.scheduler.stack.dup(1);
            }
        }

        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(2, result);
    }

    /// Emits a unary operation with result tracking and liveness awareness.
    /// If the operand is still live after this instruction, we DUP it before it gets consumed.
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
        // Check if the operand is still live after this instruction
        let a_is_live = !liveness.is_dead_after(a, block, inst_idx);

        // Check if value can be re-emitted (immediates and args don't need DUP)
        let can_reemit = matches!(
            func.value(a),
            crate::mir::Value::Immediate(_) | crate::mir::Value::Arg { .. }
        );

        self.emit_value(func, a);

        // If the operand is still live and this is the only copy on stack, DUP to preserve
        // (if there are multiple copies, the operation consuming one still leaves others).
        // Skip DUP for immediates/args since they can be re-emitted.
        if a_is_live && self.scheduler.stack.count(a) == 1 && !can_reemit {
            self.asm.emit_op(opcodes::dup(1));
            self.scheduler.stack.dup(1);
        }

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

        // Helper to check if a value can be re-emitted (immediates and args don't need DUP)
        let can_reemit = |v: ValueId| {
            matches!(func.value(v), crate::mir::Value::Immediate(_) | crate::mir::Value::Arg { .. })
        };

        // Emit val
        self.emit_value(func, val);

        // If val is still live and this is the only copy on stack, we need to DUP it
        // before it gets consumed by the store operation.
        // Skip DUP for immediates/args since they can be re-emitted.
        if val_is_live && self.scheduler.stack.count(val) == 1 && !can_reemit(val) {
            self.asm.emit_op(opcodes::dup(1));
            self.scheduler.stack.dup(1);
        }

        // Emit addr
        self.emit_value(func, addr);

        // If addr is still live and this is the only copy, DUP it too (rare but possible).
        // Skip DUP for immediates/args since they can be re-emitted.
        // NOTE: DUPing addr here would corrupt the stack layout since it was just pushed
        // on top of val. For non-immediate/arg values that are live, the caller must
        // ensure they are available via spilling or other means.
        if addr_is_live && self.scheduler.stack.count(addr) == 1 && !can_reemit(addr) {
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
                self.emit_value(func, *val);
            }
            CopySource::Temp(temp_id) => {
                // Temporaries are tracked in our temps map with their ValueId
                if let Some(&temp_val) = temps.get(temp_id) {
                    // DUP the temp value to top of stack
                    if let Some(depth) = self.scheduler.stack.find(temp_val) {
                        let dup_n = (depth + 1) as u8;
                        self.asm.emit_op(opcodes::dup(dup_n));
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
                self.asm.emit_push(U256::from(slot.byte_offset()));
                self.scheduler.stack.push_unknown();
                self.asm.emit_op(opcodes::MSTORE);
                self.scheduler.stack.pop(); // pop the untracked offset
                self.scheduler.stack.pop(); // pop the value
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
                // Emit the condition first (it's still on the stack)
                self.emit_value(func, *condition);

                // Now pop all OTHER values (condition is on top, keep it)
                // We do this by tracking that condition was just pushed and emitting POPs for
                // everything else
                while self.scheduler.depth() > 1 {
                    // SWAP to get unwanted value to top, then POP
                    self.asm.emit_op(opcodes::SWAP1);
                    self.scheduler.stack_swapped();
                    self.asm.emit_op(opcodes::POP);
                    self.scheduler.stack.pop();
                }

                // JUMPI consumes the condition
                self.asm.emit_push_label(self.block_labels[then_block]);
                self.asm.emit_op(opcodes::JUMPI);
                self.scheduler.stack.pop(); // condition consumed by JUMPI

                self.asm.emit_push_label(self.block_labels[else_block]);
                self.asm.emit_op(opcodes::JUMP);
            }

            Terminator::Switch { value, default, cases } => {
                // Pop all stack values first (live-out values are already spilled)
                self.pop_all_stack_values();

                // Emit the switch value (will reload from spill if needed)
                self.emit_value(func, *value);

                for (case_val, target) in cases {
                    // DUP the value, compare, jump if equal
                    self.asm.emit_op(opcodes::DUP1);
                    self.scheduler.stack.dup(1);
                    self.emit_value(func, *case_val);
                    self.asm.emit_op(opcodes::EQ);
                    self.scheduler.instruction_executed(2, None); // EQ consumes 2, pushes 1
                    self.asm.emit_push_label(self.block_labels[target]);
                    self.asm.emit_op(opcodes::JUMPI);
                    self.scheduler.instruction_executed(1, None); // JUMPI consumes condition
                }

                // Pop the value and jump to default
                self.asm.emit_op(opcodes::POP);
                self.scheduler.stack.pop();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower;
    use solar_interface::Session;
    use solar_sema::Compiler;
    use std::{ops::ControlFlow, path::PathBuf};

    /// Helper to compile Solidity source to bytecode, returning Result.
    fn compile_source(source: &str) -> Result<Vec<u8>, String> {
        let sess = Session::builder().with_buffer_emitter(Default::default()).build();
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
                if contract.name.as_str() == "Test" {
                    let mut module = lower::lower_contract(gcx, contract_id);
                    let mut codegen = EvmCodegen::new();
                    let bytecode = codegen.generate_module(&mut module);
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
}

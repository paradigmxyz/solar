//! EVM bytecode generation from MIR.
//!
//! This module generates EVM bytecode from MIR using:
//! - Liveness analysis to know when values die
//! - Phi elimination to convert SSA to parallel copies
//! - Stack scheduling to generate DUP/SWAP sequences
//! - Two-pass assembly for label resolution

use crate::{
    analysis::{Liveness, ParallelCopy, eliminate_phis},
    codegen::{
        assembler::{Assembler, Label, opcodes},
        stack::{ScheduledOp, StackScheduler},
    },
    mir::{BlockId, Function, InstKind, Module, Terminator, ValueId},
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
        }
    }

    /// Generates bytecode for a module (runtime code only).
    pub fn generate_module(&mut self, module: &Module) -> Vec<u8> {
        self.generate_runtime_code(module)
    }

    /// Generates deployment bytecode for a module.
    /// Returns (deployment_bytecode, runtime_bytecode).
    pub fn generate_deployment_bytecode(&mut self, module: &Module) -> (Vec<u8>, Vec<u8>) {
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

            // Generate the constructor body (which includes SSTORE for initializers)
            self.generate_function_body(ctor);

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

    /// Generates runtime bytecode for a module.
    fn generate_runtime_code(&mut self, module: &Module) -> Vec<u8> {
        self.asm = Assembler::new();
        self.block_labels.clear();
        self.block_copies.clear();

        if !module.functions.is_empty() {
            // The dispatcher generates function bodies inline
            self.generate_dispatcher(module);
        }

        let result = std::mem::take(&mut self.asm).assemble();
        result.bytecode
    }

    /// Generates the function dispatcher.
    fn generate_dispatcher(&mut self, module: &Module) {
        // Load selector from calldata
        self.asm.emit_push(U256::ZERO);
        self.asm.emit_op(opcodes::CALLDATALOAD);
        self.asm.emit_push(U256::from(0xe0));
        self.asm.emit_op(opcodes::SHR);

        // Create labels for each function
        let mut func_labels: Vec<Label> = Vec::new();
        for _ in module.functions.iter() {
            func_labels.push(self.asm.new_label());
        }
        let fallback_label = self.asm.new_label();

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

        // No match - jump to fallback
        self.asm.emit_push_label(fallback_label);
        self.asm.emit_op(opcodes::JUMP);

        // Define function entry points
        for (i, func) in module.functions.iter().enumerate() {
            self.asm.define_label(func_labels[i]);
            self.asm.emit_op(opcodes::JUMPDEST);
            self.asm.emit_op(opcodes::POP); // Pop the selector

            // Emit payable check for non-payable functions
            // Non-payable, view, and pure functions must revert if called with value
            self.emit_payable_check(func);

            // Generate function body
            self.generate_function_body(func);
        }

        // Fallback - revert
        self.asm.define_label(fallback_label);
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
                for copy in &copies {
                    self.generate_copy(func, copy);
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
            InstKind::Not(a) => self.emit_unary_op(func, *a, opcodes::NOT),
            InstKind::Shl(shift, val) => self.emit_binary_op(func, *shift, *val, opcodes::SHL),
            InstKind::Shr(shift, val) => self.emit_binary_op(func, *shift, *val, opcodes::SHR),
            InstKind::Sar(shift, val) => self.emit_binary_op(func, *shift, *val, opcodes::SAR),
            InstKind::Byte(i, x) => self.emit_binary_op(func, *i, *x, opcodes::BYTE),

            // Comparison operations
            InstKind::Lt(a, b) => self.emit_binary_op(func, *a, *b, opcodes::LT),
            InstKind::Gt(a, b) => self.emit_binary_op(func, *a, *b, opcodes::GT),
            InstKind::SLt(a, b) => self.emit_binary_op(func, *a, *b, opcodes::SLT),
            InstKind::SGt(a, b) => self.emit_binary_op(func, *a, *b, opcodes::SGT),
            InstKind::Eq(a, b) => self.emit_binary_op(func, *a, *b, opcodes::EQ),
            InstKind::IsZero(a) => {
                self.emit_unary_op_with_result(func, *a, opcodes::ISZERO, result_value)
            }

            // Memory operations
            // Note: We don't track MLOAD results because they can become stale
            // after the memory location is modified (e.g., in loops).
            // The instruction sequence must ensure operands are in the right order.
            InstKind::MLoad(addr) => self.emit_unary_op(func, *addr, opcodes::MLOAD),
            InstKind::MStore(addr, val) => self.emit_store_op(func, *addr, *val, opcodes::MSTORE),
            InstKind::MStore8(addr, val) => self.emit_store_op(func, *addr, *val, opcodes::MSTORE8),
            InstKind::MSize => self.asm.emit_op(opcodes::MSIZE),

            // Storage operations
            InstKind::SLoad(slot) => {
                self.emit_unary_op_with_result(func, *slot, opcodes::SLOAD, result_value)
            }
            InstKind::SStore(slot, val) => self.emit_store_op(func, *slot, *val, opcodes::SSTORE),
            InstKind::TLoad(slot) => self.emit_unary_op(func, *slot, opcodes::TLOAD),
            InstKind::TStore(slot, val) => self.emit_store_op(func, *slot, *val, opcodes::TSTORE),

            // Calldata operations
            InstKind::CalldataLoad(off) => self.emit_unary_op(func, *off, opcodes::CALLDATALOAD),
            InstKind::CalldataSize => self.asm.emit_op(opcodes::CALLDATASIZE),

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
            InstKind::Balance(addr) => self.emit_unary_op(func, *addr, opcodes::BALANCE),
            InstKind::BlockHash(num) => self.emit_unary_op(func, *num, opcodes::BLOCKHASH),
            InstKind::BlobHash(idx) => self.emit_unary_op(func, *idx, opcodes::BLOBHASH),
            InstKind::ExtCodeSize(addr) => self.emit_unary_op(func, *addr, opcodes::EXTCODESIZE),
            InstKind::ExtCodeHash(addr) => self.emit_unary_op(func, *addr, opcodes::EXTCODEHASH),
            InstKind::CodeSize => self.asm.emit_op(opcodes::CODESIZE),
            InstKind::ReturnDataSize => self.asm.emit_op(opcodes::RETURNDATASIZE),

            // Ternary operations
            InstKind::AddMod(a, b, n) => self.emit_ternary_op(func, *a, *b, *n, opcodes::ADDMOD),
            InstKind::MulMod(a, b, n) => self.emit_ternary_op(func, *a, *b, *n, opcodes::MULMOD),

            // Select is like a ternary conditional
            InstKind::Select(cond, true_val, false_val) => {
                // select(cond, t, f) = f + cond * (t - f)
                // Or use JUMPI-based approach
                self.emit_value(func, *false_val);
                self.emit_value(func, *true_val);
                self.emit_value(func, *cond);
                // Stack: [cond, true_val, false_val]
                // Use conditional swap approach
                // This is simplified - proper impl uses JUMPI
            }

            // Sign extend
            InstKind::SignExtend(b, x) => self.emit_binary_op(func, *b, *x, opcodes::SIGNEXTEND),

            // Phi nodes are skipped (handled by copies)
            InstKind::Phi(_) => {}

            // Contract creation
            InstKind::Create(value, offset, size) => {
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.emit_value(func, *value);
                self.asm.emit_op(opcodes::CREATE);
                self.scheduler.instruction_executed(3, None);
            }

            InstKind::Create2(value, offset, size, salt) => {
                self.emit_value(func, *salt);
                self.emit_value(func, *size);
                self.emit_value(func, *offset);
                self.emit_value(func, *value);
                self.asm.emit_op(opcodes::CREATE2);
                self.scheduler.instruction_executed(4, None);
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

            // TODO: Implement remaining operations
            _ => {}
        }

        // Drop dead values after the instruction
        let dead_ops = self.scheduler.drop_dead_values(liveness, block, inst_idx);
        for op in dead_ops {
            self.asm.emit_op(op.opcode());
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
                    // Load function argument from calldata
                    // ABI encoding: selector (4 bytes) + args (32 bytes each)
                    // Offset = 4 + index * 32
                    let offset = 4 + (*index as u64) * 32;
                    self.asm.emit_push(U256::from(offset));
                    self.asm.emit_op(opcodes::CALLDATALOAD);
                }
            }
        }
    }

    /// Emits a binary operation.
    fn emit_binary_op(&mut self, func: &Function, a: ValueId, b: ValueId, opcode: u8) {
        // Use the common logic with no result tracking
        self.emit_binary_op_with_result(func, a, b, opcode, None);
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
        // Check if either operand is already on stack as an untracked value
        let a_can_emit = self.scheduler.can_emit_value(a, func);
        let b_can_emit = self.scheduler.can_emit_value(b, func);
        let has_untracked = self.scheduler.has_untracked_on_top();
        let has_untracked_at_1 = self.scheduler.has_untracked_at_depth(1);

        if !a_can_emit && b_can_emit && has_untracked {
            // a is an untracked value on top of stack, emit b, then SWAP
            self.emit_value(func, b);
            self.asm.emit_op(opcodes::SWAP1);
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

    /// Emits a unary operation.
    /// Note: This tracks an "unknown" value on the stack to maintain correct depth.
    /// For operations that need their result tracked with a specific ValueId,
    /// use emit_unary_op_with_result.
    fn emit_unary_op(&mut self, func: &Function, a: ValueId, opcode: u8) {
        self.emit_value(func, a);
        self.asm.emit_op(opcode);
        // Pop the input and push an unknown value (for MLOAD where result may become stale)
        self.scheduler.instruction_executed_untracked(1);
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

    /// Emits a ternary operation.
    fn emit_ternary_op(&mut self, func: &Function, a: ValueId, b: ValueId, c: ValueId, opcode: u8) {
        self.emit_value(func, c);
        self.emit_value(func, b);
        self.emit_value(func, a);
        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(3, None);
    }

    /// Generates a parallel copy.
    fn generate_copy(&mut self, func: &Function, copy: &ParallelCopy) {
        // Simply push the source value - it becomes the destination
        self.emit_value(func, copy.src);
        // In a real implementation, we'd need to track that this value
        // now represents the destination ValueId
    }

    /// Generates bytecode for a terminator.
    fn generate_terminator(&mut self, func: &Function, term: &Terminator) {
        match term {
            Terminator::Jump(target) => {
                self.asm.emit_push_label(self.block_labels[target]);
                self.asm.emit_op(opcodes::JUMP);
            }

            Terminator::Branch { condition, then_block, else_block } => {
                self.emit_value(func, *condition);
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
                } else {
                    // Store return values in memory and return
                    for (i, &val) in values.iter().enumerate() {
                        self.emit_value(func, val);
                        self.asm.emit_push(U256::from(i * 32));
                        self.asm.emit_op(opcodes::MSTORE);
                    }
                    self.asm.emit_push(U256::from(values.len() * 32));
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

//! Two-pass assembler with label resolution.
//!
//! The assembler handles:
//! - Label definition and reference tracking
//! - Two-pass assembly for resolving jump targets
//! - Variable-width PUSH sizing based on offset magnitudes

use alloy_primitives::U256;
use rustc_hash::FxHashMap;

/// A label identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Label(pub u32);

/// An instruction in the assembler.
#[derive(Clone, Debug)]
pub enum AsmInst {
    /// A raw opcode with no operands.
    Op(u8),
    /// Push an immediate value (will be sized appropriately).
    Push(U256),
    /// Push a label reference (will be resolved to offset).
    PushLabel(Label),
    /// Define a label at this position.
    Label(Label),
}

/// Result of assembly.
#[derive(Debug)]
pub struct AssembledCode {
    /// The final bytecode.
    pub bytecode: Vec<u8>,
    /// Map from label to its final offset.
    pub label_offsets: FxHashMap<Label, usize>,
}

/// Two-pass assembler for EVM bytecode.
#[derive(Debug)]
pub struct Assembler {
    /// Instructions to assemble.
    instructions: Vec<AsmInst>,
    /// Next label ID.
    next_label: u32,
}

impl Assembler {
    /// Creates a new assembler.
    #[must_use]
    pub fn new() -> Self {
        Self { instructions: Vec::new(), next_label: 0 }
    }

    /// Creates a new label.
    pub fn new_label(&mut self) -> Label {
        let label = Label(self.next_label);
        self.next_label += 1;
        label
    }

    /// Emits a raw opcode.
    pub fn emit_op(&mut self, opcode: u8) {
        self.instructions.push(AsmInst::Op(opcode));
    }

    /// Emits a push instruction with an immediate value.
    pub fn emit_push(&mut self, value: U256) {
        self.instructions.push(AsmInst::Push(value));
    }

    /// Emits a push instruction that will be resolved to a label's offset.
    pub fn emit_push_label(&mut self, label: Label) {
        self.instructions.push(AsmInst::PushLabel(label));
    }

    /// Defines a label at the current position.
    pub fn define_label(&mut self, label: Label) {
        self.instructions.push(AsmInst::Label(label));
    }

    /// Assembles the instructions into bytecode.
    /// Uses an iterative two-pass algorithm that handles PUSH width changes.
    #[must_use]
    pub fn assemble(self) -> AssembledCode {
        // We need to iterate until PUSH widths stabilize
        let mut push_widths: FxHashMap<usize, u8> = FxHashMap::default();

        // Initialize all label pushes to 2 bytes (PUSH2)
        for (idx, inst) in self.instructions.iter().enumerate() {
            if matches!(inst, AsmInst::PushLabel(_)) {
                push_widths.insert(idx, 2);
            }
        }

        // Iterate until stable
        let max_iterations = 10;
        for _ in 0..max_iterations {
            let (label_offsets, new_widths) = self.compute_offsets(&push_widths);

            let mut changed = false;
            for (idx, &width) in &new_widths {
                if push_widths.get(idx) != Some(&width) {
                    changed = true;
                }
            }

            if !changed {
                // Stable - emit final bytecode
                return self.emit_bytecode(&label_offsets, &push_widths);
            }

            for (idx, width) in new_widths {
                push_widths.insert(idx, width);
            }
        }

        // Fallback - just emit with current widths
        let (label_offsets, _) = self.compute_offsets(&push_widths);
        self.emit_bytecode(&label_offsets, &push_widths)
    }

    /// Computes label offsets given current PUSH widths.
    fn compute_offsets(
        &self,
        push_widths: &FxHashMap<usize, u8>,
    ) -> (FxHashMap<Label, usize>, FxHashMap<usize, u8>) {
        let mut offset = 0usize;
        let mut label_offsets = FxHashMap::default();
        let mut new_widths = FxHashMap::default();

        for (idx, inst) in self.instructions.iter().enumerate() {
            match inst {
                AsmInst::Op(_) => {
                    offset += 1;
                }
                AsmInst::Push(value) => {
                    let width = push_width(*value);
                    offset += 1 + width as usize;
                }
                AsmInst::PushLabel(_) => {
                    // Use current estimated width
                    let width = push_widths.get(&idx).copied().unwrap_or(2);
                    offset += 1 + width as usize;
                }
                AsmInst::Label(label) => {
                    label_offsets.insert(*label, offset);
                }
            }
        }

        // Compute new widths based on resolved offsets
        for (idx, inst) in self.instructions.iter().enumerate() {
            if let AsmInst::PushLabel(label) = inst
                && let Some(&target_offset) = label_offsets.get(label)
            {
                let width = push_width(U256::from(target_offset));
                new_widths.insert(idx, width);
            }
        }

        (label_offsets, new_widths)
    }

    /// Emits the final bytecode.
    fn emit_bytecode(
        &self,
        label_offsets: &FxHashMap<Label, usize>,
        push_widths: &FxHashMap<usize, u8>,
    ) -> AssembledCode {
        let mut bytecode = Vec::new();

        for (idx, inst) in self.instructions.iter().enumerate() {
            match inst {
                AsmInst::Op(opcode) => {
                    bytecode.push(*opcode);
                }
                AsmInst::Push(value) => {
                    emit_push_value(&mut bytecode, *value);
                }
                AsmInst::PushLabel(label) => {
                    let target_offset = label_offsets.get(label).copied().unwrap_or(0);
                    let width = push_widths.get(&idx).copied().unwrap_or(2);
                    emit_push_fixed_width(&mut bytecode, U256::from(target_offset), width);
                }
                AsmInst::Label(_) => {
                    // Labels don't emit anything - they just mark positions
                    // JUMPDEST is emitted separately if needed
                }
            }
        }

        AssembledCode { bytecode, label_offsets: label_offsets.clone() }
    }
}

impl Default for Assembler {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns the number of bytes needed to push a value.
fn push_width(value: U256) -> u8 {
    if value.is_zero() {
        return 0; // PUSH0
    }

    let bytes = value.to_be_bytes::<32>();
    let first_nonzero = bytes.iter().position(|&b| b != 0).unwrap_or(32);
    (32 - first_nonzero) as u8
}

/// Emits a PUSH instruction with automatically sized width.
fn emit_push_value(bytecode: &mut Vec<u8>, value: U256) {
    if value.is_zero() {
        bytecode.push(0x5f); // PUSH0
        return;
    }

    let width = push_width(value);
    emit_push_fixed_width(bytecode, value, width);
}

/// Emits a PUSH instruction with a specific width.
fn emit_push_fixed_width(bytecode: &mut Vec<u8>, value: U256, width: u8) {
    if width == 0 {
        bytecode.push(0x5f); // PUSH0
        return;
    }

    // PUSH1 = 0x60, PUSH2 = 0x61, ..., PUSH32 = 0x7f
    bytecode.push(0x5f + width);

    let bytes = value.to_be_bytes::<32>();
    let start = 32 - width as usize;
    bytecode.extend_from_slice(&bytes[start..]);
}

/// Common EVM opcodes.
pub mod opcodes {
    pub const STOP: u8 = 0x00;
    pub const ADD: u8 = 0x01;
    pub const MUL: u8 = 0x02;
    pub const SUB: u8 = 0x03;
    pub const DIV: u8 = 0x04;
    pub const SDIV: u8 = 0x05;
    pub const MOD: u8 = 0x06;
    pub const SMOD: u8 = 0x07;
    pub const ADDMOD: u8 = 0x08;
    pub const MULMOD: u8 = 0x09;
    pub const EXP: u8 = 0x0a;
    pub const SIGNEXTEND: u8 = 0x0b;

    pub const LT: u8 = 0x10;
    pub const GT: u8 = 0x11;
    pub const SLT: u8 = 0x12;
    pub const SGT: u8 = 0x13;
    pub const EQ: u8 = 0x14;
    pub const ISZERO: u8 = 0x15;
    pub const AND: u8 = 0x16;
    pub const OR: u8 = 0x17;
    pub const XOR: u8 = 0x18;
    pub const NOT: u8 = 0x19;
    pub const BYTE: u8 = 0x1a;
    pub const SHL: u8 = 0x1b;
    pub const SHR: u8 = 0x1c;
    pub const SAR: u8 = 0x1d;

    pub const KECCAK256: u8 = 0x20;

    pub const ADDRESS: u8 = 0x30;
    pub const BALANCE: u8 = 0x31;
    pub const ORIGIN: u8 = 0x32;
    pub const CALLER: u8 = 0x33;
    pub const CALLVALUE: u8 = 0x34;
    pub const CALLDATALOAD: u8 = 0x35;
    pub const CALLDATASIZE: u8 = 0x36;
    pub const CALLDATACOPY: u8 = 0x37;
    pub const CODESIZE: u8 = 0x38;
    pub const CODECOPY: u8 = 0x39;
    pub const GASPRICE: u8 = 0x3a;
    pub const EXTCODESIZE: u8 = 0x3b;
    pub const EXTCODECOPY: u8 = 0x3c;
    pub const RETURNDATASIZE: u8 = 0x3d;
    pub const RETURNDATACOPY: u8 = 0x3e;
    pub const EXTCODEHASH: u8 = 0x3f;

    pub const BLOCKHASH: u8 = 0x40;
    pub const COINBASE: u8 = 0x41;
    pub const TIMESTAMP: u8 = 0x42;
    pub const NUMBER: u8 = 0x43;
    pub const PREVRANDAO: u8 = 0x44;
    pub const GASLIMIT: u8 = 0x45;
    pub const CHAINID: u8 = 0x46;
    pub const SELFBALANCE: u8 = 0x47;
    pub const BASEFEE: u8 = 0x48;
    pub const BLOBHASH: u8 = 0x49;
    pub const BLOBBASEFEE: u8 = 0x4a;

    pub const POP: u8 = 0x50;
    pub const MLOAD: u8 = 0x51;
    pub const MSTORE: u8 = 0x52;
    pub const MSTORE8: u8 = 0x53;
    pub const SLOAD: u8 = 0x54;
    pub const SSTORE: u8 = 0x55;
    pub const JUMP: u8 = 0x56;
    pub const JUMPI: u8 = 0x57;
    pub const PC: u8 = 0x58;
    pub const MSIZE: u8 = 0x59;
    pub const GAS: u8 = 0x5a;
    pub const JUMPDEST: u8 = 0x5b;
    pub const TLOAD: u8 = 0x5c;
    pub const TSTORE: u8 = 0x5d;
    pub const MCOPY: u8 = 0x5e;
    pub const PUSH0: u8 = 0x5f;

    pub const DUP1: u8 = 0x80;
    pub const SWAP1: u8 = 0x90;

    pub const LOG0: u8 = 0xa0;
    pub const LOG1: u8 = 0xa1;
    pub const LOG2: u8 = 0xa2;
    pub const LOG3: u8 = 0xa3;
    pub const LOG4: u8 = 0xa4;

    pub const CREATE: u8 = 0xf0;
    pub const CALL: u8 = 0xf1;
    pub const CALLCODE: u8 = 0xf2;
    pub const RETURN: u8 = 0xf3;
    pub const DELEGATECALL: u8 = 0xf4;
    pub const CREATE2: u8 = 0xf5;
    pub const STATICCALL: u8 = 0xfa;
    pub const REVERT: u8 = 0xfd;
    pub const INVALID: u8 = 0xfe;
    pub const SELFDESTRUCT: u8 = 0xff;

    /// Returns the DUP opcode for the given depth (1-16).
    #[must_use]
    pub const fn dup(n: u8) -> u8 {
        debug_assert!(n >= 1 && n <= 16);
        DUP1 + n - 1
    }

    /// Returns the SWAP opcode for the given depth (1-16).
    #[must_use]
    pub const fn swap(n: u8) -> u8 {
        debug_assert!(n >= 1 && n <= 16);
        SWAP1 + n - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_width() {
        assert_eq!(push_width(U256::ZERO), 0);
        assert_eq!(push_width(U256::from(1)), 1);
        assert_eq!(push_width(U256::from(255)), 1);
        assert_eq!(push_width(U256::from(256)), 2);
        assert_eq!(push_width(U256::from(0xFFFF)), 2);
        assert_eq!(push_width(U256::from(0x10000)), 3);
    }

    #[test]
    fn test_simple_assembly() {
        let mut asm = Assembler::new();

        asm.emit_push(U256::from(42));
        asm.emit_push(U256::from(10));
        asm.emit_op(opcodes::ADD);
        asm.emit_op(opcodes::STOP);

        let result = asm.assemble();

        // PUSH1 42, PUSH1 10, ADD, STOP
        assert_eq!(result.bytecode, vec![0x60, 42, 0x60, 10, 0x01, 0x00]);
    }

    #[test]
    fn test_label_resolution() {
        let mut asm = Assembler::new();

        let loop_label = asm.new_label();
        let end_label = asm.new_label();

        asm.define_label(loop_label);
        asm.emit_op(opcodes::JUMPDEST);
        asm.emit_push(U256::from(1));
        asm.emit_push_label(end_label);
        asm.emit_op(opcodes::JUMPI);
        asm.emit_push_label(loop_label);
        asm.emit_op(opcodes::JUMP);

        asm.define_label(end_label);
        asm.emit_op(opcodes::JUMPDEST);
        asm.emit_op(opcodes::STOP);

        let result = asm.assemble();

        // Check labels were resolved
        assert!(result.label_offsets.contains_key(&loop_label));
        assert!(result.label_offsets.contains_key(&end_label));
        assert_eq!(result.label_offsets[&loop_label], 0);
    }
}

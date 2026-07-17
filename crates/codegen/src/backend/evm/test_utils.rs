use super::assembler::op;
use std::fmt::Write;

pub(crate) fn disassemble(bytecode: &[u8]) -> String {
    let mut output = String::with_capacity(bytecode.len().saturating_mul(8));
    let mut offset = 0;

    while offset < bytecode.len() {
        let opcode = bytecode[offset];
        offset += 1;

        if (op::PUSH1..=op::PUSH32).contains(&opcode) {
            let width = usize::from(opcode - op::PUSH1 + 1);
            write!(output, "PUSH{width} 0x").unwrap();
            let end = offset.saturating_add(width).min(bytecode.len());
            for byte in &bytecode[offset..end] {
                write!(output, "{byte:02x}").unwrap();
            }
            offset = end;
        } else if let Some(mnemonic) = op::mnemonic(opcode) {
            output.extend(mnemonic.bytes().map(|byte| char::from(byte.to_ascii_uppercase())));
        } else {
            write!(output, "UNKNOWN 0x{opcode:02x}").unwrap();
        }
        output.push('\n');
    }

    output
}

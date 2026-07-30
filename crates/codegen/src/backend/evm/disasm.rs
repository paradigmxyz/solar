//! EVM bytecode disassembly.

use super::op;
use solar_config::EvmVersion;
use std::fmt::Write;

/// Disassembles EVM bytecode into one opcode per line.
pub fn disassemble(bytecode: &[u8]) -> String {
    let mut output = String::with_capacity(bytecode.len().saturating_mul(8));

    for instruction in instructions(bytecode) {
        if instruction.push_width != 0 {
            let width = instruction.push_width;
            write!(output, "PUSH{width} 0x").unwrap();
            for byte in instruction.data {
                write!(output, "{byte:02x}").unwrap();
            }
        } else if let Some(mnemonic) = op::mnemonic(instruction.opcode) {
            output.extend(mnemonic.bytes().map(|byte| char::from(byte.to_ascii_uppercase())));
        } else {
            write!(output, "UNKNOWN 0x{:02x}", instruction.opcode).unwrap();
        }
        output.push('\n');
    }

    output
}

/// Disassembles EVM bytecode in the format used by solc's Standard JSON output.
pub fn disassemble_standard_json(bytecode: &[u8], evm_version: EvmVersion) -> String {
    let mut output = String::with_capacity(bytecode.len().saturating_mul(8));

    for instruction in instructions(bytecode) {
        if instruction.push_width != 0 {
            let width = instruction.push_width;
            write!(output, "PUSH{width} 0x").unwrap();
            if let Some(first) = instruction.data.iter().position(|byte| *byte != 0) {
                write!(output, "{:X}", instruction.data[first]).unwrap();
                for byte in &instruction.data[first + 1..] {
                    write!(output, "{byte:02X}").unwrap();
                }
                for _ in instruction.data.len()..usize::from(width) {
                    output.push_str("00");
                }
            } else {
                output.push('0');
            }
        } else if let Some(mnemonic) = standard_json_mnemonic(instruction.opcode, evm_version) {
            output.extend(mnemonic.bytes().map(|byte| char::from(byte.to_ascii_uppercase())));
        } else {
            write!(output, "0x{:X}", instruction.opcode).unwrap();
        }
        output.push(' ');
    }

    output
}

struct DecodedInstruction<'a> {
    opcode: u8,
    push_width: u8,
    data: &'a [u8],
}

fn instructions(bytecode: &[u8]) -> impl Iterator<Item = DecodedInstruction<'_>> {
    let mut offset = 0;
    std::iter::from_fn(move || {
        let &opcode = bytecode.get(offset)?;
        offset += 1;

        let push_width =
            if (op::PUSH1..=op::PUSH32).contains(&opcode) { opcode - op::PUSH1 + 1 } else { 0 };
        let end = offset.saturating_add(usize::from(push_width)).min(bytecode.len());
        let data = &bytecode[offset..end];
        offset = end;
        Some(DecodedInstruction { opcode, push_width, data })
    })
}

fn standard_json_mnemonic(opcode: u8, evm_version: EvmVersion) -> Option<&'static str> {
    if matches!(
        opcode,
        op::DATALOAD
            | op::DATALOADN
            | op::DATASIZE
            | op::DATACOPY
            | op::RJUMP
            | op::RJUMPI
            | op::RJUMPV
            | op::CALLF
            | op::RETF
            | op::JUMPF
            | op::DUPN
            | op::SWAPN
            | op::EXCHANGE
            | op::EOFCREATE
            | op::RETURNCONTRACT
            | op::RETURNDATALOAD
            | op::EXTCALL
            | op::EXTDELEGATECALL
            | op::EXTSTATICCALL
    ) {
        None
    } else if opcode == op::PREVRANDAO && evm_version < EvmVersion::Paris {
        Some("difficulty")
    } else {
        op::mnemonic(opcode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snapbox::{assert_data_eq, str};

    #[test]
    fn standard_json_matches_solc_format() {
        let actual = format!(
            "pushes: {:?}\nunknown: {:?}\npre-paris: {:?}\nparis: {:?}\n",
            disassemble_standard_json(&[op::PUSH2, 0x01, 0x20, op::PUSH2, 0x01], EvmVersion::Osaka,),
            disassemble_standard_json(&[0x0c, op::DATALOAD], EvmVersion::Osaka),
            disassemble_standard_json(&[op::PREVRANDAO], EvmVersion::Homestead),
            disassemble_standard_json(&[op::PREVRANDAO], EvmVersion::Paris),
        );
        assert_data_eq!(
            actual,
            str![[r#"
pushes: "PUSH2 0x120 PUSH2 0x100 "
unknown: "0xC 0xD0 "
pre-paris: "DIFFICULTY "
paris: "PREVRANDAO "

"#]]
        );
    }
}

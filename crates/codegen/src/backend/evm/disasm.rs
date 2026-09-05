//! Bytecode presentation in human and Standard JSON conventions.
//!
//! Both formats walk arbitrary bytes without requiring executable code. PUSH data
//! is never decoded as instructions. Human output preserves the available bytes
//! of truncated literals; Standard JSON zero-pads them to the declared width.
//! Invalid EIP-8024 immediates remain separate instructions so jump destinations
//! retain their legacy interpretation.

use super::op;
use alloy_primitives::U256;
use solar_config::EvmVersion;
use std::fmt::Write;

/// Formats bytecode as one instruction per line, with byte-exact lowercase literals.
pub fn disassemble(bytecode: &[u8], evm_version: EvmVersion) -> String {
    format(bytecode, evm_version, false)
}

/// Formats space-separated opcodes and numeric uppercase literals for Standard JSON.
pub fn disassemble_standard_json(bytecode: &[u8], evm_version: EvmVersion) -> String {
    format(bytecode, evm_version, true)
}

fn format(bytecode: &[u8], evm_version: EvmVersion, standard_json: bool) -> String {
    let mut output = String::new();
    let labels = if standard_json { Vec::new() } else { jump_destinations(bytecode, evm_version) };
    let mut pc = 0;
    let mut previous_push = false;
    while let Some(&opcode) = bytecode.get(pc) {
        if !standard_json && opcode == op::JUMPDEST {
            let label = labels.binary_search(&pc).unwrap();
            let _ = writeln!(output, "; bb{label}");
        }
        pc += 1;
        let mut pushed = (opcode == op::PUSH0).then_some(U256::ZERO);
        if (op::PUSH1..=op::PUSH32).contains(&opcode) {
            let width = usize::from(opcode - op::PUSH0);
            let available = width.min(bytecode.len() - pc);
            let bytes = &bytecode[pc..pc + available];
            let _ = write!(output, "PUSH{width} 0x");
            let mut word = [0; 32];
            word[32 - width..32 - width + available].copy_from_slice(bytes);
            let value = U256::from_be_bytes(word);
            pushed = Some(value);
            if standard_json {
                let _ = write!(output, "{value:X}");
            } else {
                for byte in bytes {
                    let _ = write!(output, "{byte:02x}");
                }
            }
            pc += available;
        } else if matches!(opcode, op::DUPN | op::SWAPN | op::EXCHANGE)
            && evm_version.has_extended_stack_ops()
        {
            let immediate = bytecode.get(pc).copied().unwrap_or_default();
            let name = op::name(opcode).unwrap();
            let operands = if opcode == op::EXCHANGE {
                op::decode_exchange(immediate).map(|(a, b)| format!("{a}, {b}"))
            } else {
                op::decode_depth(immediate).map(|depth| depth.to_string())
            };
            if let Some(operands) = operands {
                let _ = write!(output, "{name} {operands}");
                pc += usize::from(pc < bytecode.len());
            } else {
                let _ = write!(output, "INVALID_{name}");
            }
        } else if matches!(opcode, op::DUPN | op::SWAPN | op::EXCHANGE)
            || op::name(opcode).is_none()
        {
            if standard_json {
                let _ = write!(output, "0x{opcode:X}");
            } else {
                let _ = write!(output, "UNKNOWN 0x{opcode:02x}");
            }
        } else {
            output.push_str(if opcode == op::PREVRANDAO && !evm_version.has_prev_randao() {
                "DIFFICULTY"
            } else {
                op::name(opcode).unwrap()
            });
        }
        if !standard_json
            && let Some(value) = pushed
            && matches!(bytecode.get(pc), Some(&op::JUMP | &op::JUMPI))
        {
            let label =
                usize::try_from(value).ok().and_then(|target| labels.binary_search(&target).ok());
            if let Some(label) = label {
                let _ = write!(output, " ; bb{label}");
            } else {
                output.push_str(" ; unknown");
            }
        }
        if !standard_json && matches!(opcode, op::JUMP | op::JUMPI) && !previous_push {
            output.push_str(" ; unknown");
        }
        previous_push = pushed.is_some();
        output.push(if standard_json { ' ' } else { '\n' });
    }
    output
}

/// Finds actual instruction-boundary destinations, including after invalid extended opcodes.
fn jump_destinations(bytecode: &[u8], evm_version: EvmVersion) -> Vec<usize> {
    let mut destinations = Vec::new();
    let mut pc = 0;
    while let Some(&opcode) = bytecode.get(pc) {
        if opcode == op::JUMPDEST {
            destinations.push(pc);
        }
        pc += 1;
        if (op::PUSH1..=op::PUSH32).contains(&opcode) {
            pc += usize::from(opcode - op::PUSH0).min(bytecode.len() - pc);
        } else if evm_version.has_extended_stack_ops() {
            let immediate = bytecode.get(pc).copied().unwrap_or_default();
            let valid = match opcode {
                op::DUPN | op::SWAPN => op::decode_depth(immediate).is_some(),
                op::EXCHANGE => op::decode_exchange(immediate).is_some(),
                _ => false,
            };
            pc += usize::from(valid && pc < bytecode.len());
        }
    }
    destinations
}

#[cfg(test)]
mod tests {
    use super::*;
    use snapbox::{assert_data_eq, str};

    #[test]
    fn presentation_conventions() {
        let bytes = [op::PUSH1, 0x0a, op::PREVRANDAO, 0x22, 0x61, 0x0a];
        assert_data_eq!(
            disassemble(&bytes, EvmVersion::Berlin),
            str![[r#"
PUSH1 0x0a
DIFFICULTY
UNKNOWN 0x22
PUSH2 0x0a

"#]]
        );
        assert_data_eq!(
            disassemble_standard_json(&bytes, EvmVersion::Berlin),
            str![["PUSH1 0xA DIFFICULTY 0x22 PUSH2 0xA00 "]]
        );
    }

    #[test]
    fn jump_annotations_use_instruction_boundaries() {
        let bytes = [op::JUMPDEST, op::PUSH0, op::JUMPI, op::PUSH1, 0x5b, op::JUMP, op::JUMP];
        assert_data_eq!(
            disassemble(&bytes, EvmVersion::Shanghai),
            str![[r#"
; bb0
JUMPDEST
PUSH0 ; bb0
JUMPI
PUSH1 0x5b ; unknown
JUMP
JUMP ; unknown

"#]]
        );
    }

    #[test]
    fn extended_immediates_preserve_instruction_boundaries() {
        let bytes = [op::DUPN, 0x5b, op::DUPN, 0x80, op::EXCHANGE, 0x80, op::EXCHANGE, 0x52];
        assert_data_eq!(
            disassemble(&bytes, EvmVersion::Amsterdam),
            str![[r#"
INVALID_DUPN
; bb0
JUMPDEST
DUPN 17
EXCHANGE 1, 16
INVALID_EXCHANGE
MSTORE

"#]]
        );
        assert_data_eq!(
            disassemble(&[op::DUPN, 0x80], EvmVersion::Osaka),
            str![[r#"
UNKNOWN 0xe6
DUP1

"#]]
        );
    }
}

//! EVM bytecode disassembly.

use super::op;
use evm2::{SpecId, bytecode::BasicBlocks};
use solar_config::EvmVersion;
use std::fmt::Write;

/// Disassembles EVM bytecode into labelled basic blocks.
pub fn disassemble(bytecode: &[u8], evm_version: EvmVersion) -> String {
    let mut output = BasicBlocks::new(bytecode, spec_id(evm_version)).to_string();
    if !output.is_empty() {
        output.push('\n');
    }
    output
}

fn spec_id(evm_version: EvmVersion) -> SpecId {
    match evm_version {
        EvmVersion::Homestead => SpecId::HOMESTEAD,
        EvmVersion::TangerineWhistle => SpecId::TANGERINE,
        EvmVersion::SpuriousDragon => SpecId::SPURIOUS_DRAGON,
        EvmVersion::Byzantium => SpecId::BYZANTIUM,
        EvmVersion::Constantinople | EvmVersion::Petersburg => SpecId::PETERSBURG,
        EvmVersion::Istanbul => SpecId::ISTANBUL,
        EvmVersion::Berlin => SpecId::BERLIN,
        EvmVersion::London => SpecId::LONDON,
        EvmVersion::Paris => SpecId::MERGE,
        EvmVersion::Shanghai => SpecId::SHANGHAI,
        EvmVersion::Cancun => SpecId::CANCUN,
        EvmVersion::Prague => SpecId::PRAGUE,
        EvmVersion::Osaka => SpecId::OSAKA,
        // `EvmVersion` is `#[non_exhaustive]`: treat versions without a spec as the latest.
        EvmVersion::Amsterdam | _ => SpecId::AMSTERDAM,
    }
}

/// Disassembles EVM bytecode in the format used by solc's Standard JSON output.
pub fn disassemble_standard_json(bytecode: &[u8], evm_version: EvmVersion) -> String {
    let mut output = String::with_capacity(bytecode.len().saturating_mul(8));

    for instruction in instructions(bytecode, evm_version) {
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
        } else {
            match instruction.kind {
                DecodedOpcode::StackImmediate(immediate) => {
                    write_stack_immediate(&mut output, instruction.opcode, immediate)
                }
                DecodedOpcode::InvalidStackImmediate => {
                    output.push_str("INVALID_");
                    write_opcode_name(&mut output, instruction.opcode);
                }
                DecodedOpcode::Opcode => {
                    if let Some(mnemonic) = versioned_mnemonic(instruction.opcode, evm_version) {
                        write_mnemonic(&mut output, mnemonic);
                    } else {
                        write!(output, "0x{:X}", instruction.opcode).unwrap();
                    }
                }
                DecodedOpcode::Unavailable => {
                    write!(output, "0x{:X}", instruction.opcode).unwrap();
                }
            }
        }
        output.push(' ');
    }

    output
}

fn write_stack_immediate(output: &mut String, opcode: u8, immediate: u8) {
    match opcode {
        op::DUPN | op::SWAPN => {
            write_opcode_name(output, opcode);
            write!(output, " {}", op::decode_stack_depth(immediate).unwrap()).unwrap();
        }
        op::EXCHANGE => {
            let (n, m) = op::decode_exchange(immediate).unwrap();
            write_opcode_name(output, opcode);
            write!(output, " {n}, {m}").unwrap();
        }
        _ => unreachable!(),
    }
}

fn write_opcode_name(output: &mut String, opcode: u8) {
    write_mnemonic(output, op::mnemonic(opcode).expect("known opcode"));
}

fn write_mnemonic(output: &mut String, mnemonic: &str) {
    output.extend(mnemonic.bytes().map(|byte| char::from(byte.to_ascii_uppercase())));
}

struct DecodedInstruction<'a> {
    opcode: u8,
    push_width: u8,
    data: &'a [u8],
    kind: DecodedOpcode,
}

#[derive(Clone, Copy)]
enum DecodedOpcode {
    Opcode,
    StackImmediate(u8),
    InvalidStackImmediate,
    Unavailable,
}

fn instructions(
    bytecode: &[u8],
    evm_version: EvmVersion,
) -> impl Iterator<Item = DecodedInstruction<'_>> {
    let mut offset = 0;
    std::iter::from_fn(move || {
        let &opcode = bytecode.get(offset)?;
        offset += 1;

        let push_width =
            if (op::PUSH1..=op::PUSH32).contains(&opcode) { opcode - op::PUSH1 + 1 } else { 0 };
        let kind = if (opcode == op::SLOTNUM && !evm_version.has_slot_num())
            || (matches!(opcode, op::DUPN | op::SWAPN | op::EXCHANGE)
                && !evm_version.has_extended_stack_ops())
        {
            DecodedOpcode::Unavailable
        } else if matches!(opcode, op::DUPN | op::SWAPN | op::EXCHANGE) {
            let immediate = bytecode.get(offset).copied().unwrap_or(0);
            let valid = match opcode {
                op::DUPN | op::SWAPN => op::decode_stack_depth(immediate).is_some(),
                op::EXCHANGE => op::decode_exchange(immediate).is_some(),
                _ => unreachable!(),
            };
            if valid {
                DecodedOpcode::StackImmediate(immediate)
            } else {
                DecodedOpcode::InvalidStackImmediate
            }
        } else {
            DecodedOpcode::Opcode
        };
        let immediate_is_in_code =
            matches!(kind, DecodedOpcode::StackImmediate(_)) && offset < bytecode.len();
        let end = offset
            .saturating_add(usize::from(push_width) + usize::from(immediate_is_in_code))
            .min(bytecode.len());
        let data = &bytecode[offset..end];
        offset = end;
        Some(DecodedInstruction { opcode, push_width, data, kind })
    })
}

fn versioned_mnemonic(opcode: u8, evm_version: EvmVersion) -> Option<&'static str> {
    if opcode == op::PREVRANDAO && evm_version < EvmVersion::Paris {
        Some("difficulty")
    } else {
        op::mnemonic(opcode)
    }
}

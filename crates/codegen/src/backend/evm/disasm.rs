//! EVM bytecode disassembly.

use super::op;
use solar_config::EvmVersion;
use solar_data_structures::bit_set::DenseBitSet;
use std::{
    collections::{BTreeMap, VecDeque},
    fmt::Write,
};

/// Disassembles EVM bytecode into one opcode per line and labels reachable jump destinations.
pub fn disassemble(bytecode: &[u8], evm_version: EvmVersion) -> String {
    let mut output = String::with_capacity(bytecode.len().saturating_mul(8));
    let instructions = instructions(bytecode, evm_version).collect::<Vec<_>>();
    let labels = reachable_jumpdest_labels(&instructions);

    for (index, instruction) in instructions.iter().enumerate() {
        if instruction.opcode == op::JUMPDEST
            && let Some(label) = labels.get(&instruction.offset)
        {
            writeln!(output, "; bb{label}").unwrap();
        }

        if instruction.push_width != 0 {
            let width = instruction.push_width;
            write!(output, "PUSH{width} 0x").unwrap();
            for byte in instruction.data {
                write!(output, "{byte:02x}").unwrap();
            }
        } else {
            match instruction.kind {
                DecodedOpcode::StackImmediate(immediate) => {
                    write_stack_immediate(&mut output, instruction.opcode, immediate)
                }
                DecodedOpcode::InvalidStackImmediate => {
                    output.push_str(invalid_extended_stack_name(instruction.opcode))
                }
                DecodedOpcode::Opcode => {
                    if let Some(mnemonic) = versioned_mnemonic(instruction.opcode, evm_version) {
                        output.extend(
                            mnemonic.bytes().map(|byte| char::from(byte.to_ascii_uppercase())),
                        );
                    } else {
                        write!(output, "UNKNOWN 0x{:02x}", instruction.opcode).unwrap();
                    }
                }
                DecodedOpcode::Unavailable => {
                    write!(output, "UNKNOWN 0x{:02x}", instruction.opcode).unwrap();
                }
            }
        }
        if is_push(instruction)
            && instructions
                .get(index + 1)
                .is_some_and(|next| matches!(next.opcode, op::JUMP | op::JUMPI))
        {
            if let Some(label) = pushed_offset(instruction).and_then(|offset| labels.get(&offset)) {
                write!(output, " ; bb{label}").unwrap();
            } else {
                output.push_str(" ; unknown");
            }
        } else if matches!(instruction.opcode, op::JUMP | op::JUMPI)
            && !index.checked_sub(1).is_some_and(|previous| is_push(&instructions[previous]))
        {
            output.push_str(" ; unknown");
        }
        output.push('\n');
    }

    output
}

fn reachable_jumpdest_labels(instructions: &[DecodedInstruction<'_>]) -> BTreeMap<usize, usize> {
    let offsets = instructions
        .iter()
        .enumerate()
        .map(|(index, inst)| (inst.offset, index))
        .collect::<BTreeMap<_, _>>();
    let jumpdests = instructions
        .iter()
        .enumerate()
        .filter_map(|(index, inst)| (inst.opcode == op::JUMPDEST).then_some(index))
        .collect::<Vec<_>>();
    let mut reachable = DenseBitSet::<usize>::new_empty(instructions.len());
    let mut pending = VecDeque::from([0]);
    let mut all_jumpdests_pending = false;

    while let Some(index) = pending.pop_front() {
        let Some(instruction) = instructions.get(index) else { continue };
        if !reachable.insert(index) {
            continue;
        }

        if instruction.opcode == op::JUMP {
            add_jump_successors(
                instructions,
                &offsets,
                &jumpdests,
                index,
                &mut pending,
                &mut all_jumpdests_pending,
            );
        } else if instruction.opcode == op::JUMPI {
            pending.push_back(index + 1);
            add_jump_successors(
                instructions,
                &offsets,
                &jumpdests,
                index,
                &mut pending,
                &mut all_jumpdests_pending,
            );
        } else if !op::is_terminal(instruction.opcode)
            && matches!(instruction.kind, DecodedOpcode::Opcode | DecodedOpcode::StackImmediate(_))
        {
            pending.push_back(index + 1);
        }
    }

    instructions
        .iter()
        .enumerate()
        .filter(|&(index, inst)| reachable.contains(index) && inst.opcode == op::JUMPDEST)
        .enumerate()
        .map(|(label, (_, inst))| (inst.offset, label))
        .collect()
}

fn add_jump_successors(
    instructions: &[DecodedInstruction<'_>],
    offsets: &BTreeMap<usize, usize>,
    jumpdests: &[usize],
    index: usize,
    pending: &mut VecDeque<usize>,
    all_jumpdests_pending: &mut bool,
) {
    if let Some(previous) = index.checked_sub(1)
        && is_push(&instructions[previous])
    {
        if let Some(target) = pushed_offset(&instructions[previous])
            .and_then(|offset| offsets.get(&offset))
            .copied()
            .filter(|&target| instructions[target].opcode == op::JUMPDEST)
        {
            pending.push_back(target);
        }
    } else {
        // An unresolved dynamic jump may target any JUMPDEST.
        if !*all_jumpdests_pending {
            pending.extend(jumpdests.iter().copied());
            *all_jumpdests_pending = true;
        }
    }
}

fn pushed_offset(instruction: &DecodedInstruction<'_>) -> Option<usize> {
    if instruction.opcode == op::PUSH0 {
        Some(0)
    } else {
        (instruction.push_width != 0)
            .then(|| {
                instruction.data.iter().try_fold(0usize, |value, &byte| {
                    value.checked_mul(256)?.checked_add(byte.into())
                })
            })
            .flatten()
    }
}

fn is_push(instruction: &DecodedInstruction<'_>) -> bool {
    instruction.opcode == op::PUSH0 || instruction.push_width != 0
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
                    output.push_str(invalid_extended_stack_name(instruction.opcode))
                }
                DecodedOpcode::Opcode => {
                    if let Some(mnemonic) = versioned_mnemonic(instruction.opcode, evm_version) {
                        output.extend(
                            mnemonic.bytes().map(|byte| char::from(byte.to_ascii_uppercase())),
                        );
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
            let name = if opcode == op::DUPN { "DUPN" } else { "SWAPN" };
            write!(output, "{name} {}", op::decode_stack_depth(immediate).unwrap()).unwrap();
        }
        op::EXCHANGE => {
            let (n, m) = op::decode_exchange(immediate).unwrap();
            write!(output, "EXCHANGE {n}, {m}").unwrap();
        }
        _ => unreachable!(),
    }
}

struct DecodedInstruction<'a> {
    offset: usize,
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
        let instruction_offset = offset;
        let &opcode = bytecode.get(offset)?;
        offset += 1;

        let push_width =
            if (op::PUSH1..=op::PUSH32).contains(&opcode) { opcode - op::PUSH1 + 1 } else { 0 };
        let kind = if !opcode_is_available(opcode, evm_version) {
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
        Some(DecodedInstruction { offset: instruction_offset, opcode, push_width, data, kind })
    })
}

fn invalid_extended_stack_name(opcode: u8) -> &'static str {
    match opcode {
        op::DUPN => "INVALID_DUPN",
        op::SWAPN => "INVALID_SWAPN",
        op::EXCHANGE => "INVALID_EXCHANGE",
        _ => unreachable!(),
    }
}

fn opcode_is_available(opcode: u8, evm_version: EvmVersion) -> bool {
    (opcode != op::SLOTNUM || evm_version.has_slot_num())
        && (!matches!(opcode, op::DUPN | op::SWAPN | op::EXCHANGE)
            || evm_version.has_extended_stack_ops())
}

fn versioned_mnemonic(opcode: u8, evm_version: EvmVersion) -> Option<&'static str> {
    if opcode == op::PREVRANDAO && evm_version < EvmVersion::Paris {
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
            "pushes: {:?}\nmixed: {:?}\npre-paris: {:?}\nparis: {:?}\n",
            disassemble_standard_json(&[op::PUSH2, 0x01, 0x20, op::PUSH2, 0x01], EvmVersion::Osaka,),
            disassemble_standard_json(&[0x0c, op::DATALOAD], EvmVersion::Osaka),
            disassemble_standard_json(&[op::PREVRANDAO], EvmVersion::Homestead),
            disassemble_standard_json(&[op::PREVRANDAO], EvmVersion::Paris),
        );
        assert_data_eq!(
            actual,
            str![[r#"
pushes: "PUSH2 0x120 PUSH2 0x100 "
mixed: "0xC DATALOAD "
pre-paris: "DIFFICULTY "
paris: "PREVRANDAO "

"#]]
        );
    }

    #[test]
    fn disassembles_eip_8024_immediates() {
        assert_data_eq!(
            disassemble(
                &[op::DUP1, op::SWAP16, op::DUPN, 0x80, op::SWAPN, 0xdb, op::EXCHANGE, 0x9d,],
                EvmVersion::Amsterdam,
            ),
            str![[r#"
DUP1
SWAP16
DUPN 17
SWAPN 108
EXCHANGE 2, 3

"#]]
        );
        assert_data_eq!(
            disassemble(&[op::SWAPN, op::JUMPDEST, op::EXCHANGE], EvmVersion::Amsterdam,),
            str![[r#"
INVALID_SWAPN
JUMPDEST
EXCHANGE 9, 16

"#]]
        );
        assert_data_eq!(
            disassemble(&[op::DUPN, op::JUMPDEST, op::SLOTNUM, op::JUMPDEST], EvmVersion::Osaka),
            str![[r#"
UNKNOWN 0xe6
JUMPDEST
UNKNOWN 0x4b
JUMPDEST

"#]]
        );
    }
}

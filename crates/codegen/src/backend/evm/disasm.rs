//! EVM bytecode disassembly.

use super::op;
use solar_config::EvmVersion;
use std::{
    collections::{BTreeMap, VecDeque},
    fmt::Write,
};

/// Disassembles EVM bytecode into one opcode per line and labels reachable jump destinations.
pub fn disassemble(bytecode: &[u8]) -> String {
    let mut output = String::with_capacity(bytecode.len().saturating_mul(8));
    let instructions = instructions(bytecode).collect::<Vec<_>>();
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
        } else if let Some(mnemonic) = op::mnemonic(instruction.opcode) {
            output.extend(mnemonic.bytes().map(|byte| char::from(byte.to_ascii_uppercase())));
        } else {
            write!(output, "UNKNOWN 0x{:02x}", instruction.opcode).unwrap();
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
    let mut reachable = vec![false; instructions.len()];
    let mut pending = VecDeque::from([0]);
    let mut all_jumpdests_pending = false;

    while let Some(index) = pending.pop_front() {
        let Some(instruction) = instructions.get(index) else { continue };
        if std::mem::replace(&mut reachable[index], true) {
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
        } else if !op::is_terminal(instruction.opcode) {
            pending.push_back(index + 1);
        }
    }

    instructions
        .iter()
        .enumerate()
        .filter(|&(index, inst)| reachable[index] && inst.opcode == op::JUMPDEST)
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
    offset: usize,
    opcode: u8,
    push_width: u8,
    data: &'a [u8],
}

fn instructions(bytecode: &[u8]) -> impl Iterator<Item = DecodedInstruction<'_>> {
    let mut offset = 0;
    std::iter::from_fn(move || {
        let instruction_offset = offset;
        let &opcode = bytecode.get(offset)?;
        offset += 1;

        let push_width =
            if (op::PUSH1..=op::PUSH32).contains(&opcode) { opcode - op::PUSH1 + 1 } else { 0 };
        let end = offset.saturating_add(usize::from(push_width)).min(bytecode.len());
        let data = &bytecode[offset..end];
        offset = end;
        Some(DecodedInstruction { offset: instruction_offset, opcode, push_width, data })
    })
}

fn standard_json_mnemonic(opcode: u8, evm_version: EvmVersion) -> Option<&'static str> {
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
}

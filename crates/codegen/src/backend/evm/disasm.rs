//! EVM bytecode disassembly.

use super::op;
use evm2_bytecode::{Instruction, Instructions, SpecId};
use solar_config::EvmVersion;
use solar_data_structures::bit_set::DenseBitSet;
use std::{
    collections::{BTreeMap, VecDeque},
    fmt::Write,
};

/// Disassembles EVM bytecode into one opcode per line and labels reachable jump destinations.
pub fn disassemble(bytecode: &[u8], evm_version: EvmVersion) -> String {
    let spec_id = op::spec_id(evm_version);
    let mut output = String::with_capacity(bytecode.len().saturating_mul(8));
    let instructions = Instructions::new(bytecode, spec_id).collect::<Vec<_>>();
    let labels = reachable_jumpdest_labels(&instructions, spec_id);

    for (index, instruction) in instructions.iter().enumerate() {
        if instruction.opcode.is_jumpdest()
            && let Some(label) = labels.get(&instruction.pc)
        {
            writeln!(output, "; bb{label}").unwrap();
        }

        write!(output, "{}", instruction.display(spec_id)).unwrap();
        let follows_push = index
            .checked_sub(1)
            .is_some_and(|previous| instructions[previous].push_value().is_some());
        if instruction.push_value().is_some() && instructions.get(index + 1).is_some_and(is_jump) {
            if let Some(label) = pushed_pc(instruction).and_then(|pc| labels.get(&pc)) {
                write!(output, " ; bb{label}").unwrap();
            } else {
                output.push_str(" ; unknown");
            }
        } else if is_jump(instruction) && !follows_push {
            output.push_str(" ; unknown");
        }
        output.push('\n');
    }

    output
}

fn reachable_jumpdest_labels(
    instructions: &[Instruction<'_>],
    spec_id: SpecId,
) -> BTreeMap<usize, usize> {
    let jumpdests = instructions
        .iter()
        .enumerate()
        .filter_map(|(index, inst)| inst.opcode.is_jumpdest().then_some(index))
        .collect::<Vec<_>>();
    let mut reachable = DenseBitSet::<usize>::new_empty(instructions.len());
    let mut pending = VecDeque::from([0]);
    let mut all_jumpdests_pending = false;

    while let Some(index) = pending.pop_front() {
        let Some(instruction) = instructions.get(index) else { continue };
        if !reachable.insert(index) {
            continue;
        }

        if is_jump(instruction) {
            if instruction.opcode.get() == op::JUMPI {
                pending.push_back(index + 1);
            }
            add_jump_successors(
                instructions,
                &jumpdests,
                index,
                &mut pending,
                &mut all_jumpdests_pending,
            );
        } else if !op::is_terminal(instruction.opcode.get())
            && instruction.opcode.is_enabled(spec_id)
            && instruction.stack_io().is_some()
        {
            pending.push_back(index + 1);
        }
    }

    instructions
        .iter()
        .enumerate()
        .filter(|&(index, inst)| reachable.contains(index) && inst.opcode.is_jumpdest())
        .enumerate()
        .map(|(label, (_, inst))| (inst.pc, label))
        .collect()
}

fn add_jump_successors(
    instructions: &[Instruction<'_>],
    jumpdests: &[usize],
    index: usize,
    pending: &mut VecDeque<usize>,
    all_jumpdests_pending: &mut bool,
) {
    if let Some(previous) = index.checked_sub(1)
        && instructions[previous].push_value().is_some()
    {
        if let Some(target) = pushed_pc(&instructions[previous])
            .and_then(|pc| instructions.binary_search_by_key(&pc, |inst| inst.pc).ok())
            .filter(|&target| instructions[target].opcode.is_jumpdest())
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

fn is_jump(instruction: &Instruction<'_>) -> bool {
    matches!(instruction.opcode.get(), op::JUMP | op::JUMPI)
}

fn pushed_pc(instruction: &Instruction<'_>) -> Option<usize> {
    usize::try_from(instruction.push_value()?).ok()
}

/// Disassembles EVM bytecode in the format used by solc's Standard JSON output.
pub fn disassemble_standard_json(bytecode: &[u8], evm_version: EvmVersion) -> String {
    let spec_id = op::spec_id(evm_version);
    let mut output = String::with_capacity(bytecode.len().saturating_mul(8));

    for instruction in Instructions::new(bytecode, spec_id) {
        let opcode = instruction.opcode;
        if opcode.is_push()
            && let Some(value) = instruction.push_value()
        {
            write!(output, "{} 0x{value:X}", opcode.as_str()).unwrap();
        } else if !opcode.is_valid()
            || (matches!(opcode.get(), op::DUPN | op::SWAPN | op::EXCHANGE | op::SLOTNUM)
                && !opcode.is_enabled(spec_id))
        {
            write!(output, "0x{:X}", opcode.get()).unwrap();
        } else if instruction.stack_io().is_none() {
            write!(output, "INVALID_{}", opcode.as_str()).unwrap();
        } else {
            write!(output, "{}", instruction.display(spec_id)).unwrap();
        }
        output.push(' ');
    }

    output
}

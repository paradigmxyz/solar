//! Recognize bounded code reads whose operands remain inside a declared suffix.
//! Return the complete contiguous instruction range and its temporary stack peak.

use solar_evmir::{
    ir::{DataRef, Instruction, Module},
    op::{self, StackOp},
};

pub(super) struct Read {
    pub name: String,
    pub count: usize,
    pub peak: usize,
}

/// The assembler emits the sole declared entry after the executable program.
pub(super) fn suffix(module: &Module) -> Option<&[u8]> {
    if module.data.len() != 1 {
        return None;
    }
    let data = module.data.iter().next()?;
    if !data.library_relocations.is_empty() || !data.trailers.is_empty() {
        return None;
    }
    Some(data.bytes.known()?)
}

pub(super) fn recognize(instructions: &[Instruction], data_size: usize) -> Option<Read> {
    if let [base, size, sub, rest @ ..] = instructions
        && let Some(reference) = base.pushed_data()
        && reference.offset as usize == data_size
        && size.opcode == op::CODESIZE
        && sub.opcode == op::SUB
    {
        if let Some((copied, destination)) = copy(rest)
            && copied == reference
        {
            // Compute the argument length, retain it, and copy the arguments.
            return Some(Read { name: format!("copy-init-tail {destination}"), count: 7, peak: 4 });
        }
        return Some(Read { name: "size-init-tail".into(), count: 3, peak: 2 });
    }
    let count = u32::try_from(instructions.first()?.concrete_immediate()?).ok()?;
    let (reference, destination) = copy(&instructions[1..])?;
    if reference.offset.checked_add(count)? as usize > data_size {
        return None;
    }
    Some(Read {
        name: format!("copy-code-data {} {count} {destination}", reference.offset),
        count: 5,
        peak: 4,
    })
}

fn copy(instructions: &[Instruction]) -> Option<(DataRef, u32)> {
    let [dup, source, destination, copy, ..] = instructions else { return None };
    if dup.as_stack_op() != Some(StackOp::Dup(1)) || copy.opcode != op::CODECOPY {
        return None;
    }
    Some((source.pushed_data()?, u32::try_from(destination.concrete_immediate()?).ok()?))
}

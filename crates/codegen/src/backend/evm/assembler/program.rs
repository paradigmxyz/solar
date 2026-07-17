//! Final linear EVM assembly program.

use super::{AsmInst, Assembler, DeferredConst, Label, op};
use crate::backend::evm::ir::{self, BlockId};

/// Linear label-bearing opcode stream ready for final bytecode assembly.
#[derive(Clone, Debug, Default)]
pub(in crate::backend::evm) struct EvmAsmProgram {
    pub(in crate::backend::evm) instructions: Vec<AsmInst>,
}

/// Lowers finalized EVM IR into the linear assembly stream.
pub(in crate::backend::evm) fn lower_evm_ir(
    module: &ir::Module,
    labels: &mut Vec<Option<Label>>,
    assembler: &mut Assembler,
) -> EvmAsmProgram {
    allocate_referenced_labels(module, labels, assembler);

    let mut program = EvmAsmProgram::default();
    for (block_id, block) in module.blocks.iter_enumerated() {
        let original = block.label as usize;
        if let Some(label) = labels.get(original).copied().flatten() {
            program.instructions.push(AsmInst::label(label));
        }

        for inst in &block.instructions {
            program.instructions.push(lower_instruction(inst, module, labels, assembler));
        }

        if let Some(terminator) = &block.terminator {
            lower_terminator(&mut program, block_id, &terminator.kind, module, labels, assembler);
        }
    }
    program
}

fn allocate_referenced_labels(
    module: &ir::Module,
    labels: &mut Vec<Option<Label>>,
    assembler: &mut Assembler,
) {
    for block in &module.blocks {
        for inst in &block.instructions {
            for operand in &inst.operands {
                if let ir::Operand::Block(target) = operand {
                    label_for_block(module, *target, labels, assembler);
                }
            }
        }
        if let Some(ir::Terminator { kind: ir::TerminatorKind::Jump(target), .. }) =
            &block.terminator
        {
            label_for_block(module, *target, labels, assembler);
        }
    }
}

fn lower_instruction(
    inst: &ir::Instruction,
    module: &ir::Module,
    labels: &mut Vec<Option<Label>>,
    assembler: &mut Assembler,
) -> AsmInst {
    if inst.is_deferred_push() {
        let [ir::Operand::Immediate(value)] = inst.operands.as_slice() else {
            unreachable!("deferred push must have one immediate operand")
        };
        AsmInst::push_deferred(DeferredConst::from_usize(
            usize::try_from(*value).expect("deferred constant ID must fit usize"),
        ))
    } else if inst.is_immutable_push() {
        let [ir::Operand::Immediate(value)] = inst.operands.as_slice() else {
            unreachable!("immutable push must have one immediate operand")
        };
        AsmInst::push_immutable(u32::try_from(*value).expect("immutable ID must fit u32"))
    } else if inst.is_encoded_push() {
        match inst.operands.as_slice() {
            [ir::Operand::Immediate(value)] => assembler.push_inst(*value),
            [ir::Operand::Block(block)] => {
                AsmInst::push_label(label_for_block(module, *block, labels, assembler))
            }
            _ => unreachable!("push must have one immediate or block operand"),
        }
    } else {
        AsmInst::op(inst.opcode)
    }
}

fn lower_terminator(
    program: &mut EvmAsmProgram,
    block_id: BlockId,
    kind: &ir::TerminatorKind,
    module: &ir::Module,
    labels: &mut Vec<Option<Label>>,
    assembler: &mut Assembler,
) {
    match kind {
        ir::TerminatorKind::Jump(target) => {
            if next_block(module, block_id) == Some(*target) {
                return;
            }
            let label = label_for_block(module, *target, labels, assembler);
            program.instructions.push(AsmInst::push_label(label));
            program.instructions.push(AsmInst::op(op::JUMP));
        }
        ir::TerminatorKind::RawOpcode(opcode) => {
            program.instructions.push(AsmInst::op(*opcode));
        }
        ir::TerminatorKind::Stop => {
            if next_block(module, block_id).is_some() {
                program.instructions.push(AsmInst::op(op::STOP));
            }
        }
        ir::TerminatorKind::Invalid => program.instructions.push(AsmInst::op(op::INVALID)),
        ir::TerminatorKind::Return { .. }
        | ir::TerminatorKind::Revert { .. }
        | ir::TerminatorKind::SelfDestruct { .. }
        | ir::TerminatorKind::Branch { .. }
        | ir::TerminatorKind::Switch { .. } => {
            unreachable!("MIR lowering must produce machine-level EVM IR terminators")
        }
    }
}

fn next_block(module: &ir::Module, block: BlockId) -> Option<BlockId> {
    let next = block.index() + 1;
    (next < module.blocks.len()).then(|| BlockId::from_usize(next))
}

fn label_for_block(
    module: &ir::Module,
    block: BlockId,
    labels: &mut Vec<Option<Label>>,
    assembler: &mut Assembler,
) -> Label {
    let original = module.blocks[block].label as usize;
    if original >= labels.len() {
        labels.resize_with(original + 1, || None);
    }
    *labels[original].get_or_insert_with(|| assembler.new_label())
}

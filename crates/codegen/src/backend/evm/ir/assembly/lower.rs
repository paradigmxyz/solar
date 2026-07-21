//! Lowering from block EVM IR to its finalized layout-linear form.

use super::{AsmInst, Program};
use crate::backend::evm::{
    assembler::{Assembler, Label},
    ir::{self, BlockId},
    opcode as op,
};
use solar_data_structures::bit_set::DenseBitSet;

/// Lowers finalized EVM IR into the linear label-bearing assembly stream.
pub(in crate::backend::evm) fn lower_evm_ir(
    module: &ir::Module,
    labels: &mut Vec<Option<Label>>,
    assembler: &mut Assembler,
) -> Program {
    allocate_referenced_labels(module, labels, assembler);

    let mut program = Program::default();
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
    let mut referenced = DenseBitSet::new_empty(module.blocks.len());
    for (block_id, block) in module.blocks.iter_enumerated() {
        for inst in &block.instructions {
            if let Some(ir::PushValue::Block(target)) = &inst.value {
                referenced.insert(*target);
            }
        }
        if let Some(terminator) = &block.terminator {
            let next = next_block(module, block_id);
            match &terminator.kind {
                ir::TerminatorKind::Jump(target) if next != Some(*target) => {
                    referenced.insert(*target);
                }
                ir::TerminatorKind::JumpI { then_block, else_block } => {
                    if next != Some(*then_block) {
                        referenced.insert(*then_block);
                    }
                    if next != Some(*else_block) {
                        referenced.insert(*else_block);
                    }
                }
                _ => {}
            }
        }
    }
    for (block_id, block) in module.blocks.iter_enumerated() {
        let original = block.label as usize;
        if !referenced.contains(block_id)
            && let Some(label) = labels.get_mut(original)
        {
            *label = None;
        }
    }
    for block in referenced.iter() {
        label_for_block(module, block, labels, assembler);
    }
}

fn lower_instruction(
    inst: &ir::Instruction,
    module: &ir::Module,
    labels: &mut Vec<Option<Label>>,
    assembler: &mut Assembler,
) -> AsmInst {
    if let Some(id) = inst.deferred_push() {
        AsmInst::push_deferred(id)
    } else if let Some(value) = inst.immutable_push() {
        AsmInst::push_immutable(u32::try_from(value).expect("immutable ID must fit u32"))
    } else if inst.is_encoded_push() {
        match &inst.value {
            Some(ir::PushValue::Immediate(value)) => assembler.push_inst(*value),
            Some(ir::PushValue::Block(block)) => {
                AsmInst::push_label(label_for_block(module, *block, labels, assembler))
            }
            _ => unreachable!("push must have one immediate or block operand"),
        }
    } else {
        AsmInst::op(inst.opcode)
    }
}

fn lower_terminator(
    program: &mut Program,
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
        ir::TerminatorKind::JumpI { then_block, else_block } => {
            let next = next_block(module, block_id);
            if next == Some(*else_block) {
                let label = label_for_block(module, *then_block, labels, assembler);
                program.instructions.push(AsmInst::push_label(label));
                program.instructions.push(AsmInst::op(op::JUMPI));
            } else if next == Some(*then_block) {
                program.instructions.push(AsmInst::op(op::ISZERO));
                let label = label_for_block(module, *else_block, labels, assembler);
                program.instructions.push(AsmInst::push_label(label));
                program.instructions.push(AsmInst::op(op::JUMPI));
            } else {
                let then_label = label_for_block(module, *then_block, labels, assembler);
                program.instructions.push(AsmInst::push_label(then_label));
                program.instructions.push(AsmInst::op(op::JUMPI));
                let else_label = label_for_block(module, *else_block, labels, assembler);
                program.instructions.push(AsmInst::push_label(else_label));
                program.instructions.push(AsmInst::op(op::JUMP));
            }
        }
        ir::TerminatorKind::Op(opcode) => {
            program.instructions.push(AsmInst::op(*opcode));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::evm::{
        assembler::AsmInstKind,
        ir::{Block, Instruction, Terminator, TerminatorKind},
    };
    use alloy_primitives::U256;
    use solar_interface::sym;

    #[test]
    fn branch_inverts_when_then_target_falls_through() {
        solar_interface::enter(branch_inverts_when_then_target_falls_through_inner);
    }

    fn branch_inverts_when_then_target_falls_through_inner() {
        let mut module = ir::Module::new(sym::module);
        let entry = module.add_block(Block::new(0));
        let then_block = module.add_block(Block::new(1));
        let else_block = module.add_block(Block::new(2));
        module.blocks[entry].instructions.push(Instruction::push_value(U256::ONE));
        module.blocks[entry].terminator =
            Some(Terminator::new(TerminatorKind::JumpI { then_block, else_block }));
        module.blocks[then_block].terminator = Some(Terminator::new(TerminatorKind::Op(op::STOP)));
        module.blocks[else_block].terminator = Some(Terminator::new(TerminatorKind::Op(op::STOP)));

        let mut labels = vec![None; 3];
        let mut assembler = Assembler::new();
        let program = lower_evm_ir(&module, &mut labels, &mut assembler);
        let kinds: Vec<_> = program.instructions.iter().map(|inst| inst.kind()).collect();

        assert!(matches!(
            kinds.as_slice(),
            [
                AsmInstKind::PushInline(1),
                AsmInstKind::Op(op::ISZERO),
                AsmInstKind::PushLabel(_),
                AsmInstKind::Op(op::JUMPI),
                AsmInstKind::Op(op::STOP),
                AsmInstKind::Label(_),
                AsmInstKind::Op(op::STOP),
            ]
        ));
    }
}

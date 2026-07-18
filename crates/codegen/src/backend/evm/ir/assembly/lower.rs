//! Lowering from block EVM IR to its finalized layout-linear form.

use super::{AsmInst, AsmInstKind, LocalPushValues, Program};
use crate::backend::evm::{
    assembler::{Assembler, DeferredConst, Label},
    ir::{self, BlockId},
    opcode as op,
};
use alloy_primitives::U256;
use solar_data_structures::{index::Idx, map::FxHashMap};
use solar_interface::Symbol;

/// Linear label-bearing opcode stream ready for final bytecode assembly.
/// Lowers finalized EVM IR into the linear assembly stream.
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

/// Reconstructs block EVM IR from its finalized linear form for diagnostics
/// and `--emit=evm-ir` output.
pub(in crate::backend::evm) fn raise_evm_ir(
    program: &Program,
    push_values: &LocalPushValues,
    name: Symbol,
) -> ir::Module {
    if program.instructions.is_empty() {
        return ir::Module::new(name);
    }

    let mut starts = vec![0usize];
    for (index, inst) in program.instructions.iter().enumerate() {
        if matches!(inst.kind(), AsmInstKind::Label(_)) && index != 0 {
            starts.push(index);
        }
    }
    starts.push(program.instructions.len());

    let mut labels = FxHashMap::default();
    for (block, &start) in starts[..starts.len() - 1].iter().enumerate() {
        if let AsmInstKind::Label(label) = program.instructions[start].kind() {
            labels.insert(label, BlockId::from_usize(block));
        }
    }

    let max_label = labels.keys().map(|label| label.index()).max().unwrap_or(0);
    let mut module = ir::Module::new(name);
    for (block_index, window) in starts.windows(2).enumerate() {
        let (start, end) = (window[0], window[1]);
        let attached_label = match program.instructions[start].kind() {
            AsmInstKind::Label(label) => Some(label),
            _ => None,
        };
        let label = attached_label.map_or(max_label + 1 + block_index, Idx::index);
        let mut block = ir::Block::new(u32::try_from(label).expect("EVM IR label overflow"));
        let body_start = start + usize::from(attached_label.is_some());
        let body = &program.instructions[body_start..end];
        let (instruction_end, terminator) = if let [.., push, jump] = body
            && let AsmInstKind::PushLabel(target) = push.kind()
            && matches!(jump.kind(), AsmInstKind::Op(op::JUMP))
        {
            (body.len() - 2, ir::TerminatorKind::Jump(labels[&target]))
        } else if let Some(last) = body.last()
            && let AsmInstKind::Op(opcode) = last.kind()
            && op::is_terminal(opcode)
        {
            (body.len() - 1, ir::TerminatorKind::RawOpcode(opcode))
        } else if block_index + 1 < starts.len() - 1 {
            (body.len(), ir::TerminatorKind::Jump(BlockId::from_usize(block_index + 1)))
        } else {
            (body.len(), ir::TerminatorKind::Stop)
        };

        for &inst in &body[..instruction_end] {
            block.instructions.push(match inst.kind() {
                AsmInstKind::Op(opcode) => ir::Instruction::opcode(opcode),
                AsmInstKind::PushInline(value) => {
                    ir::Instruction::push(ir::Operand::Immediate(U256::from(u64::from(value))))
                }
                AsmInstKind::Push(index) => {
                    ir::Instruction::push(ir::Operand::Immediate(*push_values.get(index)))
                }
                AsmInstKind::PushLabel(label) => {
                    ir::Instruction::push(ir::Operand::Block(labels[&label]))
                }
                AsmInstKind::PushDeferred(id) => ir::Instruction::push_deferred(
                    ir::Operand::Immediate(U256::from(id.index() as u64)),
                ),
                AsmInstKind::PushImmutable(id) => ir::Instruction::push_immutable(
                    ir::Operand::Immediate(U256::from(u64::from(id))),
                ),
                AsmInstKind::Label(_) => unreachable!("labels delimit linear EVM IR blocks"),
            });
        }
        block.terminator = Some(ir::Terminator::new(terminator));
        module.add_block(block);
    }
    module
}

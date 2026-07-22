//! Lowering from block EVM IR to its finalized layout-linear form.

use super::{AsmInst, Program};
use crate::backend::evm::{
    assembler::{Assembler, Label},
    ir::{self, BlockId},
    op,
};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::bit_set::DenseBitSet;

/// Lowers finalized EVM IR into the linear label-bearing assembly stream.
pub(in crate::backend::evm) fn lower_evm_ir(
    module: &mut ir::Module,
    labels: &mut Vec<Option<Label>>,
    assembler: &mut Assembler<'_>,
    evm_version: EvmVersion,
) -> Program {
    let table_entries = materialize_indexed_jump_tables(module);
    let table_target_width = indexed_jump_target_width(module, evm_version);
    allocate_referenced_labels(module, labels, assembler);

    let mut program = Program::default();
    for (block_id, block) in module.blocks.iter_enumerated() {
        let original = block.label as usize;
        if let Some(label) = labels.get(original).copied().flatten() {
            program.define_label(label);
        }

        for inst in &block.instructions {
            program.push(lower_instruction(inst, module, labels, assembler));
        }

        if let Some(terminator) = &block.terminator {
            lower_terminator(
                &mut program,
                block_id,
                &terminator.kind,
                module,
                labels,
                assembler,
                table_entries.contains(block_id),
                table_target_width,
            );
        }
    }
    program
}

fn materialize_indexed_jump_tables(module: &mut ir::Module) -> DenseBitSet<BlockId> {
    let tables = module
        .blocks
        .iter_enumerated()
        .filter_map(|(block, data)| {
            let targets = match &data.terminator.as_ref()?.kind {
                ir::TerminatorKind::IndexedJump(targets) => targets.clone(),
                _ => return None,
            };
            Some((block, targets))
        })
        .collect::<Vec<_>>();
    let mut next_label = module
        .blocks
        .iter()
        .map(|block| block.label)
        .max()
        .map_or(0, |label| label.checked_add(1).expect("EVM IR block label overflow"));
    let mut entries = Vec::new();

    for (source, targets) in tables {
        let mut table = Vec::with_capacity(targets.len());
        for target in targets {
            let mut block = ir::Block::new(next_label);
            next_label = next_label.checked_add(1).expect("EVM IR block label overflow");
            block.terminator = Some(ir::Terminator::new(ir::TerminatorKind::Jump(target)));
            let entry = module.add_block(block);
            table.push(entry);
            entries.push(entry);
        }
        module.blocks[source]
            .terminator
            .as_mut()
            .expect("indexed jump source must have a terminator")
            .kind = ir::TerminatorKind::IndexedJump(table.into_boxed_slice());
    }

    let mut result = DenseBitSet::new_empty(module.blocks.len());
    for entry in entries {
        result.insert(entry);
    }
    result
}

fn indexed_jump_target_width(module: &ir::Module, evm_version: EvmVersion) -> u8 {
    if !module.blocks.iter().any(|block| {
        matches!(
            block.terminator.as_ref().map(|term| &term.kind),
            Some(ir::TerminatorKind::IndexedJump(_))
        )
    }) {
        return 1;
    }

    for width in 1..=32 {
        let size = estimated_module_size(module, evm_version, width);
        let bits = u32::from(width) * 8;
        if bits >= usize::BITS || size <= 1usize << bits {
            return width;
        }
    }
    unreachable!("a bytecode offset must fit one EVM word")
}

fn estimated_module_size(module: &ir::Module, evm_version: EvmVersion, width: u8) -> usize {
    let mut size = 0usize;
    for (block_id, block) in module.blocks.iter_enumerated() {
        size = size.saturating_add(1);
        for inst in &block.instructions {
            let inst_size = if inst.deferred_push().is_some() || inst.immutable_push().is_some() {
                33
            } else if inst.is_encoded_push() {
                match &inst.value {
                    Some(ir::PushValue::Immediate(value)) => push_len(*value, evm_version),
                    Some(ir::PushValue::Block(_)) => usize::from(width) + 1,
                    None => unreachable!("push must carry a value"),
                }
            } else {
                1
            };
            size = size.saturating_add(inst_size);
        }
        if let Some(term) = &block.terminator {
            size = size.saturating_add(estimated_terminator_size(
                &term.kind,
                next_block(module, block_id),
                width,
            ));
        }
    }
    size
}

fn estimated_terminator_size(kind: &ir::TerminatorKind, next: Option<BlockId>, width: u8) -> usize {
    let push = usize::from(width) + 1;
    match kind {
        ir::TerminatorKind::Jump(target) => usize::from(Some(*target) != next) * (push + 1),
        ir::TerminatorKind::JumpI { then_block, else_block } => {
            if Some(*else_block) == next {
                push + 1
            } else if Some(*then_block) == next {
                push + 2
            } else {
                push * 2 + 2
            }
        }
        ir::TerminatorKind::IndexedJump(_) => push + 5,
        ir::TerminatorKind::Op(op::STOP) => usize::from(next.is_some()),
        ir::TerminatorKind::Op(_) => 1,
    }
}

fn push_len(value: U256, evm_version: EvmVersion) -> usize {
    if value.is_zero() && evm_version.has_push0() { 1 } else { value.byte_len().max(1) + 1 }
}

fn allocate_referenced_labels(
    module: &ir::Module,
    labels: &mut Vec<Option<Label>>,
    assembler: &mut Assembler<'_>,
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
            terminator.kind.visit_label_targets(next, |target| {
                referenced.insert(target);
            });
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
    assembler: &mut Assembler<'_>,
) -> AsmInst {
    if let Some(id) = inst.deferred_push() {
        AsmInst::push_deferred(id)
    } else if let Some(value) = inst.immutable_push() {
        AsmInst::push_immutable(u32::try_from(value).expect("validated immutable ID must fit u32"))
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
    assembler: &mut Assembler<'_>,
    is_table_entry: bool,
    table_target_width: u8,
) {
    match kind {
        ir::TerminatorKind::Jump(target) => {
            if is_table_entry {
                let label = label_for_block(module, *target, labels, assembler);
                program.push(AsmInst::push_label_fixed(label, table_target_width));
                program.push_op(op::JUMP);
                return;
            }
            if next_block(module, block_id) == Some(*target) {
                return;
            }
            let label = label_for_block(module, *target, labels, assembler);
            program.push_label(label);
            program.push_op(op::JUMP);
        }
        ir::TerminatorKind::JumpI { then_block, else_block } => {
            let next = next_block(module, block_id);
            if next == Some(*else_block) {
                let label = label_for_block(module, *then_block, labels, assembler);
                program.push_label(label);
                program.push_op(op::JUMPI);
            } else if next == Some(*then_block) {
                program.push_op(op::ISZERO);
                let label = label_for_block(module, *else_block, labels, assembler);
                program.push_label(label);
                program.push_op(op::JUMPI);
            } else {
                let then_label = label_for_block(module, *then_block, labels, assembler);
                program.push_label(then_label);
                program.push_op(op::JUMPI);
                let else_label = label_for_block(module, *else_block, labels, assembler);
                program.push_label(else_label);
                program.push_op(op::JUMP);
            }
        }
        ir::TerminatorKind::IndexedJump(targets) => {
            let (&table, rest) = targets.split_first().expect("validated indexed jump table");
            debug_assert!(
                rest.iter()
                    .enumerate()
                    .all(|(index, target)| { target.index() == table.index() + index + 1 })
            );
            let stub_len = u32::from(table_target_width) + 3;
            program.push(
                AsmInst::push_inline(stub_len).expect("indexed jump stub length must fit inline"),
            );
            program.push_op(op::MUL);
            program.push_label(label_for_block(module, table, labels, assembler));
            program.push_op(op::ADD);
            program.push_op(op::JUMP);
        }
        ir::TerminatorKind::Op(opcode) => {
            if *opcode != op::STOP || next_block(module, block_id).is_some() {
                program.push_op(*opcode);
            }
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
    assembler: &mut Assembler<'_>,
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
    use solar_interface::Session;
    use solar_sema::Compiler;
    #[test]
    fn branch_inverts_when_then_target_falls_through() {
        let mut module = ir::Module::new("module");
        let entry = module.add_block(Block::new(0));
        let then_block = module.add_block(Block::new(1));
        let else_block = module.add_block(Block::new(2));
        module.blocks[entry].instructions.push(Instruction::push_value(U256::ONE));
        module.blocks[entry].terminator =
            Some(Terminator::new(TerminatorKind::JumpI { then_block, else_block }));
        module.blocks[then_block].terminator = Some(Terminator::new(TerminatorKind::Op(op::STOP)));
        module.blocks[else_block].terminator = Some(Terminator::new(TerminatorKind::Op(op::STOP)));

        let compiler = Compiler::new(Session::builder().opts(Default::default()).build());
        compiler.enter(|c| {
            let mut labels = vec![None; 3];
            let mut assembler = Assembler::new(c.gcx());
            let program = lower_evm_ir(&mut module, &mut labels, &mut assembler, EvmVersion::Osaka);
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
                ]
            ));
        });
    }

    #[test]
    fn indexed_jump_entries_are_reachable_blocks() {
        let mut module = ir::Module::new("module");
        let entry = module.add_block(Block::new(0));
        let left = module.add_block(Block::new(1));
        let right = module.add_block(Block::new(2));
        module.blocks[entry].instructions.push(Instruction::push_value(U256::ONE));
        module.blocks[entry].terminator = Some(Terminator::new(TerminatorKind::IndexedJump(
            vec![left, right].into_boxed_slice(),
        )));
        module.blocks[left].terminator = Some(Terminator::new(TerminatorKind::Op(op::STOP)));
        module.blocks[right].terminator = Some(Terminator::new(TerminatorKind::Op(op::INVALID)));

        let compiler = Compiler::new(Session::builder().opts(Default::default()).build());
        compiler.enter(|c| {
            let mut labels = vec![None; 3];
            let mut assembler = Assembler::new(c.gcx());
            let program = lower_evm_ir(&mut module, &mut labels, &mut assembler, EvmVersion::Osaka);

            let TerminatorKind::IndexedJump(entries) =
                &module.blocks[entry].terminator.as_ref().unwrap().kind
            else {
                panic!("expected indexed jump")
            };
            assert_eq!(entries.len(), 2);
            assert!(matches!(
                module.blocks[entries[0]].terminator.as_ref().map(|term| &term.kind),
                Some(TerminatorKind::Jump(target)) if *target == left
            ));
            assert!(matches!(
                module.blocks[entries[1]].terminator.as_ref().map(|term| &term.kind),
                Some(TerminatorKind::Jump(target)) if *target == right
            ));
            assert_eq!(
                program
                    .instructions
                    .iter()
                    .filter_map(|inst| match inst.kind() {
                        AsmInstKind::PushLabelFixed(_, width) => Some(width),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                vec![1, 1]
            );
        });
    }

    #[test]
    fn widens_table_targets_when_program_exceeds_push1() {
        let mut module = ir::Module::new("module");
        let entry = module.add_block(Block::new(0));
        let target = module.add_block(Block::new(1));
        for id in 0..8 {
            module.blocks[entry].instructions.push(Instruction::push_immutable(id));
        }
        module.blocks[entry].terminator =
            Some(Terminator::new(TerminatorKind::IndexedJump(vec![target].into_boxed_slice())));
        module.blocks[target].terminator = Some(Terminator::new(TerminatorKind::Op(op::STOP)));

        materialize_indexed_jump_tables(&mut module);
        assert_eq!(indexed_jump_target_width(&module, EvmVersion::Osaka), 2);
    }
}

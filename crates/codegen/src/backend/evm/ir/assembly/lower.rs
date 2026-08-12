//! Lowering from block EVM IR to its finalized layout-linear form.

use super::{AsmInst, Program, indexed_jump};
use crate::backend::evm::{
    assembler::{Assembler, Label, PreparedAssembly},
    ir::{self, BlockId},
    op,
};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec};

impl Assembler<'_> {
    #[tracing::instrument(
        name = "evm_ir_pipeline",
        level = "debug",
        skip_all,
        fields(program = %self.program.name()),
    )]
    pub(in crate::backend::evm) fn prepare(&mut self, capture_evm_ir: bool) -> PreparedAssembly {
        let Some((mut ir_program, mut labels)) = self.finish_evm_ir() else {
            return PreparedAssembly::default();
        };

        ir::builder::resolve_known_deferred_constants(&mut ir_program, &self.deferred_values);

        let input_is_valid = cfg!(debug_assertions) && ir::builder::is_valid(&ir_program);
        let _changed = ir::run_pipeline(self.gcx, &mut ir_program, None);
        debug_assert!(!input_is_valid || ir::builder::is_valid(&ir_program));

        let program = lower_evm_ir(self, &mut ir_program, &mut labels);
        let evm_ir = capture_evm_ir.then_some(ir_program);
        PreparedAssembly {
            evm_ir,
            program,
            push_values: std::mem::take(&mut self.push_values),
            immutable_pushes: std::mem::take(&mut self.immutable_pushes),
            next_label: std::mem::take(&mut self.next_label),
            deferred_values: std::mem::take(&mut self.deferred_values),
        }
    }
}

/// Lowers finalized EVM IR into the linear label-bearing assembly stream.
pub(in crate::backend::evm) fn lower_evm_ir(
    assembler: &mut Assembler<'_>,
    module: &mut ir::Module,
    labels: &mut Vec<Option<Label>>,
) -> Program {
    let (mut indexed_jump_lowerings, mut tables) = indexed_jump::materialize_tables_with_metadata(
        module,
        assembler.gcx.sess.opts.evm_version,
        assembler.gcx.sess.opts.optimization.is_size(),
    );
    indexed_jump::initialize_indexed_jump_widths(
        &mut indexed_jump_lowerings,
        &tables,
        assembler.gcx.sess.opts.evm_version,
        assembler.gcx.sess.opts.optimization.is_size(),
    );
    for _ in 0..=32 {
        let program = lower_evm_ir_once(assembler, module, labels, &indexed_jump_lowerings);
        let (label_offsets, _) = assembler.resolve_label_offsets(&program);
        if !indexed_jump::refine_indexed_jump_widths(
            module,
            &mut tables,
            &mut indexed_jump_lowerings,
            labels,
            &label_offsets,
            assembler.gcx.sess.opts.evm_version,
            assembler.gcx.sess.opts.optimization.is_size(),
        ) {
            return program;
        }
    }
    panic!("indexed jump widths did not reach a fixed point")
}

fn lower_evm_ir_once(
    assembler: &mut Assembler<'_>,
    module: &mut ir::Module,
    labels: &mut Vec<Option<Label>>,
    indexed_jump_lowerings: &IndexVec<BlockId, indexed_jump::IndexedJumpLowering>,
) -> Program {
    allocate_referenced_labels(assembler, module, labels);

    let mut referenced_data = DenseBitSet::new_empty(module.data.len());
    for block in &module.blocks {
        for inst in &block.instructions {
            if let Some(ir::PushValue::Data(data)) = inst.value {
                referenced_data.insert(data);
            }
        }
    }

    let mut program = Program::default();
    for (block_id, block) in module.blocks.iter_enumerated() {
        let original = block.label as usize;
        if let Some(label) = labels.get(original).copied().flatten() {
            program.define_label(label);
        }

        for inst in &block.instructions {
            let inst = lower_instruction(assembler, inst, module, labels);
            program.push(inst);
        }

        if let Some(terminator) = &block.terminator {
            lower_terminator(
                assembler,
                &mut program,
                block_id,
                &terminator.kind,
                module,
                labels,
                indexed_jump_lowerings[block_id],
            );
        }
    }
    if !referenced_data.is_empty()
        && let Some(block) = module.blocks.last()
        && let Some(terminator) = &block.terminator
        && matches!(&terminator.kind, ir::TerminatorKind::Op(opcode) if *opcode == op::STOP)
    {
        program.push_op(op::STOP);
    }
    program.data.clone_from(&module.data);
    for data in referenced_data.iter() {
        program.append_data(data);
    }
    program
}

fn allocate_referenced_labels(
    assembler: &mut Assembler<'_>,
    module: &ir::Module,
    labels: &mut Vec<Option<Label>>,
) {
    let mut referenced = DenseBitSet::new_empty(module.blocks.len());
    for (block_id, block) in module.blocks.iter_enumerated() {
        for inst in &block.instructions {
            if let Some(target) = inst.pushed_block() {
                referenced.insert(target);
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
        label_for_block(assembler, module, block, labels);
    }
}

fn lower_instruction(
    assembler: &mut Assembler<'_>,
    inst: &ir::Instruction,
    module: &ir::Module,
    labels: &mut Vec<Option<Label>>,
) -> AsmInst {
    if let Some(id) = inst.deferred_push() {
        AsmInst::push_deferred(id)
    } else if let Some(id) = inst.immutable_push() {
        let type_size = inst.immutable_type_size().expect("validated immutable width");
        assembler.immutable_push_inst(id, type_size)
    } else if inst.is_encoded_push() {
        match &inst.value {
            Some(ir::PushValue::Immediate(value)) => assembler.push_inst(*value),
            Some(ir::PushValue::Block(block)) => {
                AsmInst::push_label(label_for_block(assembler, module, *block, labels))
            }
            Some(ir::PushValue::Data(data)) => AsmInst::push_data(*data),
            _ => unreachable!("push must have one immediate, block, or data operand"),
        }
    } else {
        AsmInst::op(inst.opcode)
    }
}

fn lower_terminator(
    assembler: &mut Assembler<'_>,
    program: &mut Program,
    block_id: BlockId,
    kind: &ir::TerminatorKind,
    module: &ir::Module,
    labels: &mut Vec<Option<Label>>,
    indexed_jump: indexed_jump::IndexedJumpLowering,
) {
    match kind {
        ir::TerminatorKind::Jump(target) => {
            if let Some(table_target_width) = indexed_jump.outlined_entry_width {
                let label = label_for_block(assembler, module, *target, labels);
                program.push(AsmInst::push_label_fixed(label, table_target_width));
                program.push_op(op::JUMP);
                return;
            }
            if next_block(module, block_id) == Some(*target) {
                return;
            }
            let label = label_for_block(assembler, module, *target, labels);
            program.push_label(label);
            program.push_op(op::JUMP);
        }
        ir::TerminatorKind::JumpI { then_block, else_block } => {
            let next = next_block(module, block_id);
            if next == Some(*else_block) {
                let label = label_for_block(assembler, module, *then_block, labels);
                program.push_label(label);
                program.push_op(op::JUMPI);
            } else if next == Some(*then_block) {
                program.push_op(op::ISZERO);
                let label = label_for_block(assembler, module, *else_block, labels);
                program.push_label(label);
                program.push_op(op::JUMPI);
            } else {
                let then_label = label_for_block(assembler, module, *then_block, labels);
                program.push_label(then_label);
                program.push_op(op::JUMPI);
                let else_label = label_for_block(assembler, module, *else_block, labels);
                program.push_label(else_label);
                program.push_op(op::JUMP);
            }
        }
        ir::TerminatorKind::IndexedJump(targets) => {
            indexed_jump::lower(assembler, program, targets, module, labels, indexed_jump);
        }
        ir::TerminatorKind::Op(opcode) => {
            if *opcode != op::STOP || next_block(module, block_id).is_some() {
                program.push_op(*opcode);
            }
        }
    }
}

pub(super) fn next_block(module: &ir::Module, block: BlockId) -> Option<BlockId> {
    let next = block.index() + 1;
    (next < module.blocks.len()).then(|| BlockId::from_usize(next))
}

pub(super) fn label_for_block(
    assembler: &mut Assembler<'_>,
    module: &ir::Module,
    block: BlockId,
    labels: &mut Vec<Option<Label>>,
) -> Label {
    let original = module.blocks[block].label as usize;
    if original >= labels.len() {
        labels.resize_with(original + 1, || None);
    }
    *labels[original].get_or_insert_with(|| assembler.new_label())
}

//! Lowering from block EVM IR to its finalized layout-linear form.

use super::{AsmInst, AsmInstKind, Program, indexed_jump};
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
        let errors_before = self.gcx.dcx().err_count();
        let _changed = ir::run_pipeline(self.gcx, &mut ir_program, None);
        if self.gcx.dcx().err_count() != errors_before {
            return failed_preparation(ir_program, capture_evm_ir);
        }
        debug_assert!(!input_is_valid || ir::builder::is_valid(&ir_program));
        let _legalized = ir::legalize_shifts(self.gcx, &mut ir_program);
        if self.gcx.dcx().err_count() != errors_before {
            return failed_preparation(ir_program, capture_evm_ir);
        }
        if !self.gcx.sess.opts.evm_version.has_bitwise_shifting() {
            ir::validate(self.gcx.dcx(), &ir_program);
        }
        ir::validate_evm_version(self.gcx.dcx(), &ir_program, self.gcx.sess.opts.evm_version);
        if self.gcx.dcx().err_count() != errors_before {
            return failed_preparation(ir_program, capture_evm_ir);
        }

        let program = lower_evm_ir(self, &mut ir_program, &mut labels);
        validate_program_evm_version(self, &program);
        if self.gcx.dcx().err_count() != errors_before {
            return failed_preparation(ir_program, capture_evm_ir);
        }
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

fn failed_preparation(ir_program: ir::Module, capture_evm_ir: bool) -> PreparedAssembly {
    PreparedAssembly { evm_ir: capture_evm_ir.then_some(ir_program), ..Default::default() }
}

fn validate_program_evm_version(assembler: &Assembler<'_>, program: &Program) {
    let evm_version = assembler.gcx.sess.opts.evm_version;
    for inst in &program.instructions {
        let (opcode, immediate) = match inst.kind() {
            AsmInstKind::Op(opcode) => (opcode, None),
            AsmInstKind::OpImmediate(opcode, immediate) => (opcode, Some(immediate)),
            _ => continue,
        };
        let name = op::mnemonic(opcode).unwrap_or("unknown");
        if !op::is_available(opcode, evm_version) {
            assembler
                .gcx
                .dcx()
                .err(format!(
                    "final assembly opcode `{name}` is unavailable for `{evm_version}` EVM"
                ))
                .emit();
            continue;
        }
        let Some(immediate) = immediate else { continue };
        let valid = match opcode {
            op::DUPN | op::SWAPN => op::decode_stack_depth(immediate).is_some(),
            op::EXCHANGE => op::decode_exchange(immediate).is_some(),
            _ => false,
        };
        if !valid {
            assembler
                .gcx
                .dcx()
                .err(format!(
                    "final assembly opcode `{name}` has invalid immediate `0x{immediate:02x}`"
                ))
                .emit();
        }
    }
}

/// Lowers finalized EVM IR into the linear label-bearing assembly stream.
pub(in crate::backend::evm) fn lower_evm_ir(
    assembler: &mut Assembler<'_>,
    module: &mut ir::Module,
    labels: &mut Vec<Option<Label>>,
) -> Program {
    // Finalized EVM IR represents every control-flow reference as a `BlockId`; the builder's
    // assembler-label table is no longer authoritative. Rebuild it for the final module so a
    // transform that deletes a block and reuses its sparse textual label cannot inherit the
    // deleted block's assembler label.
    reset_assembler_labels(labels);
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
    let data_layout_is_observable = module.data_layout_is_observable();
    for _ in 0..=32 {
        let program = lower_evm_ir_once(
            assembler,
            module,
            labels,
            &indexed_jump_lowerings,
            data_layout_is_observable,
        );
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
    data_layout_is_observable: bool,
) -> Program {
    allocate_referenced_labels(assembler, module, labels);

    let mut referenced_data = DenseBitSet::new_empty(module.data.len());
    for (id, data) in module.data.iter_enumerated() {
        if data.emit_in_runtime {
            referenced_data.insert(id);
        }
    }
    for block in &module.blocks {
        for inst in &block.instructions {
            if let Some(ir::PushValue::Data(data)) = inst.value {
                referenced_data.insert(data.id);
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
            lower_instruction(assembler, &mut program, inst, module, labels);
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
    // Keep opaque data unreachable from physical fallthrough, including malformed internal IR.
    let ends_with_terminal = program.instructions.last().is_some_and(
        |inst| matches!(inst.kind(), AsmInstKind::Op(opcode) if op::is_terminal(opcode)),
    );
    if !referenced_data.is_empty() && !ends_with_terminal {
        program.push_op(op::STOP);
    }
    program.data.clone_from(&module.data);
    if data_layout_is_observable {
        for data in module.data.indices() {
            program.append_data(data);
        }
    } else {
        for data in referenced_data.iter() {
            program.append_data(data);
        }
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

fn reset_assembler_labels(labels: &mut [Option<Label>]) {
    labels.fill(None);
}

fn lower_instruction(
    assembler: &mut Assembler<'_>,
    program: &mut Program,
    inst: &ir::Instruction,
    module: &ir::Module,
    labels: &mut Vec<Option<Label>>,
) {
    let inst = if let Some(id) = inst.deferred_push() {
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
            Some(ir::PushValue::Data(data)) => AsmInst::push_data(program.push_data_ref(*data)),
            _ => unreachable!("push must have one immediate, block, or data operand"),
        }
    } else if let Some(stack_op) = inst.as_stack_op() {
        match stack_op
            .lowering(assembler.gcx.sess.opts.evm_version)
            .expect("stack operation must support the target EVM version")
        {
            op::StackOpLowering::Direct(opcode, immediate) => {
                program.push(immediate.map_or_else(
                    || AsmInst::op(opcode),
                    |value| AsmInst::op_immediate(opcode, value),
                ))
            }
            op::StackOpLowering::LegacyExchange(opcodes) => {
                for opcode in opcodes {
                    program.push_op(opcode);
                }
            }
        }
        return;
    } else {
        AsmInst::op(inst.opcode)
    };
    program.push(inst);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::evm::ir::Block;
    use solar_interface::{Session, sym};
    use solar_sema::Compiler;

    #[test]
    fn finalized_ir_does_not_retain_builder_labels() {
        let mut module = ir::Module::new(sym::module);
        let entry = module.add_block(Block::new(0));
        let middle = module.add_block(Block::new(2));
        // Model a transform reusing textual label 1 after deleting its original block.
        let target = module.add_block(Block::new(1));
        module.blocks[entry].terminator =
            Some(ir::Terminator::new(ir::TerminatorKind::Jump(target)));
        module.blocks[middle].terminator =
            Some(ir::Terminator::new(ir::TerminatorKind::Op(op::INVALID)));
        module.blocks[target].terminator =
            Some(ir::Terminator::new(ir::TerminatorKind::Op(op::STOP)));

        let compiler = Compiler::new(Session::builder().opts(Default::default()).build());
        compiler.enter(|c| {
            let mut assembler = Assembler::new(c.gcx());
            let old_entry = assembler.new_label();
            let old_target = assembler.new_label();
            let mut labels = vec![Some(old_entry), Some(old_target), None];

            let program = lower_evm_ir(&mut assembler, &mut module, &mut labels);
            let target_label =
                labels[module.blocks[target].label as usize].expect("referenced target label");

            assert_ne!(target_label, old_target);
            assert!(program.instructions.iter().any(|inst| {
                matches!(inst.kind(), AsmInstKind::PushLabel(label) if label == target_label)
            }));
            assert!(program.instructions.iter().any(|inst| {
                matches!(inst.kind(), AsmInstKind::Label(label) if label == target_label)
            }));
        });
    }
}

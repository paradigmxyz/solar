//! Final preparation of block EVM IR and lowering to primitive assembly.
//!
//! Optimization, shift legalization and target validation finish before byte encoding.
//! Block identities supply compact labels independently of textual names. Indexed tables
//! are materialized here while control-flow identity is still explicit; their entry widths
//! are refined against exact assembler offsets until stable. Ordinary label PUSH widths
//! remain the assembler's responsibility. Opaque data follows the instruction stream, with
//! original data order retained whenever code can observe its layout.

use super::{AsmInst, AsmInstKind, Program, indexed_jump};
use crate::backend::evm::{
    assembler::{Assembler, DeferredConst, Label, PreparedAssembly},
    ir::{self, BlockId},
    op,
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
use solar_sema::Gcx;

/// Optimizes and legalizes finalized block IR before constructing primitive assembly.
#[tracing::instrument(name = "evm_ir_pipeline", level = "debug", skip_all, fields(program = %ir_program.name()))]
pub(in crate::backend::evm) fn prepare(
    gcx: Gcx<'_>,
    mut ir_program: ir::Module,
    deferred_values: FxHashMap<DeferredConst, U256>,
    capture_evm_ir: bool,
) -> PreparedAssembly {
    if ir_program.blocks.is_empty() {
        return PreparedAssembly::default();
    }
    ir::builder::resolve_known_deferred_constants(&mut ir_program, &deferred_values);
    let input_is_valid = cfg!(debug_assertions) && ir::verify::Verifier::is_valid(&ir_program);
    let errors_before = gcx.dcx().err_count();
    let _changed = ir::run_pipeline(gcx, &mut ir_program, None);
    if gcx.dcx().err_count() != errors_before {
        return failed_preparation(ir_program, capture_evm_ir);
    }
    debug_assert!(!input_is_valid || ir::verify::Verifier::is_valid(&ir_program));
    let _legalized = ir::legalize_shifts(gcx, &mut ir_program);
    if gcx.dcx().err_count() != errors_before {
        return failed_preparation(ir_program, capture_evm_ir);
    }
    ir::verify::Verifier::new(gcx).verify_after_legalization(&ir_program);
    if gcx.dcx().err_count() != errors_before {
        return failed_preparation(ir_program, capture_evm_ir);
    }
    let program = lower_evm_ir(gcx, &mut ir_program, &deferred_values);
    validate_program_evm_version(gcx, &program);
    if gcx.dcx().err_count() != errors_before {
        return failed_preparation(ir_program, capture_evm_ir);
    }
    PreparedAssembly { evm_ir: capture_evm_ir.then_some(ir_program), program, deferred_values }
}

fn failed_preparation(ir_program: ir::Module, capture_evm_ir: bool) -> PreparedAssembly {
    PreparedAssembly { evm_ir: capture_evm_ir.then_some(ir_program), ..Default::default() }
}

fn validate_program_evm_version(gcx: Gcx<'_>, program: &Program) {
    let evm_version = gcx.sess.opts.evm_version;
    for inst in &program.instructions {
        let (opcode, immediate) = match inst.kind() {
            AsmInstKind::Op(opcode) => (opcode, None),
            AsmInstKind::OpImmediate(opcode, immediate) => (opcode, Some(immediate)),
            _ => continue,
        };
        let name = op::mnemonic(opcode).unwrap_or("unknown");
        if !op::is_available(opcode, evm_version) {
            gcx.dcx()
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
            gcx.dcx()
                .err(format!(
                    "final assembly opcode `{name}` has invalid immediate `0x{immediate:02x}`"
                ))
                .emit();
        }
    }
}

/// Lowers finalized EVM IR into the linear label-bearing assembly stream.
pub(in crate::backend::evm) fn lower_evm_ir(
    gcx: Gcx<'_>,
    module: &mut ir::Module,
    deferred_values: &FxHashMap<DeferredConst, U256>,
) -> Program {
    let (mut indexed_jump_lowerings, mut tables) = indexed_jump::materialize_tables_with_metadata(
        module,
        gcx.sess.opts.evm_version,
        gcx.sess.opts.optimization.is_size(),
    );
    indexed_jump::initialize_indexed_jump_widths(
        &mut indexed_jump_lowerings,
        &tables,
        gcx.sess.opts.evm_version,
        gcx.sess.opts.optimization.is_size(),
    );
    let data_layout_is_observable = module.data_layout_is_observable();
    for _ in 0..=32 {
        let program =
            lower_evm_ir_once(gcx, module, &indexed_jump_lowerings, data_layout_is_observable);
        let label_offsets =
            Assembler::new(gcx.sess.opts.evm_version, &program, deferred_values, &[])
                .label_offsets();
        if !indexed_jump::refine_indexed_jump_widths(
            module,
            &mut tables,
            &mut indexed_jump_lowerings,
            &label_offsets,
            gcx.sess.opts.evm_version,
            gcx.sess.opts.optimization.is_size(),
        ) {
            return program;
        }
    }
    panic!("indexed jump widths did not reach a fixed point")
}

fn lower_evm_ir_once(
    gcx: Gcx<'_>,
    module: &mut ir::Module,
    indexed_jump_lowerings: &IndexVec<BlockId, indexed_jump::IndexedJumpLowering>,
    data_layout_is_observable: bool,
) -> Program {
    let referenced_labels = referenced_labels(module);

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
        if referenced_labels.contains(block_id) {
            program.define_label(label_for_block(block_id));
        }

        for inst in &block.instructions {
            lower_instruction(gcx, &mut program, inst);
        }

        if let Some(terminator) = &block.terminator {
            lower_terminator(
                gcx,
                &mut program,
                block_id,
                &terminator.kind,
                module,
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

fn referenced_labels(module: &ir::Module) -> DenseBitSet<BlockId> {
    let mut referenced = DenseBitSet::new_empty(module.blocks.len());
    for (block_id, block) in module.blocks.iter_enumerated() {
        for inst in &block.instructions {
            if let Some(target) = inst.pushed_block() {
                referenced.insert(target);
            }
        }
        if let Some(terminator) = &block.terminator {
            let next = module.next_block(block_id);
            terminator.kind.visit_label_targets(next, |target| {
                referenced.insert(target);
            });
        }
    }
    referenced
}

fn lower_instruction(gcx: Gcx<'_>, program: &mut Program, inst: &ir::Instruction) {
    // push operand -> primitive push with its typed relocation or interned value
    let inst = if let Some(value) = inst.value {
        match value {
            ir::PushValue::Immediate(value) => program.push_inst(value),
            ir::PushValue::Block(block) => AsmInst::push_label(label_for_block(block)),
            ir::PushValue::Data(data) => AsmInst::push_data(program.push_data_ref(data)),
            ir::PushValue::Deferred(id) => AsmInst::push_deferred(id),
            ir::PushValue::Immutable(id) => program.immutable_push_inst(
                id,
                inst.immutable_type_size().expect("validated immutable width"),
            ),
            ir::PushValue::Label(_) | ir::PushValue::Alloc(_) => {
                unreachable!("construction-only operand in finalized EVM IR")
            }
        }
    } else if let Some(stack_op) = inst.as_stack_op() {
        match stack_op
            .lowering(gcx.sess.opts.evm_version)
            .expect("stack operation must support the target EVM version")
        {
            // stack operation -> target opcode and optional immediate
            op::StackOpLowering::Direct(opcode, immediate) => {
                program.push(immediate.map_or_else(
                    || AsmInst::op(opcode),
                    |value| AsmInst::op_immediate(opcode, value),
                ))
            }
            // exchange -> swap sequence
            op::StackOpLowering::SwapSequence(opcodes) => {
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
    gcx: Gcx<'_>,
    program: &mut Program,
    block_id: BlockId,
    kind: &ir::TerminatorKind,
    module: &ir::Module,
    indexed_jump: indexed_jump::IndexedJumpLowering,
) {
    match kind {
        ir::TerminatorKind::Jump(target) => {
            // push_fixed target; jump
            if let Some(table_target_width) = indexed_jump.outlined_entry_width {
                let label = label_for_block(*target);
                program.push(AsmInst::push_label_fixed(label, table_target_width));
                program.push_op(op::JUMP);
                return;
            }
            if module.next_block(block_id) == Some(*target) {
                return;
            }
            // push target; jump
            let label = label_for_block(*target);
            program.push_label(label);
            program.push_op(op::JUMP);
        }
        ir::TerminatorKind::JumpI { then_block, else_block } => {
            let next = module.next_block(block_id);
            // push then; jumpi; fall through to else
            if next == Some(*else_block) {
                let label = label_for_block(*then_block);
                program.push_label(label);
                program.push_op(op::JUMPI);
            // iszero; push else; jumpi; fall through to then
            } else if next == Some(*then_block) {
                program.push_op(op::ISZERO);
                let label = label_for_block(*else_block);
                program.push_label(label);
                program.push_op(op::JUMPI);
            // push then; jumpi; push else; jump
            } else {
                let then_label = label_for_block(*then_block);
                program.push_label(then_label);
                program.push_op(op::JUMPI);
                let else_label = label_for_block(*else_block);
                program.push_label(else_label);
                program.push_op(op::JUMP);
            }
        }
        ir::TerminatorKind::IndexedJump(targets) => {
            indexed_jump::lower(gcx.sess.opts.evm_version, program, targets, indexed_jump);
        }
        ir::TerminatorKind::Op(opcode) => {
            if *opcode != op::STOP || module.next_block(block_id).is_some() {
                program.push_op(*opcode);
            }
        }
    }
}

/// Compact labels use block identity; textual labels have no role in byte encoding.
pub(super) fn label_for_block(block: BlockId) -> Label {
    Label::from_usize(block.index())
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
            let program = lower_evm_ir(c.gcx(), &mut module, &FxHashMap::default());
            let target_label = label_for_block(target);
            assert_ne!(target_label.index(), module.blocks[target].label as usize);
            assert!(program.instructions.iter().any(|inst| {
                matches!(inst.kind(), AsmInstKind::PushLabel(label) if label == target_label)
            }));
            assert!(program.instructions.iter().any(|inst| {
                matches!(inst.kind(), AsmInstKind::Label(label) if label == target_label)
            }));
        });
    }
}

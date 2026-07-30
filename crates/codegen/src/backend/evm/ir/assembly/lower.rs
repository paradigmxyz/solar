//! Lowering from block EVM IR to its finalized layout-linear form.

use super::{AsmInst, Program};
use crate::backend::evm::{
    assembler::{Assembler, Label},
    ir::{self, BlockId},
    op,
};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
};

#[derive(Clone, Copy)]
struct IndexedJumpEncoding {
    width: u8,
    packed_chunks: PackedTableChunks,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PackedTableChunks {
    #[default]
    None,
    One,
    Two,
}

#[derive(Clone, Copy, Default)]
struct IndexedJumpLowering {
    table: Option<IndexedJumpEncoding>,
    entry_width: Option<u8>,
}

#[derive(Clone, Copy)]
struct PackedTableEstimate {
    len: usize,
    width: u8,
    chunks: PackedTableChunks,
}

struct IndexedJumpTable {
    source: BlockId,
    entries: Box<[BlockId]>,
    targets: Box<[BlockId]>,
}

/// Lowers finalized EVM IR into the linear label-bearing assembly stream.
pub(in crate::backend::evm) fn lower_evm_ir(
    module: &mut ir::Module,
    labels: &mut Vec<Option<Label>>,
    assembler: &mut Assembler<'_>,
    evm_version: EvmVersion,
    pack_two_word_tables: bool,
) -> Program {
    let indexed_jump_lowerings =
        materialize_indexed_jump_tables(module, evm_version, pack_two_word_tables);
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
                indexed_jump_lowerings[block_id],
            );
        }
    }
    program
}

fn materialize_indexed_jump_tables(
    module: &mut ir::Module,
    evm_version: EvmVersion,
    pack_two_word_tables: bool,
) -> IndexVec<BlockId, IndexedJumpLowering> {
    let tables = module
        .blocks
        .iter_enumerated()
        .filter_map(|(block, data)| {
            let targets = match &data.terminator.as_ref()?.kind {
                ir::TerminatorKind::IndexedJump(targets) => targets.clone(),
                _ => return None,
            };
            Some(IndexedJumpTable { source: block, entries: Box::new([]), targets })
        })
        .collect::<Vec<_>>();
    if tables.is_empty() {
        return index_vec![IndexedJumpLowering::default(); module.blocks.len()];
    }

    let global_width = indexed_jump_global_width(module, evm_version, &tables);
    let mut next_label = module
        .blocks
        .iter()
        .map(|block| block.label)
        .max()
        .map_or(0, |label| label.checked_add(1).expect("EVM IR block label overflow"));
    let mut tables = tables;

    let no_packed_tables = index_vec![None; module.blocks.len()];
    let offsets = estimated_block_offsets(module, evm_version, global_width, &no_packed_tables);
    let mut encodings = tables
        .iter()
        .map(|table| {
            let width = indexed_jump_target_width(&table.targets, &offsets, global_width);
            IndexedJumpEncoding {
                width,
                packed_chunks: indexed_jump_packed_chunks(
                    table.targets.len(),
                    width,
                    evm_version,
                    pack_two_word_tables,
                ),
            }
        })
        .collect::<Vec<_>>();

    loop {
        let mut packed_estimates = index_vec![None; module.blocks.len()];
        for (table, encoding) in tables.iter().zip(&encodings) {
            if encoding.packed_chunks != PackedTableChunks::None {
                packed_estimates[table.source] = Some(PackedTableEstimate {
                    len: table.targets.len(),
                    width: encoding.width,
                    chunks: encoding.packed_chunks,
                });
            }
        }
        let offsets = estimated_block_offsets(module, evm_version, global_width, &packed_estimates);
        let mut changed = false;
        for (table, encoding) in tables.iter().zip(&mut encodings) {
            let required_width = indexed_jump_target_width(&table.targets, &offsets, global_width);
            if required_width > encoding.width {
                let packed_chunks = indexed_jump_packed_chunks(
                    table.targets.len(),
                    required_width,
                    evm_version,
                    pack_two_word_tables,
                );
                if encoding.packed_chunks != PackedTableChunks::None
                    && packed_chunks == PackedTableChunks::None
                {
                    encoding.packed_chunks = PackedTableChunks::None;
                } else {
                    encoding.width = required_width;
                    encoding.packed_chunks = packed_chunks;
                }
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    for (table, encoding) in tables.iter_mut().zip(&encodings) {
        if encoding.packed_chunks == PackedTableChunks::None {
            let targets = table.targets.clone();
            let mut entries = Vec::with_capacity(targets.len());
            for &target in &targets {
                let mut block = ir::Block::new(next_label);
                next_label = next_label.checked_add(1).expect("EVM IR block label overflow");
                block.terminator = Some(ir::Terminator::new(ir::TerminatorKind::Jump(target)));
                let entry = module.add_block(block);
                entries.push(entry);
            }
            module.blocks[table.source]
                .terminator
                .as_mut()
                .expect("indexed jump source must have a terminator")
                .kind = ir::TerminatorKind::IndexedJump(entries.clone().into_boxed_slice());
            table.entries = entries.into_boxed_slice();
        }
    }

    let mut lowerings = index_vec![IndexedJumpLowering::default(); module.blocks.len()];
    for (table, encoding) in tables.into_iter().zip(encodings) {
        lowerings[table.source].table = Some(encoding);
        for &entry in &table.entries {
            lowerings[entry].entry_width = Some(encoding.width);
        }
    }
    lowerings
}

fn indexed_jump_target_width(
    targets: &[BlockId],
    offsets: &IndexVec<BlockId, usize>,
    global_width: u8,
) -> u8 {
    let max_offset = targets.iter().map(|&target| offsets[target]).max().unwrap_or(0);
    (1..=global_width)
        .find(|&width| push_width_fits(max_offset.saturating_add(1), width))
        .unwrap_or(global_width)
}

fn indexed_jump_global_width(
    module: &ir::Module,
    evm_version: EvmVersion,
    tables: &[IndexedJumpTable],
) -> u8 {
    let no_packed_tables = IndexVec::new();
    (1..=32)
        .find(|&width| {
            let table_stubs = tables
                .iter()
                .map(|table| table.targets.len().saturating_mul(usize::from(width) + 3))
                .fold(0usize, usize::saturating_add);
            let size = estimated_module_size(module, evm_version, width, &no_packed_tables)
                .saturating_add(table_stubs);
            push_width_fits(size, width)
        })
        .expect("a bytecode offset must fit one EVM word")
}

fn push_width_fits(size: usize, width: u8) -> bool {
    let bits = u32::from(width) * 8;
    bits >= usize::BITS || size <= 1usize << bits
}

fn indexed_jump_packed_chunks(
    table_len: usize,
    target_width: u8,
    evm_version: EvmVersion,
    pack_two_word_tables: bool,
) -> PackedTableChunks {
    if !evm_version.has_bitwise_shifting() || table_len < 2 {
        return PackedTableChunks::None;
    }
    let bytes = table_len.saturating_mul(usize::from(target_width));
    if bytes <= 32 {
        PackedTableChunks::One
    } else {
        let entries_per_chunk = 32 / usize::from(target_width);
        let table = PackedTableEstimate {
            len: table_len,
            width: target_width,
            chunks: PackedTableChunks::Two,
        };
        if pack_two_word_tables
            && target_width.is_power_of_two()
            && entries_per_chunk >= 2
            && bytes <= 64
            && packed_indexed_jump_len(table, evm_version)
                < outlined_indexed_jump_len(table_len, target_width)
        {
            PackedTableChunks::Two
        } else {
            PackedTableChunks::None
        }
    }
}

fn outlined_indexed_jump_len(table_len: usize, target_width: u8) -> usize {
    usize::from(target_width) + 6 + table_len.saturating_mul(usize::from(target_width) + 3)
}

fn estimated_block_offsets(
    module: &ir::Module,
    evm_version: EvmVersion,
    block_target_width: u8,
    packed_tables: &IndexVec<BlockId, Option<PackedTableEstimate>>,
) -> IndexVec<BlockId, usize> {
    let mut offsets = IndexVec::with_capacity(module.blocks.len());
    let mut offset = 0usize;
    for (block_id, block) in module.blocks.iter_enumerated() {
        offsets.push(offset);
        offset = offset.saturating_add(estimated_block_size(
            module,
            block_id,
            block,
            evm_version,
            block_target_width,
            packed_tables.get(block_id).copied().flatten(),
        ));
    }
    offsets
}

fn estimated_module_size(
    module: &ir::Module,
    evm_version: EvmVersion,
    block_target_width: u8,
    packed_tables: &IndexVec<BlockId, Option<PackedTableEstimate>>,
) -> usize {
    module
        .blocks
        .iter_enumerated()
        .map(|(block_id, block)| {
            estimated_block_size(
                module,
                block_id,
                block,
                evm_version,
                block_target_width,
                packed_tables.get(block_id).copied().flatten(),
            )
        })
        .fold(0, usize::saturating_add)
}

fn estimated_block_size(
    module: &ir::Module,
    block_id: BlockId,
    block: &ir::Block,
    evm_version: EvmVersion,
    block_target_width: u8,
    packed_table: Option<PackedTableEstimate>,
) -> usize {
    let mut size = 1usize;
    for inst in &block.instructions {
        let inst_size = if inst.deferred_push().is_some() || inst.immutable_push().is_some() {
            33
        } else if inst.is_encoded_push() {
            match &inst.value {
                Some(ir::PushValue::Immediate(value)) => push_len(*value, evm_version),
                Some(ir::PushValue::Block(_)) => usize::from(block_target_width) + 1,
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
            block_target_width,
            packed_table,
            evm_version,
        ));
    }
    size
}

fn estimated_terminator_size(
    kind: &ir::TerminatorKind,
    next: Option<BlockId>,
    width: u8,
    packed_table: Option<PackedTableEstimate>,
    evm_version: EvmVersion,
) -> usize {
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
        ir::TerminatorKind::IndexedJump(_) => {
            packed_table.map_or(push + 5, |table| packed_indexed_jump_len(table, evm_version))
        }
        ir::TerminatorKind::Op(op::STOP) => usize::from(next.is_some()),
        ir::TerminatorKind::Op(_) => 1,
    }
}

fn packed_indexed_jump_len(table: PackedTableEstimate, evm_version: EvmVersion) -> usize {
    if table.chunks == PackedTableChunks::One {
        return 9 + table.len * usize::from(table.width) + usize::from(table.width);
    }
    debug_assert_eq!(table.chunks, PackedTableChunks::Two);

    let width = usize::from(table.width);
    let entries_per_chunk = 32 / width;
    let second_chunk_bytes = (table.len - entries_per_chunk) * width;
    let chunk_shift = entries_per_chunk.ilog2();
    let entry_mask = entries_per_chunk - 1;
    let scale_shift = (u32::from(table.width) * 8).ilog2();
    let target_mask = (U256::ONE << (u32::from(table.width) * 8)) - U256::ONE;
    let opcode_bytes = 14;
    let second_chunk_push = second_chunk_bytes + 1;
    let first_chunk_push = 33;
    opcode_bytes
        + push_len(U256::from(chunk_shift), evm_version)
        + second_chunk_push
        + first_chunk_push
        + push_len(U256::from(entry_mask), evm_version)
        + push_len(U256::from(scale_shift), evm_version)
        + push_len(target_mask, evm_version)
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
    indexed_jump: IndexedJumpLowering,
) {
    match kind {
        ir::TerminatorKind::Jump(target) => {
            if let Some(table_target_width) = indexed_jump.entry_width {
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
            let table_encoding = indexed_jump.table.expect("indexed jump table encoding");
            if table_encoding.packed_chunks != PackedTableChunks::None {
                let target_width = table_encoding.width;
                let scale = u32::from(target_width) * 8;
                let labels = targets
                    .iter()
                    .map(|&target| label_for_block(module, target, labels, assembler))
                    .collect::<Vec<_>>();
                if table_encoding.packed_chunks == PackedTableChunks::Two {
                    let entries_per_chunk = 32 / usize::from(target_width);
                    let (first, second) = labels.split_at(entries_per_chunk);
                    // Select one of the two words without branching.
                    program.push_op(op::DUP1);
                    program.push(
                        AsmInst::push_inline(entries_per_chunk.ilog2())
                            .expect("indexed jump chunk shift must fit inline"),
                    );
                    program.push_op(op::SHR);
                    program.push_op(op::DUP1);
                    program.push_packed_labels(second.into(), target_width);
                    program.push_op(op::MUL);
                    program.push_op(op::SWAP1);
                    program.push_op(op::ISZERO);
                    program.push_packed_labels(first.into(), target_width);
                    program.push_op(op::MUL);
                    program.push_op(op::ADD);
                    program.push_op(op::SWAP1);
                    program.push(
                        AsmInst::push_inline((entries_per_chunk - 1) as u32)
                            .expect("indexed jump chunk mask must fit inline"),
                    );
                    program.push_op(op::AND);
                }
                if scale.is_power_of_two() {
                    program.push(
                        AsmInst::push_inline(scale.ilog2())
                            .expect("indexed jump scale must fit inline"),
                    );
                    program.push_op(op::SHL);
                } else {
                    program.push(
                        AsmInst::push_inline(scale).expect("indexed jump scale must fit inline"),
                    );
                    program.push_op(op::MUL);
                }
                if table_encoding.packed_chunks == PackedTableChunks::One {
                    program.push_packed_labels(labels.into_boxed_slice(), target_width);
                    program.push_op(op::SWAP1);
                }
                program.push_op(op::SHR);
                let mask = (U256::ONE << scale) - U256::ONE;
                program.push(assembler.push_inst(mask));
                program.push_op(op::AND);
                program.push_op(op::JUMP);
                return;
            }

            let (&table, rest) = targets.split_first().expect("validated indexed jump table");
            debug_assert!(
                rest.iter()
                    .enumerate()
                    .all(|(index, target)| { target.index() == table.index() + index + 1 })
            );
            let table_target_width = table_encoding.width;
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
    use solar_interface::{Session, sym};
    use solar_sema::Compiler;

    #[test]
    fn branch_inverts_when_then_target_falls_through() {
        let mut module = ir::Module::new(sym::module);
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
            let program =
                lower_evm_ir(&mut module, &mut labels, &mut assembler, EvmVersion::Osaka, false);
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
    fn indexed_jump_packs_direct_targets() {
        let mut module = ir::Module::new(sym::module);
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
            let program =
                lower_evm_ir(&mut module, &mut labels, &mut assembler, EvmVersion::Osaka, false);

            assert_eq!(module.blocks.len(), 3);
            assert!(matches!(
                &module.blocks[entry].terminator.as_ref().unwrap().kind,
                TerminatorKind::IndexedJump(targets) if targets.as_ref() == [left, right]
            ));
            let table = program
                .instructions
                .iter()
                .find_map(|inst| match inst.kind() {
                    AsmInstKind::PushPackedLabels(labels) => Some(&program.packed_labels[labels]),
                    _ => None,
                })
                .expect("packed label table");
            assert_eq!(table.labels.len(), 2);
            assert_eq!(table.label_width, 1);
        });
    }

    #[test]
    fn indexed_jump_entries_are_reachable_blocks() {
        let mut module = ir::Module::new(sym::module);
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
            let program = lower_evm_ir(
                &mut module,
                &mut labels,
                &mut assembler,
                EvmVersion::Byzantium,
                false,
            );

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
        let mut module = ir::Module::new(sym::module);
        let entry = module.add_block(Block::new(0));
        let target = module.add_block(Block::new(1));
        for id in 0..8 {
            module.blocks[entry].instructions.push(Instruction::push_immutable(id));
        }
        module.blocks[entry].terminator =
            Some(Terminator::new(TerminatorKind::IndexedJump(vec![target].into_boxed_slice())));
        module.blocks[target].terminator = Some(Terminator::new(TerminatorKind::Op(op::STOP)));

        let lowerings = materialize_indexed_jump_tables(&mut module, EvmVersion::Osaka, false);
        let TerminatorKind::IndexedJump(entries) =
            &module.blocks[entry].terminator.as_ref().unwrap().kind
        else {
            panic!("expected indexed jump")
        };
        assert_eq!(lowerings[entries[0]].entry_width, Some(2));
    }

    #[test]
    fn packs_early_table_targets_in_large_modules() {
        let mut module = ir::Module::new(sym::module);
        let entry = module.add_block(Block::new(0));
        let targets = (1..=17)
            .map(|label| {
                let target = module.add_block(Block::new(label));
                module.blocks[target].terminator =
                    Some(Terminator::new(TerminatorKind::Op(op::STOP)));
                target
            })
            .collect::<Vec<_>>();
        module.blocks[entry].terminator =
            Some(Terminator::new(TerminatorKind::IndexedJump(targets.clone().into_boxed_slice())));
        let padding = module.add_block(Block::new(18));
        for id in 0..8 {
            module.blocks[padding].instructions.push(Instruction::push_immutable(id));
        }
        module.blocks[padding].terminator = Some(Terminator::new(TerminatorKind::Op(op::STOP)));

        let lowerings = materialize_indexed_jump_tables(&mut module, EvmVersion::Osaka, false);
        let TerminatorKind::IndexedJump(actual_targets) =
            &module.blocks[entry].terminator.as_ref().unwrap().kind
        else {
            panic!("expected indexed jump")
        };
        assert_eq!(actual_targets.as_ref(), targets);
        assert_eq!(lowerings[entry].table.unwrap().width, 1);
        assert_eq!(lowerings[entry].table.unwrap().packed_chunks, PackedTableChunks::One);
    }

    #[test]
    fn packs_early_two_word_table_targets_in_large_modules() {
        let mut module = ir::Module::new(sym::module);
        let entry = module.add_block(Block::new(0));
        let targets = (1..=33)
            .map(|label| {
                let target = module.add_block(Block::new(label));
                module.blocks[target].terminator =
                    Some(Terminator::new(TerminatorKind::Op(op::STOP)));
                target
            })
            .collect::<Vec<_>>();
        module.blocks[entry].terminator =
            Some(Terminator::new(TerminatorKind::IndexedJump(targets.clone().into_boxed_slice())));
        let padding = module.add_block(Block::new(34));
        for id in 0..8 {
            module.blocks[padding].instructions.push(Instruction::push_immutable(id));
        }
        module.blocks[padding].terminator = Some(Terminator::new(TerminatorKind::Op(op::STOP)));

        let lowerings = materialize_indexed_jump_tables(&mut module, EvmVersion::Osaka, true);
        let TerminatorKind::IndexedJump(actual_targets) =
            &module.blocks[entry].terminator.as_ref().unwrap().kind
        else {
            panic!("expected indexed jump")
        };
        assert_eq!(actual_targets.as_ref(), targets);
        assert_eq!(lowerings[entry].table.unwrap().width, 1);
        assert_eq!(lowerings[entry].table.unwrap().packed_chunks, PackedTableChunks::Two);
    }

    #[test]
    fn outlines_when_packing_would_widen_targets() {
        let mut module = ir::Module::new(sym::module);
        let entry = module.add_block(Block::new(0));
        let targets = (1..=24)
            .map(|label| {
                let target = module.add_block(Block::new(label));
                for _ in 0..4 {
                    module.blocks[target].instructions.push(Instruction::push_value(U256::ONE));
                }
                module.blocks[target].terminator =
                    Some(Terminator::new(TerminatorKind::Op(op::STOP)));
                target
            })
            .collect::<Vec<_>>();
        module.blocks[entry].terminator =
            Some(Terminator::new(TerminatorKind::IndexedJump(targets.clone().into_boxed_slice())));
        let padding = module.add_block(Block::new(25));
        for id in 0..8 {
            module.blocks[padding].instructions.push(Instruction::push_immutable(id));
        }
        module.blocks[padding].terminator = Some(Terminator::new(TerminatorKind::Op(op::STOP)));

        let lowerings = materialize_indexed_jump_tables(&mut module, EvmVersion::Osaka, false);
        let TerminatorKind::IndexedJump(entries) =
            &module.blocks[entry].terminator.as_ref().unwrap().kind
        else {
            panic!("expected indexed jump")
        };
        assert_ne!(entries.as_ref(), targets);
        assert_eq!(lowerings[entry].table.unwrap().width, 1);
        assert_eq!(lowerings[entry].table.unwrap().packed_chunks, PackedTableChunks::None);
        assert!(entries.iter().all(|&entry| lowerings[entry].entry_width == Some(1)));
    }

    #[test]
    fn two_word_packing_requires_a_size_win() {
        assert_eq!(
            indexed_jump_packed_chunks(7, 8, EvmVersion::Osaka, true),
            PackedTableChunks::Two
        );
        assert_eq!(
            indexed_jump_packed_chunks(3, 16, EvmVersion::Osaka, true),
            PackedTableChunks::None
        );
    }
}

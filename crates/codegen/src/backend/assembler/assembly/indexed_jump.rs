//! Indexed jump table planning and assembly lowering.
//!
//! An indexed jump takes its target from a table in one of two encodings:
//!
//! - A packed word of one-byte entries, picked with `BYTE`, either absolute or relative to the
//!   table's earliest target. It needs at most 32 entries.
//! - A stride table: equal-width entry stubs, `JUMPDEST PUSHn target JUMP`, laid out densely with
//!   no other `JUMPDEST` between them. The jump goes to `table + index * stride`.
//!
//! Entries wider than one byte are never packed into a word and picked out with a shift and a
//! mask, as in `PUSH16 table; SHR; PUSH2 0xffff AND; JUMP`. The JIT of evm2 (alloy-rs/evm2,
//! PR #791) tracks jump targets as intervals. It resolves the `BYTE` pick and the stride table,
//! whose target is a bounded interval, but not the shifted pick: `SHL 4` of `0..=7` becomes the
//! interval `0..=112`, and `SHR` and `AND` on a packed word have no interval fold. One unresolved
//! jump makes the JIT drop every jump resolution in the contract, including internal returns and
//! the stride tables. Measured on evm2, an 8-case assembly switch in a loop with a hashed index
//! tied across lowerings on the interpreter, while binary search ran 2.3x faster than the shifted
//! pick on the JIT. A 40-case stride table tied binary search on the JIT, ran 36-39% faster on the
//! interpreter and used 25-31% less gas.
//!
//! The switch planner prices the encoding chosen here, through `estimated_indexed_jump_code_size`
//! and `packs_indexed_jump`, so tables that cost too much as stride tables lose to binary or
//! linear search.

use super::{Program, lower};
use crate::backend::{
    assembler::{Assembler, Label},
    evm::{
        ir::{
            self, BlockId, ImmediateMaterialization, ImmediateMaterializationOp,
            immediate_materialization_len,
        },
        op::{self, WORD_BYTES, push_len},
    },
};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::index::{IndexVec, index_vec};

fn push_immediate(
    assembler: &mut Assembler<'_>,
    program: &mut Program,
    evm_version: EvmVersion,
    value: U256,
) {
    ImmediateMaterialization::new(evm_version, value).for_each(|op| match op {
        ImmediateMaterializationOp::Push(value) => program.push(assembler.push_inst(value)),
        ImmediateMaterializationOp::Opcode(opcode) => program.push_op(opcode),
    });
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct IndexedJumpEncoding {
    width: u8,
    /// Whether the table is one-byte entries packed into a word and picked with `BYTE`.
    packed: bool,
    base: Option<(BlockId, u8)>,
}

#[derive(Clone, Copy, Default)]
pub(super) struct IndexedJumpLowering {
    table: Option<IndexedJumpEncoding>,
    /// Width of the target push in outlined entry blocks; on the source block,
    /// this also determines the entry stride.
    pub(super) outlined_entry_width: Option<u8>,
}

/// Returns the conservative target width used before final EVM IR layout.
///
/// Runtime code is bounded below `PUSH2`'s address range, and Shanghai bounds
/// initcode below it as well. Earlier initcode has no protocol size bound, so
/// planning it with `PUSH3` avoids underestimating labels that may cross the
/// `PUSH2` boundary. Final lowering still chooses the narrowest width that fits
/// the resolved block offsets.
pub(in crate::backend) fn indexed_jump_target_width_bound(
    evm_version: EvmVersion,
    is_constructor: bool,
) -> usize {
    usize::from(is_constructor && evm_version < EvmVersion::Shanghai) + 2
}

#[derive(Clone, Copy)]
struct PackedTableEstimate {
    len: usize,
    base_width: u8,
}

pub(super) struct IndexedJumpTable {
    pub(super) source: BlockId,
    /// The blocks reached by the original indexed jump.
    pub(super) targets: Box<[BlockId]>,
    /// Outlined entry blocks, when the table is not packed.
    pub(super) entries: Box<[BlockId]>,
}

pub(super) fn materialize_tables_with_metadata(
    module: &mut ir::Module,
    evm_version: EvmVersion,
    allow_relative: bool,
) -> (IndexVec<BlockId, IndexedJumpLowering>, Vec<IndexedJumpTable>) {
    let tables = module
        .blocks
        .iter_enumerated()
        .filter_map(|(block, data)| {
            let targets = match &data.terminator.as_ref()?.kind {
                ir::TerminatorKind::IndexedJump(targets) => targets.clone(),
                _ => return None,
            };
            Some(IndexedJumpTable { source: block, targets, entries: Box::new([]) })
        })
        .collect::<Vec<_>>();
    if tables.is_empty() {
        return (index_vec![IndexedJumpLowering::default(); module.blocks.len()], Vec::new());
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
            choose_indexed_jump_encoding(
                &table.targets,
                &offsets,
                global_width,
                evm_version,
                allow_relative,
            )
        })
        .collect::<Vec<_>>();

    loop {
        let mut packed_estimates = index_vec![None; module.blocks.len()];
        for (table, encoding) in tables.iter().zip(&encodings) {
            if encoding.packed {
                packed_estimates[table.source] = Some(PackedTableEstimate {
                    len: table.targets.len(),
                    base_width: encoding.base.map_or(0, |(_, width)| width),
                });
            }
        }
        let offsets = estimated_block_offsets(module, evm_version, global_width, &packed_estimates);
        let mut changed = false;
        for (table, encoding) in tables.iter().zip(&mut encodings) {
            let next = update_indexed_jump_encoding(
                *encoding,
                &table.targets,
                &offsets,
                global_width,
                evm_version,
                allow_relative,
            );
            if next != *encoding {
                *encoding = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    for (table, encoding) in tables.iter_mut().zip(&encodings) {
        if !encoding.packed {
            let mut entries = Vec::with_capacity(table.targets.len());
            for &target in &table.targets {
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
    for (table, encoding) in tables.iter().zip(encodings) {
        lowerings[table.source].table = Some(encoding);
        if !encoding.packed {
            lowerings[table.source].outlined_entry_width = Some(encoding.width);
            for &entry in &table.entries {
                lowerings[entry].outlined_entry_width = Some(encoding.width);
            }
        }
    }
    (lowerings, tables)
}

/// Resets the selected table shapes to a one-byte entry width before exact
/// layout. The subsequent refinement only widens entries after resolving all
/// labels, so it reaches the least width that fits the final program.
pub(super) fn initialize_indexed_jump_widths(
    lowerings: &mut IndexVec<BlockId, IndexedJumpLowering>,
    tables: &[IndexedJumpTable],
) {
    for table in tables {
        let encoding = lowerings[table.source].table.expect("indexed jump table lowering");
        lowerings[table.source].table = Some(IndexedJumpEncoding {
            width: 1,
            packed: encoding.packed,
            base: encoding.base.map(|(base, _)| (base, 1)),
        });
        if !encoding.packed {
            lowerings[table.source].outlined_entry_width = Some(1);
            for &entry in &table.entries {
                lowerings[entry].outlined_entry_width = Some(1);
            }
        }
    }
}

/// Refines indexed-jump widths after the assembler has resolved every label.
///
/// The first lowering uses block-size estimates so it can materialize the
/// table shape. This pass then uses the actual label offsets, including
/// ordinary jumps and labels outside the table, and updates widths until the
/// label layout reaches a fixed point. A packed table falls back to outlined
/// entries if a resolved width no longer fits one byte.
pub(super) fn refine_indexed_jump_widths(
    module: &mut ir::Module,
    tables: &mut [IndexedJumpTable],
    lowerings: &mut IndexVec<BlockId, IndexedJumpLowering>,
    labels: &[Option<Label>],
    label_offsets: &solar_data_structures::map::FxHashMap<Label, usize>,
    evm_version: EvmVersion,
) -> bool {
    let global_width = label_offsets.values().copied().max().map_or(1, |offset| {
        (1u8..=32)
            .find(|&width| push_width_fits(offset.saturating_add(1), width))
            .expect("a bytecode offset must fit one EVM word")
    });
    let mut block_offsets = index_vec![0usize; module.blocks.len()];
    for (block, data) in module.blocks.iter_enumerated() {
        let Some(label) = labels.get(data.label as usize).copied().flatten() else {
            continue;
        };
        if let Some(&offset) = label_offsets.get(&label) {
            block_offsets[block] = offset;
        }
    }

    let mut changed = false;
    for table in tables.iter_mut() {
        let current = lowerings[table.source].table.expect("indexed jump table lowering");
        let next = if !current.packed {
            IndexedJumpEncoding {
                width: current.width.max(indexed_jump_target_width(
                    &table.entries,
                    &block_offsets,
                    global_width,
                )),
                packed: false,
                base: None,
            }
        } else {
            refine_packed_indexed_jump_encoding(
                current,
                &table.targets,
                &block_offsets,
                global_width,
                evm_version,
            )
        };

        if !next.packed && current.packed {
            let original_targets = table.targets.clone();
            let mut entries = Vec::with_capacity(original_targets.len());
            let mut next_label = module
                .blocks
                .iter()
                .map(|block| block.label)
                .max()
                .map_or(0, |label| label.checked_add(1).expect("EVM IR block label overflow"));
            for target in original_targets {
                let mut block = ir::Block::new(next_label);
                next_label = next_label.checked_add(1).expect("EVM IR block label overflow");
                block.terminator = Some(ir::Terminator::new(ir::TerminatorKind::Jump(target)));
                entries.push(module.add_block(block));
            }
            module.blocks[table.source]
                .terminator
                .as_mut()
                .expect("indexed jump source must have a terminator")
                .kind = ir::TerminatorKind::IndexedJump(entries.clone().into_boxed_slice());
            table.entries = entries.into_boxed_slice();
            lowerings.resize(module.blocks.len(), IndexedJumpLowering::default());
            changed = true;
        }

        if lowerings[table.source].table != Some(next) {
            lowerings[table.source].table = Some(next);
            changed = true;
        }
        if !next.packed {
            let entry_width =
                indexed_jump_target_width(&table.targets, &block_offsets, global_width);
            let source_width =
                lowerings[table.source].outlined_entry_width.unwrap_or(1).max(entry_width);
            if lowerings[table.source].outlined_entry_width != Some(source_width) {
                lowerings[table.source].outlined_entry_width = Some(source_width);
                changed = true;
            }
            for &entry in &table.entries {
                let width = lowerings[entry].outlined_entry_width.unwrap_or(1).max(entry_width);
                if lowerings[entry].outlined_entry_width != Some(width) {
                    lowerings[entry].outlined_entry_width = Some(width);
                    changed = true;
                }
            }
        }
    }
    changed
}

fn refine_packed_indexed_jump_encoding(
    current: IndexedJumpEncoding,
    targets: &[BlockId],
    offsets: &IndexVec<BlockId, usize>,
    global_width: u8,
    evm_version: EvmVersion,
) -> IndexedJumpEncoding {
    let absolute_width =
        current.width.max(indexed_jump_target_width(targets, offsets, global_width));
    let absolute = make_indexed_jump_encoding(targets.len(), absolute_width, None, evm_version);

    let Some((base, base_width)) = current.base else {
        return absolute;
    };
    let relative_width =
        current.width.max(indexed_jump_relative_width(targets, base, offsets, global_width));
    let relative_base_width =
        base_width.max(indexed_jump_target_width(&[base], offsets, global_width));
    let relative = make_indexed_jump_encoding(
        targets.len(),
        relative_width,
        Some((base, relative_base_width)),
        evm_version,
    );

    // Keep the representation selected by the planner while it remains
    // encodable. Falling back to absolute packing is safe if a relative base
    // no longer fits; changing representations in both directions would make
    // layout refinement oscillate.
    if relative.packed { relative } else { absolute }
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

fn indexed_jump_relative_width(
    targets: &[BlockId],
    base: BlockId,
    offsets: &IndexVec<BlockId, usize>,
    global_width: u8,
) -> u8 {
    let base_offset = offsets[base];
    let max_delta = targets
        .iter()
        .map(|&target| {
            offsets[target]
                .checked_sub(base_offset)
                .expect("packed label base must precede every target")
        })
        .max()
        .unwrap_or(0);
    (1..=global_width)
        .find(|&width| push_width_fits(max_delta.saturating_add(1), width))
        .unwrap_or(global_width)
}

fn choose_indexed_jump_encoding(
    targets: &[BlockId],
    offsets: &IndexVec<BlockId, usize>,
    global_width: u8,
    evm_version: EvmVersion,
    allow_relative: bool,
) -> IndexedJumpEncoding {
    let absolute_width = indexed_jump_target_width(targets, offsets, global_width);
    let absolute = make_indexed_jump_encoding(targets.len(), absolute_width, None, evm_version);
    if !allow_relative {
        return absolute;
    }

    let base = earliest_indexed_jump_base(targets, offsets);
    let width = indexed_jump_relative_width(targets, base, offsets, global_width);
    let base_width = indexed_jump_target_width(&[base], offsets, global_width);
    let relative =
        make_indexed_jump_encoding(targets.len(), width, Some((base, base_width)), evm_version);
    choose_shorter_relative_encoding(absolute, relative, targets.len(), evm_version)
}

fn earliest_indexed_jump_base(targets: &[BlockId], offsets: &IndexVec<BlockId, usize>) -> BlockId {
    targets
        .iter()
        .min_by_key(|&&target| offsets[target])
        .copied()
        .expect("indexed jump must have targets")
}

fn make_indexed_jump_encoding(
    table_len: usize,
    width: u8,
    base: Option<(BlockId, u8)>,
    evm_version: EvmVersion,
) -> IndexedJumpEncoding {
    IndexedJumpEncoding {
        width,
        packed: packs_indexed_jump(table_len, width.into(), evm_version),
        base,
    }
}

fn choose_shorter_relative_encoding(
    absolute: IndexedJumpEncoding,
    relative: IndexedJumpEncoding,
    table_len: usize,
    evm_version: EvmVersion,
) -> IndexedJumpEncoding {
    if relative.packed
        && indexed_jump_encoding_len(relative, table_len, evm_version)
            < indexed_jump_encoding_len(absolute, table_len, evm_version)
    {
        relative
    } else {
        absolute
    }
}

fn update_indexed_jump_encoding(
    encoding: IndexedJumpEncoding,
    targets: &[BlockId],
    offsets: &IndexVec<BlockId, usize>,
    global_width: u8,
    evm_version: EvmVersion,
    allow_relative: bool,
) -> IndexedJumpEncoding {
    let absolute_width = indexed_jump_target_width(targets, offsets, global_width);
    if let Some((base, previous_base_width)) = encoding.base {
        let width =
            indexed_jump_relative_width(targets, base, offsets, global_width).max(encoding.width);
        let base_width =
            indexed_jump_target_width(&[base], offsets, global_width).max(previous_base_width);
        let relative =
            make_indexed_jump_encoding(targets.len(), width, Some((base, base_width)), evm_version);
        let absolute = make_indexed_jump_encoding(targets.len(), absolute_width, None, evm_version);
        return choose_shorter_relative_encoding(absolute, relative, targets.len(), evm_version);
    }

    if absolute_width <= encoding.width {
        return encoding;
    }
    let absolute = if encoding.packed {
        IndexedJumpEncoding { packed: false, ..encoding }
    } else {
        make_indexed_jump_encoding(targets.len(), absolute_width, None, evm_version)
    };
    if !allow_relative {
        return absolute;
    }

    let base = earliest_indexed_jump_base(targets, offsets);
    let width = indexed_jump_relative_width(targets, base, offsets, global_width);
    let base_width = indexed_jump_target_width(&[base], offsets, global_width);
    let relative =
        make_indexed_jump_encoding(targets.len(), width, Some((base, base_width)), evm_version);
    choose_shorter_relative_encoding(absolute, relative, targets.len(), evm_version)
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

/// Estimates the encoded size of an indexed-jump dispatch with absolute targets.
pub(in crate::backend) fn estimated_indexed_jump_code_size(
    table_len: usize,
    target_width: usize,
    base_width: usize,
    evm_version: EvmVersion,
) -> usize {
    if packs_indexed_jump(table_len, target_width, evm_version) {
        packed_indexed_jump_len(PackedTableEstimate { len: table_len, base_width: 0 }, evm_version)
    } else {
        usize::from(u8::try_from(base_width).expect("indexed jump base width must fit u8"))
            + 6
            + table_len.saturating_mul(target_width + 3)
    }
}

/// Returns whether an indexed jump of `table_len` targets of `target_width` bytes picks its
/// target out of a packed word with `BYTE`.
///
/// Wider entries are never packed: the pick needs a shift and mask that bytecode analyzers cannot
/// bound, so they use the stride table instead.
pub(in crate::backend) fn packs_indexed_jump(
    table_len: usize,
    target_width: usize,
    evm_version: EvmVersion,
) -> bool {
    evm_version.has_bitwise_shifting() && (2..=WORD_BYTES).contains(&table_len) && target_width == 1
}

/// Returns the largest source-block terminator size for target widths through
/// `max_target_width`. Outlined entry stubs are appended after the existing
/// block order, and relative packing is only selected when smaller than the
/// corresponding absolute encoding.
pub(in crate::backend) fn estimated_indexed_jump_terminator_size(
    table_len: usize,
    max_target_width: u8,
    evm_version: EvmVersion,
    allow_relative: bool,
) -> usize {
    (1..=max_target_width)
        .map(|target_width| {
            if packs_indexed_jump(table_len, target_width.into(), evm_version) {
                packed_indexed_jump_len(
                    PackedTableEstimate {
                        len: table_len,
                        base_width: if allow_relative { max_target_width } else { 0 },
                    },
                    evm_version,
                )
            } else {
                usize::from(target_width) + 6
            }
        })
        .max()
        .unwrap_or(0)
}

fn indexed_jump_encoding_len(
    encoding: IndexedJumpEncoding,
    table_len: usize,
    evm_version: EvmVersion,
) -> usize {
    if !encoding.packed {
        return outlined_indexed_jump_len(table_len, encoding.width);
    }
    packed_indexed_jump_len(
        PackedTableEstimate {
            len: table_len,
            base_width: encoding.base.map_or(0, |(_, width)| width),
        },
        evm_version,
    )
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
        let inst_size = if let Some(type_size) = inst.immutable_type_size() {
            usize::from(type_size.bytes()) + 1
        } else if inst.is_encoded_push() {
            if inst.pushed_library().is_some() {
                21
            } else if let Some(value) = inst.pushed_value() {
                push_len(evm_version, value)
            } else if inst.pushed_block().is_some() {
                usize::from(block_target_width) + 1
            } else if inst.pushed_data().is_some() {
                4
            } else if let Some(size) = inst.pushed_data_size() {
                push_len(evm_version, size.bound())
            } else {
                unreachable!("push must carry a value")
            }
        } else if let Some(stack_op) = inst.as_stack_op() {
            stack_op
                .assembled_len(evm_version)
                .expect("indexed jump planning only runs on target-compatible stack operations")
        } else {
            1
        };
        size = size.saturating_add(inst_size);
    }
    if let Some(term) = &block.terminator {
        size = size.saturating_add(estimated_terminator_size(
            &term.kind,
            module.next_block(block_id),
            module.code_follows,
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
    code_follows: bool,
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
        ir::TerminatorKind::Op(op::STOP) => usize::from(next.is_some() || code_follows),
        ir::TerminatorKind::Op(_) => 1,
    }
}

fn packed_indexed_jump_len(table: PackedTableEstimate, evm_version: EvmVersion) -> usize {
    let byte_offset = WORD_BYTES - table.len;
    let table_len = 4
        + table.len
        + usize::from(byte_offset != 0)
            * (immediate_materialization_len(evm_version, U256::from(byte_offset)) + 1);
    table_len + usize::from(table.base_width) + usize::from(table.base_width != 0) * 2
}

pub(super) fn lower(
    assembler: &mut Assembler<'_>,
    program: &mut Program,
    targets: &[BlockId],
    module: &ir::Module,
    labels: &mut Vec<Option<Label>>,
    indexed_jump: IndexedJumpLowering,
) {
    let evm_version = assembler.gcx.sess.opts.evm_version;
    let table_encoding = indexed_jump.table.expect("indexed jump table encoding");
    if table_encoding.packed {
        debug_assert_eq!(table_encoding.width, 1);
        let base = table_encoding
            .base
            .map(|(base, _)| lower::label_for_block(assembler, module, base, labels));
        let mut labels = targets
            .iter()
            .map(|&target| lower::label_for_block(assembler, module, target, labels))
            .collect::<Vec<_>>();
        // Select the byte of the packed word, counted from its most significant end.
        let byte_offset = WORD_BYTES - labels.len();
        if byte_offset != 0 {
            push_immediate(assembler, program, evm_version, U256::from(byte_offset));
            program.push_op(op::ADD);
        }
        labels.reverse();
        program.push_packed_labels(labels.into_boxed_slice(), base, 1);
        program.push_op(op::SWAP1);
        program.push_op(op::BYTE);
        if let Some(base) = base {
            program.push_label(base);
            program.push_op(op::ADD);
        }
        program.push_op(op::JUMP);
        return;
    }

    let (&table, rest) = targets.split_first().expect("validated indexed jump table");
    debug_assert!(
        rest.iter()
            .enumerate()
            .all(|(index, target)| { target.index() == table.index() + index + 1 })
    );
    let entry_width = indexed_jump.outlined_entry_width.expect("outlined indexed jump entry width");
    let stub_len = u32::from(entry_width) + 3;
    push_immediate(assembler, program, evm_version, U256::from(stub_len));
    program.push_op(op::MUL);
    program.push_label(lower::label_for_block(assembler, module, table, labels));
    program.push_op(op::ADD);
    program.push_op(op::JUMP);
}

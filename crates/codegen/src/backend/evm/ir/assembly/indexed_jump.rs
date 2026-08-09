//! Indexed jump table planning and assembly lowering.

use super::{AsmInst, Program, lower};
use crate::backend::evm::{
    assembler::{Assembler, Label},
    ir::{self, BlockId},
    op, push_len,
};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::index::{IndexVec, index_vec};

#[derive(Clone, Copy, PartialEq, Eq)]
struct IndexedJumpEncoding {
    width: u8,
    packed_chunks: PackedTableChunks,
    base: Option<(BlockId, u8)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PackedTableChunks {
    #[default]
    None,
    One,
    Two,
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
pub(in crate::backend::evm) fn indexed_jump_target_width_bound(
    evm_version: EvmVersion,
    is_constructor: bool,
) -> usize {
    usize::from(is_constructor && evm_version < EvmVersion::Shanghai) + 2
}

#[derive(Clone, Copy)]
struct PackedTableEstimate {
    len: usize,
    width: u8,
    chunks: PackedTableChunks,
    base_width: u8,
}

pub(super) struct IndexedJumpTable {
    pub(super) source: BlockId,
    /// The blocks reached by the original indexed jump.
    pub(super) targets: Box<[BlockId]>,
    /// Outlined entry blocks, when the table is not packed.
    pub(super) entries: Box<[BlockId]>,
}

#[cfg(test)]
pub(super) fn materialize_tables(
    module: &mut ir::Module,
    evm_version: EvmVersion,
    pack_two_word_tables: bool,
) -> IndexVec<BlockId, IndexedJumpLowering> {
    materialize_tables_with_metadata(module, evm_version, pack_two_word_tables).0
}

pub(super) fn materialize_tables_with_metadata(
    module: &mut ir::Module,
    evm_version: EvmVersion,
    pack_two_word_tables: bool,
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
                pack_two_word_tables,
            )
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
                pack_two_word_tables,
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
        if encoding.packed_chunks == PackedTableChunks::None {
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
        if encoding.packed_chunks == PackedTableChunks::None {
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
    evm_version: EvmVersion,
    pack_two_word_tables: bool,
) {
    for table in tables {
        let encoding = lowerings[table.source].table.expect("indexed jump table lowering");
        let chunks = if encoding.packed_chunks == PackedTableChunks::None {
            PackedTableChunks::None
        } else {
            indexed_jump_packed_chunks(
                table.targets.len(),
                1,
                evm_version,
                pack_two_word_tables,
                outlined_indexed_jump_len(table.targets.len(), 1),
            )
        };
        lowerings[table.source].table = Some(IndexedJumpEncoding {
            width: 1,
            packed_chunks: chunks,
            base: encoding.base.map(|(base, _)| (base, 1)),
        });
        if chunks == PackedTableChunks::None {
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
/// label layout reaches a fixed point. A packed table may fall back to outlined
/// entries if its resolved widths no longer fit in one or two words.
pub(super) fn refine_indexed_jump_widths(
    module: &mut ir::Module,
    tables: &mut [IndexedJumpTable],
    lowerings: &mut IndexVec<BlockId, IndexedJumpLowering>,
    labels: &[Option<Label>],
    label_offsets: &solar_data_structures::map::FxHashMap<Label, usize>,
    evm_version: EvmVersion,
    pack_two_word_tables: bool,
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
        let next = if current.packed_chunks == PackedTableChunks::None {
            IndexedJumpEncoding {
                width: current.width.max(indexed_jump_target_width(
                    &table.entries,
                    &block_offsets,
                    global_width,
                )),
                packed_chunks: PackedTableChunks::None,
                base: None,
            }
        } else {
            refine_packed_indexed_jump_encoding(
                current,
                &table.targets,
                &block_offsets,
                global_width,
                evm_version,
                pack_two_word_tables,
            )
        };

        if next.packed_chunks == PackedTableChunks::None
            && current.packed_chunks != PackedTableChunks::None
        {
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
        if next.packed_chunks == PackedTableChunks::None {
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
    pack_two_word_tables: bool,
) -> IndexedJumpEncoding {
    let absolute_width =
        current.width.max(indexed_jump_target_width(targets, offsets, global_width));
    let outlined_len = outlined_indexed_jump_len(targets.len(), absolute_width);
    let absolute = make_indexed_jump_encoding(
        targets.len(),
        absolute_width,
        None,
        evm_version,
        pack_two_word_tables,
        outlined_len,
    );

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
        pack_two_word_tables,
        outlined_len,
    );

    // Keep the representation selected by the planner while it remains
    // encodable. Falling back to absolute packing is safe if a relative base
    // no longer fits; changing representations in both directions would make
    // layout refinement oscillate.
    if relative.packed_chunks != PackedTableChunks::None { relative } else { absolute }
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
    let outlined_len = outlined_indexed_jump_len(targets.len(), absolute_width);
    let absolute = make_indexed_jump_encoding(
        targets.len(),
        absolute_width,
        None,
        evm_version,
        allow_relative,
        outlined_len,
    );
    if !allow_relative {
        return absolute;
    }

    let base = earliest_indexed_jump_base(targets, offsets);
    let width = indexed_jump_relative_width(targets, base, offsets, global_width);
    let base_width = indexed_jump_target_width(&[base], offsets, global_width);
    let relative = make_indexed_jump_encoding(
        targets.len(),
        width,
        Some((base, base_width)),
        evm_version,
        true,
        outlined_len,
    );
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
    pack_two_word_tables: bool,
    outlined_len: usize,
) -> IndexedJumpEncoding {
    IndexedJumpEncoding {
        width,
        packed_chunks: indexed_jump_packed_chunks(
            table_len,
            width,
            evm_version,
            pack_two_word_tables,
            outlined_len,
        ),
        base,
    }
}

fn choose_shorter_relative_encoding(
    absolute: IndexedJumpEncoding,
    relative: IndexedJumpEncoding,
    table_len: usize,
    evm_version: EvmVersion,
) -> IndexedJumpEncoding {
    if relative.packed_chunks != PackedTableChunks::None
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
    pack_two_word_tables: bool,
) -> IndexedJumpEncoding {
    let absolute_width = indexed_jump_target_width(targets, offsets, global_width);
    if let Some((base, previous_base_width)) = encoding.base {
        let width =
            indexed_jump_relative_width(targets, base, offsets, global_width).max(encoding.width);
        let base_width =
            indexed_jump_target_width(&[base], offsets, global_width).max(previous_base_width);
        let outlined_len = outlined_indexed_jump_len(targets.len(), absolute_width);
        let relative = make_indexed_jump_encoding(
            targets.len(),
            width,
            Some((base, base_width)),
            evm_version,
            pack_two_word_tables,
            outlined_len,
        );
        let absolute = make_indexed_jump_encoding(
            targets.len(),
            absolute_width,
            None,
            evm_version,
            pack_two_word_tables,
            outlined_len,
        );
        return choose_shorter_relative_encoding(absolute, relative, targets.len(), evm_version);
    }

    if absolute_width <= encoding.width {
        return encoding;
    }
    let outlined_len = outlined_indexed_jump_len(targets.len(), absolute_width);
    let absolute_chunks = indexed_jump_packed_chunks(
        targets.len(),
        absolute_width,
        evm_version,
        pack_two_word_tables,
        outlined_len,
    );
    let absolute = if encoding.packed_chunks != PackedTableChunks::None
        && absolute_chunks == PackedTableChunks::None
    {
        IndexedJumpEncoding { packed_chunks: absolute_chunks, ..encoding }
    } else {
        IndexedJumpEncoding { width: absolute_width, packed_chunks: absolute_chunks, base: None }
    };
    if !pack_two_word_tables {
        return absolute;
    }

    let base = earliest_indexed_jump_base(targets, offsets);
    let width = indexed_jump_relative_width(targets, base, offsets, global_width);
    let base_width = indexed_jump_target_width(&[base], offsets, global_width);
    let relative = make_indexed_jump_encoding(
        targets.len(),
        width,
        Some((base, base_width)),
        evm_version,
        true,
        outlined_len,
    );
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
pub(in crate::backend::evm) fn estimated_indexed_jump_code_size(
    table_len: usize,
    target_width: usize,
    base_width: usize,
    evm_version: EvmVersion,
    pack_two_word_tables: bool,
) -> usize {
    let target_width = u8::try_from(target_width).expect("indexed jump target width must fit u8");
    let base_width = u8::try_from(base_width).expect("indexed jump base width must fit u8");
    let outlined_len =
        usize::from(base_width) + 6 + table_len.saturating_mul(usize::from(target_width) + 3);
    let packed_chunks = indexed_jump_packed_chunks(
        table_len,
        target_width,
        evm_version,
        pack_two_word_tables,
        outlined_len,
    );
    match packed_chunks {
        PackedTableChunks::None => outlined_len,
        chunks => packed_indexed_jump_len(
            PackedTableEstimate { len: table_len, width: target_width, chunks, base_width: 0 },
            evm_version,
        ),
    }
}

fn indexed_jump_packed_chunks(
    table_len: usize,
    target_width: u8,
    evm_version: EvmVersion,
    pack_two_word_tables: bool,
    outlined_len: usize,
) -> PackedTableChunks {
    if !supports_indexed_jump_packing(table_len, evm_version) {
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
            base_width: 0,
        };
        if pack_two_word_tables
            && target_width.is_power_of_two()
            && entries_per_chunk >= 2
            && bytes <= 64
            && packed_indexed_jump_len(table, evm_version) < outlined_len
        {
            PackedTableChunks::Two
        } else {
            PackedTableChunks::None
        }
    }
}

pub(in crate::backend::evm) fn packs_indexed_jump(
    table_len: usize,
    target_width: usize,
    evm_version: EvmVersion,
) -> bool {
    supports_indexed_jump_packing(table_len, evm_version)
        && table_len.saturating_mul(target_width) <= 32
}

fn supports_indexed_jump_packing(table_len: usize, evm_version: EvmVersion) -> bool {
    evm_version.has_bitwise_shifting() && table_len >= 2
}

/// Returns the largest source-block terminator size for target widths through
/// `max_target_width`. Outlined entry stubs are appended after the existing
/// block order, and relative packing is only selected when smaller than the
/// corresponding absolute encoding.
pub(in crate::backend::evm) fn estimated_indexed_jump_terminator_size(
    table_len: usize,
    max_target_width: u8,
    evm_version: EvmVersion,
    pack_two_word_tables: bool,
) -> usize {
    (1..=max_target_width)
        .map(|target_width| {
            let outlined_len = outlined_indexed_jump_len(table_len, target_width);
            let packed_chunks = indexed_jump_packed_chunks(
                table_len,
                target_width,
                evm_version,
                pack_two_word_tables,
                outlined_len,
            );
            if packed_chunks == PackedTableChunks::None {
                usize::from(target_width) + 6
            } else {
                packed_indexed_jump_len(
                    PackedTableEstimate {
                        len: table_len,
                        width: target_width,
                        chunks: packed_chunks,
                        base_width: if pack_two_word_tables { max_target_width } else { 0 },
                    },
                    evm_version,
                )
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
    if encoding.packed_chunks == PackedTableChunks::None {
        return outlined_indexed_jump_len(table_len, encoding.width);
    }
    packed_indexed_jump_len(
        PackedTableEstimate {
            len: table_len,
            width: encoding.width,
            chunks: encoding.packed_chunks,
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
        let inst_size = if inst.deferred_push().is_some() {
            33
        } else if let Some(type_size) = inst.immutable_type_size() {
            usize::from(type_size.bytes()) + 1
        } else if inst.is_encoded_push() {
            if let Some(value) = inst.pushed_value() {
                push_len(evm_version, value)
            } else if inst.pushed_block().is_some() {
                usize::from(block_target_width) + 1
            } else if inst.pushed_data().is_some() {
                4
            } else {
                unreachable!("push must carry a value")
            }
        } else {
            1
        };
        size = size.saturating_add(inst_size);
    }
    if let Some(term) = &block.terminator {
        size = size.saturating_add(estimated_terminator_size(
            &term.kind,
            lower::next_block(module, block_id),
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
    let table_len = if table.chunks == PackedTableChunks::One {
        9 + table.len * usize::from(table.width) + usize::from(table.width)
    } else {
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
            + push_len(evm_version, U256::from(chunk_shift))
            + second_chunk_push
            + first_chunk_push
            + push_len(evm_version, U256::from(entry_mask))
            + push_len(evm_version, U256::from(scale_shift))
            + push_len(evm_version, target_mask)
    };
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
    let table_encoding = indexed_jump.table.expect("indexed jump table encoding");
    if table_encoding.packed_chunks != PackedTableChunks::None {
        let target_width = table_encoding.width;
        let scale = u32::from(target_width) * 8;
        let base = table_encoding
            .base
            .map(|(base, _)| lower::label_for_block(assembler, module, base, labels));
        let labels = targets
            .iter()
            .map(|&target| lower::label_for_block(assembler, module, target, labels))
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
            program.push_packed_labels(second.into(), base, target_width);
            program.push_op(op::MUL);
            program.push_op(op::SWAP1);
            program.push_op(op::ISZERO);
            program.push_packed_labels(first.into(), base, target_width);
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
                AsmInst::push_inline(scale.ilog2()).expect("indexed jump scale must fit inline"),
            );
            program.push_op(op::SHL);
        } else {
            program.push(AsmInst::push_inline(scale).expect("indexed jump scale must fit inline"));
            program.push_op(op::MUL);
        }
        if table_encoding.packed_chunks == PackedTableChunks::One {
            program.push_packed_labels(labels.into_boxed_slice(), base, target_width);
            program.push_op(op::SWAP1);
        }
        program.push_op(op::SHR);
        let mask = (U256::ONE << scale) - U256::ONE;
        program.push(assembler.push_inst(mask));
        program.push_op(op::AND);
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
    program.push(AsmInst::push_inline(stub_len).expect("indexed jump stub length must fit inline"));
    program.push_op(op::MUL);
    program.push_label(lower::label_for_block(assembler, module, table, labels));
    program.push_op(op::ADD);
    program.push_op(op::JUMP);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backend::evm::{
            assembler::AsmInstKind,
            ir::{Block, Instruction, Terminator, TerminatorKind},
        },
        mir::{ImmutableId, TypeSize},
    };
    use alloy_primitives::U256;
    use solar_config::CompileOpts;
    use solar_interface::{Session, sym};
    use solar_sema::Compiler;

    #[test]
    fn bounds_target_width_by_artifact_kind() {
        assert_eq!(indexed_jump_target_width_bound(EvmVersion::Byzantium, false), 2);
        assert_eq!(indexed_jump_target_width_bound(EvmVersion::Byzantium, true), 3);
        assert_eq!(indexed_jump_target_width_bound(EvmVersion::Shanghai, true), 2);
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
                super::super::lower::lower_evm_ir(&mut assembler, &mut module, &mut labels);

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

        let opts = CompileOpts { evm_version: EvmVersion::Byzantium, ..Default::default() };
        let compiler = Compiler::new(Session::builder().opts(opts).build());
        compiler.enter(|c| {
            let mut labels = vec![None; 3];
            let mut assembler = Assembler::new(c.gcx());
            let program =
                super::super::lower::lower_evm_ir(&mut assembler, &mut module, &mut labels);

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
            module.blocks[entry].instructions.push(Instruction::push_immutable(
                ImmutableId::new(id),
                TypeSize::new_int_bits(256),
            ));
        }
        module.blocks[entry].terminator =
            Some(Terminator::new(TerminatorKind::IndexedJump(vec![target].into_boxed_slice())));
        module.blocks[target].terminator = Some(Terminator::new(TerminatorKind::Op(op::STOP)));

        let lowerings = materialize_tables(&mut module, EvmVersion::Osaka, false);
        let TerminatorKind::IndexedJump(entries) =
            &module.blocks[entry].terminator.as_ref().unwrap().kind
        else {
            panic!("expected indexed jump")
        };
        assert_eq!(lowerings[entries[0]].outlined_entry_width, Some(2));
    }

    #[test]
    fn exact_width_uses_offsets_of_all_table_targets() {
        let mut module = ir::Module::new(sym::module);
        let entry = module.add_block(Block::new(0));
        let target = module.add_block(Block::new(1));
        for id in 0..8 {
            module.blocks[entry].instructions.push(Instruction::push_immutable(
                ImmutableId::new(id),
                TypeSize::new_int_bits(256),
            ));
        }
        module.blocks[entry].terminator =
            Some(Terminator::new(TerminatorKind::IndexedJump(vec![target].into_boxed_slice())));
        module.blocks[target].terminator = Some(Terminator::new(TerminatorKind::Op(op::STOP)));

        let compiler = Compiler::new(Session::builder().opts(Default::default()).build());
        compiler.enter(|c| {
            let mut labels = vec![None; 2];
            let mut assembler = Assembler::new(c.gcx());
            let program =
                super::super::lower::lower_evm_ir(&mut assembler, &mut module, &mut labels);

            assert_eq!(
                program
                    .instructions
                    .iter()
                    .filter_map(|inst| match inst.kind() {
                        AsmInstKind::PushLabelFixed(_, width) => Some(width),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                vec![2]
            );
        });
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
            module.blocks[padding].instructions.push(Instruction::push_immutable(
                ImmutableId::new(id),
                TypeSize::new_int_bits(256),
            ));
        }
        module.blocks[padding].terminator = Some(Terminator::new(TerminatorKind::Op(op::STOP)));

        let lowerings = materialize_tables(&mut module, EvmVersion::Osaka, false);
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
    fn packs_relative_targets_after_large_blocks() {
        let mut module = ir::Module::new(sym::module);
        let entry = module.add_block(Block::new(0));
        let padding = module.add_block(Block::new(1));
        for id in 0..8 {
            module.blocks[padding].instructions.push(Instruction::push_immutable(
                ImmutableId::new(id),
                TypeSize::new_int_bits(256),
            ));
        }
        module.blocks[padding].terminator = Some(Terminator::new(TerminatorKind::Op(op::STOP)));
        let targets = (2..=11)
            .map(|label| {
                let target = module.add_block(Block::new(label));
                module.blocks[target].terminator =
                    Some(Terminator::new(TerminatorKind::Op(op::STOP)));
                target
            })
            .collect::<Vec<_>>();
        module.blocks[entry].terminator =
            Some(Terminator::new(TerminatorKind::IndexedJump(targets.clone().into_boxed_slice())));

        let lowerings = materialize_tables(&mut module, EvmVersion::Osaka, true);
        let encoding = lowerings[entry].table.unwrap();
        assert_eq!(encoding.width, 1);
        assert_eq!(encoding.packed_chunks, PackedTableChunks::One);
        assert_eq!(encoding.base, Some((targets[0], 2)));
    }

    #[test]
    fn switches_to_relative_packing_after_table_growth() {
        let mut module = ir::Module::new(sym::module);
        let first = module.add_block(Block::new(0));
        let second = module.add_block(Block::new(1));
        let padding = module.add_block(Block::new(2));
        for _ in 0..108 {
            module.blocks[padding].instructions.push(Instruction::push_value(U256::ONE));
        }
        module.blocks[padding].terminator = Some(Terminator::new(TerminatorKind::Op(op::STOP)));
        let targets = (3..=12)
            .map(|label| {
                let target = module.add_block(Block::new(label));
                module.blocks[target].terminator =
                    Some(Terminator::new(TerminatorKind::Op(op::STOP)));
                target
            })
            .collect::<Vec<_>>();
        for source in [first, second] {
            module.blocks[source].terminator = Some(Terminator::new(TerminatorKind::IndexedJump(
                targets.clone().into_boxed_slice(),
            )));
        }

        let lowerings = materialize_tables(&mut module, EvmVersion::Osaka, true);
        for source in [first, second] {
            let encoding = lowerings[source].table.unwrap();
            assert_eq!(encoding.width, 1);
            assert_eq!(encoding.packed_chunks, PackedTableChunks::One);
            assert_eq!(encoding.base, Some((targets[0], 2)));
        }
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
            module.blocks[padding].instructions.push(Instruction::push_immutable(
                ImmutableId::new(id),
                TypeSize::new_int_bits(256),
            ));
        }
        module.blocks[padding].terminator = Some(Terminator::new(TerminatorKind::Op(op::STOP)));

        let lowerings = materialize_tables(&mut module, EvmVersion::Osaka, true);
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
            module.blocks[padding].instructions.push(Instruction::push_immutable(
                ImmutableId::new(id),
                TypeSize::new_int_bits(256),
            ));
        }
        module.blocks[padding].terminator = Some(Terminator::new(TerminatorKind::Op(op::STOP)));

        let lowerings = materialize_tables(&mut module, EvmVersion::Osaka, false);
        let TerminatorKind::IndexedJump(entries) =
            &module.blocks[entry].terminator.as_ref().unwrap().kind
        else {
            panic!("expected indexed jump")
        };
        assert_ne!(entries.as_ref(), targets);
        assert_eq!(lowerings[entry].table.unwrap().width, 1);
        assert_eq!(lowerings[entry].table.unwrap().packed_chunks, PackedTableChunks::None);
        assert!(entries.iter().all(|&entry| lowerings[entry].outlined_entry_width == Some(1)));
    }

    #[test]
    fn two_word_packing_requires_a_size_win() {
        assert_eq!(
            indexed_jump_packed_chunks(
                7,
                8,
                EvmVersion::Osaka,
                true,
                outlined_indexed_jump_len(7, 8),
            ),
            PackedTableChunks::Two
        );
        assert_eq!(
            indexed_jump_packed_chunks(
                3,
                16,
                EvmVersion::Osaka,
                true,
                outlined_indexed_jump_len(3, 16),
            ),
            PackedTableChunks::None
        );
    }

    #[test]
    fn indexed_jump_terminator_estimate_includes_packed_tables() {
        assert_eq!(estimated_indexed_jump_terminator_size(2, 2, EvmVersion::Osaka, false), 15);
        assert!(estimated_indexed_jump_terminator_size(32, 2, EvmVersion::Osaka, true) > 32);
        assert_eq!(estimated_indexed_jump_terminator_size(33, 2, EvmVersion::Osaka, true), 61);
        assert_eq!(estimated_indexed_jump_terminator_size(65, 2, EvmVersion::Osaka, true), 8);
        assert_eq!(estimated_indexed_jump_terminator_size(10, 3, EvmVersion::Osaka, false), 42);
        assert_eq!(estimated_indexed_jump_terminator_size(10, 3, EvmVersion::Byzantium, true), 9);
    }
}

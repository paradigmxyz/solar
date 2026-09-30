//! Final EVM instruction locations used by source-level debug formats.
//!
//! Like DWARF line tables, [`DebugInfo`] records a row only where the location
//! state changes. Each row covers a run of consecutive instructions that share
//! one interned [`DebugLocation`]. Consumers recover instruction offsets and
//! opcodes by decoding the final bytecode inside each run, so program data
//! between instructions always starts a new row.

use super::op;
use smallvec::SmallVec;
use solar_data_structures::{
    index::{IndexSlice, IndexVec},
    map::FxIndexSet,
    newtype_index,
};
use solar_interface::{Span, Symbol};
use std::iter::FusedIterator;

pub use crate::source_info::MAX_DEBUG_SPANS;

/// Source origins associated with one machine instruction.
pub type DebugSpans = SmallVec<[Span; 2]>;

/// Source-language identity of a function activation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DebugFunction {
    /// Function identifier in the source language.
    pub identifier: Symbol,
    /// Source range of the complete declaration.
    pub declaration: Span,
}

/// Function activation transition associated with an instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DebugFunctionExit {
    /// Successful return from the active function.
    Return,
    /// Revert from the active function.
    Revert,
}

/// Source location state shared by a run of instructions.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DebugLocation {
    /// Source spans associated with the instruction.
    ///
    /// More than one span means an optimization shared this instruction
    /// between multiple source-level origins.
    pub source_spans: DebugSpans,
    /// Function entered after this instruction executes.
    pub function_invoke: Option<DebugFunction>,
    /// Function activation closed after this instruction executes.
    pub function_exit: Option<DebugFunctionExit>,
    /// Legacy source-map modifier nesting depth for this instruction.
    pub modifier_depth: u32,
}

impl DebugLocation {
    fn matches(
        &self,
        source_spans: &[Span],
        function_invoke: Option<DebugFunction>,
        function_exit: Option<DebugFunctionExit>,
        modifier_depth: u32,
    ) -> bool {
        self.source_spans.as_slice() == source_spans
            && self.function_invoke == function_invoke
            && self.function_exit == function_exit
            && self.modifier_depth == modifier_depth
    }
}

newtype_index! {
    /// An index into a [`DebugInfo`] location table.
    struct DebugLocationId;
}

/// A run of consecutive instructions with the same location.
#[derive(Clone, Copy, Debug)]
struct DebugRow {
    /// Byte offset of the first instruction.
    start: u32,
    /// Number of instructions in the run.
    len: u32,
    location: DebugLocationId,
}

/// Final instruction locations of one bytecode artifact.
#[derive(Clone, Debug, Default)]
pub struct DebugInfo {
    locations: IndexVec<DebugLocationId, DebugLocation>,
    rows: Vec<DebugRow>,
    /// Number of recorded instructions.
    len: usize,
}

impl DebugInfo {
    /// Returns the recorded instructions, decoded from the bytecode they describe.
    pub fn instructions<'a>(&'a self, bytecode: &'a [u8]) -> DebugInstructions<'a> {
        DebugInstructions {
            locations: &self.locations,
            bytecode,
            rows: self.rows.iter(),
            location: DebugLocationId::from_usize(0),
            offset: 0,
            remaining_in_row: 0,
            remaining: self.len,
        }
    }
}

/// One instruction in finalized bytecode with its originating source location.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DebugInstruction<'a> {
    /// Byte offset of the opcode in the artifact.
    pub offset: u32,
    /// Raw EVM opcode byte.
    pub opcode: u8,
    /// Source location state of the instruction.
    pub location: &'a DebugLocation,
}

/// Iterator over the instructions of a [`DebugInfo`].
#[derive(Clone, Debug)]
pub struct DebugInstructions<'a> {
    locations: &'a IndexSlice<DebugLocationId, [DebugLocation]>,
    bytecode: &'a [u8],
    rows: std::slice::Iter<'a, DebugRow>,
    location: DebugLocationId,
    offset: u32,
    remaining_in_row: u32,
    remaining: usize,
}

impl<'a> Iterator for DebugInstructions<'a> {
    type Item = DebugInstruction<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_in_row == 0 {
            let row = self.rows.next()?;
            self.offset = row.start;
            self.remaining_in_row = row.len;
            self.location = row.location;
        }
        let offset = self.offset;
        let opcode = self.bytecode[offset as usize];
        self.offset += encoded_len(opcode);
        self.remaining_in_row -= 1;
        self.remaining -= 1;
        Some(DebugInstruction { offset, opcode, location: &self.locations[self.location] })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl ExactSizeIterator for DebugInstructions<'_> {}

impl FusedIterator for DebugInstructions<'_> {}

/// Records [`DebugInfo`] rows while bytecode is emitted.
#[derive(Debug, Default)]
pub(crate) struct DebugInfoBuilder {
    locations: FxIndexSet<DebugLocation>,
    rows: Vec<DebugRow>,
    len: usize,
    /// Offset at which the current row's next instruction decodes.
    next_offset: u32,
}

impl DebugInfoBuilder {
    /// Records one instruction whose opcode byte is at `offset`.
    pub(crate) fn record(
        &mut self,
        offset: usize,
        opcode: u8,
        source_spans: &[Span],
        function_invoke: Option<DebugFunction>,
        function_exit: Option<DebugFunctionExit>,
        modifier_depth: u32,
    ) {
        let offset = u32::try_from(offset).expect("EVM bytecode offset exceeds u32");
        self.len += 1;
        // Extend the current run only when decoding it reaches this instruction.
        if let Some(row) = self.rows.last_mut()
            && self.next_offset == offset
            && self.locations[row.location.index()].matches(
                source_spans,
                function_invoke,
                function_exit,
                modifier_depth,
            )
        {
            row.len += 1;
        } else {
            let (location, _) = self.locations.insert_full(DebugLocation {
                source_spans: source_spans.iter().copied().collect(),
                function_invoke,
                function_exit,
                modifier_depth,
            });
            self.rows.push(DebugRow {
                start: offset,
                len: 1,
                location: DebugLocationId::from_usize(location),
            });
        }
        self.next_offset = offset + encoded_len(opcode);
    }

    pub(crate) fn finish(mut self) -> DebugInfo {
        let mut locations = self.locations.into_iter().collect::<Vec<_>>();
        locations.shrink_to_fit();
        self.rows.shrink_to_fit();
        DebugInfo { locations: IndexVec::from_vec(locations), rows: self.rows, len: self.len }
    }
}

/// Returns the byte length that run decoding assigns to an instruction.
///
/// An instruction whose real length differs ends its run, so this never has
/// to classify every opcode for the selected EVM version.
const fn encoded_len(opcode: u8) -> u32 {
    match opcode {
        op::PUSH1..=op::PUSH32 => 1 + (opcode - op::PUSH0) as u32,
        op::DUPN | op::SWAPN | op::EXCHANGE => 2,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_interface::BytePos;

    #[test]
    fn runs_split_at_location_changes_and_gaps() {
        let a = Span::new(BytePos(0), BytePos(1));
        let b = Span::new(BytePos(1), BytePos(2));
        let bytecode = [
            op::PUSH1,
            0x08,
            op::JUMP,
            0xaa,
            0xbb,
            op::PUSH0,
            op::ADD,
            op::DUPN,
            op::STOP,
            op::STOP,
        ];
        // `DUPN` without an immediate, as encoded before EIP-8024.
        let instructions = [(0, a), (2, a), (5, a), (6, b), (7, a), (8, a), (9, a)];

        let mut builder = DebugInfoBuilder::default();
        for (offset, span) in instructions {
            builder.record(offset, bytecode[offset], &[span], None, None, 0);
        }
        let info = builder.finish();

        assert_eq!(info.rows.len(), 5);
        assert_eq!(info.locations.len(), 2);
        let decoded = info
            .instructions(&bytecode)
            .map(|instruction| (instruction.offset as usize, instruction.location.source_spans[0]))
            .collect::<Vec<_>>();
        assert_eq!(decoded, instructions);
        assert!(
            info.instructions(&bytecode)
                .all(|instruction| instruction.opcode == bytecode[instruction.offset as usize])
        );
    }
}

//! Final EVM instruction locations used by source-level debug formats.
//!
//! Like DWARF line tables, [`DebugInfo`] records a row only where the location
//! state changes. Each row covers a run of consecutive instructions that share
//! one interned location. Consumers recover instruction offsets and opcodes by
//! decoding the final bytecode inside each run, so program data between
//! instructions always starts a new row.
//!
//! Like a DWARF line program, the rows form a byte stream of LEB128 numbers.
//! Each row stores the signed distance from the previous run's decoded end to
//! its first instruction, which is zero unless program data or a mismatched
//! instruction length ends the previous run, then the run length, then the
//! signed change of location index from the previous row. Signed numbers use
//! zigzag encoding.
//!
//! Each interned location takes 16 bytes. A single source span is stored
//! inline; span sets of any other size are ranges of a shared span table, and
//! function identities are indices into a function table.

use super::op;
use smallvec::SmallVec;
use solar_data_structures::{
    index::IndexVec,
    map::{FxHashMap, FxIndexSet},
    newtype_index,
};
use solar_interface::{BytePos, Span, Symbol};
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DebugLocation<'a> {
    /// Source spans associated with the instruction.
    ///
    /// More than one span means an optimization shared this instruction
    /// between multiple source-level origins.
    pub source_spans: &'a [Span],
    /// Function entered after this instruction executes.
    pub function_invoke: Option<DebugFunction>,
    /// Function activation closed after this instruction executes.
    pub function_exit: Option<DebugFunctionExit>,
    /// Legacy source-map modifier nesting depth for this instruction.
    pub modifier_depth: u32,
}

newtype_index! {
    /// An index into a [`DebugInfo`] location table.
    struct DebugLocationId;

    /// An index into a [`DebugInfo`] function table.
    struct DebugFunctionId;
}

/// Interned form of a [`DebugLocation`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Location {
    /// The only source span, or the bounds of a [`DebugInfo::spans`] range
    /// when [`Flags::table_spans`] is set.
    span: Span,
    function_invoke: Option<DebugFunctionId>,
    flags: Flags,
}

const _: () = assert!(size_of::<Location>() == 16);

/// Function exit, span storage, and modifier depth of a [`Location`].
///
/// Bits 0 and 1 hold the exit, bit 2 marks table spans, and the remaining
/// bits hold the modifier depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Flags(u32);

impl Flags {
    const EXIT_MASK: u32 = 0b11;
    const TABLE_SPANS: u32 = 1 << 2;
    const DEPTH_SHIFT: u32 = 3;

    fn new(
        function_exit: Option<DebugFunctionExit>,
        table_spans: bool,
        modifier_depth: u32,
    ) -> Self {
        assert!(modifier_depth <= u32::MAX >> Self::DEPTH_SHIFT, "modifier depth exceeds 29 bits");
        let exit = match function_exit {
            None => 0,
            Some(DebugFunctionExit::Return) => 1,
            Some(DebugFunctionExit::Revert) => 2,
        };
        let table_spans = if table_spans { Self::TABLE_SPANS } else { 0 };
        Self(modifier_depth << Self::DEPTH_SHIFT | table_spans | exit)
    }

    fn function_exit(self) -> Option<DebugFunctionExit> {
        match self.0 & Self::EXIT_MASK {
            0 => None,
            1 => Some(DebugFunctionExit::Return),
            _ => Some(DebugFunctionExit::Revert),
        }
    }

    fn table_spans(self) -> bool {
        self.0 & Self::TABLE_SPANS != 0
    }

    fn modifier_depth(self) -> u32 {
        self.0 >> Self::DEPTH_SHIFT
    }
}

/// Final instruction locations of one bytecode artifact.
#[derive(Clone, Debug, Default)]
pub struct DebugInfo {
    locations: IndexVec<DebugLocationId, Location>,
    /// Source span sets that do not have exactly one span.
    spans: Vec<Span>,
    functions: IndexVec<DebugFunctionId, DebugFunction>,
    /// Encoded rows, as described in the module documentation.
    rows: Vec<u8>,
    /// Number of recorded instructions.
    len: usize,
}

impl DebugInfo {
    /// Returns the recorded instructions, decoded from the bytecode they describe.
    pub fn instructions<'a>(&'a self, bytecode: &'a [u8]) -> DebugInstructions<'a> {
        DebugInstructions {
            info: self,
            bytecode,
            rows: &self.rows,
            location_id: DebugLocationId::from_usize(0),
            location: DebugLocation::default(),
            offset: 0,
            remaining_in_row: 0,
            remaining: self.len,
        }
    }

    fn location(&self, id: DebugLocationId) -> DebugLocation<'_> {
        let location = &self.locations[id];
        let source_spans = if location.flags.table_spans() {
            &self.spans[location.span.to_range()]
        } else {
            std::slice::from_ref(&location.span)
        };
        DebugLocation {
            source_spans,
            function_invoke: location.function_invoke.map(|id| self.functions[id]),
            function_exit: location.flags.function_exit(),
            modifier_depth: location.flags.modifier_depth(),
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
    pub location: DebugLocation<'a>,
}

/// Iterator over the instructions of a [`DebugInfo`].
#[derive(Clone, Debug)]
pub struct DebugInstructions<'a> {
    info: &'a DebugInfo,
    bytecode: &'a [u8],
    /// Encoded rows after the current one.
    rows: &'a [u8],
    location_id: DebugLocationId,
    location: DebugLocation<'a>,
    offset: u32,
    remaining_in_row: u32,
    remaining: usize,
}

impl<'a> Iterator for DebugInstructions<'a> {
    type Item = DebugInstruction<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        if self.remaining_in_row == 0 {
            let row = Row::decode(&mut self.rows);
            self.offset = offset_by(self.offset, row.gap);
            self.remaining_in_row = row.len;
            let location = offset_by(self.location_id.index() as u32, row.location);
            self.location_id = DebugLocationId::from_usize(location as usize);
            self.location = self.info.location(self.location_id);
        }
        let offset = self.offset;
        let opcode = self.bytecode[offset as usize];
        self.offset += encoded_len(opcode);
        self.remaining_in_row -= 1;
        self.remaining -= 1;
        Some(DebugInstruction { offset, opcode, location: self.location })
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
    info: DebugInfo,
    locations: FxIndexSet<Location>,
    functions: FxIndexSet<DebugFunction>,
    /// Ranges of [`DebugInfo::spans`] holding each span set added to it.
    span_sets: FxHashMap<DebugSpans, Span>,
    /// The current row, encoded once it ends.
    row: Option<PendingRow>,
    /// Location index of the last encoded row.
    encoded_location: usize,
    /// Offset at which the current row's next instruction decodes.
    next_offset: u32,
}

/// A row that may still grow.
#[derive(Debug)]
struct PendingRow {
    gap: i64,
    len: u32,
    location: DebugLocationId,
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
        let location = self.location(source_spans, function_invoke, function_exit, modifier_depth);
        self.info.len += 1;
        // Extend the current run only when decoding it reaches this instruction.
        if let Some(row) = &mut self.row
            && self.next_offset == offset
            && self.locations[row.location.index()] == location
        {
            row.len += 1;
        } else {
            let (location, _) = self.locations.insert_full(location);
            let row = PendingRow {
                gap: i64::from(offset) - i64::from(self.next_offset),
                len: 1,
                location: DebugLocationId::from_usize(location),
            };
            if let Some(row) = self.row.replace(row) {
                self.encode(row);
            }
        }
        self.next_offset = offset + encoded_len(opcode);
    }

    pub(crate) fn finish(mut self) -> DebugInfo {
        if let Some(row) = self.row.take() {
            self.encode(row);
        }
        let mut info = self.info;
        info.locations = self.locations.into_iter().collect();
        info.functions = self.functions.into_iter().collect();
        info.spans.shrink_to_fit();
        info.rows.shrink_to_fit();
        info
    }

    /// Returns the interned form of a location, adding its span set and function on first use.
    fn location(
        &mut self,
        source_spans: &[Span],
        function_invoke: Option<DebugFunction>,
        function_exit: Option<DebugFunctionExit>,
        modifier_depth: u32,
    ) -> Location {
        let (span, table_spans) = match source_spans {
            &[span] => (span, false),
            spans => {
                let range = match self.span_sets.get(spans) {
                    Some(&range) => range,
                    None => {
                        let start = BytePos::from_usize(self.info.spans.len());
                        self.info.spans.extend_from_slice(spans);
                        let end = BytePos::from_usize(self.info.spans.len());
                        let range = Span::new_unchecked(start, end);
                        self.span_sets.insert(spans.into(), range);
                        range
                    }
                };
                (range, true)
            }
        };
        let function_invoke = function_invoke
            .map(|function| DebugFunctionId::from_usize(self.functions.insert_full(function).0));
        let flags = Flags::new(function_exit, table_spans, modifier_depth);
        Location { span, function_invoke, flags }
    }

    fn encode(&mut self, row: PendingRow) {
        let location = row.location.index() as i64 - self.encoded_location as i64;
        Row { gap: row.gap, len: row.len, location }.encode(&mut self.info.rows);
        self.encoded_location = row.location.index();
    }
}

/// One encoded row, relative to the previous row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Row {
    /// Distance from the previous run's decoded end to the first instruction.
    gap: i64,
    /// Number of instructions in the run.
    len: u32,
    /// Change of location index from the previous row.
    location: i64,
}

impl Row {
    fn encode(self, out: &mut Vec<u8>) {
        write_leb128(out, zigzag(self.gap));
        write_leb128(out, self.len.into());
        write_leb128(out, zigzag(self.location));
    }

    fn decode(bytes: &mut &[u8]) -> Self {
        Self {
            gap: unzigzag(read_leb128(bytes)),
            len: u32::try_from(read_leb128(bytes)).expect("debug info run length exceeds u32"),
            location: unzigzag(read_leb128(bytes)),
        }
    }
}

fn write_leb128(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push(value as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn read_leb128(bytes: &mut &[u8]) -> u64 {
    let mut value = 0;
    let mut shift = 0;
    loop {
        let (&byte, rest) = bytes.split_first().expect("truncated debug info row");
        *bytes = rest;
        value |= u64::from(byte & 0x7f) << shift;
        if byte < 0x80 {
            return value;
        }
        shift += 7;
    }
}

/// Maps signed values of small magnitude to small unsigned values.
fn zigzag(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

fn unzigzag(value: u64) -> i64 {
    (value >> 1) as i64 ^ -((value & 1) as i64)
}

/// Adds a decoded signed delta to an unsigned position.
fn offset_by(base: u32, delta: i64) -> u32 {
    u32::try_from(i64::from(base) + delta).expect("invalid debug info row")
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

//! Linear EVM assembly program used before final bytecode emission.

use super::{AsmInst, AsmInstKind, Label, op};
use solar_data_structures::map::FxHashSet;

/// Linear EVM assembly program used by the assembler.
///
/// This sits below structured EVM IR: it is a label-bearing opcode stream with
/// unresolved PUSH operands and layout metadata, ready for final assembly into
/// bytecode.
#[derive(Clone, Debug, Default)]
pub(in crate::backend::evm) struct EvmAsmProgram {
    pub(in crate::backend::evm) instructions: Vec<AsmInst>,
    cold_labels: FxHashSet<Label>,
}

impl EvmAsmProgram {
    /// Clears all instructions and block metadata.
    pub(in crate::backend::evm) fn clear(&mut self) {
        self.instructions.clear();
        self.cold_labels.clear();
    }

    /// Emits an assembler instruction.
    pub(in crate::backend::evm) fn push(&mut self, inst: AsmInst) {
        self.instructions.push(inst);
    }

    /// Marks the block beginning at `label` as cold.
    pub(in crate::backend::evm) fn mark_cold(&mut self, label: Label) {
        self.cold_labels.insert(label);
    }

    /// Moves non-fallthrough cold terminal blocks to the end of the program.
    ///
    /// The pass only moves blocks that start with a cold label, end with a
    /// terminal opcode, and are preceded by a terminal opcode. This excludes
    /// physical fallthrough edges, which are stack-sensitive in MIR-to-EVM
    /// lowering.
    pub(in crate::backend::evm) fn move_cold_terminal_blocks_to_end(&mut self) -> usize {
        let mut ranges = Vec::new();
        let mut start = 0;

        while start < self.instructions.len() {
            let end = self.next_block_start(start + 1).unwrap_or(self.instructions.len());
            if self.is_movable_cold_terminal_block(start, end) {
                ranges.push(start..end);
            }
            start = end;
        }

        if ranges.is_empty() {
            return 0;
        }

        let mut moved = Vec::new();
        let mut optimized = Vec::with_capacity(self.instructions.len());
        let mut range_index = 0;
        let mut index = 0;

        while index < self.instructions.len() {
            if range_index < ranges.len() && index == ranges[range_index].start {
                moved.extend_from_slice(&self.instructions[ranges[range_index].clone()]);
                index = ranges[range_index].end;
                range_index += 1;
            } else {
                optimized.push(self.instructions[index]);
                index += 1;
            }
        }

        let moved_count = ranges.len();
        optimized.extend(moved);
        self.instructions = optimized;
        moved_count
    }

    fn next_block_start(&self, start: usize) -> Option<usize> {
        self.instructions[start..]
            .iter()
            .position(|inst| matches!(inst.kind(), AsmInstKind::Label(_)))
            .map(|offset| start + offset)
    }

    fn is_movable_cold_terminal_block(&self, start: usize, end: usize) -> bool {
        if start == 0 || start >= end {
            return false;
        }
        let AsmInstKind::Label(label) = self.instructions[start].kind() else {
            return false;
        };
        if !self.cold_labels.contains(&label) {
            return false;
        }
        if !matches!(self.instructions[start - 1].kind(), AsmInstKind::Op(op) if op::is_terminal(op))
        {
            return false;
        }
        matches!(self.instructions[end - 1].kind(), AsmInstKind::Op(op) if op::is_terminal(op))
    }
}

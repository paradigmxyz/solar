//! Structured EVM backend IR and final linear assembly program.

use super::{AsmInst, AsmInstKind, Label, op};
use solar_data_structures::map::FxHashSet;

/// Structured EVM backend IR used while MIR lowering emits EVM code.
///
/// This is intentionally still instruction-close to the final assembly layer:
/// operands such as unresolved labels, deferred constants, and immutable
/// placeholders are preserved as assembler operands. The value of this layer is
/// block structure and metadata, which backend layout and peephole passes can
/// query before final linearization.
#[derive(Clone, Debug, Default)]
pub(in crate::backend::evm) struct EvmIrProgram {
    blocks: Vec<EvmIrAsmBlock>,
    current: Option<usize>,
    cold_labels: FxHashSet<Label>,
}

impl EvmIrProgram {
    /// Clears all blocks and metadata.
    pub(in crate::backend::evm) fn clear(&mut self) {
        self.blocks.clear();
        self.current = None;
        self.cold_labels.clear();
    }

    /// Emits an instruction into the current EVM IR block.
    pub(in crate::backend::evm) fn push(&mut self, inst: AsmInst) {
        let block = self.current_block_mut();
        block.instructions.push(inst);
    }

    /// Defines a label, starting a new structured EVM IR block.
    pub(in crate::backend::evm) fn define_label(&mut self, label: Label) {
        let block = EvmIrAsmBlock {
            label: Some(label),
            cold: self.cold_labels.contains(&label),
            instructions: Vec::new(),
        };
        self.blocks.push(block);
        self.current = Some(self.blocks.len() - 1);
    }

    /// Marks the block beginning at `label` as cold.
    pub(in crate::backend::evm) fn mark_cold(&mut self, label: Label) {
        self.cold_labels.insert(label);
        if let Some(block) = self.blocks.iter_mut().find(|block| block.label == Some(label)) {
            block.cold = true;
        }
    }

    /// Returns a linear assembly view for tests.
    #[cfg(test)]
    pub(in crate::backend::evm) fn instructions(&self) -> Vec<AsmInst> {
        self.to_asm_program().instructions
    }

    /// Lowers structured EVM IR blocks to the final linear assembly program.
    pub(in crate::backend::evm) fn to_asm_program(&self) -> EvmAsmProgram {
        let mut program = EvmAsmProgram::default();
        for block in &self.blocks {
            if let Some(label) = block.label {
                program.instructions.push(AsmInst::label(label));
                if block.cold {
                    program.cold_labels.insert(label);
                }
            }
            program.instructions.extend_from_slice(&block.instructions);
        }
        program
    }

    fn current_block_mut(&mut self) -> &mut EvmIrAsmBlock {
        let index = match self.current {
            Some(index) => index,
            None => {
                self.blocks.push(EvmIrAsmBlock::default());
                let index = self.blocks.len() - 1;
                self.current = Some(index);
                index
            }
        };
        &mut self.blocks[index]
    }
}

#[derive(Clone, Debug, Default)]
struct EvmIrAsmBlock {
    label: Option<Label>,
    cold: bool,
    instructions: Vec<AsmInst>,
}

/// Linear EVM assembly program used by the final assembler.
///
/// This is the MC-like layer below structured EVM IR: a label-bearing opcode
/// stream with unresolved PUSH operands and layout metadata, ready for final
/// assembly into bytecode.
#[derive(Clone, Debug, Default)]
pub(in crate::backend::evm) struct EvmAsmProgram {
    pub(in crate::backend::evm) instructions: Vec<AsmInst>,
    cold_labels: FxHashSet<Label>,
}

impl EvmAsmProgram {
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

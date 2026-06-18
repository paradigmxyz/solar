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
            }
            program.instructions.extend_from_slice(&block.instructions);
        }
        program
    }

    /// Runs block-aware EVM IR optimizations.
    pub(in crate::backend::evm) fn optimize_blocks<S>(&mut self, estimated_inst_size: S) -> usize
    where
        S: FnMut(AsmInst) -> usize,
    {
        self.deduplicate_terminal_blocks(estimated_inst_size)
            + self.move_cold_terminal_blocks_to_end()
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

    fn move_cold_terminal_blocks_to_end(&mut self) -> usize {
        let mut moved = Vec::new();
        let mut kept = Vec::with_capacity(self.blocks.len());
        let mut moved_count = 0;

        for index in 0..self.blocks.len() {
            if self.is_movable_cold_terminal_block(index) {
                moved.push(self.blocks[index].clone());
                moved_count += 1;
            } else {
                kept.push(self.blocks[index].clone());
            }
        }

        if moved_count == 0 {
            return 0;
        }

        kept.extend(moved);
        self.blocks = kept;
        moved_count
    }

    fn is_movable_cold_terminal_block(&self, index: usize) -> bool {
        if index == 0 {
            return false;
        }
        let block = &self.blocks[index];
        if block.label.is_none() || !block.cold || !block.ends_with_terminal() {
            return false;
        }
        self.blocks[index - 1].ends_with_terminal()
    }

    fn deduplicate_terminal_blocks<S>(&mut self, mut estimated_inst_size: S) -> usize
    where
        S: FnMut(AsmInst) -> usize,
    {
        let mut canonical = Vec::<(Vec<AsmInst>, Label)>::new();
        let mut changed = 0;

        for block in &mut self.blocks {
            let Some(label) = block.label else {
                continue;
            };
            if !block.ends_with_terminal() {
                continue;
            }

            let key = block.instructions.clone();
            if let Some((_, target)) = canonical.iter().find(|(known, _)| *known == key) {
                let current_size = 1 + block
                    .instructions
                    .iter()
                    .map(|&inst| estimated_inst_size(inst))
                    .sum::<usize>();
                let replacement_size = 1 + 3 + 1; // JUMPDEST + PUSH2(label) + JUMP.
                if current_size > replacement_size {
                    block.instructions = vec![AsmInst::push_label(*target), AsmInst::op(op::JUMP)];
                    changed += 1;
                }
            } else {
                canonical.push((key, label));
            }
        }

        changed
    }
}

#[derive(Clone, Debug, Default)]
struct EvmIrAsmBlock {
    label: Option<Label>,
    cold: bool,
    instructions: Vec<AsmInst>,
}

impl EvmIrAsmBlock {
    fn ends_with_terminal(&self) -> bool {
        matches!(self.instructions.last().map(|inst| inst.kind()), Some(AsmInstKind::Op(op)) if op::is_terminal(op))
    }
}

/// Linear EVM assembly program used by the final assembler.
///
/// This is the MC-like layer below structured EVM IR: a label-bearing opcode
/// stream with unresolved PUSH operands, ready for final assembly into bytecode.
#[derive(Clone, Debug, Default)]
pub(in crate::backend::evm) struct EvmAsmProgram {
    pub(in crate::backend::evm) instructions: Vec<AsmInst>,
}

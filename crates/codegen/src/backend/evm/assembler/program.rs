//! Structured assembler block program and final linear assembly program.

use super::{AsmInst, AsmInstKind, Assembler, DeferredConst, Label, op};
use crate::backend::evm::ir;
use alloy_primitives::U256;
use solar_data_structures::bit_set::GrowableBitSet;
use solar_interface::{diagnostics::DiagCtxt, sym};

/// Structured assembler block program used while MIR lowering emits EVM code.
///
/// This is intentionally still instruction-close to the final assembly layer:
/// operands such as unresolved labels, deferred constants, and immutable
/// placeholders are preserved as assembler operands. The parseable EVM backend
/// IR lives in [`crate::backend::evm::ir`]; this private layer is the bridge
/// between that target-specific IR direction and final linear assembly.
#[derive(Clone, Debug, Default)]
pub(in crate::backend::evm) struct StructuredAsmProgram {
    blocks: Vec<StructuredAsmBlock>,
    current: Option<usize>,
    cold_labels: GrowableBitSet<Label>,
}

impl StructuredAsmProgram {
    /// Clears all blocks and metadata.
    pub(in crate::backend::evm) fn clear(&mut self) {
        self.blocks.clear();
        self.current = None;
        self.cold_labels.clear();
    }

    /// Emits an instruction into the current structured assembler block.
    pub(in crate::backend::evm) fn push(&mut self, inst: AsmInst) {
        let block = self.current_block_mut();
        block.instructions.push(inst);
    }

    /// Defines a label, starting a new structured assembler block.
    pub(in crate::backend::evm) fn define_label(&mut self, label: Label) {
        let block = StructuredAsmBlock {
            label: Some(label),
            hotness: if self.cold_labels.contains(label) {
                ir::Hotness::Cold
            } else {
                ir::Hotness::Hot
            },
            instructions: Vec::new(),
        };
        self.blocks.push(block);
        self.current = Some(self.blocks.len() - 1);
    }

    /// Marks the block beginning at `label` as cold.
    pub(in crate::backend::evm) fn mark_cold(&mut self, label: Label) {
        self.cold_labels.insert(label);
        if let Some(block) = self.blocks.iter_mut().find(|block| block.label == Some(label)) {
            block.hotness = ir::Hotness::Cold;
        }
    }

    /// Returns a linear assembly view for tests.
    #[cfg(test)]
    pub(in crate::backend::evm) fn instructions(&self) -> Vec<AsmInst> {
        self.to_asm_program().instructions
    }

    /// Lowers structured assembler blocks to the final linear assembly program.
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

    /// Runs machine-level EVM IR passes over the structured assembler blocks.
    ///
    /// MIR lowering currently emits stack-scheduled assembler instructions.
    /// This bridge makes the production backend pass through the same untyped
    /// block IR used by `solar evm-opt` while preserving unresolved assembler
    /// operands such as labels, deferred constants, and immutable placeholders.
    ///
    /// # Experimental `StackSchedule` gating
    ///
    /// When `assembler.config.evm_ir_stack_schedule` is true (off by default) the
    /// bridge runs [`ir::STACK_SCHEDULE_PASS`]. The reality of what the bridge
    /// feeds the scheduler matters here: MIR lowering has already materialized
    /// every virtual stack-word operand into physical `dup`/`swap`/`pop` and
    /// `push`/opcode instructions, so `to_evm_ir_module` produces
    /// *operand-cleared* IR — no instruction carries an [`ir::Operand::Value`],
    /// and the only terminators emitted here (`jump`/raw terminal
    /// opcode/`stop`/`invalid`) carry no value operands either. `StackSchedule`
    /// only rewrites instructions that have value operands to materialize, so on
    /// this input it has nothing to materialize: every instruction is replayed
    /// onto its model stack and pushed back unchanged, and blocks it cannot model
    /// are restored verbatim. It is therefore a *near no-op* whose only
    /// observable effect is recording inferred `(in ...)` entry signatures,
    /// which `from_evm_ir_module` ignores.
    ///
    /// To make turning the flag on provably safe, the scheduled module is checked
    /// against the verifier oracle and the bytecode-bearing instruction stream is
    /// required to be unchanged; if either check fails the scheduled module is
    /// discarded and the original (pre-schedule) module is used, so enabling the
    /// flag can never produce invalid or divergent bytecode.
    pub(in crate::backend::evm) fn lower_through_evm_ir(
        &mut self,
        assembler: &mut Assembler,
    ) -> usize {
        if self.blocks.is_empty() {
            return 0;
        }
        let (mut module, mut labels) = self
            .to_evm_ir_module(assembler)
            .expect("non-empty structured assembly must lower to EVM IR");

        // Low-level assembler users may provide fragments whose stack inputs
        // come from outside the program. Preserve the verifier invariant only
        // for complete modules that satisfy it before this bridge.
        let input_is_valid = cfg!(debug_assertions) && is_valid_evm_ir(&module);
        let mut changed = 0;
        let pass_options = ir::PassOptions { time_passes: assembler.config.time_passes };

        if assembler.config.evm_ir_stack_schedule {
            // Differential safety net: schedule a clone, and only adopt it if the
            // verifier accepts it and its bytecode-bearing instruction stream is
            // identical to the input. On the bridge's operand-cleared IR this
            // always holds (the pass is a near no-op), but the guard means an
            // unexpected rewrite is dropped instead of changing produced code.
            let mut scheduled = module.clone();
            if ir::run_pass(&mut scheduled, &ir::STACK_SCHEDULE_PASS, pass_options)
                && is_valid_evm_ir(&scheduled)
                && modules_have_equal_code(&module, &scheduled)
            {
                module = scheduled;
                changed += 1;
            }
        }

        for pass in ir::DEFAULT_LAYOUT_PIPELINE {
            changed += usize::from(ir::run_pass(&mut module, pass, pass_options));
        }
        debug_assert!(!input_is_valid || is_valid_evm_ir(&module));

        *self = Self::from_evm_ir_module(&module, &mut labels, assembler);
        changed
    }

    /// Resolves deferred constants while preserving structured block boundaries.
    pub(in crate::backend::evm) fn resolve_deferred_consts<F>(&mut self, mut resolve: F)
    where
        F: FnMut(DeferredConst) -> AsmInst,
    {
        for block in &mut self.blocks {
            for inst in &mut block.instructions {
                if let AsmInstKind::PushDeferred(id) = inst.kind() {
                    *inst = resolve(id);
                }
            }
        }
    }

    fn current_block_mut(&mut self) -> &mut StructuredAsmBlock {
        let index = match self.current {
            Some(index) => index,
            None => {
                self.blocks.push(StructuredAsmBlock::default());
                let index = self.blocks.len() - 1;
                self.current = Some(index);
                index
            }
        };
        &mut self.blocks[index]
    }

    fn to_evm_ir_module(
        &self,
        assembler: &mut Assembler,
    ) -> Option<(ir::Module, Vec<Option<Label>>)> {
        if self.blocks.is_empty() {
            return None;
        }

        let labels: Vec<_> = self.blocks.iter().map(|block| block.label).collect();
        let mut label_to_block = std::collections::BTreeMap::new();
        for (index, block) in self.blocks.iter().enumerate() {
            if let Some(label) = block.label {
                label_to_block.insert(label, crate::backend::evm::ir::BlockId::from_usize(index));
            }
        }

        let mut module = ir::Module::new(sym::asm);
        for (index, block) in self.blocks.iter().enumerate() {
            let mut ir_block = ir::Block::new(index as u32);
            ir_block.metadata.hotness = block.hotness;
            module.add_block(ir_block);
        }

        for (index, block) in self.blocks.iter().enumerate() {
            let block_id = crate::backend::evm::ir::BlockId::from_usize(index);
            let next_block = (index + 1 < self.blocks.len())
                .then(|| crate::backend::evm::ir::BlockId::from_usize(index + 1));
            let (instructions, terminator) =
                Self::translate_block_to_evm_ir(block, next_block, &label_to_block, assembler);
            module.blocks[block_id].instructions = instructions;
            module.blocks[block_id].terminator = Some(ir::Terminator::new(terminator));
        }

        Some((module, labels))
    }

    fn translate_block_to_evm_ir(
        block: &StructuredAsmBlock,
        next_block: Option<crate::backend::evm::ir::BlockId>,
        label_to_block: &std::collections::BTreeMap<Label, crate::backend::evm::ir::BlockId>,
        assembler: &mut Assembler,
    ) -> (Vec<ir::Instruction>, ir::TerminatorKind) {
        let mut body_len = block.instructions.len();
        let terminator =
            if let Some((target, len)) = Self::trailing_static_jump(block, label_to_block) {
                body_len = len;
                ir::TerminatorKind::Jump(target)
            } else if let Some(AsmInstKind::Op(opcode)) =
                block.instructions.last().map(|inst| inst.kind())
                && op::is_terminal(opcode)
            {
                body_len = body_len.saturating_sub(1);
                ir::TerminatorKind::RawOpcode(opcode)
            } else {
                next_block.map_or(ir::TerminatorKind::Stop, ir::TerminatorKind::Jump)
            };

        let mut instructions = Vec::with_capacity(body_len);
        for &inst in &block.instructions[..body_len] {
            instructions.push(Self::inst_to_evm_ir(inst, label_to_block, assembler));
        }
        (instructions, terminator)
    }

    fn trailing_static_jump(
        block: &StructuredAsmBlock,
        label_to_block: &std::collections::BTreeMap<Label, crate::backend::evm::ir::BlockId>,
    ) -> Option<(crate::backend::evm::ir::BlockId, usize)> {
        let [rest @ .., push, jump] = block.instructions.as_slice() else {
            return None;
        };
        if jump.kind() != AsmInstKind::Op(op::JUMP) {
            return None;
        }
        let AsmInstKind::PushLabel(label) = push.kind() else {
            return None;
        };
        Some((*label_to_block.get(&label)?, rest.len()))
    }

    fn inst_to_evm_ir(
        inst: AsmInst,
        label_to_block: &std::collections::BTreeMap<Label, crate::backend::evm::ir::BlockId>,
        assembler: &mut Assembler,
    ) -> ir::Instruction {
        match inst.kind() {
            AsmInstKind::Op(opcode) => ir::Instruction::opcode(opcode),
            AsmInstKind::PushInline(value) => {
                push_instruction(ir::Operand::Immediate(U256::from(value)))
            }
            AsmInstKind::Push(index) => {
                push_instruction(ir::Operand::Immediate(assembler.push_value(index)))
            }
            AsmInstKind::PushLabel(label) => {
                let block =
                    *label_to_block.get(&label).expect("assembler label reference must be defined");
                push_instruction(ir::Operand::Block(block))
            }
            AsmInstKind::PushDeferred(id) => {
                ir::Instruction::push_deferred(ir::Operand::Immediate(U256::from(id.index())))
            }
            AsmInstKind::PushImmutable(id) => {
                ir::Instruction::push_immutable(ir::Operand::Immediate(U256::from(id)))
            }
            AsmInstKind::Label(_) => panic!("labels must begin structured assembler blocks"),
        }
    }

    fn from_evm_ir_module(
        module: &ir::Module,
        labels: &mut Vec<Option<Label>>,
        assembler: &mut Assembler,
    ) -> Self {
        let mut program = Self::default();
        for (block_id, block) in module.blocks.iter_enumerated() {
            let original = block.label as usize;
            let label = labels.get(original).copied().flatten();
            if let Some(label) = label {
                program.define_label(label);
                if block.metadata.hotness.is_cold() {
                    program.mark_cold(label);
                }
            } else {
                program.blocks.push(StructuredAsmBlock {
                    label: None,
                    hotness: block.metadata.hotness,
                    instructions: Vec::new(),
                });
                program.current = Some(program.blocks.len() - 1);
            }

            for inst in &block.instructions {
                if let Some(asm_inst) = Self::evm_ir_inst_to_asm(inst, module, labels, assembler) {
                    program.push(asm_inst);
                }
            }

            if let Some(term) = &block.terminator {
                Self::emit_evm_ir_terminator(
                    &mut program,
                    block_id,
                    &term.kind,
                    module,
                    labels,
                    assembler,
                );
            }
        }
        program
    }

    fn evm_ir_inst_to_asm(
        inst: &ir::Instruction,
        module: &ir::Module,
        labels: &mut Vec<Option<Label>>,
        assembler: &mut Assembler,
    ) -> Option<AsmInst> {
        if inst.is_deferred_push() {
            let [ir::Operand::Immediate(value)] = inst.operands.as_slice() else {
                return None;
            };
            Some(AsmInst::push_deferred(DeferredConst::from_usize(usize::try_from(*value).ok()?)))
        } else if inst.is_immutable_push() {
            let [ir::Operand::Immediate(value)] = inst.operands.as_slice() else {
                return None;
            };
            Some(AsmInst::push_immutable(u32::try_from(*value).ok()?))
        } else if inst.is_encoded_push() {
            match inst.operands.as_slice() {
                [ir::Operand::Immediate(value)] => Some(assembler.push_inst(*value)),
                [ir::Operand::Block(block)] => {
                    Some(AsmInst::push_label(label_for_block(module, *block, labels, assembler)))
                }
                _ => None,
            }
        } else {
            Some(AsmInst::op(inst.opcode))
        }
    }

    fn emit_evm_ir_terminator(
        program: &mut Self,
        block_id: crate::backend::evm::ir::BlockId,
        kind: &ir::TerminatorKind,
        module: &ir::Module,
        labels: &mut Vec<Option<Label>>,
        assembler: &mut Assembler,
    ) {
        match kind {
            ir::TerminatorKind::Jump(target) => {
                if next_block(module, block_id) == Some(*target) {
                    return;
                }
                let label = label_for_block(module, *target, labels, assembler);
                program.push(AsmInst::push_label(label));
                program.push(AsmInst::op(op::JUMP));
            }
            ir::TerminatorKind::RawOpcode(opcode) => program.push(AsmInst::op(*opcode)),
            // Falling off the end of bytecode is an implicit stop, so the
            // bridge's synthetic final `Stop` does not need an opcode.
            ir::TerminatorKind::Stop => {}
            ir::TerminatorKind::Invalid => program.push(AsmInst::op(op::INVALID)),
            ir::TerminatorKind::Return { .. }
            | ir::TerminatorKind::Revert { .. }
            | ir::TerminatorKind::SelfDestruct { .. }
            | ir::TerminatorKind::Branch { .. }
            | ir::TerminatorKind::Switch { .. } => {
                unreachable!("structured assembler bridge only emits machine-level terminators")
            }
        }
    }
}

/// Whether two modules produce the same bytecode-bearing block stream.
///
/// `StackSchedule` records inferred `(in ...)` entry signatures on the blocks it
/// schedules, but `from_evm_ir_module` never reads `entry_stack`, so it does not
/// affect produced bytecode. This compares the parts the bridge actually lowers
/// back to assembly — block labels, hot/cold metadata, instruction streams, and
/// terminators — while ignoring the entry-signature bookkeeping.
fn modules_have_equal_code(before: &ir::Module, after: &ir::Module) -> bool {
    before.entry_block == after.entry_block
        && before.blocks.len() == after.blocks.len()
        && before.blocks.iter().zip(after.blocks.iter()).all(|(a, b)| {
            a.label == b.label
                && a.metadata == b.metadata
                && a.instructions == b.instructions
                && a.terminator == b.terminator
        })
}

fn is_valid_evm_ir(module: &ir::Module) -> bool {
    let dcx = DiagCtxt::with_silent_emitter(None);
    ir::Verifier::new(&dcx).verify_module(module);
    dcx.has_errors().is_ok()
}

fn push_instruction(operand: ir::Operand) -> ir::Instruction {
    ir::Instruction::push(operand)
}

fn next_block(
    module: &ir::Module,
    block: crate::backend::evm::ir::BlockId,
) -> Option<crate::backend::evm::ir::BlockId> {
    let next = block.index() + 1;
    (next < module.blocks.len()).then(|| crate::backend::evm::ir::BlockId::from_usize(next))
}

fn label_for_block(
    module: &ir::Module,
    block: crate::backend::evm::ir::BlockId,
    labels: &mut Vec<Option<Label>>,
    assembler: &mut Assembler,
) -> Label {
    let original = module.blocks[block].label as usize;
    if original >= labels.len() {
        labels.resize_with(original + 1, || None);
    }
    *labels[original].get_or_insert_with(|| assembler.new_label())
}

#[derive(Clone, Debug, Default)]
struct StructuredAsmBlock {
    label: Option<Label>,
    hotness: ir::Hotness,
    instructions: Vec<AsmInst>,
}

/// Linear EVM assembly program used by the final assembler.
///
/// This is the MC-like layer below structured assembler blocks: a label-bearing opcode
/// stream with unresolved PUSH operands, ready for final assembly into bytecode.
#[derive(Clone, Debug, Default)]
pub(in crate::backend::evm) struct EvmAsmProgram {
    pub(in crate::backend::evm) instructions: Vec<AsmInst>,
}

impl EvmAsmProgram {
    /// Converts the final linear program back to EVM IR without running passes.
    pub(in crate::backend::evm) fn to_evm_ir_module(
        &self,
        assembler: &mut Assembler,
    ) -> Option<ir::Module> {
        let mut structured = StructuredAsmProgram::default();
        for &inst in &self.instructions {
            if let AsmInstKind::Label(label) = inst.kind() {
                structured.define_label(label);
            } else {
                structured.push(inst);
            }
        }
        structured.to_evm_ir_module(assembler).map(|(module, _)| module)
    }
}

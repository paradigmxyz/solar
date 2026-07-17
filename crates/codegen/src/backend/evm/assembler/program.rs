//! Structured assembler block program and final linear assembly program.

use super::{AsmInst, AsmInstKind, DeferredConst, Label, op};
use crate::backend::evm::ir;
use alloy_primitives::U256;
use solar_data_structures::bit_set::GrowableBitSet;
use solar_interface::diagnostics::DiagCtxt;

const OP_PREFIX: &str = "op_";
const PUSH_MNEMONIC: &str = "push";
const PUSH_DEFERRED_MNEMONIC: &str = "push_deferred";
const PUSH_IMMUTABLE_MNEMONIC: &str = "push_immutable";

pub(in crate::backend::evm) trait StructuredAsmContext {
    fn push_value(&self, index: super::PushValueId) -> U256;
    fn push_inst(&mut self, value: U256) -> AsmInst;
    fn new_label(&mut self) -> Label;
    /// Whether EVM IR pass timings should be printed.
    fn time_passes(&self) -> bool {
        false
    }
    /// Whether the bridge should run the experimental EVM IR `StackSchedule`
    /// pass. Off by default; see [`StructuredAsmProgram::optimize_with_evm_ir`].
    fn run_evm_ir_stack_schedule(&self) -> bool {
        false
    }
    /// Whether the bridge should run EVM IR layout/code-size passes.
    fn run_evm_ir_layout(&self) -> bool {
        true
    }
}

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
    /// When `context.run_evm_ir_stack_schedule()` is true (off by default) the
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
    pub(in crate::backend::evm) fn optimize_with_evm_ir<C>(&mut self, context: &mut C) -> usize
    where
        C: StructuredAsmContext,
    {
        let Some((mut module, mut labels)) = self.to_evm_ir_module(context) else {
            return 0;
        };

        // Low-level assembler users may provide fragments whose stack inputs
        // come from outside the program. Preserve the verifier invariant only
        // for complete modules that satisfy it before this bridge.
        let input_is_valid = cfg!(debug_assertions) && is_valid_evm_ir(&module);
        let mut changed = 0;
        let pass_options = ir::PassOptions { time_passes: context.time_passes() };

        if context.run_evm_ir_stack_schedule() {
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

        if context.run_evm_ir_layout() {
            for pass in ir::DEFAULT_LAYOUT_PIPELINE {
                changed += usize::from(ir::run_pass(&mut module, pass, pass_options));
            }
        }
        debug_assert!(!input_is_valid || is_valid_evm_ir(&module));

        *self = Self::from_evm_ir_module(&module, &mut labels, context);
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

    fn to_evm_ir_module<C>(&self, context: &mut C) -> Option<(ir::Module, Vec<Option<Label>>)>
    where
        C: StructuredAsmContext,
    {
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

        let mut module = ir::Module::new("asm");
        for (index, block) in self.blocks.iter().enumerate() {
            let mut ir_block = ir::Block::new(format!("bb{index}"));
            ir_block.metadata.hotness = block.hotness;
            module.add_block(ir_block);
        }

        for (index, block) in self.blocks.iter().enumerate() {
            let block_id = crate::backend::evm::ir::BlockId::from_usize(index);
            let next_block = (index + 1 < self.blocks.len())
                .then(|| crate::backend::evm::ir::BlockId::from_usize(index + 1));
            let (instructions, terminator) =
                Self::translate_block_to_evm_ir(block, next_block, &label_to_block, context)?;
            module.blocks[block_id].instructions = instructions;
            module.blocks[block_id].terminator = Some(ir::Terminator::new(terminator));
        }

        Some((module, labels))
    }

    fn translate_block_to_evm_ir<C>(
        block: &StructuredAsmBlock,
        next_block: Option<crate::backend::evm::ir::BlockId>,
        label_to_block: &std::collections::BTreeMap<Label, crate::backend::evm::ir::BlockId>,
        context: &mut C,
    ) -> Option<(Vec<ir::Instruction>, ir::TerminatorKind)>
    where
        C: StructuredAsmContext,
    {
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
                ir::TerminatorKind::Jump(next_block?)
            };

        let mut instructions = Vec::with_capacity(body_len);
        for &inst in &block.instructions[..body_len] {
            instructions.push(Self::inst_to_evm_ir(inst, label_to_block, context)?);
        }
        Some((instructions, terminator))
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

    fn inst_to_evm_ir<C>(
        inst: AsmInst,
        label_to_block: &std::collections::BTreeMap<Label, crate::backend::evm::ir::BlockId>,
        context: &mut C,
    ) -> Option<ir::Instruction>
    where
        C: StructuredAsmContext,
    {
        Some(match inst.kind() {
            AsmInstKind::Op(opcode) => {
                if let Some(stack_op) = stack_op_from_opcode(opcode) {
                    ir::Instruction::stack_op(stack_op)
                } else {
                    ir::Instruction::new(opcode_mnemonic(opcode), Vec::new())
                }
            }
            AsmInstKind::PushInline(value) => {
                push_instruction(ir::Operand::Immediate(U256::from(value)))
            }
            AsmInstKind::Push(index) => {
                push_instruction(ir::Operand::Immediate(context.push_value(index)))
            }
            AsmInstKind::PushLabel(label) => {
                let block = *label_to_block.get(&label)?;
                push_instruction(ir::Operand::Block(block))
            }
            AsmInstKind::PushDeferred(id) => ir::Instruction::new(
                PUSH_DEFERRED_MNEMONIC,
                vec![ir::Operand::Immediate(U256::from(id.index()))],
            ),
            AsmInstKind::PushImmutable(id) => ir::Instruction::new(
                PUSH_IMMUTABLE_MNEMONIC,
                vec![ir::Operand::Immediate(U256::from(id))],
            ),
            AsmInstKind::Label(_) => return None,
        })
    }

    fn from_evm_ir_module<C>(
        module: &ir::Module,
        labels: &mut Vec<Option<Label>>,
        context: &mut C,
    ) -> Self
    where
        C: StructuredAsmContext,
    {
        let mut program = Self::default();
        for (block_id, block) in module.blocks.iter_enumerated() {
            let original = original_block_index(&block.label);
            let label = original.and_then(|index| labels.get(index).copied().flatten());
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
                if let Some(asm_inst) = Self::evm_ir_inst_to_asm(inst, module, labels, context) {
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
                    context,
                );
            }
        }
        program
    }

    fn evm_ir_inst_to_asm<C>(
        inst: &ir::Instruction,
        module: &ir::Module,
        labels: &mut Vec<Option<Label>>,
        context: &mut C,
    ) -> Option<AsmInst>
    where
        C: StructuredAsmContext,
    {
        match &inst.kind {
            ir::InstructionKind::Stack(op) => Some(AsmInst::op(opcode_from_stack_op(*op))),
            ir::InstructionKind::Operation(mnemonic) if mnemonic == PUSH_MNEMONIC => {
                match inst.operands.as_slice() {
                    [ir::Operand::Immediate(value)] => Some(context.push_inst(*value)),
                    [ir::Operand::Block(block)] => {
                        Some(AsmInst::push_label(label_for_block(module, *block, labels, context)))
                    }
                    _ => None,
                }
            }
            ir::InstructionKind::Operation(mnemonic) if mnemonic == PUSH_DEFERRED_MNEMONIC => {
                let [ir::Operand::Immediate(value)] = inst.operands.as_slice() else {
                    return None;
                };
                Some(AsmInst::push_deferred(DeferredConst::from_usize(
                    usize::try_from(*value).ok()?,
                )))
            }
            ir::InstructionKind::Operation(mnemonic) if mnemonic == PUSH_IMMUTABLE_MNEMONIC => {
                let [ir::Operand::Immediate(value)] = inst.operands.as_slice() else {
                    return None;
                };
                Some(AsmInst::push_immutable(u32::try_from(*value).ok()?))
            }
            ir::InstructionKind::Operation(mnemonic) => {
                parse_opcode_mnemonic(mnemonic).map(AsmInst::op)
            }
        }
    }

    fn emit_evm_ir_terminator<C>(
        program: &mut Self,
        block_id: crate::backend::evm::ir::BlockId,
        kind: &ir::TerminatorKind,
        module: &ir::Module,
        labels: &mut Vec<Option<Label>>,
        context: &mut C,
    ) where
        C: StructuredAsmContext,
    {
        match kind {
            ir::TerminatorKind::Jump(target) => {
                if next_block(module, block_id) == Some(*target) {
                    return;
                }
                let label = label_for_block(module, *target, labels, context);
                program.push(AsmInst::push_label(label));
                program.push(AsmInst::op(op::JUMP));
            }
            ir::TerminatorKind::RawOpcode(opcode) => program.push(AsmInst::op(*opcode)),
            ir::TerminatorKind::Stop => program.push(AsmInst::op(op::STOP)),
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
    let mut inst = ir::Instruction::new(PUSH_MNEMONIC, vec![operand]);
    inst.metadata.stack = Some(ir::StackEffect::new(0, 1));
    inst
}

fn opcode_mnemonic(opcode: u8) -> String {
    op::mnemonic(opcode).map(str::to_string).unwrap_or_else(|| format!("{OP_PREFIX}{opcode:02x}"))
}

fn parse_opcode_mnemonic(mnemonic: &str) -> Option<u8> {
    if let Some(opcode) = op::from_mnemonic(mnemonic) {
        return Some(opcode);
    }
    let value = mnemonic.strip_prefix(OP_PREFIX)?;
    u8::from_str_radix(value, 16).ok()
}

fn stack_op_from_opcode(opcode: u8) -> Option<ir::StackOp> {
    match opcode {
        op::POP => Some(ir::StackOp::Pop),
        op::DUP1..=op::DUP16 => ir::StackOp::dup(opcode - op::DUP1 + 1),
        op::SWAP1..=op::SWAP16 => ir::StackOp::swap(opcode - op::SWAP1 + 1),
        _ => None,
    }
}

fn opcode_from_stack_op(op: ir::StackOp) -> u8 {
    match op {
        ir::StackOp::Dup(n) => op::dup(n),
        ir::StackOp::Swap(n) => op::swap(n),
        ir::StackOp::Pop => op::POP,
    }
}

fn original_block_index(label: &str) -> Option<usize> {
    label.strip_prefix("bb")?.parse().ok()
}

fn next_block(
    module: &ir::Module,
    block: crate::backend::evm::ir::BlockId,
) -> Option<crate::backend::evm::ir::BlockId> {
    let next = block.index() + 1;
    (next < module.blocks.len()).then(|| crate::backend::evm::ir::BlockId::from_usize(next))
}

fn label_for_block<C>(
    module: &ir::Module,
    block: crate::backend::evm::ir::BlockId,
    labels: &mut Vec<Option<Label>>,
    context: &mut C,
) -> Label
where
    C: StructuredAsmContext,
{
    let original =
        original_block_index(&module.blocks[block].label).unwrap_or_else(|| block.index());
    if original >= labels.len() {
        labels.resize_with(original + 1, || None);
    }
    *labels[original].get_or_insert_with(|| context.new_label())
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
    pub(in crate::backend::evm) fn to_evm_ir_module<C>(&self, context: &mut C) -> Option<ir::Module>
    where
        C: StructuredAsmContext,
    {
        let mut structured = StructuredAsmProgram::default();
        for &inst in &self.instructions {
            if let AsmInstKind::Label(label) = inst.kind() {
                structured.define_label(label);
            } else {
                structured.push(inst);
            }
        }
        structured.to_evm_ir_module(context).map(|(module, _)| module)
    }
}

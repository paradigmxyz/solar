//! EVM IR verifier.

use super::*;
use crate::backend::evm::op;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::FxHashSet,
};
use solar_interface::diagnostics::{DiagCtxt, ErrorGuaranteed};
use std::fmt;

/// Stateful EVM IR verifier.
struct Verifier<'a> {
    dcx: &'a DiagCtxt,
}

impl<'a> Verifier<'a> {
    const fn new(dcx: &'a DiagCtxt) -> Self {
        Self { dcx }
    }

    #[track_caller]
    fn error(&self, msg: impl fmt::Display) -> ErrorGuaranteed {
        // TODO: Use EVM IR debug-info spans when emitting verifier diagnostics.
        let msg = fmt::from_fn(|f| write!(f, "EVM IR verification failed: {msg}"));
        self.dcx.err(msg.to_string()).emit()
    }

    #[track_caller]
    fn error_in_block(&self, block: BlockId, msg: impl fmt::Display) -> ErrorGuaranteed {
        self.error(format_args!("block {}: {msg}", block.index()))
    }

    fn verify_module(&self, module: &Module) {
        let errors_before = self.dcx.err_count();
        if !solar_parse::lexer::is_ident(module.name.as_str()) {
            self.error(format_args!("invalid program name `{}`", module.name));
        }
        if module.blocks.is_empty() {
            self.error("program has no blocks");
            return;
        }
        match module.entry_block {
            Some(entry) if self.block_exists(module, entry) => {}
            Some(entry) => {
                self.error(format_args!("entry block `{}` is out of range", entry.index()));
            }
            None => {
                self.error("program has no entry block");
            }
        }

        let mut labels = FxHashSet::default();
        for (block_id, block) in module.blocks.iter_enumerated() {
            if !labels.insert(block.label) {
                self.error_in_block(
                    block_id,
                    format_args!("duplicate block label `bb{}`", block.label),
                );
            }
            for inst in &block.instructions {
                self.verify_instruction_shape(block_id, module, inst);
            }
            let Some(term) = &block.terminator else {
                self.error_in_block(block_id, "missing terminator");
                continue;
            };
            self.verify_terminator_shape(block_id, term);
            term.kind.visit_targets(|target| {
                if !self.block_exists(module, target) {
                    self.error_in_block(
                        block_id,
                        format_args!("target block `{}` is out of range", target.index()),
                    );
                }
            });
        }

        if self.dcx.err_count() == errors_before {
            self.verify_stack_ops(module);
        }
    }

    fn verify_instruction_shape(&self, block_id: BlockId, module: &Module, inst: &Instruction) {
        if inst.is_encoded_push() {
            let Some(value) = &inst.value else {
                self.error_in_block(
                    block_id,
                    format_args!("`{}` must carry a value", inst.mnemonic()),
                );
                return;
            };
            if inst.opcode != op::PUSH32 {
                self.error_in_block(block_id, "encoded push must use the `PUSH32` opcode");
            }
            match inst.encoding {
                Instruction::ENCODED_PUSH => {}
                encoding if encoding == Instruction::ENCODED_PUSH | Instruction::DEFERRED => {
                    self.verify_assembly_id(block_id, inst, value, "deferred constant");
                }
                encoding if encoding == Instruction::ENCODED_PUSH | Instruction::IMMUTABLE => {
                    self.verify_assembly_id(block_id, inst, value, "immutable");
                }
                _ => {
                    self.error_in_block(block_id, "invalid encoded push kind");
                }
            };
            if let PushValue::Block(target) = value
                && !self.block_exists(module, *target)
            {
                self.error_in_block(
                    block_id,
                    format_args!("push target block `{}` is out of range", target.index()),
                );
            }
        } else if inst.value.is_some() {
            self.error_in_block(block_id, "only `push` instructions can carry a value");
        }

        match (inst.metadata.stack, default_instruction_stack_effect(inst)) {
            (Some(effect), Some(expected)) if effect != expected => {
                self.error_in_block(
                    block_id,
                    format_args!(
                        "`{}` has stack effect {}->{}, expected {}->{}",
                        inst.mnemonic(),
                        effect.inputs,
                        effect.outputs,
                        expected.inputs,
                        expected.outputs
                    ),
                );
            }
            (None, None) => {
                self.error_in_block(
                    block_id,
                    format_args!(
                        "instruction `{}` must declare an explicit stack effect",
                        inst.mnemonic()
                    ),
                );
            }
            _ => {}
        }
    }

    fn verify_assembly_id(
        &self,
        block_id: BlockId,
        inst: &Instruction,
        value: &PushValue,
        name: &str,
    ) {
        let PushValue::Immediate(value) = value else {
            self.error_in_block(
                block_id,
                format_args!("`{}` must carry an immediate {name} ID", inst.mnemonic()),
            );
            return;
        };
        if u32::try_from(*value).ok().is_none_or(|value| value > assembly::AsmInst::PAYLOAD_MASK) {
            self.error_in_block(block_id, format_args!("{name} ID exceeds the assembler limit"));
        }
    }

    fn verify_terminator_shape(&self, block_id: BlockId, term: &Terminator) {
        if let TerminatorKind::Op(opcode) = &term.kind
            && !op::is_terminal(*opcode)
        {
            self.error_in_block(
                block_id,
                format_args!("terminator opcode `0x{opcode:02x}` is not terminal"),
            );
        }
        match (term.metadata.stack, default_terminator_stack_effect(&term.kind)) {
            (Some(effect), Some(expected)) if effect != expected => {
                self.error_in_block(
                    block_id,
                    format_args!(
                        "`{}` has stack effect {}->{}, expected {}->{}",
                        terminator_name(&term.kind),
                        effect.inputs,
                        effect.outputs,
                        expected.inputs,
                        expected.outputs
                    ),
                );
            }
            (None, None) => {
                self.error_in_block(
                    block_id,
                    format_args!(
                        "terminator `{}` must declare an explicit stack effect",
                        terminator_name(&term.kind)
                    ),
                );
            }
            _ => {}
        }
    }

    /// Checks physical stack operations along every direct path from the entry block.
    fn verify_stack_ops(&self, module: &Module) {
        let Some(entry) = module.entry_block else { return };
        let mut entry_depths: IndexVec<BlockId, Option<usize>> =
            index_vec![None; module.blocks.len()];
        entry_depths[entry] = Some(0);
        let mut pending = vec![entry];
        let mut invalid = DenseBitSet::new_empty(module.blocks.len());
        while let Some(block_id) = pending.pop() {
            if invalid.contains(block_id) {
                continue;
            }
            let block = &module.blocks[block_id];
            let mut stack = ModelStack { depth: entry_depths[block_id].unwrap() };
            let mut valid = true;
            for inst in &block.instructions {
                if inst.is_physical_stack_op() {
                    if self.apply_physical_stack_op(block_id, inst.opcode, &mut stack).is_err() {
                        valid = false;
                        break;
                    }
                } else {
                    let effect = inst
                        .metadata
                        .stack
                        .or_else(|| default_instruction_stack_effect(inst))
                        .expect("instruction stack effect must be known after shape validation");
                    if self.apply_effect(block_id, inst.mnemonic(), effect, &mut stack).is_err() {
                        valid = false;
                        break;
                    }
                }
            }
            if valid && let Some(term) = &block.terminator {
                let effect = default_terminator_stack_effect(&term.kind)
                    .or(term.metadata.stack)
                    .expect("terminator stack effect must be known after shape validation");
                valid = self
                    .apply_effect(block_id, terminator_name(&term.kind), effect, &mut stack)
                    .is_ok();
            }
            if !valid {
                invalid.insert(block_id);
                continue;
            }
            let Some(term) = &block.terminator else { continue };
            term.kind.visit_targets(|target| {
                let update = match entry_depths[target] {
                    Some(previous) => stack.depth < previous,
                    None => true,
                };
                if update && !invalid.contains(target) {
                    entry_depths[target] = Some(stack.depth);
                    pending.push(target);
                }
            });
        }
    }

    fn apply_effect(
        &self,
        block_id: BlockId,
        name: impl fmt::Display,
        effect: StackEffect,
        stack: &mut ModelStack,
    ) -> Result<(), ErrorGuaranteed> {
        let inputs = usize::from(effect.inputs);
        if !stack.ensure_depth(inputs) {
            return Err(self.error_in_block(
                block_id,
                format_args!(
                    "`{name}` consumes {} stack words but only {} are available",
                    effect.inputs, stack.depth
                ),
            ));
        }
        stack.depth = stack.depth - inputs + usize::from(effect.outputs);
        Ok(())
    }

    fn apply_physical_stack_op(
        &self,
        block_id: BlockId,
        opcode: u8,
        stack: &mut ModelStack,
    ) -> Result<(), ErrorGuaranteed> {
        match opcode {
            op::DUP1..=op::DUP16 => {
                let n = opcode - op::DUP1 + 1;
                if !stack.ensure_depth(usize::from(n)) {
                    return Err(self.error_in_block(
                        block_id,
                        format_args!(
                            "`dup{n}` reaches depth {n} but the stack has {}",
                            stack.depth
                        ),
                    ));
                }
                stack.depth += 1;
            }
            op::SWAP1..=op::SWAP16 => {
                let n = opcode - op::SWAP1 + 1;
                if !stack.ensure_depth(usize::from(n) + 1) {
                    return Err(self.error_in_block(
                        block_id,
                        format_args!(
                            "`swap{n}` reaches depth {n} but the stack has {}",
                            stack.depth
                        ),
                    ));
                }
            }
            op::POP => {
                if !stack.ensure_depth(1) {
                    return Err(self.error_in_block(block_id, "`pop` on an empty stack"));
                }
                stack.depth -= 1;
            }
            _ => unreachable!("checked physical stack opcode"),
        }
        Ok(())
    }

    fn block_exists(&self, module: &Module, block: BlockId) -> bool {
        block.index() < module.blocks.len()
    }
}

struct ModelStack {
    depth: usize,
}

impl ModelStack {
    fn ensure_depth(&self, depth: usize) -> bool {
        self.depth >= depth
    }
}

fn terminator_name(kind: &TerminatorKind) -> &'static str {
    match kind {
        TerminatorKind::Jump(_) => "jump",
        TerminatorKind::JumpI { .. } => "jumpi",
        TerminatorKind::Op(opcode) => op::mnemonic(*opcode).unwrap_or("terminal"),
    }
}

pub(super) fn validate(dcx: &DiagCtxt, module: &Module) {
    Verifier::new(dcx).verify_module(module);
}

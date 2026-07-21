//! EVM IR verifier.

use super::*;
use crate::backend::evm::opcode as op;
use solar_data_structures::map::FxHashSet;
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
            self.verify_terminator_shape(block_id, &term.kind);
            term.kind.visit_targets(|target| {
                if !self.block_exists(module, target) {
                    self.error_in_block(
                        block_id,
                        format_args!("target block `{}` is out of range", target.index()),
                    );
                }
            });
        }

        self.verify_stack_ops(module);
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

        let expected = default_instruction_stack_effect(inst);
        if let Some(effect) = inst.metadata.stack
            && op::stack_io(inst.opcode).is_some()
            && effect != expected
        {
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
    }

    fn verify_terminator_shape(&self, block_id: BlockId, kind: &TerminatorKind) {
        if let TerminatorKind::Op(opcode) = kind
            && !op::is_terminal(*opcode)
        {
            self.error_in_block(
                block_id,
                format_args!("terminator opcode `0x{opcode:02x}` is not terminal"),
            );
        }
    }

    /// Checks physical stack operations precisely in the entry block and
    /// relative to an implicit incoming stack in every other block.
    fn verify_stack_ops(&self, module: &Module) {
        for (block_id, block) in module.blocks.iter_enumerated() {
            let mut stack =
                ModelStack { depth: 0, infinite_floor: module.entry_block != Some(block_id) };
            for inst in &block.instructions {
                if inst.is_physical_stack_op() {
                    if self.apply_physical_stack_op(block_id, inst.opcode, &mut stack).is_err() {
                        break;
                    }
                } else {
                    let effect = inst
                        .metadata
                        .stack
                        .unwrap_or_else(|| default_instruction_stack_effect(inst));
                    if self.apply_effect(block_id, inst.mnemonic(), effect, &mut stack).is_err() {
                        break;
                    }
                }
            }
            if let Some(term) = &block.terminator {
                let effect = term
                    .metadata
                    .stack
                    .unwrap_or_else(|| default_terminator_stack_effect(&term.kind));
                self.apply_effect(block_id, terminator_name(&term.kind), effect, &mut stack).ok();
            }
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
    infinite_floor: bool,
}

impl ModelStack {
    fn ensure_depth(&mut self, depth: usize) -> bool {
        if self.depth >= depth {
            return true;
        }
        if !self.infinite_floor {
            return false;
        }
        self.depth = depth;
        true
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

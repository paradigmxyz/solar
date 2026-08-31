//! EVM IR verifier.

use super::*;
use crate::backend::evm::{op, stack::MAX_STACK_DEPTH};
use solar_config::EvmVersion;
use solar_data_structures::{index::IndexVec, map::FxHashSet};
use solar_interface::diagnostics::{DiagCtxt, ErrorGuaranteed};
use std::fmt;

/// EVM IR verifier.
struct Verifier<'a> {
    dcx: &'a DiagCtxt,
    evm_version: Option<EvmVersion>,
}

impl<'a> Verifier<'a> {
    const fn new(dcx: &'a DiagCtxt) -> Self {
        Self { dcx, evm_version: None }
    }

    const fn for_evm_version(dcx: &'a DiagCtxt, evm_version: EvmVersion) -> Self {
        Self { dcx, evm_version: Some(evm_version) }
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
        if module.blocks.is_empty() {
            self.error("program has no blocks");
            return;
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
            if inst.encoding & Instruction::IMMUTABLE != 0
                && !(op::PUSH1..=op::PUSH32).contains(&inst.opcode)
            {
                self.error_in_block(
                    block_id,
                    "encoded immutable push must use a `PUSH1` through `PUSH32` opcode",
                );
            } else if inst.encoding & Instruction::IMMUTABLE == 0 && inst.opcode != op::PUSH32 {
                self.error_in_block(block_id, "encoded push must use the `PUSH32` opcode");
            }
            match inst.encoding {
                Instruction::ENCODED_PUSH => {}
                encoding if encoding == Instruction::ENCODED_PUSH | Instruction::DEFERRED => {
                    self.verify_assembly_id(block_id, inst, value, "deferred constant");
                }
                encoding if encoding == Instruction::ENCODED_PUSH | Instruction::IMMUTABLE => {
                    self.verify_immutable_id(block_id, inst, value);
                }
                encoding if encoding == Instruction::ENCODED_PUSH | Instruction::DATA => {
                    let PushValue::Data(data) = value else {
                        self.error_in_block(block_id, "`push_data` must carry a data ID");
                        return;
                    };
                    if data.id.index() >= module.data.len() {
                        self.error_in_block(
                            block_id,
                            format_args!("program data `{}` is out of range", data.id.index()),
                        );
                    } else if data.offset as usize > module.data[data.id].bytes.len() {
                        self.error_in_block(
                            block_id,
                            format_args!(
                                "program data offset `{}` exceeds data size `{}`",
                                data.offset,
                                module.data[data.id].bytes.len()
                            ),
                        );
                    }
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
        } else {
            if inst.value.is_some() {
                self.error_in_block(block_id, "only `push` instructions can carry a value");
            }
            if let Some(stack_op) = inst.as_stack_op() {
                if inst.opcode != stack_op.ir_opcode() {
                    self.error_in_block(block_id, "logical stack operation has the wrong opcode");
                }
                if !stack_op.is_valid() {
                    self.error_in_block(block_id, "logical stack operation has invalid depths");
                } else if let Some(evm_version) = self.evm_version
                    && stack_op.assembled_len(evm_version).is_none()
                {
                    self.error_in_block(
                        block_id,
                        format_args!("`{}` requires Amsterdam-compatible EVM", inst.mnemonic()),
                    );
                }
            } else if op::StackOp::from_legacy_opcode(inst.opcode).is_some()
                || matches!(inst.opcode, op::DUPN | op::SWAPN | op::EXCHANGE)
            {
                self.error_in_block(
                    block_id,
                    format_args!("`{}` must use the logical stack-op form", inst.mnemonic()),
                );
            } else if inst.opcode == op::PUSH0 {
                self.error_in_block(block_id, "`push0` must use the logical push form");
            } else if let Some(evm_version) = self.evm_version
                && inst.opcode == op::SLOTNUM
                && !evm_version.has_slot_num()
            {
                self.error_in_block(block_id, "`slotnum` requires Amsterdam-compatible EVM");
            }
            if (op::PUSH1..=op::PUSH32).contains(&inst.opcode) {
                self.error_in_block(
                    block_id,
                    format_args!("`{}` must carry an encoded push value", inst.mnemonic()),
                );
            }
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

    fn verify_immutable_id(&self, block_id: BlockId, inst: &Instruction, value: &PushValue) {
        let PushValue::Immediate(value) = value else {
            self.error_in_block(
                block_id,
                format_args!("`{}` must carry an immediate immutable ID", inst.mnemonic()),
            );
            return;
        };
        if u32::try_from(*value).ok().is_none_or(|value| value == u32::MAX) {
            self.error_in_block(block_id, "immutable ID exceeds the index limit");
        }
    }

    fn verify_terminator_shape(&self, block_id: BlockId, term: &Terminator) {
        if matches!(&term.kind, TerminatorKind::IndexedJump(targets) if targets.is_empty()) {
            self.error_in_block(block_id, "`indexed_jump` must have at least one target");
        }
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

    /// Checks physical stack operations along generated direct control-flow edges.
    fn verify_stack_ops(&self, module: &Module) {
        let mut entry_depths = IndexVec::<BlockId, _>::from_vec(vec![None; module.blocks.len()]);
        entry_depths[BlockId::ENTRY] = Some(0);
        let mut alternate_depths = FxHashSet::default();
        let mut pending = vec![(BlockId::ENTRY, 0)];
        while let Some((block_id, mut stack)) = pending.pop() {
            let block = &module.blocks[block_id];
            let term =
                block.terminator.as_ref().expect("terminator must exist after shape validation");
            let mut physical_targets = Vec::new();
            let mut valid = true;
            for (index, inst) in block.instructions.iter().enumerate() {
                if inst.is_physical_stack_op() {
                    if self.apply_physical_stack_op(block_id, inst, &mut stack).is_err() {
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
                if inst.opcode == op::JUMPI
                    && let Some(target) = index
                        .checked_sub(1)
                        .and_then(|index| block.instructions[index].pushed_block())
                {
                    physical_targets.push((target, stack));
                }
            }
            if valid {
                let next = Self::next_block(module, block_id);
                let lowering_growth = match &term.kind {
                    TerminatorKind::IndexedJump(_) => 3,
                    TerminatorKind::Jump(target) => usize::from(Some(*target) != next),
                    TerminatorKind::JumpI { .. } => 1,
                    TerminatorKind::Op(_) => 0,
                };
                if lowering_growth != 0
                    && self
                        .ensure_stack_limit(
                            block_id,
                            terminator_name(&term.kind),
                            stack + lowering_growth,
                        )
                        .is_err()
                {
                    valid = false;
                } else {
                    let effect = default_terminator_stack_effect(&term.kind)
                        .or(term.metadata.stack)
                        .expect("terminator stack effect must be known after shape validation");
                    valid = self
                        .apply_effect(block_id, terminator_name(&term.kind), effect, &mut stack)
                        .is_ok();
                }
            }
            if !valid {
                continue;
            }
            term.kind.visit_targets(|target| physical_targets.push((target, stack)));
            for (target, depth) in physical_targets {
                match entry_depths[target] {
                    None => {
                        entry_depths[target] = Some(depth);
                        pending.push((target, depth));
                    }
                    Some(first) if first != depth && alternate_depths.insert((target, depth)) => {
                        pending.push((target, depth));
                    }
                    Some(_) => {}
                }
            }
        }
    }

    fn apply_effect(
        &self,
        block_id: BlockId,
        name: impl fmt::Display,
        effect: StackEffect,
        stack: &mut usize,
    ) -> Result<(), ErrorGuaranteed> {
        let inputs = usize::from(effect.inputs);
        if *stack < inputs {
            return Err(self.error_in_block(
                block_id,
                format_args!(
                    "`{name}` consumes {} stack words but only {} are available",
                    effect.inputs, *stack
                ),
            ));
        }
        *stack = *stack - inputs + usize::from(effect.outputs);
        self.ensure_stack_limit(block_id, name, *stack)
    }

    fn apply_physical_stack_op(
        &self,
        block_id: BlockId,
        inst: &Instruction,
        stack: &mut usize,
    ) -> Result<(), ErrorGuaranteed> {
        let stack_op = inst.as_stack_op().expect("checked physical stack operation");
        let name = match stack_op {
            op::StackOp::Dup(n) => {
                if *stack < usize::from(n) {
                    return Err(self.error_in_block(
                        block_id,
                        format_args!("`dup {n}` reaches depth {n} but the stack has {}", *stack),
                    ));
                }
                *stack += 1;
                "dup"
            }
            op::StackOp::Swap(n) => {
                if *stack < usize::from(n) + 1 {
                    return Err(self.error_in_block(
                        block_id,
                        format_args!("`swap {n}` reaches depth {n} but the stack has {}", *stack),
                    ));
                }
                "swap"
            }
            op::StackOp::Exchange(_, m) => {
                if *stack < usize::from(m) + 1 {
                    return Err(self.error_in_block(
                        block_id,
                        format_args!("`exchange` reaches depth {m} but the stack has {}", *stack),
                    ));
                }
                "exchange"
            }
            op::StackOp::Pop => {
                if *stack == 0 {
                    return Err(self.error_in_block(block_id, "`pop` on an empty stack"));
                }
                *stack -= 1;
                "pop"
            }
        };
        self.ensure_stack_limit(block_id, name, *stack)
    }

    fn ensure_stack_limit(
        &self,
        block_id: BlockId,
        name: impl fmt::Display,
        depth: usize,
    ) -> Result<(), ErrorGuaranteed> {
        if depth > MAX_STACK_DEPTH {
            Err(self.error_in_block(
                block_id,
                format_args!(
                    "`{name}` grows the stack to {depth} words, exceeding the limit of {MAX_STACK_DEPTH}"
                ),
            ))
        } else {
            Ok(())
        }
    }

    fn block_exists(&self, module: &Module, block: BlockId) -> bool {
        block.index() < module.blocks.len()
    }

    fn next_block(module: &Module, block: BlockId) -> Option<BlockId> {
        let next = block.index() + 1;
        (next < module.blocks.len()).then(|| BlockId::from_usize(next))
    }
}

fn terminator_name(kind: &TerminatorKind) -> &'static str {
    match kind {
        TerminatorKind::Jump(_) => "jump",
        TerminatorKind::JumpI { .. } => "jumpi",
        TerminatorKind::IndexedJump(_) => "indexed_jump",
        TerminatorKind::Op(opcode) => op::mnemonic(*opcode).unwrap_or("terminal"),
    }
}

pub(super) fn validate(dcx: &DiagCtxt, module: &Module) {
    Verifier::new(dcx).verify_module(module);
}

pub(super) fn validate_evm_version(
    dcx: &DiagCtxt,
    module: &Module,
    evm_version: EvmVersion,
    allow_legacy_opcodes: bool,
) {
    for (block_id, block) in module.blocks.iter_enumerated() {
        for inst in &block.instructions {
            if let Some(stack_op) = inst.as_stack_op() {
                validate_stack_op(dcx, block_id, inst, stack_op, evm_version);
            } else {
                validate_opcode(dcx, block_id, inst.opcode, evm_version, allow_legacy_opcodes);
            }
        }
        if let Some(Terminator { kind: TerminatorKind::Op(opcode), .. }) = &block.terminator {
            validate_opcode(dcx, block_id, *opcode, evm_version, allow_legacy_opcodes);
        }
    }
}

pub(super) fn validate_stack_ops_for_evm_version(
    dcx: &DiagCtxt,
    module: &Module,
    evm_version: EvmVersion,
) {
    for (block_id, block) in module.blocks.iter_enumerated() {
        for inst in &block.instructions {
            if let Some(stack_op) = inst.as_stack_op() {
                validate_stack_op(dcx, block_id, inst, stack_op, evm_version);
            }
        }
    }
}

fn validate_stack_op(
    dcx: &DiagCtxt,
    block: BlockId,
    inst: &Instruction,
    stack_op: op::StackOp,
    evm_version: EvmVersion,
) {
    if stack_op.lowering(evm_version).is_none() {
        dcx.err(format!(
            "EVM IR verification failed: block {}: `{}` requires Amsterdam-compatible EVM",
            block.index(),
            inst.mnemonic()
        ))
        .emit();
    }
}

fn validate_opcode(
    dcx: &DiagCtxt,
    block: BlockId,
    opcode: u8,
    evm_version: EvmVersion,
    allow_legacy_opcodes: bool,
) {
    if allow_legacy_opcodes
        && ((!evm_version.has_bitwise_shifting() && matches!(opcode, op::SHL | op::SHR | op::SAR))
            || (!evm_version.supports_returndata() && opcode == op::REVERT))
    {
        return;
    }
    if !op::is_available(opcode, evm_version) {
        let name = op::mnemonic(opcode).unwrap_or("unknown");
        dcx.err(format!(
            "EVM IR verification failed: block {}: opcode `{name}` is unavailable for `{evm_version}` EVM",
            block.index()
        ))
        .emit();
    }
}

pub(super) fn validate_for_evm_version(dcx: &DiagCtxt, module: &Module, evm_version: EvmVersion) {
    Verifier::for_evm_version(dcx, evm_version).verify_module(module);
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_interface::sym;

    #[test]
    fn indexed_jump_reserves_lowering_stack() {
        for (depth, expected_errors) in [(1021, 0), (1022, 1)] {
            let mut module = Module::new(sym::module);
            let entry = module.add_block(Block::new(0));
            let target = module.add_block(Block::new(1));
            module.blocks[entry]
                .instructions
                .extend((0..depth).map(|_| Instruction::push_value(U256::ZERO)));
            module.blocks[entry].terminator =
                Some(Terminator::new(TerminatorKind::IndexedJump(vec![target].into_boxed_slice())));
            module.blocks[target].terminator = Some(Terminator::new(TerminatorKind::Op(op::STOP)));

            let dcx = DiagCtxt::with_silent_emitter(None);
            validate(&dcx, &module);
            assert_eq!(dcx.err_count(), expected_errors);
        }
    }

    #[test]
    fn gates_amsterdam_instructions() {
        let mut module = Module::new(sym::module);
        let entry = module.add_block(Block::new(0));
        module.blocks[entry]
            .instructions
            .extend((0..17).map(|_| Instruction::push_value(U256::ZERO)));
        module.blocks[entry].instructions.push(Instruction::stack_op(op::StackOp::Dup(17)));
        module.blocks[entry].instructions.push(Instruction::opcode(op::SLOTNUM));
        module.blocks[entry].terminator = Some(Terminator::new(TerminatorKind::Op(op::STOP)));

        let osaka = DiagCtxt::with_silent_emitter(None);
        validate_for_evm_version(&osaka, &module, EvmVersion::Osaka);
        assert_eq!(osaka.err_count(), 2);

        let amsterdam = DiagCtxt::with_silent_emitter(None);
        validate_for_evm_version(&amsterdam, &module, EvmVersion::Amsterdam);
        assert_eq!(amsterdam.err_count(), 0);
    }
}

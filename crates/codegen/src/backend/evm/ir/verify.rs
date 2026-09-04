//! EVM IR verifier.
//!
//! Two checks run over a module. The shape check is local: it validates labels, push encodings,
//! terminators, and that every instruction declares a stack effect consistent with its opcode.
//! The stack-operation check is global: it walks the direct control-flow edges and models the
//! physical stack height so that per-block imbalances, operand underflows, and depth violations
//! are caught before assembly.
//!
//! EVM IR is one flat CFG of blocks with no function boundaries: an internal call is a block
//! that pushes a return address and jumps to the callee's first block, and a return is a dynamic
//! `jump` through that address, which carries no static target. The walk therefore sees a
//! recursive function as an ordinary cycle whose modeled stack height grows by the return address
//! on every round, and a concrete-depth walk that follows it never converges.
//!
//! The walk instead propagates a range of entry depths per block and checks each bound where it
//! is the worst case. Operand availability (`dup`, `swap`, `pop`, and every instruction's inputs)
//! is monotone in the entry depth, so it is checked at the block's minimum; the 1024-word stack
//! limit is checked at the maximum. This is as strong as visiting every distinct depth, and it is
//! bounded: minimums only fall, maximums only rise, and a maximum may not rise across an edge
//! that closes a cycle. Cutting the growth there is what makes recursion compile, and it is the
//! only sound choice available statically, because the recursion depth of a legal program is a
//! runtime property; exceeding the stack limit through it is a runtime failure, exactly as it is
//! for solc. The cost is that the limit is checked per pass through a cycle rather than
//! accumulated around it, so a cycle whose body is not stack-balanced is reported only if one
//! pass through it already overflows.

use super::*;
use crate::backend::evm::{op, stack::MAX_STACK_DEPTH};
use solar_config::EvmVersion;
use solar_data_structures::{index::IndexVec, map::FxHashSet};
use solar_interface::diagnostics::{DiagCtxt, ErrorGuaranteed};
use solar_sema::Gcx;
use std::fmt;

/// EVM IR verifier.
pub(super) struct Verifier<'a> {
    dcx: &'a DiagCtxt,
    evm_version: EvmVersion,
}

impl<'a> Verifier<'a> {
    pub(super) fn new(gcx: Gcx<'a>) -> Self {
        Self { dcx: gcx.dcx(), evm_version: gcx.sess.opts.evm_version }
    }

    pub(super) const fn for_evm_version(dcx: &'a DiagCtxt, evm_version: EvmVersion) -> Self {
        Self { dcx, evm_version }
    }

    /// Checks the target requirements EVM IR passes rely on.
    pub(super) fn verify_before_pipeline(&self, module: &Module) {
        self.verify_stack_ops_for_evm_version(module);
    }

    /// Checks the module and its output target support after target legalization.
    ///
    /// This is the only stack-operation check that reports in release builds, so it runs for
    /// every EVM version: which module the check sees must not depend on whether legalization
    /// had shifts to rewrite.
    pub(super) fn verify_after_legalization(&self, module: &Module) {
        self.verify_module(module);
        self.verify_target_support(module);
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

    pub(super) fn verify_module(&self, module: &Module) {
        if self.verify_module_shape(module) {
            self.verify_stack_ops(module);
        }
    }

    pub(super) fn verify_module_shape(&self, module: &Module) -> bool {
        let errors_before = self.dcx.err_count();
        if module.blocks.is_empty() {
            self.error("program has no blocks");
            return false;
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
            // A `keep_with_next` instruction only constrains the boundary that follows it, so the
            // block must still hold the instruction it is kept with. This is what makes the
            // constraint a guarantee: any transform that turns that boundary into a block
            // boundary is rejected here instead of silently inserting a jump.
            if let Some(last) = block.instructions.last()
                && last.keeps_with_next()
            {
                self.error_in_block(
                    block_id,
                    format_args!(
                        "`{}` must be followed by the instruction it is kept with",
                        last.mnemonic()
                    ),
                );
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

        self.dcx.err_count() == errors_before
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
                }
            } else if op::StackOp::from_single_byte_evm_opcode(inst.opcode).is_some()
                || matches!(inst.opcode, op::DUPN | op::SWAPN | op::EXCHANGE)
            {
                self.error_in_block(
                    block_id,
                    format_args!("`{}` must use the logical stack-op form", inst.mnemonic()),
                );
            } else if inst.opcode == op::PUSH0 {
                self.error_in_block(block_id, "`push0` must use the logical push form");
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
        if term.metadata.keep_with_next {
            self.error_in_block(
                block_id,
                format_args!("terminator `{}` cannot be kept with a next instruction", term.kind),
            );
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
                        term.kind, effect.inputs, effect.outputs, expected.inputs, expected.outputs
                    ),
                );
            }
            (None, None) => {
                self.error_in_block(
                    block_id,
                    format_args!(
                        "terminator `{}` must declare an explicit stack effect",
                        term.kind
                    ),
                );
            }
            _ => {}
        }
    }

    /// Checks physical stack operations along generated direct control-flow edges.
    ///
    /// See the module documentation for the bounds the walk propagates and for why cycle edges
    /// are not allowed to raise a block's maximum entry depth.
    fn verify_stack_ops(&self, module: &Module) {
        let cycle_edges = cycle_edges(module);
        let mut entry_depths =
            IndexVec::<BlockId, Option<DepthBounds>>::from_vec(vec![None; module.blocks.len()]);
        entry_depths[BlockId::ENTRY] = Some(DepthBounds::exact(0));
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
                        .effective_stack_effect()
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
                let lowering_growth = term.kind.lowering_stack_growth(module.next_block(block_id));
                if lowering_growth != 0
                    && self
                        .ensure_stack_limit(block_id, &term.kind, stack + lowering_growth)
                        .is_err()
                {
                    valid = false;
                } else {
                    let effect = default_terminator_stack_effect(&term.kind)
                        .or(term.metadata.stack)
                        .expect("terminator stack effect must be known after shape validation");
                    valid = self.apply_effect(block_id, &term.kind, effect, &mut stack).is_ok();
                }
            }
            if !valid {
                continue;
            }
            term.kind.visit_targets(|target| physical_targets.push((target, stack)));
            for (target, depth) in physical_targets {
                match &mut entry_depths[target] {
                    None => {
                        entry_depths[target] = Some(DepthBounds::exact(depth));
                        pending.push((target, depth));
                    }
                    Some(bounds) => {
                        // A cycle edge may lower the minimum, which terminates because depths
                        // are non-negative, but it may not raise the maximum: one round of a
                        // recursive call leaves its return address behind, so following the
                        // growth would never converge.
                        if depth < bounds.min {
                            bounds.min = depth;
                            pending.push((target, depth));
                        } else if depth > bounds.max && !cycle_edges.contains(&(block_id, target)) {
                            bounds.max = depth;
                            pending.push((target, depth));
                        }
                    }
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

    fn verify_target_support(&self, module: &Module) {
        for (block_id, block) in module.blocks.iter_enumerated() {
            for inst in &block.instructions {
                if let Some(stack_op) = inst.as_stack_op() {
                    if stack_op.lowering(self.evm_version).is_none() {
                        self.error_in_block(
                            block_id,
                            format_args!("`{}` requires Amsterdam-compatible EVM", inst.mnemonic()),
                        );
                    }
                } else {
                    self.verify_opcode(block_id, inst.opcode);
                }
            }
            if let Some(Terminator { kind: TerminatorKind::Op(opcode), .. }) = &block.terminator {
                self.verify_opcode(block_id, *opcode);
            }
        }
    }

    fn verify_stack_ops_for_evm_version(&self, module: &Module) {
        for (block_id, block) in module.blocks.iter_enumerated() {
            for inst in &block.instructions {
                if let Some(stack_op) = inst.as_stack_op()
                    && stack_op.lowering(self.evm_version).is_none()
                {
                    self.error_in_block(
                        block_id,
                        format_args!("`{}` requires Amsterdam-compatible EVM", inst.mnemonic()),
                    );
                }
            }
        }
    }

    fn verify_opcode(&self, block: BlockId, opcode: u8) {
        if !op::is_available(opcode, self.evm_version) {
            let name = op::mnemonic(opcode).unwrap_or("unknown");
            self.error_in_block(
                block,
                format_args!("opcode `{name}` is unavailable for `{}` EVM", self.evm_version),
            );
        }
    }

    fn block_exists(&self, module: &Module, block: BlockId) -> bool {
        block.index() < module.blocks.len()
    }

    pub(super) fn is_valid(module: &Module) -> bool {
        let dcx = DiagCtxt::with_silent_emitter(None);
        Verifier::for_evm_version(&dcx, EvmVersion::Osaka).verify_module(module);
        dcx.has_errors().is_ok()
    }
}

/// The range of stack depths a block has been entered with by the stack-operation walk.
#[derive(Clone, Copy, Debug)]
struct DepthBounds {
    min: usize,
    max: usize,
}

impl DepthBounds {
    /// Bounds for a block entered at a single depth so far.
    const fn exact(depth: usize) -> Self {
        Self { min: depth, max: depth }
    }
}

/// Appends every direct control-flow successor of `block`, in walk order.
///
/// The `push bbN; jumpi` pair is a physical conditional branch inside a block, so its target is
/// an edge as well. Dynamic jumps through a stack value, which is how an internal function
/// returns, carry no target and therefore no edge.
fn direct_successors(block: &Block, out: &mut Vec<BlockId>) {
    for (index, inst) in block.instructions.iter().enumerate() {
        if inst.opcode == op::JUMPI
            && let Some(target) =
                index.checked_sub(1).and_then(|index| block.instructions[index].pushed_block())
        {
            out.push(target);
        }
    }
    if let Some(term) = &block.terminator {
        term.kind.visit_targets(|target| out.push(target));
    }
}

/// Returns the edges that close a cycle in a depth-first walk from the entry block.
///
/// Removing them leaves an acyclic graph, which is what bounds the stack-depth walk. Only edges
/// reachable from the entry are classified; the walk never leaves that region either.
fn cycle_edges(module: &Module) -> FxHashSet<(BlockId, BlockId)> {
    /// Depth-first states: not yet reached, on the current path, and fully walked.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        Unseen,
        OnPath,
        Done,
    }

    let mut edges = FxHashSet::default();
    if module.blocks.is_empty() {
        return edges;
    }
    let mut states = IndexVec::<BlockId, _>::from_vec(vec![State::Unseen; module.blocks.len()]);
    // Successors of every block on the current path, each frame owning the tail of the buffer
    // from its recorded start.
    let mut successors = Vec::new();
    let mut path = vec![(BlockId::ENTRY, 0)];
    states[BlockId::ENTRY] = State::OnPath;
    direct_successors(&module.blocks[BlockId::ENTRY], &mut successors);
    while let Some(&(block_id, start)) = path.last() {
        if successors.len() == start {
            states[block_id] = State::Done;
            path.pop();
            continue;
        }
        let target = successors.pop().expect("frame owns the buffer tail");
        match states[target] {
            State::Unseen => {
                states[target] = State::OnPath;
                path.push((target, successors.len()));
                direct_successors(&module.blocks[target], &mut successors);
            }
            State::OnPath => {
                edges.insert((block_id, target));
            }
            State::Done => {}
        }
    }
    edges
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
            Verifier::for_evm_version(&dcx, EvmVersion::Osaka).verify_module(&module);
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
        Verifier::for_evm_version(&osaka, EvmVersion::Osaka).verify_before_pipeline(&module);
        assert_eq!(osaka.err_count(), 1);

        let amsterdam = DiagCtxt::with_silent_emitter(None);
        Verifier::for_evm_version(&amsterdam, EvmVersion::Amsterdam)
            .verify_before_pipeline(&module);
        assert_eq!(amsterdam.err_count(), 0);

        let osaka = DiagCtxt::with_silent_emitter(None);
        Verifier::for_evm_version(&osaka, EvmVersion::Osaka).verify_after_legalization(&module);
        assert_eq!(osaka.err_count(), 2);

        let amsterdam = DiagCtxt::with_silent_emitter(None);
        Verifier::for_evm_version(&amsterdam, EvmVersion::Amsterdam)
            .verify_after_legalization(&module);
        assert_eq!(amsterdam.err_count(), 0);
    }
}

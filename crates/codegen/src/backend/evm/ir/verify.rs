//! EVM IR verifier.

use super::*;
use crate::backend::evm::assembler::op;
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    index::IndexVec,
    map::FxHashSet,
};
use solar_interface::{
    diagnostics::{DiagCtxt, ErrorGuaranteed},
    kw, sym,
};
use std::fmt;

/// Stateful EVM IR verifier.
struct Verifier<'a> {
    dcx: &'a DiagCtxt,
}

impl<'a> Verifier<'a> {
    /// Creates a verifier that emits findings into `dcx`.
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

    /// Verifies basic EVM IR invariants.
    fn verify_module(&self, module: &Module) {
        let errors_before = self.dcx.err_count();
        if !solar_parse::lexer::is_ident(module.name.as_str()) {
            self.error(format_args!("invalid program name `{}`", module.name));
        }
        if module.blocks.is_empty() {
            self.error("program has no blocks");
            return;
        }
        let entry = match module.entry_block {
            Some(entry) if self.block_exists(module, entry) => Some(entry),
            Some(entry) => {
                self.error(format_args!("entry block `{}` is out of range", entry.index()));
                None
            }
            None => {
                self.error("program has no entry block");
                None
            }
        };

        let mut labels = FxHashSet::default();
        for (block_id, block) in module.blocks.iter_enumerated() {
            if !labels.insert(block.label) {
                self.error_in_block(
                    block_id,
                    format_args!("duplicate block label `bb{}`", block.label),
                );
            }
            if block.terminator.is_none() {
                self.error_in_block(block_id, "missing terminator");
            }
        }

        let mut value_names = FxHashSet::default();
        for (_, value) in module.values.iter_enumerated() {
            if !solar_parse::lexer::is_ident(value.name.as_str()) {
                self.error(format_args!("invalid value name `%{}`", value.name));
            }
            if !value_names.insert(value.name) {
                self.error(format_args!("duplicate value name `%{}`", value.name));
            }
        }

        let mut defined_values = DenseBitSet::new_empty(module.values.len());
        for (block_id, block) in module.blocks.iter_enumerated() {
            for inst in &block.instructions {
                self.verify_instruction_shape(block_id, inst);
                if let Some(result) = inst.result {
                    if !self.value_exists(module, result) {
                        self.error_in_block(
                            block_id,
                            format_args!("result value `{}` is out of range", result.index()),
                        );
                    } else if !defined_values.insert(result) {
                        self.error_in_block(
                            block_id,
                            format_args!(
                                "value `%{}` is defined more than once",
                                module.value(result).name
                            ),
                        );
                    }
                }
                for operand in &inst.operands {
                    self.verify_operand(block_id, module, operand);
                }
                self.verify_metadata_is_untyped(block_id, &inst.metadata);
            }
            let Some(term) = &block.terminator else { continue };
            self.verify_terminator_shape(block_id, &term.kind);
            visit_terminator_operands(&term.kind, |operand| {
                self.verify_operand(block_id, module, operand);
                Ok::<(), ()>(())
            })
            .unwrap();
            self.verify_metadata_is_untyped(block_id, &term.metadata);
            visit_terminator_targets(&term.kind, |target| {
                if !self.block_exists(module, target) {
                    self.error_in_block(
                        block_id,
                        format_args!("target block `{}` is out of range", target.index()),
                    );
                }
                Ok::<(), ()>(())
            })
            .unwrap();
        }

        for (block_id, block) in module.blocks.iter_enumerated() {
            for &value in &block.entry_stack {
                if !self.value_exists(module, value) {
                    self.error_in_block(
                        block_id,
                        format_args!("entry stack value `{}` is out of range", value.index()),
                    );
                } else if !defined_values.contains(value) {
                    self.error_in_block(
                        block_id,
                        format_args!(
                            "entry stack value `%{}` is never defined",
                            module.value(value).name
                        ),
                    );
                }
            }
            for inst in &block.instructions {
                for operand in &inst.operands {
                    self.verify_value_defined(block_id, module, operand, &defined_values);
                }
            }
            let Some(term) = &block.terminator else { continue };
            visit_terminator_operands(&term.kind, |operand| {
                self.verify_value_defined(block_id, module, operand, &defined_values);
                Ok::<(), ()>(())
            })
            .unwrap();
        }

        if entry.is_some() && self.dcx.err_count() == errors_before {
            self.verify_stack_consistency(module);
        }
    }
}

pub(super) fn validate(dcx: &DiagCtxt, module: &Module) {
    Verifier::new(dcx).verify_module(module);
}

/// One abstract stack word tracked by the consistency simulator.
///
/// Words carry their value identity when known so cross-block edges can compare
/// the exact words a predecessor leaves with those a successor declares. Words
/// produced by `push` or by an extra output of a multi-result op have no SSA
/// name and are modeled as [`AbstractWord::Unknown`]; two `Unknown` words are
/// never considered equal across an edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AbstractWord {
    /// A word with a known SSA value identity.
    Value(ValueId),
    /// An anonymous word (a `push` immediate or a synthesized output) whose
    /// identity is not an SSA value.
    Unknown,
}

/// An abstract model stack: known words (top first) over an optional implicit
/// floor of predecessor-inherited words.
///
/// EVM basic blocks share one runtime stack. A block that is reachable from
/// predecessors may consume words those predecessors left below the portion the
/// block declares as its `entry_stack`. The production backend exploits this:
/// it does not declare an `entry_stack` and models opcodes as stack-neutral,
/// relying on physical `push`/`dup`/`swap`/`pop` to thread an implicitly
/// inherited stack between blocks. To stay sound without rejecting that
/// convention, non-entry blocks model an **unbounded floor** of unknown words
/// below `words`: an op that reaches past the known words draws fresh
/// [`AbstractWord::Unknown`] floor words instead of underflowing.
///
/// The entry block has no predecessors, so its floor is empty (`infinite_floor`
/// is `false`) and physical-op underflow is a real error and is rejected.
struct ModelStack {
    /// Known words, index 0 is the top of stack.
    words: Vec<AbstractWord>,
    /// Whether unknown words may be drawn from below `words`.
    infinite_floor: bool,
}

impl ModelStack {
    fn len(&self) -> usize {
        self.words.len()
    }

    /// Ensures at least `depth` words are modeled, materializing implicit floor
    /// words when allowed. Returns `false` if the stack is genuinely too shallow.
    fn ensure_depth(&mut self, depth: usize) -> bool {
        if self.words.len() >= depth {
            return true;
        }
        if !self.infinite_floor {
            return false;
        }
        while self.words.len() < depth {
            self.words.push(AbstractWord::Unknown);
        }
        true
    }

    fn push(&mut self, word: AbstractWord) {
        self.words.insert(0, word);
    }

    fn contains(&self, word: AbstractWord) -> bool {
        // A live word may be sitting in the implicit floor; over-approximate by
        // treating any value as reachable when a floor is present.
        self.words.contains(&word) || self.infinite_floor
    }
}

/// Simulates each block's stack and checks cross-block edge consistency.
///
/// For every block we start from its declared `entry_stack` (top first), apply
/// each instruction's stack effect to a `ModelStack` of word identities, apply
/// the terminator's effect, and record the resulting exit stack. Physical stack
/// ops (`dupN`/`swapN`/`pop`) are applied precisely; on the entry block (which
/// has no implicit floor) underflows and out-of-range depths are rejected.
///
/// Then, for every CFG edge `pred -> succ`, the successor's declared
/// `entry_stack` must be a **prefix** of the predecessor's exit stack (both top
/// first): the words a successor declares as incoming are exactly the top `k`
/// words the predecessor leaves, in order. The predecessor may leave additional
/// words below them — a successor only names the prefix it consumes. The entry
/// block must start from an empty stack.
impl Verifier<'_> {
    fn verify_stack_consistency(&self, module: &Module) {
        if let Some(entry) = module.entry_block
            && !module.blocks[entry].entry_stack.is_empty()
        {
            self.error_in_block(entry, "entry block must start from an empty stack");
        }

        let mut exit_stacks: IndexVec<BlockId, Option<Vec<AbstractWord>>> =
            IndexVec::with_capacity(module.blocks.len());
        for (block_id, block) in module.blocks.iter_enumerated() {
            let is_entry = module.entry_block == Some(block_id);
            exit_stacks.push(self.simulate_block(module, block_id, block, is_entry).ok());
        }

        for (block_id, block) in module.blocks.iter_enumerated() {
            let Some(exit) = &exit_stacks[block_id] else { continue };
            let term = block.terminator.as_ref().expect("checked above");
            visit_terminator_targets(&term.kind, |succ| {
                let succ_entry: Vec<AbstractWord> = module.blocks[succ]
                    .entry_stack
                    .iter()
                    .map(|&value| AbstractWord::Value(value))
                    .collect();
                if !exit.starts_with(&succ_entry) {
                    self.error_in_block(
                        block_id,
                        format_args!(
                            "stack on edge to `{}` is inconsistent: successor declares incoming \
                                             stack [{}] but predecessor leaves [{}]",
                            format_args!("bb{}", module.blocks[succ].label),
                            self.format_entry_stack(module, &module.blocks[succ].entry_stack),
                            self.format_abstract_stack(module, exit),
                        ),
                    );
                }
                Ok::<(), ()>(())
            })
            .unwrap();
        }
    }

    /// Computes a block's exit stack, rejecting any entry-block physical-stack-op
    /// underflow or out-of-range depth and any reference to a word not live on the
    /// model stack.
    fn simulate_block(
        &self,
        module: &Module,
        block_id: BlockId,
        block: &Block,
        is_entry: bool,
    ) -> Result<Vec<AbstractWord>, ErrorGuaranteed> {
        let mut stack = ModelStack {
            words: block.entry_stack.iter().map(|&value| AbstractWord::Value(value)).collect(),
            infinite_floor: !is_entry,
        };

        for inst in &block.instructions {
            self.simulate_instruction(module, block_id, inst, &mut stack)?;
        }

        let term = block.terminator.as_ref().expect("checked above");
        self.simulate_terminator(module, block_id, &term.kind, &mut stack)?;
        Ok(stack.words)
    }

    fn simulate_instruction(
        &self,
        module: &Module,
        block_id: BlockId,
        inst: &Instruction,
        stack: &mut ModelStack,
    ) -> Result<(), ErrorGuaranteed> {
        if inst.is_physical_stack_op() {
            self.apply_physical_stack_op(block_id, inst.opcode, stack)
        } else if inst.is_encoded_push() {
            // An encoded `push` adds one word: its SSA result if it has one,
            // otherwise an anonymous immediate word.
            stack.push(self.result_word(inst));
            Ok(())
        } else if !inst.operands.is_empty() {
            // Unscheduled op: its value operands are still present, so they must
            // be live on the model stack. They are not consumed (the operands
            // sit on the stack until scheduling clears them); the result, if
            // any, is pushed on top.
            for operand in &inst.operands {
                if let Operand::Value(value) = operand
                    && !stack.contains(AbstractWord::Value(*value))
                {
                    return Err(self.error_in_block(
                        block_id,
                        format_args!(
                            "operand `%{}` of `{}` is not live on the stack",
                            module.value(*value).name,
                            inst.mnemonic()
                        ),
                    ));
                }
            }
            if inst.result.is_some() {
                stack.push(self.result_word(inst));
            }
            Ok(())
        } else {
            // Scheduled op: operands cleared. Pop its declared inputs and push
            // its outputs.
            let effect =
                inst.metadata.stack.unwrap_or_else(|| default_instruction_stack_effect(inst));
            self.apply_effect(block_id, inst, effect, stack)
        }
    }

    fn apply_effect(
        &self,
        block_id: BlockId,
        inst: &Instruction,
        effect: StackEffect,
        stack: &mut ModelStack,
    ) -> Result<(), ErrorGuaranteed> {
        let inputs = usize::from(effect.inputs);
        if !stack.ensure_depth(inputs) {
            return Err(self.error_in_block(
                block_id,
                format_args!(
                    "`{}` consumes {} stack words but only {} are available",
                    inst.mnemonic(),
                    effect.inputs,
                    stack.len()
                ),
            ));
        }
        stack.words.drain(0..inputs);
        for index in 0..effect.outputs {
            let word = if index == 0 { self.result_word(inst) } else { AbstractWord::Unknown };
            stack.push(word);
        }
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
                let depth = usize::from(n);
                if !stack.ensure_depth(depth) {
                    return Err(self.error_in_block(
                        block_id,
                        format_args!(
                            "`dup{n}` reaches depth {n} but the stack has {}",
                            stack.len()
                        ),
                    ));
                }
                let word = stack.words[depth - 1];
                stack.push(word);
            }
            op::SWAP1..=op::SWAP16 => {
                let n = opcode - op::SWAP1 + 1;
                let depth = usize::from(n);
                if !stack.ensure_depth(depth + 1) {
                    return Err(self.error_in_block(
                        block_id,
                        format_args!(
                            "`swap{n}` reaches depth {n} but the stack has {}",
                            stack.len()
                        ),
                    ));
                }
                stack.words.swap(0, depth);
            }
            op::POP => {
                if !stack.ensure_depth(1) {
                    return Err(self.error_in_block(block_id, "`pop` on an empty stack"));
                }
                stack.words.remove(0);
            }
            _ => unreachable!("checked physical stack opcode"),
        }
        Ok(())
    }

    fn simulate_terminator(
        &self,
        module: &Module,
        block_id: BlockId,
        kind: &TerminatorKind,
        stack: &mut ModelStack,
    ) -> Result<(), ErrorGuaranteed> {
        // A terminator that still carries value operands is unscheduled: those
        // operands must be live. We still apply the terminator's stack effect to the
        // abstract exit stack: even in virtual form, branch/switch/return/revert
        // consume their operand words at runtime, so successors must not be allowed
        // to claim those consumed words as incoming stack values.
        let mut result = Ok(());
        visit_terminator_operands(kind, |operand| {
            if let Operand::Value(value) = operand
                && !stack.contains(AbstractWord::Value(*value))
            {
                result = Err(self.error_in_block(
                    block_id,
                    format_args!(
                        "terminator operand `%{}` is not live on the stack",
                        module.value(*value).name
                    ),
                ));
            }
            Ok::<(), ErrorGuaranteed>(())
        })?;
        result?;

        self.apply_terminator_effect(block_id, kind, stack)
    }

    fn apply_terminator_effect(
        &self,
        block_id: BlockId,
        kind: &TerminatorKind,
        stack: &mut ModelStack,
    ) -> Result<(), ErrorGuaranteed> {
        let mut consumed = GrowableBitSet::new_empty();
        visit_terminator_operands(kind, |operand| {
            if let Operand::Value(value) = operand
                && consumed.insert(*value)
            {
                self.consume_stack_value(block_id, kind, *value, stack)?;
            }
            Ok::<(), ErrorGuaranteed>(())
        })?;

        let effect = default_terminator_stack_effect(kind);
        let remaining_inputs = usize::from(effect.inputs).saturating_sub(consumed.len());
        if !stack.ensure_depth(remaining_inputs) {
            return Err(self.error_in_block(
                block_id,
                format_args!(
                    "`{}` consumes {} stack words but only {} are available",
                    self.terminator_name(kind),
                    effect.inputs,
                    stack.len()
                ),
            ));
        }
        stack.words.drain(0..remaining_inputs);
        Ok(())
    }

    fn consume_stack_value(
        &self,
        block_id: BlockId,
        kind: &TerminatorKind,
        value: ValueId,
        stack: &mut ModelStack,
    ) -> Result<(), ErrorGuaranteed> {
        let needle = AbstractWord::Value(value);
        let Some(index) = stack.words.iter().position(|word| *word == needle) else {
            return Err(self.error_in_block(
                block_id,
                format_args!(
                    "`{}` consumes an operand that is not live on the stack",
                    self.terminator_name(kind)
                ),
            ));
        };
        stack.words.remove(index);
        Ok(())
    }

    fn terminator_name(&self, kind: &TerminatorKind) -> &'static str {
        match kind {
            TerminatorKind::Jump(_) => "jump",
            TerminatorKind::Branch { .. } => "br",
            TerminatorKind::Switch { .. } => "switch",
            TerminatorKind::Return { .. } => "return",
            TerminatorKind::Revert { .. } => "revert",
            TerminatorKind::Stop => "stop",
            TerminatorKind::Invalid => "invalid",
            TerminatorKind::SelfDestruct { .. } => "selfdestruct",
            TerminatorKind::RawOpcode(_) => "terminal",
        }
    }

    /// The word a result-producing instruction leaves on top.
    fn result_word(&self, inst: &Instruction) -> AbstractWord {
        inst.result.map(AbstractWord::Value).unwrap_or(AbstractWord::Unknown)
    }

    fn format_entry_stack<'a>(
        &self,
        module: &'a Module,
        stack: &'a [ValueId],
    ) -> impl fmt::Display + 'a {
        fmt::from_fn(move |f| {
            for (index, &value) in stack.iter().enumerate() {
                if index != 0 {
                    f.write_str(", ")?;
                }
                write!(f, "%{}", module.value(value).name)?;
            }
            Ok(())
        })
    }

    fn format_abstract_stack<'a>(
        &self,
        module: &'a Module,
        stack: &'a [AbstractWord],
    ) -> impl fmt::Display + 'a {
        fmt::from_fn(move |f| {
            for (index, word) in stack.iter().enumerate() {
                if index != 0 {
                    f.write_str(", ")?;
                }
                match word {
                    AbstractWord::Value(value) => write!(f, "%{}", module.value(*value).name)?,
                    AbstractWord::Unknown => f.write_str("<word>")?,
                }
            }
            Ok(())
        })
    }

    fn verify_instruction_shape(&self, block_id: BlockId, inst: &Instruction) {
        if inst.is_physical_stack_op() {
            let expected = default_instruction_stack_effect(inst);
            if inst.result.is_some() {
                self.error_in_block(
                    block_id,
                    format_args!(
                        "physical stack op `{}` cannot define an SSA value",
                        inst.mnemonic()
                    ),
                );
            }
            if !inst.operands.is_empty() {
                self.error_in_block(
                    block_id,
                    format_args!("physical stack op `{}` cannot have operands", inst.mnemonic()),
                );
            }
            if let Some(effect) = inst.metadata.stack
                && effect != expected
            {
                self.error_in_block(
                    block_id,
                    format_args!(
                        "physical stack op `{}` has stack effect {}->{}, expected {}->{}",
                        inst.mnemonic(),
                        effect.inputs,
                        effect.outputs,
                        expected.inputs,
                        expected.outputs
                    ),
                );
            }
        } else if inst.is_encoded_push() {
            if inst.operands.len() != 1 {
                self.error_in_block(
                    block_id,
                    format_args!("`{}` must have one operand", inst.mnemonic()),
                );
            } else if matches!(inst.operands[0], Operand::Value(_)) {
                self.error_in_block(
                    block_id,
                    format_args!("`{}` cannot take a stack value operand", inst.mnemonic()),
                );
            }
        } else {
            if inst.operands.is_empty()
                && inst.metadata.stack.is_none()
                && op::stack_io(inst.opcode).is_none()
            {
                self.error_in_block(
                    block_id,
                    format_args!(
                        "operand-cleared instruction `{}` must declare an explicit stack effect",
                        inst.mnemonic()
                    ),
                );
            }
            for operand in &inst.operands {
                if !matches!(operand, Operand::Value(_)) {
                    self.error_in_block(
                        block_id,
                        "non-`push` instruction operands must be stack values",
                    );
                }
            }
        }
    }

    fn verify_terminator_shape(&self, block_id: BlockId, kind: &TerminatorKind) {
        match kind {
            TerminatorKind::Branch { condition, .. } => {
                self.verify_stack_value_operand(block_id, condition, "branch condition")
            }
            TerminatorKind::Switch { value, cases, .. } => {
                self.verify_stack_value_operand(block_id, value, "switch value");
                for (case, _) in cases {
                    if !matches!(case, Operand::Immediate(_)) {
                        self.error_in_block(block_id, "switch case values must be immediates");
                    }
                }
            }
            TerminatorKind::Return { offset, size } | TerminatorKind::Revert { offset, size } => {
                self.verify_stack_value_operand(block_id, offset, "memory offset");
                self.verify_stack_value_operand(block_id, size, "memory size");
            }
            TerminatorKind::SelfDestruct { recipient } => {
                self.verify_stack_value_operand(block_id, recipient, "selfdestruct recipient")
            }
            TerminatorKind::Jump(_)
            | TerminatorKind::Stop
            | TerminatorKind::Invalid
            | TerminatorKind::RawOpcode(_) => {}
        }
    }

    fn verify_stack_value_operand(&self, block_id: BlockId, operand: &Operand, what: &str) {
        if !matches!(operand, Operand::Value(_)) {
            self.error_in_block(block_id, format_args!("{what} must be a stack value"));
        }
    }

    fn verify_metadata_is_untyped(&self, block_id: BlockId, metadata: &Metadata) {
        for item in &metadata.attrs {
            if matches!(item.key, kw::Type | sym::mir_type | sym::result_ty | sym::ty) {
                self.error_in_block(
                    block_id,
                    format_args!("EVM IR is untyped; metadata key `{}` is not allowed", item.key),
                );
            }
        }
    }

    fn verify_operand(&self, block_id: BlockId, module: &Module, operand: &Operand) {
        match operand {
            Operand::Value(value) if !self.value_exists(module, *value) => {
                self.error_in_block(
                    block_id,
                    format_args!("value `{}` is out of range", value.index()),
                );
            }
            Operand::Block(block) if !self.block_exists(module, *block) => {
                self.error_in_block(
                    block_id,
                    format_args!("block `{}` is out of range", block.index()),
                );
            }
            _ => {}
        }
    }

    fn verify_value_defined(
        &self,
        block_id: BlockId,
        module: &Module,
        operand: &Operand,
        defined_values: &DenseBitSet<ValueId>,
    ) {
        if let Operand::Value(value) = operand
            && self.value_exists(module, *value)
            && !defined_values.contains(*value)
        {
            self.error_in_block(
                block_id,
                format_args!("value `%{}` is used but never defined", module.value(*value).name),
            );
        }
    }

    fn block_exists(&self, module: &Module, block: BlockId) -> bool {
        block.index() < module.blocks.len()
    }

    fn value_exists(&self, module: &Module, value: ValueId) -> bool {
        value.index() < module.values.len()
    }
}

fn visit_terminator_operands<E>(
    kind: &TerminatorKind,
    mut visit: impl FnMut(&Operand) -> Result<(), E>,
) -> Result<(), E> {
    match kind {
        TerminatorKind::Jump(_)
        | TerminatorKind::Stop
        | TerminatorKind::Invalid
        | TerminatorKind::RawOpcode(_) => {}
        TerminatorKind::Branch { condition, .. } => visit(condition)?,
        TerminatorKind::Switch { value, cases, .. } => {
            visit(value)?;
            for (case, _) in cases {
                visit(case)?;
            }
        }
        TerminatorKind::Return { offset, size } | TerminatorKind::Revert { offset, size } => {
            visit(offset)?;
            visit(size)?;
        }
        TerminatorKind::SelfDestruct { recipient } => visit(recipient)?,
    }
    Ok(())
}

fn visit_terminator_targets<E>(
    kind: &TerminatorKind,
    mut visit: impl FnMut(BlockId) -> Result<(), E>,
) -> Result<(), E> {
    match kind {
        TerminatorKind::Jump(target) => visit(*target)?,
        TerminatorKind::Branch { then_block, else_block, .. } => {
            visit(*then_block)?;
            visit(*else_block)?;
        }
        TerminatorKind::Switch { default, cases, .. } => {
            visit(*default)?;
            for (_, target) in cases {
                visit(*target)?;
            }
        }
        TerminatorKind::Return { .. }
        | TerminatorKind::Revert { .. }
        | TerminatorKind::Stop
        | TerminatorKind::Invalid
        | TerminatorKind::SelfDestruct { .. }
        | TerminatorKind::RawOpcode(_) => {}
    }
    Ok(())
}

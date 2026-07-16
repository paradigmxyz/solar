//! EVM IR verifier.

use super::*;
use solar_data_structures::{index::IndexVec, map::FxHashSet};
use std::fmt as std_fmt;

/// An error produced while validating EVM IR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmIrVerifyError {
    /// Human-readable validation failure.
    pub msg: String,
}

impl EvmIrVerifyError {
    fn new(msg: impl Into<String>) -> Self {
        Self { msg: msg.into() }
    }

    fn in_block(block: EvmIrBlockId, msg: impl Into<String>) -> Self {
        Self::new(format!("block {}: {}", block.index(), msg.into()))
    }
}

impl std_fmt::Display for EvmIrVerifyError {
    fn fmt(&self, f: &mut std_fmt::Formatter<'_>) -> std_fmt::Result {
        write!(f, "EVM IR verification failed: {}", self.msg)
    }
}

impl std::error::Error for EvmIrVerifyError {}

/// Verifies basic EVM IR invariants.
///
/// # Errors
///
/// Returns an [`EvmIrVerifyError`] if the module contains invalid references,
/// duplicate definitions, missing terminators, or malformed identifiers.
pub fn verify_evm_ir_module(module: &EvmIrModule) -> Result<(), EvmIrVerifyError> {
    if !is_valid_ident(&module.name) {
        return Err(EvmIrVerifyError::new(format!("invalid program name `{}`", module.name)));
    }
    if module.blocks.is_empty() {
        return Err(EvmIrVerifyError::new("program has no blocks"));
    }
    let Some(entry) = module.entry_block else {
        return Err(EvmIrVerifyError::new("program has no entry block"));
    };
    if !block_exists(module, entry) {
        return Err(EvmIrVerifyError::new(format!(
            "entry block `{}` is out of range",
            entry.index()
        )));
    }

    let mut labels = FxHashSet::default();
    for (block_id, block) in module.blocks.iter_enumerated() {
        if !is_valid_block_label(&block.label) {
            return Err(EvmIrVerifyError::in_block(
                block_id,
                format!("invalid block label `{}`", block.label),
            ));
        }
        if !labels.insert(block.label.as_str()) {
            return Err(EvmIrVerifyError::in_block(
                block_id,
                format!("duplicate block label `{}`", block.label),
            ));
        }
        if block.terminator.is_none() {
            return Err(EvmIrVerifyError::in_block(block_id, "missing terminator"));
        }
    }

    let mut value_names = FxHashSet::default();
    for (_, value) in module.values.iter_enumerated() {
        if !is_valid_value_name(&value.name) {
            return Err(EvmIrVerifyError::new(format!("invalid value name `%{}`", value.name)));
        }
        if !value_names.insert(value.name.as_str()) {
            return Err(EvmIrVerifyError::new(format!("duplicate value name `%{}`", value.name)));
        }
    }

    let mut defined_values = FxHashSet::default();
    for (block_id, block) in module.blocks.iter_enumerated() {
        for inst in &block.instructions {
            verify_instruction_shape(block_id, inst)?;
            if let Some(result) = inst.result {
                if !value_exists(module, result) {
                    return Err(EvmIrVerifyError::in_block(
                        block_id,
                        format!("result value `{}` is out of range", result.index()),
                    ));
                }
                if !defined_values.insert(result) {
                    return Err(EvmIrVerifyError::in_block(
                        block_id,
                        format!("value `%{}` is defined more than once", module.value(result).name),
                    ));
                }
            }
            for operand in &inst.operands {
                verify_operand(block_id, module, operand)?;
            }
            verify_metadata_is_untyped(block_id, &inst.metadata)?;
        }
        let term = block.terminator.as_ref().expect("checked above");
        verify_terminator_shape(block_id, &term.kind)?;
        visit_terminator_operands(&term.kind, |operand| {
            verify_operand(block_id, module, operand)?;
            Ok(())
        })?;
        visit_terminator_targets(&term.kind, |target| {
            if !block_exists(module, target) {
                return Err(EvmIrVerifyError::in_block(
                    block_id,
                    format!("target block `{}` is out of range", target.index()),
                ));
            }
            Ok(())
        })?;
        verify_metadata_is_untyped(block_id, &term.metadata)?;
    }

    for (block_id, block) in module.blocks.iter_enumerated() {
        for &value in &block.entry_stack {
            if !value_exists(module, value) {
                return Err(EvmIrVerifyError::in_block(
                    block_id,
                    format!("entry stack value `{}` is out of range", value.index()),
                ));
            }
            if !defined_values.contains(&value) {
                return Err(EvmIrVerifyError::in_block(
                    block_id,
                    format!("entry stack value `%{}` is never defined", module.value(value).name),
                ));
            }
        }
        for inst in &block.instructions {
            for operand in &inst.operands {
                verify_value_defined(block_id, module, operand, &defined_values)?;
            }
        }
        let term = block.terminator.as_ref().expect("checked above");
        visit_terminator_operands(&term.kind, |operand| {
            verify_value_defined(block_id, module, operand, &defined_values)?;
            Ok(())
        })?;
    }

    verify_stack_consistency(module)?;

    Ok(())
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
    Value(EvmIrValueId),
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
/// each instruction's stack effect to a [`ModelStack`] of word identities, apply
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
fn verify_stack_consistency(module: &EvmIrModule) -> Result<(), EvmIrVerifyError> {
    if let Some(entry) = module.entry_block
        && !module.blocks[entry].entry_stack.is_empty()
    {
        return Err(EvmIrVerifyError::in_block(
            entry,
            "entry block must start from an empty stack",
        ));
    }

    let mut exit_stacks: IndexVec<EvmIrBlockId, Vec<AbstractWord>> =
        IndexVec::with_capacity(module.blocks.len());
    for (block_id, block) in module.blocks.iter_enumerated() {
        let is_entry = module.entry_block == Some(block_id);
        exit_stacks.push(simulate_block(module, block_id, block, is_entry)?);
    }

    for (block_id, block) in module.blocks.iter_enumerated() {
        let exit = &exit_stacks[block_id];
        let term = block.terminator.as_ref().expect("checked above");
        let mut result = Ok(());
        visit_terminator_targets(&term.kind, |succ| {
            let succ_entry: Vec<AbstractWord> = module.blocks[succ]
                .entry_stack
                .iter()
                .map(|&value| AbstractWord::Value(value))
                .collect();
            if !exit.starts_with(&succ_entry) {
                result = Err(EvmIrVerifyError::in_block(
                    block_id,
                    format!(
                        "stack on edge to `{}` is inconsistent: successor declares incoming \
                         stack [{}] but predecessor leaves [{}]",
                        module.blocks[succ].label,
                        format_entry_stack(module, &module.blocks[succ].entry_stack),
                        format_abstract_stack(module, exit),
                    ),
                ));
            }
            Ok::<(), EvmIrVerifyError>(())
        })?;
        result?;
    }

    Ok(())
}

/// Computes a block's exit stack, rejecting any entry-block physical-stack-op
/// underflow or out-of-range depth and any reference to a word not live on the
/// model stack.
fn simulate_block(
    module: &EvmIrModule,
    block_id: EvmIrBlockId,
    block: &EvmIrBlock,
    is_entry: bool,
) -> Result<Vec<AbstractWord>, EvmIrVerifyError> {
    let mut stack = ModelStack {
        words: block.entry_stack.iter().map(|&value| AbstractWord::Value(value)).collect(),
        infinite_floor: !is_entry,
    };

    for inst in &block.instructions {
        simulate_instruction(module, block_id, inst, &mut stack)?;
    }

    let term = block.terminator.as_ref().expect("checked above");
    simulate_terminator(module, block_id, &term.kind, &mut stack)?;
    Ok(stack.words)
}

fn simulate_instruction(
    module: &EvmIrModule,
    block_id: EvmIrBlockId,
    inst: &EvmIrInstruction,
    stack: &mut ModelStack,
) -> Result<(), EvmIrVerifyError> {
    match &inst.kind {
        EvmIrInstructionKind::Stack(op) => apply_physical_stack_op(block_id, *op, stack),
        EvmIrInstructionKind::Operation(_) if is_encoded_push_instruction(inst) => {
            // An encoded `push` adds one word: its SSA result if it has one,
            // otherwise an anonymous immediate word.
            stack.push(result_word(inst));
            Ok(())
        }
        EvmIrInstructionKind::Operation(_) if !inst.operands.is_empty() => {
            // Unscheduled op: its value operands are still present, so they must
            // be live on the model stack. They are not consumed (the operands
            // sit on the stack until scheduling clears them); the result, if
            // any, is pushed on top.
            for operand in &inst.operands {
                if let EvmIrOperand::Value(value) = operand
                    && !stack.contains(AbstractWord::Value(*value))
                {
                    return Err(EvmIrVerifyError::in_block(
                        block_id,
                        format!(
                            "operand `%{}` of `{}` is not live on the stack",
                            module.value(*value).name,
                            inst.mnemonic()
                        ),
                    ));
                }
            }
            if inst.result.is_some() {
                stack.push(result_word(inst));
            }
            Ok(())
        }
        EvmIrInstructionKind::Operation(_) => {
            // Scheduled op: operands cleared. Pop its declared inputs and push
            // its outputs.
            let effect =
                inst.metadata.stack.unwrap_or_else(|| default_instruction_stack_effect(inst));
            apply_effect(block_id, inst, effect, stack)
        }
    }
}

fn apply_effect(
    block_id: EvmIrBlockId,
    inst: &EvmIrInstruction,
    effect: EvmIrStackEffect,
    stack: &mut ModelStack,
) -> Result<(), EvmIrVerifyError> {
    let inputs = usize::from(effect.inputs);
    if !stack.ensure_depth(inputs) {
        return Err(EvmIrVerifyError::in_block(
            block_id,
            format!(
                "`{}` consumes {} stack words but only {} are available",
                inst.mnemonic(),
                effect.inputs,
                stack.len()
            ),
        ));
    }
    stack.words.drain(0..inputs);
    for index in 0..effect.outputs {
        let word = if index == 0 { result_word(inst) } else { AbstractWord::Unknown };
        stack.push(word);
    }
    Ok(())
}

fn apply_physical_stack_op(
    block_id: EvmIrBlockId,
    op: EvmIrStackOp,
    stack: &mut ModelStack,
) -> Result<(), EvmIrVerifyError> {
    match op {
        EvmIrStackOp::Dup(n) => {
            let depth = usize::from(n);
            if !stack.ensure_depth(depth) {
                return Err(EvmIrVerifyError::in_block(
                    block_id,
                    format!("`dup{n}` reaches depth {n} but the stack has {}", stack.len()),
                ));
            }
            let word = stack.words[depth - 1];
            stack.push(word);
        }
        EvmIrStackOp::Swap(n) => {
            let depth = usize::from(n);
            if !stack.ensure_depth(depth + 1) {
                return Err(EvmIrVerifyError::in_block(
                    block_id,
                    format!("`swap{n}` reaches depth {n} but the stack has {}", stack.len()),
                ));
            }
            stack.words.swap(0, depth);
        }
        EvmIrStackOp::Pop => {
            if !stack.ensure_depth(1) {
                return Err(EvmIrVerifyError::in_block(block_id, "`pop` on an empty stack"));
            }
            stack.words.remove(0);
        }
    }
    Ok(())
}

fn simulate_terminator(
    module: &EvmIrModule,
    block_id: EvmIrBlockId,
    kind: &EvmIrTerminatorKind,
    stack: &mut ModelStack,
) -> Result<(), EvmIrVerifyError> {
    // A terminator that still carries value operands is unscheduled: those
    // operands must be live. We still apply the terminator's stack effect to the
    // abstract exit stack: even in virtual form, branch/switch/return/revert
    // consume their operand words at runtime, so successors must not be allowed
    // to claim those consumed words as incoming stack values.
    let mut result = Ok(());
    visit_terminator_operands(kind, |operand| {
        if let EvmIrOperand::Value(value) = operand
            && !stack.contains(AbstractWord::Value(*value))
        {
            result = Err(EvmIrVerifyError::in_block(
                block_id,
                format!(
                    "terminator operand `%{}` is not live on the stack",
                    module.value(*value).name
                ),
            ));
        }
        Ok::<(), EvmIrVerifyError>(())
    })?;
    result?;

    apply_terminator_effect(block_id, kind, stack)
}

fn apply_terminator_effect(
    block_id: EvmIrBlockId,
    kind: &EvmIrTerminatorKind,
    stack: &mut ModelStack,
) -> Result<(), EvmIrVerifyError> {
    let mut consumed = FxHashSet::default();
    visit_terminator_operands(kind, |operand| {
        if let EvmIrOperand::Value(value) = operand
            && consumed.insert(*value)
        {
            consume_stack_value(block_id, kind, *value, stack)?;
        }
        Ok::<(), EvmIrVerifyError>(())
    })?;

    let effect = default_terminator_stack_effect(kind);
    let remaining_inputs = usize::from(effect.inputs).saturating_sub(consumed.len());
    if !stack.ensure_depth(remaining_inputs) {
        return Err(EvmIrVerifyError::in_block(
            block_id,
            format!(
                "`{}` consumes {} stack words but only {} are available",
                terminator_name(kind),
                effect.inputs,
                stack.len()
            ),
        ));
    }
    stack.words.drain(0..remaining_inputs);
    Ok(())
}

fn consume_stack_value(
    block_id: EvmIrBlockId,
    kind: &EvmIrTerminatorKind,
    value: EvmIrValueId,
    stack: &mut ModelStack,
) -> Result<(), EvmIrVerifyError> {
    let needle = AbstractWord::Value(value);
    let Some(index) = stack.words.iter().position(|word| *word == needle) else {
        return Err(EvmIrVerifyError::in_block(
            block_id,
            format!(
                "`{}` consumes an operand that is not live on the stack",
                terminator_name(kind)
            ),
        ));
    };
    stack.words.remove(index);
    Ok(())
}

fn terminator_name(kind: &EvmIrTerminatorKind) -> &'static str {
    match kind {
        EvmIrTerminatorKind::Fallthrough(_) => "fallthrough",
        EvmIrTerminatorKind::FallthroughNext => "fallthrough_next",
        EvmIrTerminatorKind::Jump(_) => "jump",
        EvmIrTerminatorKind::Branch { .. } => "br",
        EvmIrTerminatorKind::Switch { .. } => "switch",
        EvmIrTerminatorKind::Return { .. } => "return",
        EvmIrTerminatorKind::Revert { .. } => "revert",
        EvmIrTerminatorKind::Stop => "stop",
        EvmIrTerminatorKind::Invalid => "invalid",
        EvmIrTerminatorKind::SelfDestruct { .. } => "selfdestruct",
        EvmIrTerminatorKind::RawOpcode(_) => "terminal",
    }
}

/// The word a result-producing instruction leaves on top.
fn result_word(inst: &EvmIrInstruction) -> AbstractWord {
    inst.result.map(AbstractWord::Value).unwrap_or(AbstractWord::Unknown)
}

fn format_entry_stack(module: &EvmIrModule, stack: &[EvmIrValueId]) -> String {
    stack
        .iter()
        .map(|&value| format!("%{}", module.value(value).name))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_abstract_stack(module: &EvmIrModule, stack: &[AbstractWord]) -> String {
    stack
        .iter()
        .map(|word| match word {
            AbstractWord::Value(value) => format!("%{}", module.value(*value).name),
            AbstractWord::Unknown => "<word>".to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn verify_instruction_shape(
    block_id: EvmIrBlockId,
    inst: &EvmIrInstruction,
) -> Result<(), EvmIrVerifyError> {
    if let EvmIrInstructionKind::Stack(op) = &inst.kind {
        let expected = op.stack_effect();
        if inst.result.is_some() {
            return Err(EvmIrVerifyError::in_block(
                block_id,
                format!("physical stack op `{}` cannot define an SSA value", op.mnemonic()),
            ));
        }
        if !inst.operands.is_empty() {
            return Err(EvmIrVerifyError::in_block(
                block_id,
                format!("physical stack op `{}` cannot have operands", op.mnemonic()),
            ));
        }
        if let Some(effect) = inst.metadata.stack
            && effect != expected
        {
            return Err(EvmIrVerifyError::in_block(
                block_id,
                format!(
                    "physical stack op `{}` has stack effect {}->{}, expected {}->{}",
                    op.mnemonic(),
                    effect.inputs,
                    effect.outputs,
                    expected.inputs,
                    expected.outputs
                ),
            ));
        }
    } else if is_encoded_push_instruction(inst) {
        if inst.operands.len() != 1 {
            return Err(EvmIrVerifyError::in_block(
                block_id,
                format!("`{}` must have one operand", inst.mnemonic()),
            ));
        }
        if matches!(inst.operands[0], EvmIrOperand::Value(_)) {
            return Err(EvmIrVerifyError::in_block(
                block_id,
                format!("`{}` cannot take a stack value operand", inst.mnemonic()),
            ));
        }
    } else {
        for operand in &inst.operands {
            if !matches!(operand, EvmIrOperand::Value(_)) {
                return Err(EvmIrVerifyError::in_block(
                    block_id,
                    "non-`push` instruction operands must be stack values",
                ));
            }
        }
    }
    Ok(())
}

fn verify_terminator_shape(
    block_id: EvmIrBlockId,
    kind: &EvmIrTerminatorKind,
) -> Result<(), EvmIrVerifyError> {
    match kind {
        EvmIrTerminatorKind::Branch { condition, .. } => {
            verify_stack_value_operand(block_id, condition, "branch condition")?
        }
        EvmIrTerminatorKind::Switch { value, cases, .. } => {
            verify_stack_value_operand(block_id, value, "switch value")?;
            for (case, _) in cases {
                if !matches!(case, EvmIrOperand::Immediate(_)) {
                    return Err(EvmIrVerifyError::in_block(
                        block_id,
                        "switch case values must be immediates",
                    ));
                }
            }
        }
        EvmIrTerminatorKind::Return { offset, size }
        | EvmIrTerminatorKind::Revert { offset, size } => {
            verify_stack_value_operand(block_id, offset, "memory offset")?;
            verify_stack_value_operand(block_id, size, "memory size")?;
        }
        EvmIrTerminatorKind::SelfDestruct { recipient } => {
            verify_stack_value_operand(block_id, recipient, "selfdestruct recipient")?
        }
        EvmIrTerminatorKind::Fallthrough(_)
        | EvmIrTerminatorKind::FallthroughNext
        | EvmIrTerminatorKind::Jump(_)
        | EvmIrTerminatorKind::Stop
        | EvmIrTerminatorKind::Invalid
        | EvmIrTerminatorKind::RawOpcode(_) => {}
    }
    Ok(())
}

fn verify_stack_value_operand(
    block_id: EvmIrBlockId,
    operand: &EvmIrOperand,
    what: &str,
) -> Result<(), EvmIrVerifyError> {
    if matches!(operand, EvmIrOperand::Value(_)) {
        return Ok(());
    }
    Err(EvmIrVerifyError::in_block(block_id, format!("{what} must be a stack value")))
}

fn verify_metadata_is_untyped(
    block_id: EvmIrBlockId,
    metadata: &EvmIrMetadata,
) -> Result<(), EvmIrVerifyError> {
    for item in &metadata.attrs {
        if matches!(item.key.as_str(), "type" | "ty" | "result_ty" | "mir_type") {
            return Err(EvmIrVerifyError::in_block(
                block_id,
                format!("EVM IR is untyped; metadata key `{}` is not allowed", item.key),
            ));
        }
    }
    Ok(())
}

fn verify_operand(
    block_id: EvmIrBlockId,
    module: &EvmIrModule,
    operand: &EvmIrOperand,
) -> Result<(), EvmIrVerifyError> {
    match operand {
        EvmIrOperand::Value(value) if !value_exists(module, *value) => {
            Err(EvmIrVerifyError::in_block(
                block_id,
                format!("value `{}` is out of range", value.index()),
            ))
        }
        EvmIrOperand::Block(block) if !block_exists(module, *block) => {
            Err(EvmIrVerifyError::in_block(
                block_id,
                format!("block `{}` is out of range", block.index()),
            ))
        }
        _ => Ok(()),
    }
}

fn verify_value_defined(
    block_id: EvmIrBlockId,
    module: &EvmIrModule,
    operand: &EvmIrOperand,
    defined_values: &FxHashSet<EvmIrValueId>,
) -> Result<(), EvmIrVerifyError> {
    if let EvmIrOperand::Value(value) = operand
        && !defined_values.contains(value)
    {
        return Err(EvmIrVerifyError::in_block(
            block_id,
            format!("value `%{}` is used but never defined", module.value(*value).name),
        ));
    }
    Ok(())
}

fn block_exists(module: &EvmIrModule, block: EvmIrBlockId) -> bool {
    block.index() < module.blocks.len()
}

fn value_exists(module: &EvmIrModule, value: EvmIrValueId) -> bool {
    value.index() < module.values.len()
}

fn visit_terminator_operands<E>(
    kind: &EvmIrTerminatorKind,
    mut visit: impl FnMut(&EvmIrOperand) -> Result<(), E>,
) -> Result<(), E> {
    match kind {
        EvmIrTerminatorKind::Fallthrough(_)
        | EvmIrTerminatorKind::FallthroughNext
        | EvmIrTerminatorKind::Jump(_)
        | EvmIrTerminatorKind::Stop
        | EvmIrTerminatorKind::Invalid
        | EvmIrTerminatorKind::RawOpcode(_) => {}
        EvmIrTerminatorKind::Branch { condition, .. } => visit(condition)?,
        EvmIrTerminatorKind::Switch { value, cases, .. } => {
            visit(value)?;
            for (case, _) in cases {
                visit(case)?;
            }
        }
        EvmIrTerminatorKind::Return { offset, size }
        | EvmIrTerminatorKind::Revert { offset, size } => {
            visit(offset)?;
            visit(size)?;
        }
        EvmIrTerminatorKind::SelfDestruct { recipient } => visit(recipient)?,
    }
    Ok(())
}

fn visit_terminator_targets<E>(
    kind: &EvmIrTerminatorKind,
    mut visit: impl FnMut(EvmIrBlockId) -> Result<(), E>,
) -> Result<(), E> {
    match kind {
        EvmIrTerminatorKind::Fallthrough(target) | EvmIrTerminatorKind::Jump(target) => {
            visit(*target)?
        }
        EvmIrTerminatorKind::Branch { then_block, else_block, .. } => {
            visit(*then_block)?;
            visit(*else_block)?;
        }
        EvmIrTerminatorKind::Switch { default, cases, .. } => {
            visit(*default)?;
            for (_, target) in cases {
                visit(*target)?;
            }
        }
        EvmIrTerminatorKind::Return { .. }
        | EvmIrTerminatorKind::Revert { .. }
        | EvmIrTerminatorKind::FallthroughNext
        | EvmIrTerminatorKind::Stop
        | EvmIrTerminatorKind::Invalid
        | EvmIrTerminatorKind::SelfDestruct { .. }
        | EvmIrTerminatorKind::RawOpcode(_) => {}
    }
    Ok(())
}

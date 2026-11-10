//! Checks helper annotations before plugin-edited EVM IR is committed.
//!
//! Existing helper contracts remain trusted only when their instructions, control flow,
//! continuations and nested call contracts are unchanged. In particular, backend return spills
//! need not be re-proved by this stack-only checker after an unrelated edit. New and changed
//! helpers start with exactly their advertised inputs and a symbolic return word.
//! Direct edges propagate symbolic stacks; joins and loops must agree. Nested calls use their
//! annotations; new and changed callees are checked separately, including recursive helpers. A
//! return must consume the original return word and leave exactly the declared outputs. Unsupported
//! indirect transfers are rejected rather than accepting an annotation as a proof.
//!
//! Instrumentation probes leave the stack as they found it: an edge coverage probe,
//! `push <id>; edgecov`, neither reads nor leaves stack words, and a comparison operand log,
//! `push <id>; cmplog`, leaves the two words it records unchanged. A new block holding only probes
//! and a jump therefore forwards its incoming stack unchanged. Comparing a helper with its input
//! skips probes and follows such forwarding blocks to their targets, so instrumenting a trusted
//! helper keeps it trusted.

use crate::backend::evm::{
    ir::{self, BlockId, Instruction, Module, StackEffect, TerminatorKind},
    op,
};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Word {
    Unknown,
    Return,
    Label(BlockId),
}

pub(super) fn verify(previous: &Module, module: &Module) -> Result<(), String> {
    for (entry, block) in module.blocks.iter_enumerated() {
        let Some(call) = block.metadata.call_effect else { continue };
        if unchanged_helper(previous, module, entry) {
            continue;
        }
        let mut stack = vec![Word::Unknown; usize::from(call.effect.inputs)];
        let ret = stack
            .len()
            .checked_sub(usize::from(call.ret) + 1)
            .ok_or("helper return address is outside its inputs")?;
        stack[ret] = Word::Return;
        let mut seen = module.blocks.iter().map(|_| None).collect::<IndexVec<BlockId, _>>();
        let mut pending = vec![(entry, stack)];
        while let Some((id, mut stack)) = pending.pop() {
            if let Some(previous) = &seen[id] {
                if previous != &stack {
                    return Err(format!(
                        "helper bb{} has incompatible stack states at bb{}",
                        entry.index(),
                        id.index()
                    ));
                }
                continue;
            }
            seen[id] = Some(stack.clone());
            let block = &module.blocks[id];
            for inst in &block.instructions {
                let top = stack.len().checked_sub(1);
                match inst.as_stack_op() {
                    Some(op::StackOp::Dup(n)) => {
                        let at = stack
                            .len()
                            .checked_sub(usize::from(n))
                            .ok_or("helper stack underflow")?;
                        stack.push(stack[at]);
                    }
                    Some(op::StackOp::Swap(n)) => {
                        let at = stack
                            .len()
                            .checked_sub(usize::from(n) + 1)
                            .ok_or("helper stack underflow")?;
                        stack.swap(top.ok_or("helper stack underflow")?, at);
                    }
                    Some(op::StackOp::Exchange(n, m)) => {
                        let a = stack
                            .len()
                            .checked_sub(usize::from(n) + 1)
                            .ok_or("helper stack underflow")?;
                        let b = stack
                            .len()
                            .checked_sub(usize::from(m) + 1)
                            .ok_or("helper stack underflow")?;
                        stack.swap(a, b);
                    }
                    _ => {
                        let branch = (inst.opcode == op::JUMPI).then(|| stack.last().copied());
                        apply(&mut stack, inst.stack_effect())?;
                        if let Some(label) = inst.pushed_block() {
                            *stack.last_mut().ok_or("helper label has no result")? =
                                Word::Label(label);
                        }
                        if let Some(Some(Word::Label(target))) = branch {
                            pending.push((target, stack.clone()));
                        } else if branch.is_some() {
                            return Err("cannot verify helper indirect conditional jump".into());
                        }
                        if inst.opcode == op::JUMP {
                            return Err("cannot verify helper inline indirect jump".into());
                        }
                    }
                }
                if stack.len() > op::MAX_STACK_DEPTH {
                    return Err("helper exceeds the EVM stack limit".into());
                }
            }
            let term = &block.terminator.as_ref().ok_or("unterminated helper block")?.kind;
            match term {
                TerminatorKind::Jump(target)
                    if module.blocks[*target].metadata.call_effect.is_some() =>
                {
                    let nested = module.blocks[*target].metadata.call_effect.unwrap();
                    let ret = stack
                        .len()
                        .checked_sub(usize::from(nested.ret) + 1)
                        .ok_or("helper call underflow")?;
                    let destination = stack[ret];
                    apply(&mut stack, nested.effect)?;
                    match destination {
                        Word::Label(next) => pending.push((next, stack)),
                        Word::Return => check_return(entry, &stack, call.effect.outputs)?,
                        Word::Unknown => {
                            return Err("cannot verify helper call return address".into());
                        }
                    }
                }
                TerminatorKind::Op(op::JUMP) => {
                    match stack.pop().ok_or("helper return underflow")? {
                        Word::Return => check_return(entry, &stack, call.effect.outputs)?,
                        Word::Label(next) => pending.push((next, stack)),
                        Word::Unknown => return Err("cannot verify helper dynamic return".into()),
                    }
                }
                _ => {
                    apply(
                        &mut stack,
                        ir::default_terminator_stack_effect(term)
                            .ok_or("unknown helper terminator")?,
                    )?;
                    term.visit_targets(|target| pending.push((target, stack.clone())));
                }
            }
        }
    }
    Ok(())
}

fn apply(stack: &mut Vec<Word>, effect: StackEffect) -> Result<(), String> {
    let remaining =
        stack.len().checked_sub(usize::from(effect.inputs)).ok_or("helper stack underflow")?;
    stack.truncate(remaining);
    stack.resize(remaining + usize::from(effect.outputs), Word::Unknown);
    Ok(())
}

fn check_return(entry: BlockId, stack: &[Word], outputs: u8) -> Result<(), String> {
    if stack.len() != usize::from(outputs) || stack.contains(&Word::Return) {
        return Err(format!(
            "helper bb{} return does not match its declared call effect",
            entry.index()
        ));
    }
    Ok(())
}

/// Trust the input's contract only while all its code and continuation edges are unchanged.
/// Nested helpers are contract boundaries: their bodies are checked separately by `verify`,
/// while callers rely on the same annotation they used before the edit. Follow pushed labels
/// too, since inline branches and return continuations need not appear as terminator targets.
fn unchanged_helper(previous: &Module, module: &Module, entry: BlockId) -> bool {
    let mut pending = vec![entry];
    let mut seen = DenseBitSet::new_empty(module.blocks.len());
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(before) = previous.blocks.get(id) else { return false };
        let after = &module.blocks[id];
        let forward = |block| forward_probe_blocks(previous, module, block);
        let after_terminator = after.terminator.as_ref().map(|term| {
            let mut kind = term.kind.clone();
            kind.visit_targets_mut(|target| *target = forward(*target));
            kind
        });
        let (before_body, after_body) =
            (unprobed(&before.instructions), unprobed(&after.instructions));
        // Debug context must never influence whether executable code is accepted.
        if before.metadata.call_effect != after.metadata.call_effect
            || before.terminator.as_ref().map(|term| &term.kind) != after_terminator.as_ref()
            || before_body.len() != after_body.len()
            || !before_body.iter().zip(&after_body).all(|(a, b)| {
                a.opcode == b.opcode
                    && a.encoding == b.encoding
                    && match (a.pushed_block(), b.pushed_block()) {
                        (Some(a), Some(b)) => a == forward(b),
                        _ => a.value == b.value,
                    }
                    && a.as_stack_op() == b.as_stack_op()
                    && a.metadata.keep_with_next == b.metadata.keep_with_next
            })
        {
            return false;
        }
        let mut same_contracts = true;
        let mut target = |target: BlockId| {
            if target != entry && module.blocks[target].metadata.call_effect.is_some() {
                same_contracts &=
                    previous.blocks.get(target).and_then(|block| block.metadata.call_effect)
                        == module.blocks[target].metadata.call_effect;
            } else {
                pending.push(target);
            }
        };
        for inst in &after_body {
            if let Some(label) = inst.pushed_block() {
                target(forward(label));
            }
        }
        if let Some(term) = &after_terminator {
            term.visit_targets(&mut target);
        }
        if !same_contracts {
            return false;
        }
    }
    true
}

/// Returns the instructions without instrumentation probes, `push <id>; edgecov` and
/// `push <id>; cmplog`, whose net stack effect is nothing.
fn unprobed(instructions: &[Instruction]) -> Vec<&Instruction> {
    let probe =
        |inst: Option<&Instruction>| inst.is_some_and(|inst| op::is_instrumentation(inst.opcode));
    instructions
        .iter()
        .enumerate()
        .filter(|&(index, inst)| {
            !probe(Some(inst))
                && !(inst.keeps_with_next()
                    && inst.pushed_value().is_some()
                    && probe(instructions.get(index + 1)))
        })
        .map(|(_, inst)| inst)
        .collect()
}

/// Follows blocks the edit added that hold only probes and a jump to the block they forward to.
fn forward_probe_blocks(previous: &Module, module: &Module, mut block: BlockId) -> BlockId {
    for _ in 0..module.blocks.len() {
        let body = &module.blocks[block];
        if block.index() < previous.blocks.len()
            || body.metadata.call_effect.is_some()
            || !unprobed(&body.instructions).is_empty()
        {
            break;
        }
        let Some(TerminatorKind::Jump(next)) = body.terminator.as_ref().map(|term| &term.kind)
        else {
            break;
        };
        block = *next;
    }
    block
}

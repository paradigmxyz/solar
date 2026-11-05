//! Checks helper annotations before plugin-edited EVM IR is committed.
//!
//! Each declared helper starts with exactly its advertised inputs and a symbolic return word.
//! Direct edges propagate symbolic stacks; joins and loops must agree. Nested calls use their
//! own annotations, all of which are checked separately, including recursive helpers. A return
//! must consume the original return word and leave exactly the declared outputs. Unsupported
//! indirect transfers are rejected rather than accepting an annotation as a proof.

use crate::backend::evm::{
    ir::{self, BlockId, Module, StackEffect, TerminatorKind},
    op,
};
use solar_data_structures::index::IndexVec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Word {
    Unknown,
    Return,
    Label(BlockId),
}

pub(super) fn verify(module: &Module) -> Result<(), String> {
    for (entry, block) in module.blocks.iter_enumerated() {
        let Some(call) = block.metadata.call_effect else { continue };
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

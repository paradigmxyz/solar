//! Reconstruct old induction values on loop exits instead of carrying both versions.
//!
//! A loop often computes `next = old + constant`, carries `next` to its header,
//! and uses `old` only on its exit. Keeping both versions alive at the branch
//! adds stack transfers to every iteration. Reconstruct `old = next - constant`
//! in a single-predecessor exit block, after the branch, using wrapping word
//! arithmetic. Subtraction updates use the inverse addition. Memory accesses
//! and the loop's original instructions remain in place.
//!
//! This gas-only late pass runs after scalar simplification so CSE cannot undo
//! the placement. It considers direct backedges or one empty latch, immediate
//! increments, and at most two reconstructions per function. The exit must not
//! contain phis or have another predecessor. Enclosing loops are allowed. Checked
//! arithmetic is never introduced or removed. This is a bounded live-range
//! tradeoff: one exit computation can replace repeated backedge stack cleanup.

use crate::mir::{
    BlockId, Function, InstKind, Instruction, Module, Terminator, Value,
    pass::{MirPass, ModuleAnalyses},
};
use solar_sema::Gcx;

pub(crate) struct LoopExitRemat;

impl MirPass for LoopExitRemat {
    fn name(&self) -> &'static str {
        "loop-exit-remat"
    }

    fn is_enabled(&self, gcx: Gcx<'_>, _: &Module) -> bool {
        gcx.sess.opts.optimization.is_gas()
    }

    fn run_pass(&self, _: Gcx<'_>, module: &mut Module, _: &mut ModuleAnalyses) -> bool {
        let mut changed = false;
        for func in &mut module.functions {
            changed |= run(func);
        }
        changed
    }
}

fn latch(func: &Function, header: BlockId, block: BlockId) -> bool {
    block == header
        || (func.blocks[block].instructions.is_empty()
            && func.blocks[block].terminator == Some(Terminator::Jump(header)))
}

fn run(func: &mut Function) -> bool {
    let mut candidates = Vec::new();
    'headers: for (header, body) in func.blocks.iter_enumerated() {
        let Some(Terminator::Branch { then_block, else_block, .. }) = body.terminator else {
            continue;
        };
        for (back, exit) in [(then_block, else_block), (else_block, then_block)] {
            if !latch(func, header, back)
                || exit == header
                || func.blocks[exit].predecessors.as_slice() != [header]
                || func.blocks[exit]
                    .instructions
                    .iter()
                    .any(|&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)))
            {
                continue;
            }
            for &inst in &body.instructions {
                let instruction = func.inst(inst);
                let (old, constant, subtract) = match instruction.kind {
                    InstKind::Add(a, b) if func.value(b).as_immediate().is_some() => (a, b, true),
                    InstKind::Add(a, b) if func.value(a).as_immediate().is_some() => (b, a, true),
                    InstKind::Sub(a, b) if func.value(b).as_immediate().is_some() => (a, b, false),
                    _ => continue,
                };
                if let Some(next) = func.inst_result_value(inst)
                    && let Value::Inst(phi) = func.value(old)
                    && body.instructions.contains(phi)
                    && let InstKind::Phi(incoming) = &func.inst(*phi).kind
                    && incoming.iter().any(|&(pred, value)| pred == back && value == next)
                    && instruction
                        .metadata
                        .effect()
                        .is_none_or(|effect| effect == instruction.kind.effect_kind())
                    && (func.blocks[exit]
                        .instructions
                        .iter()
                        .any(|&id| func.inst(id).operands().contains(&old))
                        || func.blocks[exit]
                            .terminator
                            .as_ref()
                            .is_some_and(|term| term.operands().contains(&old)))
                {
                    candidates.push((exit, old, next, constant, subtract, func.value_ty(old)));
                    if candidates.len() == 2 {
                        break 'headers;
                    }
                }
            }
        }
    }
    if candidates.is_empty() {
        return false;
    }
    for (exit, old, next, constant, subtract, ty) in candidates {
        // loop: next = old +/- constant; branch condition, loop, exit
        // exit: recovered = next -/+ constant; use(recovered)
        let kind =
            if subtract { InstKind::Sub(next, constant) } else { InstKind::Add(next, constant) };
        // NOTE: This inverse computation has no corresponding source expression.
        let (inst, recovered) =
            func.alloc_value_inst(Instruction::new(kind, ty).with_debug_info_dropped());
        for id in func.blocks[exit].instructions.clone() {
            func.inst_mut(id).rewrite_operands(|value| {
                if *value == old {
                    *value = recovered;
                }
            });
        }
        if let Some(term) = &mut func.blocks[exit].terminator {
            term.visit_operands_mut(|value| {
                if *value == old {
                    *value = recovered;
                }
            });
        }
        func.blocks[exit].instructions.insert(0, inst);
    }
    true
}

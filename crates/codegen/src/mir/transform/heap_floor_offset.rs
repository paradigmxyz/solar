//! Folds constant offsets into the heap floor.
//!
//! `heap_floor` is the initial free memory pointer, a word that codegen fixes only after it has
//! placed the frames and spill slots below it, so no earlier pass can evaluate arithmetic on it.
//! Allocation lowering then leaves `add floor, 32`, `add floor, 64`, and `add floor, 128` beside
//! it, and scheduling reads each from the one stack copy of the floor with
//! `dup; push offset; add`: four bytes and nine gas for a word that is one push once the floor is
//! known.
//!
//! This pass rewrites each `add` of the floor and a constant that fits in 64 bits into
//! `heap_floor_offset offset`, a nullary read that codegen resolves together with the floor and
//! pushes at each use like an immediate. The floor is far below 2^64, so the sum cannot wrap. It
//! follows chains: `add (add floor, 32), 64` becomes `heap_floor_offset 96`. The rewrite keeps
//! the instruction, its result, and its metadata, and the floor stays for its other uses.
//!
//! The pass runs after the memory and alias analyses that reason about `add floor, offset` as a
//! pointer plus a constant, which the new operation hides, so those analyses see the original
//! form. Frame addresses have no counterpart here: lowered MIR forms them through pointer casts
//! and frame slot operations, not `add`, and the EVM IR peephole folds what placement leaves.

use crate::mir::{
    Function, InstKind, MirType, Module, Value, ValueId,
    pass::{MirPass, ModuleAnalyses, run_function_pass},
};
use solar_sema::Gcx;

pub(crate) struct HeapFloorOffsetPass;

impl MirPass for HeapFloorOffsetPass {
    fn name(&self) -> &'static str {
        "heap-floor-offset"
    }

    fn run_pass(&self, _: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        run_function_pass(module, analyses, |func, _| run(func))
    }
}

fn run(func: &mut Function) -> bool {
    let mut changed = false;
    // A chain's inner `add` may sit in a later block than its user, so repeat until none folds.
    while fold_adds(func) {
        changed = true;
    }
    changed
}

fn fold_adds(func: &mut Function) -> bool {
    let mut changed = false;
    for inst in func.instructions().collect::<Vec<_>>() {
        let InstKind::Add(a, b) = func.inst(inst).kind else { continue };
        // add floor, constant => heap_floor_offset constant
        let Some(offset) = floor_offset(func, a)
            .zip(func.value_u64(b))
            .or_else(|| floor_offset(func, b).zip(func.value_u64(a)))
            .and_then(|(floor, constant)| floor.checked_add(constant))
        else {
            continue;
        };
        if func.inst(inst).result_ty != Some(MirType::I256) {
            continue;
        }
        let instruction = func.inst_mut(inst);
        instruction.kind = InstKind::HeapFloorOffset(offset);
        instruction.metadata.set_effect(None);
        changed = true;
    }
    changed
}

/// The constant offset from the heap floor of `value`, when it is the floor or a folded sum.
fn floor_offset(func: &Function, value: ValueId) -> Option<u64> {
    let Value::Inst(inst) = func.value(value) else { return None };
    match func.inst(*inst).kind {
        InstKind::HeapFloor => Some(0),
        InstKind::HeapFloorOffset(offset) => Some(offset),
        _ => None,
    }
}

//! Shared utilities for MIR transforms.

use crate::mir::{
    BlockId, Function, FunctionBuilder, InstId, InstKind, Terminator, ValueId,
    memory::EvmMemoryLayout, utils::IndexLists,
};
use solar_sema::hir::StateMutability;

/// Whether an external entry must reject nonzero callvalue.
pub(super) fn rejects_callvalue(func: &Function) -> bool {
    matches!(
        func.attributes.state_mutability,
        StateMutability::NonPayable | StateMutability::View | StateMutability::Pure
    )
}

/// Whether a library's external entry may only run through `DELEGATECALL`.
///
/// Only non-view functions are guarded; view and pure functions accept direct calls.
pub(super) fn needs_delegatecall_guard(func: &Function) -> bool {
    matches!(
        func.attributes.state_mutability,
        StateMutability::NonPayable | StateMutability::Payable
    )
}

/// Redirects successor predecessor metadata after splitting `from` into a
/// continuation block `to`.
pub(super) fn redirect_successor_predecessors(func: &mut Function, from: BlockId, to: BlockId) {
    let successors =
        func.blocks[to].terminator.as_ref().map(Terminator::successors).unwrap_or_default();
    for successor in successors {
        for predecessor in &mut func.blocks[successor].predecessors {
            if *predecessor == from {
                *predecessor = to;
            }
        }
        let instruction_count = func.blocks[successor].instructions.len();
        for index in 0..instruction_count {
            let inst = func.blocks[successor].instructions[index];
            let InstKind::Phi(incoming) = &mut func.inst_mut(inst).kind else { continue };
            for (predecessor, _) in incoming {
                if *predecessor == from {
                    *predecessor = to;
                }
            }
        }
    }
}

/// Incremental form of the shared dispatch callvalue-hoisting predicate:
/// every external entry (selector-bearing, receive, or fallback) rejects value.
///
/// `LowerAbi` and `LowerDispatch` both use this while performing their
/// existing module scans, so they must observe every function and agree.
pub(super) struct DispatchCallvalue {
    any: bool,
    all_reject: bool,
}

impl Default for DispatchCallvalue {
    fn default() -> Self {
        Self { any: false, all_reject: true }
    }
}

impl DispatchCallvalue {
    pub(super) fn observe(&mut self, func: &Function) {
        let external =
            func.selector.is_some() || func.attributes.is_receive || func.attributes.is_fallback;
        if !external || func.attributes.is_constructor {
            return;
        }
        self.any = true;
        self.all_reject &= rejects_callvalue(func);
    }

    pub(super) const fn hoists(&self) -> bool {
        self.any && self.all_reject
    }
}

/// Preflights local frame offsets when a signature changes its scalar slot count.
pub(super) fn rebase_frame_offsets(func: &Function, slots: usize) -> Option<Vec<(InstId, u64)>> {
    let old_slots = func.params.len().checked_add(func.return_components().len())?;
    if old_slots == slots {
        return Some(Vec::new());
    }
    let base = |slots| {
        u64::try_from(slots)
            .ok()?
            .checked_mul(EvmMemoryLayout::WORD_SIZE)?
            .checked_add(EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE)
    };
    let old_base = base(old_slots)?;
    let new_base = base(slots)?;
    let mut offsets = Vec::new();
    for inst in func.instructions() {
        if let InstKind::InternalFrameAddr(offset) = func.inst(inst).kind {
            offsets.push((inst, new_base.checked_add(offset.checked_sub(old_base)?)?));
        }
    }
    Some(offsets)
}

/// Rebuilds `block` in order, letting `expand` replace instructions with new code.
///
/// `expand` returns `false` to keep an instruction, which then moves to the builder's current
/// block. When an expansion splits the block, the original terminator and its source context move
/// to the final continuation and successor phis follow it.
pub(super) fn expand_block(
    builder: &mut FunctionBuilder<'_>,
    block: BlockId,
    mut expand: impl FnMut(&mut FunctionBuilder<'_>, InstId) -> bool,
) {
    let instructions = std::mem::take(&mut builder.func_mut().blocks[block].instructions);
    let (terminator, metadata) = builder.func_mut().blocks[block].take_terminator();
    builder.switch_to_block(block);
    for inst in instructions {
        if !expand(builder, inst) {
            // continuation: original instruction
            let current = builder.current_block();
            builder.func_mut().blocks[current].instructions.push(inst);
        }
    }
    // continuation: original terminator
    let end = builder.current_block();
    if let Some(terminator) = terminator {
        builder.func_mut().blocks[end].set_terminator(terminator, metadata);
    }
    if end != block {
        redirect_successor_predecessors(builder.func_mut(), block, end);
    }
}

/// A reader of a value: an instruction, or a block's terminator.
#[derive(Clone, Copy)]
pub(super) enum User {
    Inst(InstId),
    Terminator(BlockId),
}

/// The instructions and terminators that read each value of a function.
pub(super) fn value_users(func: &Function) -> IndexLists<ValueId, User> {
    let mut pairs = Vec::new();
    for (block, data) in func.blocks.iter_enumerated() {
        for &inst in &data.instructions {
            func.inst(inst).visit_operands(|operand| pairs.push((operand, User::Inst(inst))));
        }
        if let Some(terminator) = &data.terminator {
            terminator.visit_operands(|operand| pairs.push((operand, User::Terminator(block))));
        }
    }
    IndexLists::new(func.num_values(), pairs.iter().copied())
}

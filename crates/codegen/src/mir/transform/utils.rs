//! Shared utilities for MIR transforms.

use crate::mir::{
    BlockId, Function, FunctionBuilder, FunctionId, InstId, InstKind, InstructionMetadata,
    Terminator, ValueId,
    memory::EvmMemoryLayout,
    utils::{IndexLists, replace_terminator},
};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec};
use solar_evmir::debug::DebugLocKind;
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
/// every dispatched function (selector-bearing, receive, or fallback) rejects value.
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
        if !func.is_dispatched() {
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
    let old_base = EvmMemoryLayout::internal_frame_prefix(old_slots)?;
    let new_base = EvmMemoryLayout::internal_frame_prefix(slots)?;
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
/// to the final continuation and successor phis follow it. Replacement instructions inherit the
/// operation being expanded, rather than an empty or previous operation's builder context.
pub(super) fn expand_block(
    builder: &mut FunctionBuilder<'_>,
    block: BlockId,
    mut expand: impl FnMut(&mut FunctionBuilder<'_>, InstId) -> bool,
) {
    let instructions = std::mem::take(&mut builder.func_mut().blocks[block].instructions);
    let (terminator, metadata) = builder.func_mut().blocks[block].take_terminator();
    builder.switch_to_block(block);
    for inst in instructions {
        let context = builder.func().inst(inst).metadata.debug_context();
        builder.set_debug_context(&context);
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

/// Retargets every edge from `pred` to `old` so it enters `new`, keeping predecessor lists in sync.
///
/// Returns whether `pred` had an edge to `old`. The caller supplies `new`'s phi inputs for `pred`.
pub(super) fn redirect_edge(
    func: &mut Function,
    pred: BlockId,
    old: BlockId,
    new: BlockId,
) -> bool {
    let Some(mut terminator) = func.blocks[pred].terminator.clone() else { return false };
    if !terminator.has_successor(old) {
        return false;
    }
    // pred: jump/branch/switch old -> new
    terminator.for_each_successor_mut(|target| {
        if *target == old {
            *target = new;
        }
    });
    replace_terminator(func, pred, terminator);
    true
}

/// Whether a result of `block_id` is read outside it by a reader `ignored` does not accept: an
/// instruction of a block, or with `None` the block's terminator. `results` is scratch space and
/// ends holding the block's results.
///
/// A phi input is used at the end of its incoming edge, not in the phi's block: a successor's phi
/// reads the input on the edge from `block_id` inside it, while one of the block's own phis, as a
/// loop-carried value does, reads in a block that can be entered without passing through
/// `block_id`, unless that phi is ignored.
pub(super) fn block_results_have_external_uses(
    func: &Function,
    block_id: BlockId,
    ignored: impl Fn(BlockId, Option<InstId>) -> bool,
    results: &mut DenseBitSet<ValueId>,
) -> bool {
    results.clear();
    for &inst in &func.blocks[block_id].instructions {
        if let Some(result) = func.inst_result_value(inst) {
            results.insert(result);
        }
    }
    if results.is_empty() {
        return false;
    }

    let external = |operand| results.contains(operand);
    func.blocks.iter_enumerated().any(|(other_block, block)| {
        let own = other_block == block_id;
        block.instructions.iter().any(|&inst_id| {
            let kind = &func.inst(inst_id).kind;
            let reads = match kind {
                InstKind::Phi(incoming) if !own => {
                    incoming.iter().any(|&(pred, value)| pred != block_id && external(value))
                }
                InstKind::Phi(_) => kind.any_operand(external),
                _ => !own && kind.any_operand(external),
            };
            reads && !ignored(other_block, Some(inst_id))
        }) || (!own
            && !ignored(other_block, None)
            && block.terminator.as_ref().is_some_and(|term| term.any_operand(external)))
    })
}

/// [`block_results_have_external_uses`] from the readers of each result, as [`value_users`] lists
/// them, and the block of each instruction, as `Function::inst_block_table` maps them, instead of
/// a scan of the function.
pub(super) fn block_results_have_external_users(
    func: &Function,
    users: &IndexLists<ValueId, User>,
    homes: &IndexVec<InstId, Option<BlockId>>,
    block_id: BlockId,
    ignored: impl Fn(BlockId, Option<InstId>) -> bool,
) -> bool {
    func.blocks[block_id].instructions.iter().filter_map(|&inst| func.inst_result_value(inst)).any(
        |result| {
            users.get(result).iter().any(|&user| match user {
                User::Inst(inst) => {
                    let block = homes[inst].expect("a user is in a block");
                    let reads = match &func.inst(inst).kind {
                        InstKind::Phi(incoming) if block != block_id => incoming
                            .iter()
                            .any(|&(pred, value)| pred != block_id && value == result),
                        InstKind::Phi(_) => true,
                        _ => block != block_id,
                    };
                    reads && !ignored(block, Some(inst))
                }
                User::Terminator(block) => block != block_id && !ignored(block, None),
            })
        },
    )
}

/// Returns whether `func` has a private MIR signature that direct callers may rewrite, given the
/// module's `Module::root_functions`.
pub(super) fn is_internal_body(
    roots: &DenseBitSet<FunctionId>,
    id: FunctionId,
    func: &Function,
) -> bool {
    !roots.contains(id) && !func.is_public()
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

/// The debug context for code a pass appends to `block`, which runs exactly when the block's
/// terminator runs. A loop preheader's terminator carries the loop statement's location.
///
/// Preheader code runs once each time the loop statement starts, so it takes that statement's
/// location and never a body span, since the body may not run. LLVM's passes drop it, for example
/// LICM (`updateLocationAfterHoist` in `LICM.cpp`, which is `dropLocation` in `DebugInfo.cpp`),
/// because a DWARF line table repeats the previous row, and our source maps repeat nothing.
///
/// A terminator without a location or a reason for lacking one, such as the one of a preheader
/// that `loop-canonicalize` created, gives `kind` instead.
pub(super) fn terminator_debug_context(
    func: &Function,
    block: BlockId,
    kind: DebugLocKind,
) -> InstructionMetadata {
    let mut context = func.blocks[block].terminator_metadata.debug_context();
    if context.lacks_debug_location() {
        context.drop_debug_loc(kind);
    }
    context
}

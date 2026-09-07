//! Bounded literal selection for exclusive arms in the default Gas pipeline.
//!
//! Two hot arms may differ only in their initial literal PUSH. Their equal suffix
//! runs once after arithmetic selection, followed by an equal fixed exit or a
//! conditional with one shared true target and two equal exclusive Return leaves.
//! The latter leaves become one fresh hot Return block, larger than every possible
//! nonterminal parent ID. Cold blocks must be fixed exits, so inlining cannot move
//! the parent into a cold predecessor. The exclusive false leaf then follows its
//! parent in layout; later default Gas sharing preserves that fallthrough. Size
//! transforms can append nonterminal owners after fresh-leaf creation and before
//! block layout, so Size is excluded. Arbitrary custom pipelines are outside this
//! layout proof. No new transfer is executed. Matching is one level deep and never
//! follows arbitrary continuations.
//!
//! ISZERO normalizes arbitrary conditions. A final canonical comparison or ISZERO
//! already yields zero/one and permits selection directly from that word. Literal
//! differences must be powers of two. Selection has the original branch's stack
//! peak and pays two possible later destination markers in its gas/byte budget.
//! There is no join-transfer credit. Final pipeline sizes still require measurement.
//!
//! The CFG caller gates code, gas and forwarded-gas observations before mutation.
//! Exposed/root/loop/shared arms, explicit effects and glued boundaries decline.
//! Debug metadata does not affect admission: application merges alternative origins
//! and marks newly synthesized arithmetic unknown. The existing joined-diamond
//! rule reuses only literal arithmetic, retaining its original costing and guards.

use super::{
    Block, BlockId, InstKind, Instruction, Module, TerminatorKind,
    cfg::{inline_debug_entry, merge_block_debug},
    immediate,
};
use crate::backend::evm::op;
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec};

/// A fully checked local replacement, separating admission from mutation.
pub(super) struct Fold {
    pub(super) selection: Vec<Instruction>,
    pub(super) arms: [BlockId; 2],
    pub(super) returns: Option<[BlockId; 2]>,
}

/// Preserves hot parents through unique-entry inlining and bounds later marker changes.
pub(super) fn cold_paths_allowed(module: &Module) -> bool {
    module.block_ids().all(|id| {
        let block = &module.blocks[id];
        (block.terminator.kind != TerminatorKind::Return || (!block.cold && !block.loop_header))
            && (!block.cold
                || (fixed_exit(&block.terminator.kind)
                    && !block.insts.iter().any(|inst| {
                        matches!(inst.kind, InstKind::PushLabel(_))
                            || matches!(
                                inst.kind,
                                InstKind::Op(op::JUMP | op::JUMPI | op::JUMPDEST)
                            )
                    })))
    })
}

fn fixed_exit(kind: &TerminatorKind) -> bool {
    matches!(
        kind,
        TerminatorKind::Return
            | TerminatorKind::Revert
            | TerminatorKind::Stop
            | TerminatorKind::Invalid
    )
}

fn ordinary(block: &Block) -> bool {
    block.terminator.stack_effect.is_none()
        && !block.terminator.keep_with_next
        && block.insts.iter().all(|inst| inst.stack_effect.is_none() && !inst.keep_with_next)
}

/// Matches one direct arm pair, never mutating a failed candidate.
pub(super) fn direct(
    module: &Module,
    id: BlockId,
    incoming: &IndexVec<BlockId, usize>,
    exposed: &DenseBitSet<BlockId>,
    removed: &DenseBitSet<BlockId>,
    version: EvmVersion,
) -> Option<Fold> {
    let parent = &module.blocks[id];
    let TerminatorKind::JumpI(yes, no) = parent.terminator.kind else { return None };
    let domain = removed.domain_size();
    if yes.index() >= domain || no.index() >= domain {
        return None;
    }
    // Reject non-literal shapes before entry, metadata and exclusivity checks.
    let a = &module.blocks[yes];
    let b = &module.blocks[no];
    let ([yes_push, suffix @ ..], [no_push, other @ ..]) = (a.insts.as_slice(), b.insts.as_slice())
    else {
        return None;
    };
    let (InstKind::Push(yes_value), InstKind::Push(no_value)) = (&yes_push.kind, &no_push.kind)
    else {
        return None;
    };
    if yes_value == no_value || suffix.len() != other.len() {
        return None;
    }
    let exclusive = |target: BlockId| {
        target.index() < domain
            && target != id
            && Some(target) != module.block_ids().next()
            && !removed.contains(target)
            && !exposed.contains(target)
            && incoming[target] == 1
            && !module.blocks[target].cold
            && !module.blocks[target].loop_header
    };
    if removed.contains(id)
        || exposed.contains(id)
        || parent.cold
        || parent.loop_header
        || parent.terminator.stack_effect.is_some()
        || parent.terminator.keep_with_next
        || !super::split_allowed(&parent.insts, parent.insts.len())
        || yes == no
        || !exclusive(yes)
        || !exclusive(no)
    {
        return None;
    }
    if suffix != other || !ordinary(a) || !ordinary(b) {
        return None;
    }
    let returns = if fixed_exit(&a.terminator.kind) && a.terminator == b.terminator {
        None
    } else if let TerminatorKind::JumpI(yes_target, a_return) = a.terminator.kind
        && let TerminatorKind::JumpI(no_target, b_return) = b.terminator.kind
        && yes_target == no_target
        && yes_target.index() < domain
        && !removed.contains(yes_target)
        && !exposed.contains(yes_target)
        && Some(yes_target) != module.block_ids().next()
        && !module.blocks[yes_target].loop_header
        && a_return != b_return
        && ![id, yes, no, a_return, b_return].contains(&yes_target)
        && ![yes, no].contains(&a_return)
        && ![yes, no].contains(&b_return)
        && exclusive(a_return)
        && exclusive(b_return)
        && module.blocks[a_return].terminator.kind == TerminatorKind::Return
        && module.blocks[b_return].terminator.kind == TerminatorKind::Return
        && ordinary(&module.blocks[a_return])
        && ordinary(&module.blocks[b_return])
        && module.blocks[a_return].insts == module.blocks[b_return].insts
        && module.blocks[a_return].terminator == module.blocks[b_return].terminator
    {
        Some([a_return, b_return])
    } else {
        return None;
    };
    let boolean = parent.insts.last().is_some_and(|inst| {
        inst.stack_effect.is_none()
            && !inst.keep_with_next
            && matches!(
                inst.kind,
                InstKind::Op(op::EQ | op::LT | op::GT | op::SLT | op::SGT | op::ISZERO)
            )
    });
    let selection = select(*yes_value, *no_value, boolean)?;
    let cost = immediate::cost(version, &selection);
    let a_cost = immediate::cost(version, std::slice::from_ref(yes_push));
    let b_cost = immediate::cost(version, std::slice::from_ref(no_push));
    // Original control pays at least PUSH/JUMPI and one target marker in bytes.
    // Charge two later markers, without credit for duplicate exits or a join.
    if cost.0 + 2 >= 4 + a_cost.0 + b_cost.0 + immediate::cost(version, suffix).0
        || cost.1 + 2 > 13 + a_cost.1.min(b_cost.1)
    {
        return None;
    }
    Some(Fold { selection, arms: [yes, no], returns })
}

/// Emits an admitted fold and returns its optional fresh Return block.
pub(super) fn apply(module: &mut Module, id: BlockId, fold: Fold) -> Option<BlockId> {
    let [yes, no] = fold.arms;
    let mut fresh_return = None;
    merge_block_debug(module, yes, no);
    let mut common = module.blocks[yes].clone();
    if let Some([a, b]) = fold.returns {
        merge_block_debug(module, a, b);
        // common condition; jumpi true_target, fresh_return
        // fresh_return: <identical return body>
        let fresh = module.append_block(module.blocks[a].clone());
        let TerminatorKind::JumpI(target, _) = common.terminator.kind else {
            unreachable!("checked conditional arm")
        };
        common.terminator.kind = TerminatorKind::JumpI(target, fresh);
        fresh_return = Some(fresh);
    }
    let selection = fold.selection.into_iter().map(|mut inst| {
        if module.debug_info_tracked {
            inst.debug =
                Some(Box::new(super::DebugMetadata { dropped: true, ..Default::default() }));
        }
        inst
    });
    // condition; jumpi {push yes; suffix}, {push no; suffix}
    // select literal; suffix; common terminator
    common.insts = selection.chain(common.insts.into_iter().skip(1)).collect();
    inline_debug_entry(&mut common);
    module.blocks[id].insts.extend(common.insts);
    module.blocks[id].terminator = common.terminator;
    fresh_return
}

/// Selects literal endpoints from an arbitrary condition or a proved zero/one word.
pub(super) fn select(yes: U256, no: U256, boolean: bool) -> Option<Vec<Instruction>> {
    if yes == no {
        return None;
    }
    let difference = if no > yes { no - yes } else { yes - no };
    if !(difference & (difference - U256::ONE)).is_zero() {
        return None;
    }
    // condition -> !condition when needed -> scaled zero/one
    // base + delta, or base - delta
    let mut selection = Vec::new();
    if !boolean {
        selection.push(InstKind::Op(op::ISZERO).into());
    }
    let shift = difference.trailing_zeros();
    if shift != 0 {
        selection.extend([InstKind::Push(U256::from(shift)).into(), InstKind::Op(op::SHL).into()]);
    }
    let (base, increasing) = if boolean { (no, yes > no) } else { (yes, no > yes) };
    selection.extend([
        InstKind::Push(base).into(),
        InstKind::Op(if increasing { op::ADD } else { op::SUB }).into(),
    ]);
    Some(selection)
}

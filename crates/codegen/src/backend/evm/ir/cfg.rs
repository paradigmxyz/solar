//! Control-flow simplification, terminal sharing and block placement.
//!
//! Simplification forwards empty jumps, removes redundant conditional edges,
//! joins identical conditional successors, merges uniquely entered blocks, and
//! folds exclusive one-literal arms to a shared continuation when their difference
//! is a power of two. Literal selection uses ISZERO and a shift plus ADD/SUB,
//! requires ordinary metadata and unchanged stack peak, and must reduce primitive
//! bytes without increasing either path's gas. Code/gas/forwarded-gas observations
//! disable this new rule; the existing simplifications retain their own policy. It
//! retains the closure of all explicit and
//! address-taken targets. Unknown computed jumps preserve the module, including
//! the instructions whose references require those targets to have JUMPDESTs.
//! Stable block IDs survive all removals and layout changes. Terminal sharing
//! redirects identical exiting suffixes only when their encoded body exceeds a
//! jump; it never merges distinct effects or instruction metadata. Terminal-body
//! sharing rejects code observations and unknown computed entries, excludes GAS
//! within the shared body, and proves room for the added jump address. Pushed
//! labels also block sharing unless machine lowering proves they remain private
//! control state; parsed IR can expose their numeric addresses as ordinary data.
//! Tail merging uses the same relocation exclusions and additionally declines
//! modules that read GAS or forward gas to external calls or creations: its new
//! transfer can affect observations after the shared suffix, not only within it.
//! In size mode, an exact six-byte Return/Revert suffix may instead be shared
//! by an atomic group of at least eight blocks. All members independently pass
//! the transfer-peak proof before any rewrite. Group costing reserves five bytes
//! per transfer and one shared label, without credit for fallthrough; ordinary
//! pair matching and gas-mode thresholds are unchanged. This is a bounded cost
//! estimate through PUSH3 label widths, not an assembler fixed-point size proof.
//! Unequal conditional terminators can match through one empty forwarding block
//! per target; only accepted shared tails use those destinations. Equal original
//! terminators retain their existing behavior, and metadata is never discarded. A final
//! taken-edge-only cleanup redirects duplicate exits to surviving identical
//! bodies without adding a transfer or changing the surviving layout. The
//! earliest identical body remains the owner and may gain a JUMPDEST, except
//! when the module forwards gas: then the owner must already be addressable. Only
//! taken-only duplicates can be removed. Private pushed labels require control-only
//! provenance and their targets stay protected. Modules with indirect control require
//! already-addressable owners, no forwarded-gas observations, and proved executed
//! dynamic destinations. Observer and dynamic-target checks run only for actual
//! matches before mutation. Public labels, unknown control and code observations
//! block the transform. Placement
//! forms unconditional and conditional-false traces, removing their encoded
//! PUSH/JUMP transfers while keeping cold traces after hot ones. Existing
//! unconditional trace edges reserve their targets in hotness/reference order,
//! preventing new conditional traces from stealing their preferred fallthrough. These transforms
//! operate before assembly, where block references and loop/cold annotations are still explicit.

use super::{
    Block, BlockId, EvmPass, InstKind, Module, Terminator, TerminatorKind, verify::successors,
};
use crate::{backend::evm::op, timing::PassTimer};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec};
use solar_sema::Gcx;

pub(super) struct CfgSimplify;
pub(super) struct BlockLayout;
pub(super) struct TerminalDedup;
pub(super) struct RedirectTerminals;
pub(super) struct ShareReverts;
pub(super) struct TailMerge;

impl EvmPass for CfgSimplify {
    fn name(&self) -> &'static str {
        "cfg-simplify"
    }
    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        simplify(module, gcx.sess.opts.evm_version)
    }
}
impl EvmPass for BlockLayout {
    fn name(&self) -> &'static str {
        "block-layout"
    }
    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        layout(module)
    }
}
impl EvmPass for TerminalDedup {
    fn name(&self) -> &'static str {
        "terminal-dedup"
    }
    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        terminal_dedup(module)
    }
}
impl EvmPass for RedirectTerminals {
    fn name(&self) -> &'static str {
        "redirect-terminals"
    }
    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        redirect_terminals(module)
    }
}
impl EvmPass for ShareReverts {
    fn name(&self) -> &'static str {
        "share-reverts"
    }
    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        share_reverts(module)
    }
}
impl EvmPass for TailMerge {
    fn name(&self) -> &'static str {
        "tail-merge"
    }
    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        tail_merge(gcx, module)
    }
}

fn references(block: &Block) -> impl Iterator<Item = BlockId> + '_ {
    successors(&block.terminator.kind).chain(
        block.insts.iter().filter_map(|inst| {
            if let InstKind::PushLabel(id) = inst.kind { Some(id) } else { None }
        }),
    )
}

fn redirect(module: &mut Module, targets: &IndexVec<BlockId, BlockId>) -> bool {
    let mut changed = false;
    for id in module.block_ids().collect::<Vec<_>>() {
        let block = &mut module.blocks[id];
        // push old_target -> push target
        for inst in &mut block.insts {
            if let InstKind::PushLabel(target) = &mut inst.kind {
                changed |= *target != targets[*target];
                *target = targets[*target];
            }
        }
        let mut replace = |target: &mut BlockId| {
            changed |= *target != targets[*target];
            *target = targets[*target];
        };
        // jump old_target -> jump target
        match &mut block.terminator.kind {
            TerminatorKind::Jump(target) => replace(target),
            TerminatorKind::JumpI(yes, no) => {
                replace(yes);
                replace(no);
            }
            TerminatorKind::IndexedJump(targets) => targets.iter_mut().for_each(replace),
            _ => {}
        }
    }
    changed
}

/// Merges alternative origins, explicitly dropping provenance if either side is unknown.
pub(super) fn merge_debug(
    target: &mut Option<Box<super::DebugMetadata>>,
    source: Option<&super::DebugMetadata>,
) {
    match (target.as_mut(), source) {
        (Some(target), Some(source)) => target.merge(source),
        (Some(target), None) => {
            target.source_spans.clear();
            target.dropped = true;
            target.function_invoke = None;
            target.function_exit = None;
        }
        (None, Some(_)) => {
            *target = Some(Box::new(super::DebugMetadata { dropped: true, ..Default::default() }));
        }
        (None, None) => {}
    }
}

/// A shared body represents every original path, not just its surviving owner.
fn merge_block_debug(module: &mut Module, owner: BlockId, other: BlockId) {
    if !module.debug_info_tracked || owner == other {
        return;
    }
    let source = module.blocks[other].clone();
    let target = &mut module.blocks[owner];
    for (target, source) in target.insts.iter_mut().zip(&source.insts) {
        merge_debug(&mut target.debug, source.debug.as_deref());
    }
    merge_debug(&mut target.terminator.debug, source.terminator.debug.as_deref());
    if target.function_invoke != source.function_invoke {
        target.function_invoke = None;
    }
}

/// Moves an eliminated block's entry event onto its first surviving instruction.
fn inline_debug_entry(block: &mut Block) {
    if let Some(invoke) = block.function_invoke.take() {
        let debug = if let Some(inst) = block.insts.first_mut() {
            &mut inst.debug
        } else {
            &mut block.terminator.debug
        };
        let debug = debug.get_or_insert_with(Default::default);
        if debug.function_invoke.is_none() || debug.function_invoke == Some(invoke) {
            debug.function_invoke = Some(invoke);
        } else {
            debug.function_invoke = None;
        }
    }
}

fn simplify(module: &mut Module, version: EvmVersion) -> bool {
    if module.block_ids().any(|id| {
        module.blocks[id].terminator.kind == TerminatorKind::DynamicJump
            || module.blocks[id].insts.iter().any(|inst| inst.kind == InstKind::Op(op::JUMPI))
    }) && super::verify::has_unknown_jump(module)
    {
        return false;
    }
    if empty_revert_program(module) {
        return true;
    }
    let mut allow_literal_selection = None;
    let mut changed = false;
    loop {
        let ids = module.block_ids().collect::<Vec<_>>();
        let mut progress = false;
        for &id in &ids {
            let same_successors =
                if let TerminatorKind::JumpI(yes, no) = module.blocks[id].terminator.kind {
                    yes != no
                        && module.blocks[yes].insts == module.blocks[no].insts
                        && module.blocks[yes].terminator == module.blocks[no].terminator
                        && !module.blocks[yes]
                            .insts
                            .iter()
                            .any(|inst| matches!(inst.kind, InstKind::Op(op::PC | op::GAS)))
                } else {
                    false
                };
            if same_successors
                && let TerminatorKind::JumpI(yes, no) = module.blocks[id].terminator.kind
            {
                merge_block_debug(module, yes, no);
            }
            let block = &mut module.blocks[id];
            // jumpi identical_body, identical_body -> pop; jump identical_body
            if same_successors && let TerminatorKind::JumpI(yes, _) = block.terminator.kind {
                let debug = block.terminator.debug.clone();
                let mut pop: super::Instruction = InstKind::Op(op::POP).into();
                pop.debug = debug.clone();
                if let Some(debug) = &mut pop.debug {
                    debug.function_invoke = None;
                    debug.function_exit = None;
                }
                block.insts.push(pop);
                block.terminator = TerminatorKind::Jump(yes).into();
                block.terminator.debug = debug;
                progress = true;
            }
            // jumpi target, target -> pop; jump target
            if let TerminatorKind::JumpI(yes, no) = block.terminator.kind
                && yes == no
            {
                let debug = block.terminator.debug.clone();
                let mut pop: super::Instruction = InstKind::Op(op::POP).into();
                pop.debug = debug.clone();
                if let Some(debug) = &mut pop.debug {
                    debug.function_invoke = None;
                    debug.function_exit = None;
                }
                block.insts.push(pop);
                block.terminator = TerminatorKind::Jump(yes).into();
                block.terminator.debug = debug;
                progress = true;
            }
            // push target; jumpi; jump target -> pop; jump target
            if let TerminatorKind::Jump(target) = block.terminator.kind
                && let [.., push, jump] = block.insts.as_slice()
                && push.kind == InstKind::PushLabel(target)
                && jump.kind == InstKind::Op(op::JUMPI)
                && push.stack_effect.is_none()
                && jump.stack_effect.is_none()
            {
                let mut debug = jump.debug.clone();
                merge_debug(&mut debug, push.debug.as_deref());
                block.insts.truncate(block.insts.len() - 2);
                let mut pop: super::Instruction = InstKind::Op(op::POP).into();
                pop.debug = debug;
                block.insts.push(pop);
                progress = true;
            }
        }
        let mut targets = module.blocks.indices().collect::<IndexVec<BlockId, _>>();
        for &id in &ids {
            let mut target = id;
            let mut visited = DenseBitSet::new_empty(module.blocks.len());
            while visited.insert(target)
                && module.blocks[target].insts.is_empty()
                && let TerminatorKind::Jump(next) = module.blocks[target].terminator.kind
                && next != id
            {
                target = next;
            }
            if module.blocks[target].insts.is_empty()
                && matches!(
                    module.blocks[target].terminator.kind,
                    TerminatorKind::Stop
                        | TerminatorKind::Return
                        | TerminatorKind::Revert
                        | TerminatorKind::Invalid
                )
            {
                target = id;
            }
            targets[id] = target;
        }
        progress |= redirect(module, &targets);

        let mut incoming = IndexVec::<BlockId, usize>::from_vec(vec![0; module.blocks.len()]);
        let mut exposed = DenseBitSet::new_empty(module.blocks.len());
        for &id in &ids {
            for target in successors(&module.blocks[id].terminator.kind) {
                incoming[target] += 1;
            }
            for inst in &module.blocks[id].insts {
                if let InstKind::PushLabel(target) = inst.kind {
                    exposed.insert(target);
                }
            }
        }
        let mut removed = DenseBitSet::new_empty(module.blocks.len());
        if version.has_bitwise_shifting() && allow_literal_selection != Some(false) {
            for &id in &ids {
                if let Some((selection, [yes, no, join])) =
                    literal_diamond(module, id, &incoming, &exposed, &removed, version)
                    && *allow_literal_selection.get_or_insert_with(|| {
                        !sharing_observes_code(module)
                            && !module
                                .block_ids()
                                .any(|id| module.blocks[id].insts.iter().any(observes_gas))
                    })
                {
                    let mut continuation = module.blocks[join].clone();
                    inline_debug_entry(&mut continuation);
                    let selection = selection
                        .into_iter()
                        .map(|mut inst| {
                            if module.debug_info_tracked {
                                inst.debug = Some(Box::new(super::DebugMetadata {
                                    dropped: true,
                                    ..Default::default()
                                }));
                            }
                            inst
                        })
                        .collect::<Vec<_>>();
                    // condition; jumpi {push yes}, {push no}; shared continuation
                    // iszero; scale difference; select literal; shared continuation
                    module.blocks[id].insts.extend(selection);
                    module.blocks[id].insts.extend(continuation.insts);
                    module.blocks[id].terminator = continuation.terminator;
                    removed.insert(yes);
                    removed.insert(no);
                    removed.insert(join);
                    progress = true;
                }
            }
        }
        for &id in &ids {
            if !removed.contains(id)
                && let TerminatorKind::Jump(target) = module.blocks[id].terminator.kind
                && target != id
                && incoming[target] == 1
                && !exposed.contains(target)
                && !removed.contains(target)
                && Some(target) != ids.first().copied()
            {
                let mut target_block = module.blocks[target].clone();
                inline_debug_entry(&mut target_block);
                // predecessor instructions
                // target instructions
                // target terminator
                module.blocks[id].insts.extend(target_block.insts);
                module.blocks[id].terminator = target_block.terminator;
                removed.insert(target);
                progress = true;
            }
        }
        let mut reachable = DenseBitSet::new_empty(module.blocks.len());
        let mut pending = ids.first().copied().into_iter().collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            if reachable.insert(id) {
                pending.extend(references(&module.blocks[id]));
            }
        }
        let active = ids
            .iter()
            .copied()
            .filter(|&id| !removed.contains(id) && reachable.contains(id))
            .collect::<Vec<_>>();
        progress |= active.len() != ids.len();
        module.layout = Some(active);
        changed |= progress;
        if !progress {
            return changed;
        }
    }
}

/// Collapses an exclusive two-literal diamond without introducing a transfer.
fn literal_diamond(
    module: &Module,
    id: BlockId,
    incoming: &IndexVec<BlockId, usize>,
    exposed: &DenseBitSet<BlockId>,
    removed: &DenseBitSet<BlockId>,
    version: EvmVersion,
) -> Option<(Vec<super::Instruction>, [BlockId; 3])> {
    let parent = &module.blocks[id];
    let TerminatorKind::JumpI(yes, no) = parent.terminator.kind else { return None };
    let a = &module.blocks[yes];
    let b = &module.blocks[no];
    let TerminatorKind::Jump(join) = a.terminator.kind else { return None };
    if parent.terminator.stack_effect.is_some()
        || a.terminator.stack_effect.is_some()
        || b.terminator.stack_effect.is_some()
        || b.terminator.kind != TerminatorKind::Jump(join)
        || yes == no
        || yes == join
        || no == join
        || id == join
        || incoming[yes] != 1
        || incoming[no] != 1
        || incoming[join] != 2
        || removed.contains(id)
        || [yes, no, join].iter().any(|&block| {
            block == id
                || Some(block) == module.block_ids().next()
                || exposed.contains(block)
                || removed.contains(block)
                || module.blocks[block].loop_header
                || module.blocks[block].cold != parent.cold
        })
    {
        return None;
    }
    let [yes_push] = a.insts.as_slice() else { return None };
    let [no_push] = b.insts.as_slice() else { return None };
    let (InstKind::Push(yes_value), InstKind::Push(no_value)) = (&yes_push.kind, &no_push.kind)
    else {
        return None;
    };
    if yes_push.stack_effect.is_some() || no_push.stack_effect.is_some() || yes_value == no_value {
        return None;
    }
    let difference =
        if no_value > yes_value { *no_value - *yes_value } else { *yes_value - *no_value };
    if !(difference & (difference - U256::ONE)).is_zero() {
        return None;
    }
    // condition -> !condition -> (!condition << log2(difference))
    // yes + delta, or yes - delta
    let mut selection = vec![InstKind::Op(op::ISZERO).into()];
    let shift = difference.trailing_zeros();
    if shift != 0 {
        selection.extend([InstKind::Push(U256::from(shift)).into(), InstKind::Op(op::SHL).into()]);
    }
    selection.extend([
        InstKind::Push(*yes_value).into(),
        InstKind::Op(if no_value > yes_value { op::ADD } else { op::SUB }).into(),
    ]);
    let cost = super::immediate::cost(version, &selection);
    let a_cost = super::immediate::cost(version, &a.insts);
    let b_cost = super::immediate::cost(version, &b.insts);
    // At least one arm jumps to the join; both its destination and the taken
    // arm need JUMPDEST. Every path pays JUMPI, one literal and the join label.
    if cost.0 >= 8 + a_cost.0 + b_cost.0 || cost.1 > 14 + a_cost.1.min(b_cost.1) {
        return None;
    }
    // Selection needs one word and peaks one above it, exactly the original
    // condition plus temporary JUMPI target. The joined body sees the same stack.
    Some((selection, [yes, no, join]))
}

fn layout(module: &mut Module) -> bool {
    let old = module.block_ids().collect::<Vec<_>>();
    let Some(&entry) = old.first() else { return false };
    let mut references = IndexVec::<BlockId, usize>::from_vec(vec![0; module.blocks.len()]);
    let mut preferred =
        IndexVec::<BlockId, Option<BlockId>>::from_vec(vec![None; module.blocks.len()]);
    for &id in &old {
        for inst in &module.blocks[id].insts {
            if let InstKind::PushLabel(target) = inst.kind {
                references[target] += 1;
            }
        }
    }
    let mut roots = old.clone();
    roots.sort_by_key(|&id| {
        (id != entry, module.blocks[id].cold, std::cmp::Reverse(references[id]), id)
    });
    // Reserve unconditional trace edges before adding conditional fallthroughs.
    for &id in &roots {
        if let TerminatorKind::Jump(target) = module.blocks[id].terminator.kind {
            preferred[target].get_or_insert(id);
        }
    }
    for &id in &roots {
        if let TerminatorKind::JumpI(_, target) = module.blocks[id].terminator.kind {
            preferred[target].get_or_insert(id);
        }
    }
    roots.sort_by_key(|&id| {
        (
            id != entry,
            module.blocks[id].cold,
            preferred[id].is_some(),
            std::cmp::Reverse(references[id]),
            id,
        )
    });
    let mut placed = DenseBitSet::new_empty(module.blocks.len());
    let mut order = Vec::with_capacity(old.len());
    for mut id in roots {
        while placed.insert(id) {
            // trace_head; unconditional_successor; ...
            order.push(id);
            let target = match module.blocks[id].terminator.kind {
                TerminatorKind::Jump(target) => Some(target),
                TerminatorKind::JumpI(_, target) => Some(target),
                _ => None,
            };
            if let Some(target) = target
                && module.blocks[id].cold == module.blocks[target].cold
                && preferred[target].is_none_or(|owner| owner == id || placed.contains(owner))
            {
                id = target;
            } else {
                break;
            }
        }
    }
    let changed = order != old;
    module.layout = Some(order);
    changed
}

/// Detects observations that cannot survive relocating shared physical code.
fn sharing_observes_code(module: &Module) -> bool {
    module.block_ids().any(|id| {
        module.blocks[id].insts.iter().any(|inst| {
            matches!(inst.kind, InstKind::PushData { .. } | InstKind::PushDeferred(_))
                || (matches!(inst.kind, InstKind::PushLabel(_)) && !module.private_control_labels)
                || matches!(inst.kind, InstKind::Op(code) if op::stack_io(code).is_none())
                || matches!(
                    inst.kind,
                    InstKind::Op(
                        op::JUMP
                            | op::JUMPI
                            | op::JUMPDEST
                            | op::PC
                            | op::CODESIZE
                            | op::CODECOPY
                            | op::EXTCODECOPY
                            | op::EXTCODESIZE
                            | op::EXTCODEHASH
                    )
                )
        })
    }) || (module
        .block_ids()
        .any(|id| module.blocks[id].terminator.kind == TerminatorKind::DynamicJump)
        && super::verify::has_unknown_jump(module))
}

/// Whether an instruction reads remaining gas or forwards it to another execution.
fn observes_gas(inst: &super::Instruction) -> bool {
    matches!(
        inst.kind,
        InstKind::Op(
            op::GAS
                | op::CALL
                | op::CALLCODE
                | op::DELEGATECALL
                | op::STATICCALL
                | op::EXTCALL
                | op::EXTDELEGATECALL
                | op::EXTSTATICCALL
                | op::CREATE
                | op::CREATE2
                | op::EOFCREATE
        )
    )
}

fn terminal_dedup(module: &mut Module) -> bool {
    let ids = module.block_ids().collect::<Vec<_>>();
    if sharing_observes_code(module) {
        return false;
    }
    let mut safety = None;
    let mut changed = false;
    for (index, &id) in ids.iter().enumerate() {
        if !matches!(
            module.blocks[id].terminator.kind,
            TerminatorKind::Return | TerminatorKind::Revert | TerminatorKind::SelfDestruct
        ) || module.blocks[id].insts.len() < 3
            || module.blocks[id].insts.iter().any(observes_gas)
        {
            continue;
        }
        for &other in &ids[index + 1..] {
            if module.blocks[id].insts == module.blocks[other].insts
                && module.blocks[id].terminator == module.blocks[other].terminator
            {
                // Model the jump's temporary address before it transfers to the
                // unchanged body. Only canonical exits acquire new predecessors;
                // every later duplicate keeps its original incoming bound.
                if safety.is_none() {
                    let Ok(heights) = super::verify::stack_heights(module) else { return changed };
                    safety = Some((heights, super::verify::physical_reachability(module)));
                }
                let (heights, reachable) = safety.as_ref().unwrap();
                if !super::verify::rewrite_fits(
                    module,
                    heights,
                    reachable,
                    other,
                    0,
                    module.blocks[other].insts.len(),
                    &[InstKind::PushLabel(id).into()],
                ) {
                    continue;
                }
                merge_block_debug(module, id, other);
                module.blocks[id].cold &= module.blocks[other].cold;
                // duplicate terminal body -> jump canonical_body
                let invoke = module.blocks[other]
                    .insts
                    .first()
                    .and_then(|inst| inst.debug.as_deref())
                    .and_then(|debug| debug.function_invoke);
                module.blocks[other].insts.clear();
                module.blocks[other].terminator = TerminatorKind::Jump(id).into();
                if let Some(invoke) = invoke {
                    // NOTE: The duplicate's real transfer owns this invocation. The shared
                    // body cannot independently identify which entry path was taken.
                    if let Some(debug) = module.blocks[id]
                        .insts
                        .first_mut()
                        .and_then(|inst| inst.debug.as_deref_mut())
                    {
                        debug.function_invoke = None;
                    }
                    module.blocks[id].function_invoke = None;
                    module.blocks[other].terminator.debug = Some(Box::new(super::DebugMetadata {
                        function_invoke: Some(invoke),
                        ..Default::default()
                    }));
                }
                changed = true;
            }
        }
    }
    changed
}

/// Removes taken-only duplicate exits while retaining every possible fallthrough.
fn redirect_terminals(module: &mut Module) -> bool {
    let ids = module.block_ids().collect::<Vec<_>>();
    let mut protected = DenseBitSet::new_empty(module.blocks.len());
    let mut taken = DenseBitSet::new_empty(module.blocks.len());
    let mut forwards_gas = None;
    let mut indirect_control = false;
    let mut has_dynamic = false;
    if let Some(&entry) = ids.first() {
        protected.insert(entry);
    }
    for (position, &id) in ids.iter().enumerate() {
        let next = ids.get(position + 1).copied();
        let block = &module.blocks[id];
        if block.insts.iter().any(|inst| {
            if let InstKind::PushLabel(target) = inst.kind {
                indirect_control = true;
                taken.insert(target);
                protected.insert(target);
            }
            (matches!(inst.kind, InstKind::PushLabel(_)) && !module.private_control_labels)
                || matches!(inst.kind, InstKind::PushData { .. } | InstKind::PushDeferred(_))
                || matches!(inst.kind, InstKind::Op(code) if op::stack_io(code).is_none())
                || matches!(
                    inst.kind,
                    InstKind::Op(
                        op::JUMP
                            | op::JUMPI
                            | op::JUMPDEST
                            | op::PC
                            | op::CODESIZE
                            | op::CODECOPY
                            | op::EXTCODECOPY
                            | op::EXTCODESIZE
                            | op::EXTCODEHASH
                            | op::GAS
                    )
                )
        }) {
            return false;
        }
        match &block.terminator.kind {
            TerminatorKind::Jump(target) => {
                protected.insert(*target);
                if Some(*target) != next {
                    taken.insert(*target);
                }
            }
            TerminatorKind::JumpI(yes, no) => {
                taken.insert(*yes);
                protected.insert(*no);
                if Some(*no) != next {
                    taken.insert(*no);
                }
            }
            TerminatorKind::IndexedJump(targets) => {
                for &target in targets {
                    taken.insert(target);
                }
            }
            TerminatorKind::DynamicJump => {
                indirect_control = true;
                has_dynamic = true;
            }
            _ => {}
        }
    }
    let candidates = ids
        .iter()
        .copied()
        .filter(|&id| {
            matches!(
                module.blocks[id].terminator.kind,
                TerminatorKind::Return | TerminatorKind::Revert | TerminatorKind::SelfDestruct
            )
        })
        .collect::<Vec<_>>();
    let mut targets = module.blocks.indices().collect::<IndexVec<BlockId, _>>();
    let mut changed = false;
    for (index, &id) in candidates.iter().enumerate() {
        if targets[id] != id || (indirect_control && !taken.contains(id)) {
            continue;
        }
        for &other in &candidates[index + 1..] {
            if targets[other] == other
                && taken.contains(other)
                && !protected.contains(other)
                && module.blocks[id].insts == module.blocks[other].insts
                && module.blocks[id].terminator == module.blocks[other].terminator
            {
                // Indirect control forbids forwarded-gas changes; an added destination
                // must not alter a callee's gas. The module is immutable until redirection.
                if (indirect_control || !taken.contains(id))
                    && *forwards_gas.get_or_insert_with(|| {
                        ids.iter().any(|&id| module.blocks[id].insts.iter().any(observes_gas))
                    })
                {
                    if indirect_control {
                        return false;
                    }
                    break;
                }
                targets[other] = id;
                changed = true;
            }
        }
    }
    if changed && has_dynamic && super::verify::has_unknown_jump(module) {
        return false;
    }
    if changed {
        for &source in &ids {
            if targets[source] != source {
                merge_block_debug(module, targets[source], source);
            }
        }
        // taken duplicate_exit -> taken earlier_identical_exit
        // earlier_identical_exit; duplicate_exit -> earlier_identical_exit
        redirect(module, &targets);
        module.layout = Some(ids.into_iter().filter(|&id| targets[id] == id).collect());
    }
    changed
}

fn share_reverts(module: &mut Module) -> bool {
    let mut changed = terminal_dedup(module);
    for id in module.block_ids().collect::<Vec<_>>() {
        let block = &module.blocks[id];
        if let TerminatorKind::Jump(no) = block.terminator.kind
            && module.blocks[no].cold
            && module.blocks[no].terminator.kind == TerminatorKind::Revert
            && module.blocks[no].insts.len() == 2
            && module.blocks[no]
                .insts
                .iter()
                .all(|inst| matches!(inst.kind, InstKind::Push(value) if value.is_zero()))
            && let [.., invert, target, branch] = block.insts.as_slice()
            && invert.kind == InstKind::Op(op::ISZERO)
            && let InstKind::PushLabel(yes) = target.kind
            && branch.kind == InstKind::Op(op::JUMPI)
            && [invert, target, branch].iter().all(|inst| inst.stack_effect.is_none())
        {
            let mut debug = branch.debug.clone();
            merge_debug(&mut debug, invert.debug.as_deref());
            merge_debug(&mut debug, target.debug.as_deref());
            merge_debug(&mut debug, block.terminator.debug.as_deref());
            let block = &mut module.blocks[id];
            // iszero; push hot; jumpi; jump revert
            // -> push revert; jumpi; jump hot
            block.insts.truncate(block.insts.len() - 3);
            block.insts.extend([InstKind::PushLabel(no), InstKind::Op(op::JUMPI)].map(|kind| {
                let mut inst: super::Instruction = kind.into();
                inst.debug = debug.clone();
                if let Some(debug) = &mut inst.debug {
                    debug.function_invoke = None;
                    debug.function_exit = None;
                }
                inst
            }));
            block.terminator = TerminatorKind::Jump(yes).into();
            block.terminator.debug = debug;
            changed = true;
        }
    }
    changed
}

/// Matches conditional destinations through one empty, unannotated forwarding block.
fn forwarded_conditional(
    module: &Module,
    a: &Terminator,
    b: &Terminator,
) -> Option<TerminatorKind> {
    if a.stack_effect != b.stack_effect {
        return None;
    }
    let (TerminatorKind::JumpI(ay, an), TerminatorKind::JumpI(by, bn)) = (&a.kind, &b.kind) else {
        return None;
    };
    let forward = |id: BlockId| {
        let block = &module.blocks[id];
        if block.insts.is_empty()
            && block.terminator.stack_effect.is_none()
            && let TerminatorKind::Jump(target) = block.terminator.kind
            && target != id
        {
            target
        } else {
            id
        }
    };
    let (yes, no) = (forward(*ay), forward(*an));
    (yes == forward(*by) && no == forward(*bn)).then_some(TerminatorKind::JumpI(yes, no))
}

fn tail_merge(gcx: Gcx<'_>, module: &mut Module) -> bool {
    // A new transfer can affect any later gas observation, including forwarded
    // gas in a callee or initializer. No continuation-level exclusion is proved.
    if sharing_observes_code(module)
        || module.block_ids().any(|id| module.blocks[id].insts.iter().any(observes_gas))
    {
        return false;
    }
    let timer = PassTimer::new(gcx.sess.opts.unstable.time_passes);
    let Ok(heights) = super::verify::stack_heights(module) else { return false };
    timer.finish("EVM analysis", module.name, "tail-merge-stack", false);
    let reachable = super::verify::physical_reachability(module);
    let size = gcx.sess.opts.optimization.is_size();
    let version = gcx.sess.opts.evm_version;
    let ids = module.block_ids().collect::<Vec<_>>();
    let mut changed = false;
    for (index, &id) in ids.iter().enumerate() {
        if !size && module.blocks[id].loop_header {
            continue;
        }
        let mut tried_short_tail = false;
        for &other in &ids[index + 1..] {
            if !size && module.blocks[other].loop_header {
                continue;
            }
            let a = &module.blocks[id];
            let b = &module.blocks[other];
            let forwarded = if a.terminator == b.terminator {
                None
            } else if let Some(kind) = forwarded_conditional(module, &a.terminator, &b.terminator) {
                Some(kind)
            } else {
                continue;
            };
            let mut common =
                a.insts.iter().rev().zip(b.insts.iter().rev()).take_while(|(a, b)| a == b).count();
            // prefix; [gas; sub; call]; suffix -> prefix; [gas; sub; call]; jump tail
            // A new transfer may start only outside a kept instruction pair.
            while common != 0
                && (!super::split_allowed(&a.insts, a.insts.len() - common)
                    || !super::split_allowed(&b.insts, b.insts.len() - common))
            {
                common -= 1;
            }
            let suffix_bytes = a.insts[a.insts.len() - common..]
                .iter()
                .map(|inst| match inst.kind {
                    InstKind::Push(value) => op::push_len(version, value),
                    InstKind::Dup(depth) | InstKind::Swap(depth) => {
                        if depth <= 16 {
                            1
                        } else {
                            2
                        }
                    }
                    InstKind::Exchange(..) => 2,
                    _ => 1,
                })
                .sum::<usize>();
            let terminal_bytes = match a.terminator.kind {
                TerminatorKind::Jump(_) | TerminatorKind::JumpI(..) => 3,
                _ => 1,
            };
            let short = suffix_bytes + terminal_bytes < 7;
            if common < 3
                || (short
                    && (!size
                        || tried_short_tail
                        || suffix_bytes + terminal_bytes != 6
                        || !matches!(
                            a.terminator.kind,
                            TerminatorKind::Return | TerminatorKind::Revert
                        )))
            {
                continue;
            }
            let transfer = [InstKind::Push(alloy_primitives::U256::ZERO).into()];
            if !super::verify::rewrite_fits(
                module,
                &heights,
                &reachable,
                id,
                a.insts.len() - common,
                a.insts.len(),
                &transfer,
            ) || !super::verify::rewrite_fits(
                module,
                &heights,
                &reachable,
                other,
                b.insts.len() - common,
                b.insts.len(),
                &transfer,
            ) {
                continue;
            }
            let mut additional = Vec::new();
            if short {
                // A six-byte suffix has one exact starting point in this block.
                // Scan its group once; committing an initial pair can lose bytes.
                tried_short_tail = true;
                let suffix = &a.insts[a.insts.len() - common..];
                for &source in &ids[index + 1..] {
                    let block = &module.blocks[source];
                    if source != other
                        && (size || !block.loop_header)
                        && block.terminator == a.terminator
                        && block.insts.ends_with(suffix)
                        && super::verify::rewrite_fits(
                            module,
                            &heights,
                            &reachable,
                            source,
                            block.insts.len() - common,
                            block.insts.len(),
                            &transfer,
                        )
                    {
                        additional.push(source);
                    }
                }
                let count = 2 + additional.len();
                let body = suffix_bytes + terminal_bytes;
                // n * body -> body + jumpdest + n * (push label; jump)
                // Reserve five bytes per transfer (PUSH3 plus JUMP) and
                // a new shared JUMPDEST; do not credit a possible fallthrough.
                if count < 3 || count * body <= body + 1 + 5 * count {
                    continue;
                }
            }
            let mut terminator = a.terminator.clone();
            if let Some(kind) = forwarded {
                // jumpi empty_forwarder, other -> jumpi forwarded_target, other
                terminator.kind = kind;
            }
            let mut tail = Block {
                insts: a.insts[a.insts.len() - common..].to_vec(),
                terminator,
                cold: a.cold
                    && b.cold
                    && additional.iter().all(|&source| module.blocks[source].cold),
                loop_header: a.loop_header
                    || b.loop_header
                    || additional.iter().any(|&source| module.blocks[source].loop_header),
                function_invoke: None,
            };
            if module.debug_info_tracked {
                for source in [other].into_iter().chain(additional.iter().copied()) {
                    let source = &module.blocks[source];
                    for (target, source) in
                        tail.insts.iter_mut().zip(&source.insts[source.insts.len() - common..])
                    {
                        merge_debug(&mut target.debug, source.debug.as_deref());
                    }
                    merge_debug(&mut tail.terminator.debug, source.terminator.debug.as_deref());
                }
                // Invocation belongs to the existing per-source jump into the shared suffix.
                if let Some(debug) = tail.insts.first_mut().and_then(|inst| inst.debug.as_mut()) {
                    debug.function_invoke = None;
                }
            }
            // prefix_1; suffix; exit -> prefix_1; jump shared
            // ...
            // prefix_n; suffix; exit -> prefix_n; jump shared
            // shared: suffix; exit
            let shared = module.append_block(tail);
            for source in [id, other].into_iter().chain(additional) {
                let block = &mut module.blocks[source];
                let first = block.insts[block.insts.len() - common].debug.as_deref();
                // NOTE: A shared suffix's multiple origins cannot identify this
                // incoming edge. Prefer a unique source terminator in that case.
                let mut debug = first
                    .filter(|debug| !debug.dropped && debug.source_spans.len() == 1)
                    .or_else(|| {
                        block
                            .terminator
                            .debug
                            .as_deref()
                            .filter(|debug| !debug.dropped && debug.source_spans.len() == 1)
                    })
                    .cloned()
                    .map(Box::new);
                if let Some(invoke) = first.and_then(|debug| debug.function_invoke) {
                    debug.get_or_insert_with(Default::default).function_invoke = Some(invoke);
                } else if let Some(debug) = &mut debug {
                    debug.function_invoke = None;
                }
                block.insts.truncate(block.insts.len() - common);
                block.terminator = TerminatorKind::Jump(shared).into();
                block.terminator.debug = debug;
                if let Some(debug) = &mut block.terminator.debug {
                    debug.function_exit = None;
                }
            }
            module.layout.get_or_insert_with(|| ids.clone()).push(shared);
            changed = true;
        }
    }
    changed
}

fn empty_revert_program(module: &mut Module) -> bool {
    let Some(entry) = module.block_ids().next() else { return false };
    let mut visited = DenseBitSet::new_empty(module.blocks.len());
    let mut active = DenseBitSet::new_empty(module.blocks.len());
    let mut pending = vec![(entry, false)];
    while let Some((id, exiting)) = pending.pop() {
        if exiting {
            active.remove(id);
            continue;
        }
        if active.contains(id) {
            return false;
        }
        if !visited.insert(id) {
            continue;
        }
        active.insert(id);
        pending.push((id, true));
        let block = &module.blocks[id];
        if block.insts.iter().any(|inst| inst.stack_effect.is_some() || match inst.kind {
            InstKind::Push(_) | InstKind::Dup(_) | InstKind::Swap(_) | InstKind::Exchange(..) => false,
            InstKind::Op(opcode) => !matches!(opcode, op::ADD..=op::SIGNEXTEND | op::LT..=op::SAR | op::CLZ
                | op::POP | op::CALLVALUE | op::CALLDATASIZE | op::CALLDATALOAD | op::MLOAD | op::MSTORE | op::MSTORE8),
            _ => true,
        }) { return false; }
        match block.terminator.kind {
            TerminatorKind::Jump(target) => pending.push((target, false)),
            TerminatorKind::JumpI(yes, no) => {
                pending.push((yes, false));
                pending.push((no, false));
            }
            TerminatorKind::Revert => {
                if !matches!(block.insts.as_slice(), [.., size, offset]
                    if matches!(size.kind, InstKind::Push(value) if value.is_zero())
                    && matches!(offset.kind, InstKind::Push(value) if value.is_zero()))
                {
                    return false;
                }
            }
            _ => return false,
        }
    }
    if visited.iter().count() == 1 && module.blocks[entry].insts.len() == 2 {
        return false;
    }
    // Every acyclic path exits with revert(0, 0), and has no observable side effects.
    // push 0
    // push 0
    // revert
    module.blocks[entry].insts = vec![
        InstKind::Push(alloy_primitives::U256::ZERO).into(),
        InstKind::Push(alloy_primitives::U256::ZERO).into(),
    ];
    module.blocks[entry].terminator = TerminatorKind::Revert.into();
    if module.debug_info_tracked {
        for inst in &mut module.blocks[entry].insts {
            inst.debug =
                Some(Box::new(super::DebugMetadata { dropped: true, ..Default::default() }));
        }
        module.blocks[entry].terminator.debug =
            Some(Box::new(super::DebugMetadata { dropped: true, ..Default::default() }));
    }
    module.layout = Some(vec![entry]);
    true
}

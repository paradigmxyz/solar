//! Control-flow simplification, terminal sharing and block placement.
//!
//! Simplification forwards empty jumps, removes redundant conditional edges,
//! joins identical conditional successors, merges uniquely entered blocks, and
//! retains the closure of all explicit and
//! address-taken targets. Unknown computed jumps retain every address-taken block.
//! Stable block IDs survive all removals and layout changes. Terminal sharing
//! redirects identical exiting suffixes only when their encoded body exceeds a
//! jump; it never merges distinct effects or instruction metadata. Placement
//! forms unconditional and conditional-false traces, removing their encoded
//! PUSH/JUMP transfers while keeping cold traces after hot ones. Existing
//! unconditional trace edges reserve their targets in hotness/reference order,
//! preventing new conditional traces from stealing their preferred fallthrough. These transforms
//! operate before assembly, where block references and loop/cold annotations are still explicit.

use super::{Block, BlockId, EvmPass, InstKind, Module, TerminatorKind, verify::successors};
use crate::{backend::evm::op, timing::PassTimer};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec};
use solar_sema::Gcx;

pub(super) struct CfgSimplify;
pub(super) struct BlockLayout;
pub(super) struct TerminalDedup;
pub(super) struct ShareReverts;
pub(super) struct TailMerge;

impl EvmPass for CfgSimplify {
    fn name(&self) -> &'static str {
        "cfg-simplify"
    }
    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        simplify(module)
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
    successors(&block.terminator.kind).into_iter().chain(
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

fn simplify(module: &mut Module) -> bool {
    if empty_revert_program(module) {
        return true;
    }
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
            let block = &mut module.blocks[id];
            // jumpi identical_body, identical_body -> pop; jump identical_body
            if same_successors && let TerminatorKind::JumpI(yes, _) = block.terminator.kind {
                block.insts.push(InstKind::Op(op::POP).into());
                block.terminator = TerminatorKind::Jump(yes).into();
                progress = true;
            }
            // jumpi target, target -> pop; jump target
            if let TerminatorKind::JumpI(yes, no) = block.terminator.kind
                && yes == no
            {
                block.insts.push(InstKind::Op(op::POP).into());
                block.terminator = TerminatorKind::Jump(yes).into();
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
                block.insts.truncate(block.insts.len() - 2);
                block.insts.push(InstKind::Op(op::POP).into());
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
        for &id in &ids {
            if !removed.contains(id)
                && let TerminatorKind::Jump(target) = module.blocks[id].terminator.kind
                && target != id
                && incoming[target] == 1
                && !exposed.contains(target)
                && !removed.contains(target)
                && Some(target) != ids.first().copied()
            {
                let target_block = module.blocks[target].clone();
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

fn terminal_dedup(module: &mut Module) -> bool {
    let ids = module.block_ids().collect::<Vec<_>>();
    let mut changed = false;
    for (index, &id) in ids.iter().enumerate() {
        if !matches!(
            module.blocks[id].terminator.kind,
            TerminatorKind::Return | TerminatorKind::Revert | TerminatorKind::SelfDestruct
        ) || module.blocks[id].insts.len() < 3
        {
            continue;
        }
        for &other in &ids[index + 1..] {
            if module.blocks[id].insts == module.blocks[other].insts
                && module.blocks[id].terminator == module.blocks[other].terminator
            {
                module.blocks[id].cold &= module.blocks[other].cold;
                // duplicate terminal body -> jump canonical_body
                module.blocks[other].insts.clear();
                module.blocks[other].terminator = TerminatorKind::Jump(id).into();
                changed = true;
            }
        }
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
            let block = &mut module.blocks[id];
            // iszero; push hot; jumpi; jump revert
            // -> push revert; jumpi; jump hot
            block.insts.truncate(block.insts.len() - 3);
            block.insts.extend([InstKind::PushLabel(no), InstKind::Op(op::JUMPI)].map(Into::into));
            block.terminator = TerminatorKind::Jump(yes).into();
            changed = true;
        }
    }
    changed
}

fn tail_merge(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let timer = PassTimer::new(gcx.sess.opts.unstable.time_passes);
    let Ok(heights) = super::verify::stack_heights(module) else { return false };
    timer.finish("EVM analysis", module.name, "tail-merge-stack", false);
    let size = gcx.sess.opts.optimization.is_size();
    let version = gcx.sess.opts.evm_version;
    let ids = module.block_ids().collect::<Vec<_>>();
    let mut changed = false;
    for (index, &id) in ids.iter().enumerate() {
        if !size && module.blocks[id].loop_header {
            continue;
        }
        for &other in &ids[index + 1..] {
            if !size && module.blocks[other].loop_header {
                continue;
            }
            let a = &module.blocks[id];
            let b = &module.blocks[other];
            if a.terminator != b.terminator {
                continue;
            }
            let common =
                a.insts.iter().rev().zip(b.insts.iter().rev()).take_while(|(a, b)| a == b).count();
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
            let terminal_bytes =
                if matches!(a.terminator.kind, TerminatorKind::Jump(_)) { 3 } else { 1 };
            if common < 3 || suffix_bytes + terminal_bytes < 7 {
                continue;
            }
            let transfer = [InstKind::Push(alloy_primitives::U256::ZERO).into()];
            if !super::verify::rewrite_fits(
                module,
                &heights,
                id,
                a.insts.len() - common,
                a.insts.len(),
                &transfer,
            ) || !super::verify::rewrite_fits(
                module,
                &heights,
                other,
                b.insts.len() - common,
                b.insts.len(),
                &transfer,
            ) {
                continue;
            }
            let tail = Block {
                insts: a.insts[a.insts.len() - common..].to_vec(),
                terminator: a.terminator.clone(),
                cold: a.cold && b.cold,
                loop_header: a.loop_header || b.loop_header,
            };
            // prefix_a; suffix; exit -> prefix_a; jump shared
            // prefix_b; suffix; exit -> prefix_b; jump shared
            // shared: suffix; exit
            let shared = module.append_block(tail);
            for source in [id, other] {
                let block = &mut module.blocks[source];
                block.insts.truncate(block.insts.len() - common);
                block.terminator = TerminatorKind::Jump(shared).into();
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
    module.layout = Some(vec![entry]);
    true
}

//! Whole-block coalescing for equivalent acyclic physical control-flow regions.
//!
//! Equal exits seed reverse-CFG hashing. Once all successors of a block have
//! classes, its exact instructions, ordered successor classes and original
//! fallthrough shape form its key.
//! This discovers cloned wrappers whose only differences are block identities,
//! without pairwise scans or rediscovering Solidity semantics. Cycles and blocks
//! leading into cycles retain their identities. Instruction effects and glued
//! boundaries participate in equality; debug provenance does not.
//!
//! The earliest laid-out member owns each class. Owners and their original
//! fallthrough chains survive, so redirecting a removed region never adds an
//! encoded transfer. Code observations, gas observations, pushed addresses,
//! computed transfers and indexed tables conservatively disable the pass. These
//! exclusions also prevent unproved entries into removed blocks. Whole-block
//! redirection preserves the existing stack operations and their input demands.
//!
//! Admission requires a strict encoded-size saving: removed instruction bytes
//! alone must exceed new JUMPDESTs plus a reserve of one byte per surviving label
//! push when destinations are added. That reserve is used only below 256 bytes
//! of possible growth, which bounds each label-width increase to one byte even
//! through assembler relaxation. Gas mode additionally permits at most one new
//! destination-marker increase across one equivalence class. That class is
//! acyclic, and each edge lowers its reverse-DAG height, so an execution cannot
//! visit two members of one class. Matching fallthrough shapes also prevent a
//! cloned path acquiring an owner's jump. This bounded marker tradeoff needs
//! corpus measurements against the accepted gas baseline. The pass runs after
//! layout and retains its surviving order. It does not extract partial tails.

use super::{
    BlockId, EvmPass, InstKind, Instruction, Module, TerminatorKind, cfg, verify::successors,
};
use crate::backend::evm::op;
use solar_config::EvmVersion;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
use solar_sema::Gcx;
use std::{
    collections::VecDeque,
    hash::{Hash, Hasher},
};

pub(super) struct BlockDedup;

impl EvmPass for BlockDedup {
    fn name(&self) -> &'static str {
        "block-dedup"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        coalesce(module, gcx.sess.opts.evm_version, gcx.sess.opts.optimization.is_gas())
    }
}

/// Semantic block contents with already classified successors.
#[derive(PartialEq, Eq)]
struct Key<'a> {
    insts: &'a [Instruction],
    terminator: TerminatorKind,
    stack_effect: Option<(u8, u8)>,
    keep_with_next: bool,
    cold: bool,
    loop_header: bool,
    fallthrough: bool,
}

impl Hash for Key<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.insts.len().hash(state);
        for inst in self.insts {
            inst.kind.hash(state);
            inst.stack_effect.hash(state);
            inst.keep_with_next.hash(state);
        }
        self.terminator.hash(state);
        self.stack_effect.hash(state);
        self.keep_with_next.hash(state);
        self.cold.hash(state);
        self.loop_header.hash(state);
        self.fallthrough.hash(state);
    }
}

fn coalesce(module: &mut Module, version: EvmVersion, gas: bool) -> bool {
    if module.block_ids().any(|id| {
        let block = &module.blocks[id];
        matches!(
            block.terminator.kind,
            TerminatorKind::DynamicJump | TerminatorKind::IndexedJump(_)
        ) || block
            .insts
            .iter()
            .any(|inst| cfg::observes_gas(inst) || matches!(inst.kind, InstKind::PushLabel(_)))
    }) || cfg::sharing_observes_code(module)
    {
        return false;
    }
    let ids = module.block_ids().collect::<Vec<_>>();
    let classes = classify(module, &ids);
    let mut owners = FxHashMap::default();
    let mut targets = module.blocks.indices().collect::<IndexVec<BlockId, _>>();
    for &id in &ids {
        targets[id] = *owners.entry(classes[id]).or_insert(id);
    }
    if ids.iter().all(|&id| targets[id] == id) {
        return false;
    }

    // A surviving source keeps its original fallthrough target. Taking this
    // closure preserves internal owner traces, while a removed clone's internal
    // fallthrough edges impose no restriction on removing the rest of its region.
    let mut next = IndexVec::<BlockId, Option<BlockId>>::from_vec(vec![None; module.blocks.len()]);
    for pair in ids.windows(2) {
        next[pair[0]] = Some(pair[1]);
    }
    let mut pending = ids.iter().copied().filter(|&id| targets[id] == id).collect::<Vec<_>>();
    while let Some(id) = pending.pop() {
        let fallthrough = match module.blocks[id].terminator.kind {
            TerminatorKind::Jump(target) | TerminatorKind::JumpI(_, target) => Some(target),
            _ => None,
        };
        if let Some(target) = fallthrough
            && Some(target) == next[id]
            && targets[target] != target
        {
            targets[target] = target;
            pending.push(target);
        }
    }
    let order = ids.iter().copied().filter(|&id| targets[id] == id).collect::<Vec<_>>();
    if order.len() == ids.len() {
        return false;
    }
    if !profitable(module, &ids, &order, &targets, &classes, version, gas) {
        return false;
    }

    for &id in &ids {
        if targets[id] != id {
            cfg::merge_block_debug(module, targets[id], id);
        }
    }
    // branch clone_entry -> branch equivalent_owner
    // <owner region>; <clone region> -> <owner region>
    for &id in &order {
        remap(&mut module.blocks[id].terminator.kind, &targets);
    }
    module.layout = Some(order);
    true
}

fn classify(module: &Module, ids: &[BlockId]) -> IndexVec<BlockId, BlockId> {
    let mut fallthroughs = DenseBitSet::new_empty(module.blocks.len());
    for pair in ids.windows(2) {
        if matches!(module.blocks[pair[0]].terminator.kind,
            TerminatorKind::Jump(target) | TerminatorKind::JumpI(_, target) if target == pair[1])
        {
            fallthroughs.insert(pair[0]);
        }
    }
    let mut predecessors =
        IndexVec::<BlockId, Vec<BlockId>>::from_vec(vec![Vec::new(); module.blocks.len()]);
    let mut remaining = IndexVec::<BlockId, usize>::from_vec(vec![0; module.blocks.len()]);
    let mut pending = VecDeque::new();
    for &id in ids {
        for target in successors(&module.blocks[id].terminator.kind) {
            predecessors[target].push(id);
            remaining[id] += 1;
        }
        if remaining[id] == 0 {
            pending.push_back(id);
        }
    }
    let mut classes = module.blocks.indices().collect::<IndexVec<BlockId, _>>();
    let mut keys = FxHashMap::with_capacity_and_hasher(ids.len(), Default::default());
    while let Some(id) = pending.pop_front() {
        let block = &module.blocks[id];
        let mut terminator = block.terminator.kind.clone();
        // branch successor -> branch successor_class (in the analysis key only)
        remap(&mut terminator, &classes);
        let key = Key {
            insts: &block.insts,
            terminator,
            stack_effect: block.terminator.stack_effect,
            keep_with_next: block.terminator.keep_with_next,
            cold: block.cold,
            loop_header: block.loop_header,
            fallthrough: fallthroughs.contains(id),
        };
        classes[id] = *keys.entry(key).or_insert(id);
        for &source in &predecessors[id] {
            remaining[source] -= 1;
            if remaining[source] == 0 {
                pending.push_back(source);
            }
        }
    }
    classes
}

/// Retargets ordered structural edges; pushed and computed addresses are excluded.
fn remap(kind: &mut TerminatorKind, targets: &IndexVec<BlockId, BlockId>) {
    // jump old -> jump owner
    // jumpi old_true, old_false -> jumpi true_owner, false_owner
    match kind {
        TerminatorKind::Jump(target) => *target = targets[*target],
        TerminatorKind::JumpI(yes, no) => {
            *yes = targets[*yes];
            *no = targets[*no];
        }
        _ => {}
    }
}

fn profitable(
    module: &Module,
    old: &[BlockId],
    new: &[BlockId],
    targets: &IndexVec<BlockId, BlockId>,
    classes: &IndexVec<BlockId, BlockId>,
    version: EvmVersion,
    gas: bool,
) -> bool {
    let identity = module.blocks.indices().collect::<IndexVec<BlockId, _>>();
    let (old_destinations, _) = destinations(module, old, &identity);
    let (new_destinations, references) = destinations(module, new, targets);
    let markers = new_destinations.iter().filter(|&id| !old_destinations.contains(id)).count();
    if gas {
        // A preserved fallthrough can enter a noncanonical member of a class.
        // Count every class with an addressable surviving member, even when the
        // removed member maps to a different owner that needs no JUMPDEST.
        let mut addressed = DenseBitSet::new_empty(module.blocks.len());
        for id in new_destinations.iter() {
            addressed.insert(classes[id]);
        }
        let mut increases = DenseBitSet::new_empty(module.blocks.len());
        for &id in old {
            if addressed.contains(classes[id]) && !old_destinations.contains(id) {
                increases.insert(classes[id]);
            }
        }
        if increases.iter().take(2).count() > 1 {
            return false;
        }
    }
    // Every owner precedes the original target. With no new marker, deleting
    // bytes cannot widen a label reference. Otherwise reserve one byte for every
    // surviving address push; a total below 256 is a relaxation upper bound.
    let reserve = if markers == 0 { 0 } else { markers + references };
    if reserve >= 256 {
        return false;
    }
    let saved = old
        .iter()
        .copied()
        .filter(|&id| targets[id] != id)
        .flat_map(|id| &module.blocks[id].insts)
        .map(|inst| match inst.kind {
            InstKind::Push(value) => op::push_len(version, value),
            InstKind::PushImmutable { width, .. } => usize::from(width) + 1,
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
    // Removed terminators, address markers and shortened references are extra
    // savings; charging only the original instructions leaves those uncredited.
    saved > reserve
}

/// Counts encoded address pushes and their destination markers in a fixed order.
fn destinations(
    module: &Module,
    order: &[BlockId],
    targets: &IndexVec<BlockId, BlockId>,
) -> (DenseBitSet<BlockId>, usize) {
    let mut destinations = DenseBitSet::new_empty(module.blocks.len());
    let mut references = 0;
    for (position, &id) in order.iter().enumerate() {
        let next = order.get(position + 1).copied();
        let mut record = |target| {
            destinations.insert(target);
            references += 1;
        };
        match module.blocks[id].terminator.kind {
            TerminatorKind::Jump(target) => {
                if Some(targets[target]) != next {
                    record(targets[target]);
                }
            }
            TerminatorKind::JumpI(yes, no) => {
                record(targets[yes]);
                if Some(targets[no]) != next {
                    record(targets[no]);
                }
            }
            _ => {}
        }
    }
    (destinations, references)
}

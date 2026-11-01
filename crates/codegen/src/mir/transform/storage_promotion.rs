//! Storage scalar promotion for simple loop-carried storage updates.
//!
//! This pass recognizes loops that repeatedly update one or more exact storage
//! slots and rewrites the loop to update memory-backed scalars instead. Final
//! values are stored back to storage once on each clean loop exit.
//!
//! Safety contract:
//! - promote only exact storage aliases that are loop-invariant
//! - promote multiple slots only when they are pairwise provably disjoint
//! - reject loops with calls, storage accesses the rewrite cannot update, or non-isolated exits
//! - use shared read/write effects to detect storage traffic inside semantic operations
//! - flush dirty promoted values before any clean observable exit
//! - skip the flush on revert exits: `revert`/`invalid` roll back every storage write of the frame,
//!   so the unflushed slot is unobservable there; reads of the promoted slot on those paths are
//!   rewritten to the memory temp so revert data still sees the current value
//! - reject opaque storage accesses on rollback exits because the rewrite cannot update them.
//! - leave loop-variant mapping/array slots in storage

use crate::mir::{
    BlockId, Callee, EffectKind, Function, Immediate, InstId, InstKind, Instruction, MirType,
    Module, PtrLocation, StorageAlias, Terminator, Value, ValueId,
    analysis::{AddressSpace, AliasAnalysis, Loop, LoopAnalyzer},
    memory::EvmMemoryLayout,
    pass::{MirPass, run_function_pass},
    transform::utils::redirect_successor_predecessors,
    utils as mir_utils,
};
use alloy_primitives::U256;
use solar_data_structures::{
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};

/// Function pass for loop-carried storage scalar promotion.
pub(crate) struct StorageScalarPromotion;

impl MirPass for StorageScalarPromotion {
    fn name(&self) -> &'static str {
        "storage-promotion"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, analyses| {
            // The pass currently introduces absolute low-memory temporaries, so it
            // only handles externally callable runtime entries.
            if func.selector.is_none() {
                return false;
            }

            // Annotating aliases changes metadata only, which is not reported as a change.
            if func.annotate_storage_aliases(mir_utils::StorageAliasScope::Storage) {
                analyses.note_unreported_edit();
            }
            promote_loops(func)
        })
    }
}

/// A storage slot to keep in a memory temporary across one loop.
#[derive(Debug)]
struct Candidate {
    slot_value: ValueId,
    slot: StorageAlias,
    /// The last preheader store to the slot, whose value seeds the temporary. Only a lone
    /// candidate lacks one; its flush is then guarded by a dirty flag.
    init_store: Option<InstId>,
    /// Whether the temporary must be loaded from storage in the preheader.
    needs_initial_load: bool,
}

/// Promotes loops until none qualifies; returns whether any was promoted.
fn promote_loops(func: &mut Function) -> bool {
    let mut changed = false;

    // Promoting a loop can split its exit blocks and relocate the final
    // stores into new blocks, which invalidates the block sets of every
    // enclosing loop. Recompute loop info after each successful promotion
    // so later promotions reason about an accurate CFG. This terminates:
    // each promotion rewrites all of its loop's storage stores to memory
    // stores and only inserts new storage stores outside that loop.
    loop {
        let loop_info = LoopAnalyzer::new().analyze_structure(func);
        let Some((loop_data, preheader, candidates)) =
            loop_info.all_loops().find_map(|loop_data| {
                let (preheader, candidates) = find_candidates(func, loop_data)?;
                Some((loop_data, preheader, candidates))
            })
        else {
            return changed;
        };
        promote(func, loop_data, preheader, &candidates);
        changed = true;
    }
}

fn find_candidates(func: &Function, loop_data: &Loop) -> Option<(BlockId, Vec<Candidate>)> {
    let preheader = loop_data.preheader?;
    if loop_data.exit_blocks.is_empty()
        || !has_isolated_promotable_exits(func, loop_data)
        || !promotion_effects_are_safe(func, loop_data)
    {
        return None;
    }

    let inst_blocks = func.inst_block_table();
    let mut stores = FxHashMap::<StorageAlias, ValueId>::default();
    for block_id in &loop_data.blocks {
        for &inst_id in &func.blocks[block_id].instructions {
            if let InstKind::Store(store_slot, _, PtrLocation::Storage) = &func.inst(inst_id).kind {
                let alias = StorageAlias::for_value(func, *store_slot);
                if let Some(base) = alias.symbolic_base()
                    && value_defined_in_loop(func, &inst_blocks, base, loop_data)
                {
                    return None;
                }
                stores.entry(alias).or_insert(*store_slot);
            }
        }
    }
    if stores.is_empty() {
        return None;
    }

    // Distinct alias keys can still name the same runtime slot (for
    // example two symbolic mapping slots whose keys happen to be equal).
    // Promoting them to separate memory temps would desynchronize loads
    // and stores, so require every pair to be provably disjoint.
    let aliases = stores.keys().copied().collect::<Vec<_>>();
    if aliases.iter().enumerate().any(|(i, a)| aliases[i + 1..].iter().any(|b| a.may_alias(*b))) {
        return None;
    }

    let rewrite_blocks = promotion_block_ids(func, loop_data);
    let lone = stores.len() == 1;
    let mut candidates = Vec::with_capacity(stores.len());
    for (slot, slot_value) in stores {
        let candidate = match find_preheader_init_store(func, preheader, &slot) {
            Some((init_store, slot_value)) => Candidate {
                slot_value,
                slot,
                init_store: Some(init_store),
                needs_initial_load: false,
            },
            None if lone && !value_defined_in_loop(func, &inst_blocks, slot_value, loop_data) => {
                let needs_initial_load = rewrite_blocks.iter().any(|&block_id| {
                    func.blocks[block_id].instructions.iter().any(|&inst_id| {
                        matches!(
                            &func.inst(inst_id).kind,
                            InstKind::Load(load_slot, PtrLocation::Storage)
                                if func.storage_alias(inst_id, *load_slot) == slot
                        )
                    })
                });
                Candidate { slot_value, slot, init_store: None, needs_initial_load }
            }
            None => return None,
        };
        candidates.push(candidate);
    }
    candidates.sort_unstable_by_key(|candidate| candidate.init_store);

    if !loop_storage_accesses_are_safe(func, &rewrite_blocks, &candidates) {
        return None;
    }
    // A lone uninitialized slot has no preheader store, so its tail is unconstrained.
    let tail_is_safe =
        candidates[0].init_store.is_none() || preheader_tail_is_safe(func, preheader, &candidates);
    tail_is_safe.then_some((preheader, candidates))
}

fn has_isolated_promotable_exits(func: &Function, loop_data: &Loop) -> bool {
    loop_data.exit_blocks.iter().all(|&exit| {
        func.blocks[exit].predecessors.iter().all(|&pred| loop_data.blocks.contains(pred))
            && if exit_rolls_back(func, exit) {
                rollback_exit_has_no_observable_effects(func, exit)
            } else {
                !matches!(func.blocks[exit].terminator, Some(Terminator::SelfDestruct { .. }))
            }
    })
}

/// Returns true if the exit block ends by rolling back all storage writes
/// of the frame, which makes skipping the promoted-value flush sound.
fn exit_rolls_back(func: &Function, exit: BlockId) -> bool {
    func.blocks[exit].terminator.as_ref().is_some_and(Terminator::reverts)
}

/// A rollback exit must not contain calls (a callee could observe the
/// unflushed slot and leak that observation into the revert data) or
/// other instructions whose results escape the rolled-back frame.
fn rollback_exit_has_no_observable_effects(func: &Function, exit: BlockId) -> bool {
    func.blocks[exit].instructions.iter().all(|&inst_id| {
        let kind = &func.inst(inst_id).kind;
        !matches!(
            kind.effect_kind(),
            EffectKind::ExternalCall | EffectKind::ICall | EffectKind::Create
        ) && !kind.observes_gas()
    })
}

/// Blocks whose storage accesses are checked and rewritten by promotion:
/// the loop body plus rollback exits, whose reads of the promoted slot
/// must observe the memory temp instead of the stale storage value.
fn promotion_block_ids(func: &Function, loop_data: &Loop) -> Vec<BlockId> {
    let mut blocks: Vec<BlockId> = loop_data.blocks.iter().collect();
    blocks
        .extend(loop_data.exit_blocks.iter().copied().filter(|&exit| exit_rolls_back(func, exit)));
    blocks
}

fn promotion_effects_are_safe(func: &Function, loop_data: &Loop) -> bool {
    let alias = AliasAnalysis::new(func);
    for block_id in promotion_block_ids(func, loop_data) {
        for &inst_id in &func.blocks[block_id].instructions {
            let inst = func.inst(inst_id);
            if inst.kind.observes_gas() {
                return false;
            }
            match &inst.kind {
                InstKind::Load(_, PtrLocation::Storage)
                | InstKind::Store(_, _, PtrLocation::Storage) => {}
                InstKind::Store(_, _, PtrLocation::Transient)
                | InstKind::Call { .. }
                | InstKind::CallCode { .. }
                | InstKind::StaticCall { .. }
                | InstKind::DelegateCall { .. }
                | InstKind::ICall { function: Callee::Function(_), .. }
                | InstKind::Create(_, _, _)
                | InstKind::Create2(_, _, _, _) => return false,
                _ => {
                    let effects = alias.instruction_mod_ref(func, inst_id);
                    if effects.reads_space(AddressSpace::Storage)
                        || effects.writes_space(AddressSpace::Storage)
                    {
                        return false;
                    }
                }
            }
        }
    }
    true
}

fn loop_storage_accesses_are_safe(
    func: &Function,
    blocks: &[BlockId],
    candidates: &[Candidate],
) -> bool {
    for &block_id in blocks {
        for &inst_id in &func.blocks[block_id].instructions {
            match &func.inst(inst_id).kind {
                InstKind::Load(slot, PtrLocation::Storage) => {
                    if clobbers_candidate(candidates, func.storage_alias(inst_id, *slot)) {
                        return false;
                    }
                }
                InstKind::Store(slot, _, PtrLocation::Storage)
                    if !is_candidate(candidates, func.storage_alias(inst_id, *slot)) =>
                {
                    return false;
                }
                _ => {}
            }
        }
    }
    true
}

fn is_candidate(candidates: &[Candidate], alias: StorageAlias) -> bool {
    candidates.iter().any(|candidate| candidate.slot == alias)
}

/// Whether an access to `alias` may touch a candidate slot without being one of them.
fn clobbers_candidate(candidates: &[Candidate], alias: StorageAlias) -> bool {
    !is_candidate(candidates, alias)
        && candidates.iter().any(|candidate| candidate.slot.may_alias(alias))
}

/// Returns the last preheader store to `slot` and its slot value.
fn find_preheader_init_store(
    func: &Function,
    preheader: BlockId,
    slot: &StorageAlias,
) -> Option<(InstId, ValueId)> {
    func.blocks[preheader].instructions.iter().rev().find_map(|&inst_id| {
        match func.inst(inst_id).kind {
            InstKind::Store(store_slot, _, PtrLocation::Storage)
                if func.storage_alias(inst_id, store_slot) == *slot =>
            {
                Some((inst_id, store_slot))
            }
            _ => None,
        }
    })
}

fn preheader_tail_is_safe(func: &Function, preheader: BlockId, candidates: &[Candidate]) -> bool {
    let Some(first_init) = candidates
        .iter()
        .filter_map(|candidate| candidate.init_store)
        .filter_map(|inst_id| {
            func.blocks[preheader].instructions.iter().position(|&candidate| candidate == inst_id)
        })
        .min()
    else {
        return false;
    };

    for &inst_id in &func.blocks[preheader].instructions[first_init + 1..] {
        match &func.inst(inst_id).kind {
            InstKind::Load(load_slot, PtrLocation::Storage)
            | InstKind::Store(load_slot, _, PtrLocation::Storage) => {
                if clobbers_candidate(candidates, func.storage_alias(inst_id, *load_slot)) {
                    return false;
                }
            }
            InstKind::Store(.., PtrLocation::Memory)
            | InstKind::Store8(..)
            | InstKind::Copy(_, _, _, PtrLocation::Memory)
            | InstKind::DataCopy(_, _, _) => {}
            kind if kind.has_side_effects() || kind.observes_gas() => return false,
            _ => {}
        }
    }

    true
}

/// Rewrites the loop to use memory temporaries and flushes them on every clean exit.
fn promote(func: &mut Function, loop_data: &Loop, preheader: BlockId, candidates: &[Candidate]) {
    let temps = candidates
        .iter()
        .map(|candidate| (candidate.slot, allocate_temp_addr(func)))
        .collect::<FxHashMap<_, _>>();
    rewrite_preheader(func, preheader, candidates, &temps);

    // A lone slot without a preheader store is flushed only if the loop stored it, which
    // `(dirty_addr, dirty_value)` tracks.
    let dirty = if let [candidate @ Candidate { init_store: None, .. }] = candidates {
        let dirty_addr = allocate_temp_addr(func);
        let dirty_value = func.alloc_value(Value::Immediate(Immediate::I256(U256::from(1))));
        // [v = sload slot; mstore temp, v;] mstore dirty_addr, 0
        if candidate.needs_initial_load {
            let (load_inst, load_value) =
                alloc_load(func, candidate.slot_value, PtrLocation::Storage);
            let store_inst =
                alloc_store(func, temps[&candidate.slot], load_value, PtrLocation::Memory);
            func.blocks[preheader].instructions.extend([load_inst, store_inst]);
        }
        let false_word = func.alloc_value(Value::Immediate(Immediate::I256(U256::ZERO)));
        let dirty_store = alloc_store(func, dirty_addr, false_word, PtrLocation::Memory);
        func.blocks[preheader].instructions.push(dirty_store);
        Some((dirty_addr, dirty_value))
    } else {
        None
    };

    for block_id in promotion_block_ids(func, loop_data) {
        // Rollback exits never flush, so their rewritten stores do not
        // need to update the dirty flag.
        let dirty = dirty.filter(|_| loop_data.blocks.contains(block_id));
        let mut index = 0;
        while index < func.blocks[block_id].instructions.len() {
            let inst_id = func.blocks[block_id].instructions[index];
            if let InstKind::Load(slot, PtrLocation::Storage)
            | InstKind::Store(slot, _, PtrLocation::Storage) = &func.inst(inst_id).kind
                && let Some(&temp) = temps.get(&func.storage_alias(inst_id, *slot))
            {
                retarget_to_temp(func, inst_id, temp);
                if let Some((dirty_addr, dirty_value)) = dirty
                    && matches!(func.inst(inst_id).kind, InstKind::Store(..))
                {
                    let dirty_store =
                        alloc_store(func, dirty_addr, dirty_value, PtrLocation::Memory);
                    func.blocks[block_id].instructions.insert(index + 1, dirty_store);
                    index += 1;
                }
            }
            index += 1;
        }
    }

    for &exit in &loop_data.exit_blocks {
        if exit_rolls_back(func, exit) {
            continue;
        }
        for candidate in candidates {
            let temp_addr = temps[&candidate.slot];
            match dirty {
                Some((dirty_addr, _)) => insert_conditional_final_store(
                    func,
                    exit,
                    candidate.slot_value,
                    temp_addr,
                    dirty_addr,
                ),
                None => insert_final_store(func, exit, candidate.slot_value, temp_addr),
            }
        }
    }
}

/// Turns each preheader store into a store to its temporary, and the later preheader loads of
/// that slot into loads from it.
fn rewrite_preheader(
    func: &mut Function,
    preheader: BlockId,
    candidates: &[Candidate],
    temps: &FxHashMap<StorageAlias, ValueId>,
) {
    let mut seeded = FxHashSet::<StorageAlias>::default();
    for pos in 0..func.blocks[preheader].instructions.len() {
        let inst_id = func.blocks[preheader].instructions[pos];
        let temp = if let Some(candidate) =
            candidates.iter().find(|candidate| candidate.init_store == Some(inst_id))
        {
            seeded.insert(candidate.slot);
            Some(temps[&candidate.slot])
        } else if let InstKind::Load(slot, PtrLocation::Storage) = &func.inst(inst_id).kind {
            let alias = func.storage_alias(inst_id, *slot);
            seeded.contains(&alias).then(|| temps[&alias])
        } else {
            None
        };
        if let Some(temp) = temp {
            retarget_to_temp(func, inst_id, temp);
        }
    }
}

/// Turns a storage load or store into the same access to the memory temporary `temp`.
fn retarget_to_temp(func: &mut Function, inst_id: InstId, temp: ValueId) {
    let inst = func.inst_mut(inst_id);
    if let InstKind::Load(addr, location) | InstKind::Store(addr, _, location) = &mut inst.kind {
        *addr = temp;
        *location = PtrLocation::Memory;
    }
    inst.metadata.set_storage_alias(None);
}

/// Allocates `load addr` with the debug info dropped.
fn alloc_load(func: &mut Function, addr: ValueId, location: PtrLocation) -> (InstId, ValueId) {
    func.alloc_value_inst(
        Instruction::new(InstKind::Load(addr, location), Some(MirType::I256))
            .with_debug_info_dropped(),
    )
}

/// Allocates `store addr, value` with the debug info dropped.
fn alloc_store(
    func: &mut Function,
    addr: ValueId,
    value: ValueId,
    location: PtrLocation,
) -> InstId {
    func.alloc_inst(
        Instruction::new(InstKind::Store(addr, value, location), None).with_debug_info_dropped(),
    )
}

/// Allocates `v = mload temp_addr; sstore slot_value, v`.
fn alloc_flush(func: &mut Function, slot_value: ValueId, temp_addr: ValueId) -> [InstId; 2] {
    let (load_inst, load_value) = alloc_load(func, temp_addr, PtrLocation::Memory);
    [load_inst, alloc_store(func, slot_value, load_value, PtrLocation::Storage)]
}

fn insert_final_store(func: &mut Function, exit: BlockId, slot_value: ValueId, temp_addr: ValueId) {
    let flush = alloc_flush(func, slot_value, temp_addr);
    let insert_pos = func.blocks[exit]
        .instructions
        .iter()
        .take_while(|&&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
        .count();
    func.blocks[exit].instructions.splice(insert_pos..insert_pos, flush);
}

fn insert_conditional_final_store(
    func: &mut Function,
    exit: BlockId,
    slot_value: ValueId,
    temp_addr: ValueId,
    dirty_addr: ValueId,
) {
    let continuation = func.alloc_block();
    let store_block = func.alloc_block();

    let old_instructions = std::mem::take(&mut func.blocks[exit].instructions);
    let (old_terminator, terminator_metadata) = func.blocks[exit].take_terminator();

    // Keep existing exit phis in place; only the non-phi tail moves behind the dirty check.
    let split_pos = old_instructions
        .iter()
        .take_while(|&&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
        .count();
    let mut exit_instructions = old_instructions[..split_pos].to_vec();
    let continuation_instructions = old_instructions[split_pos..].to_vec();

    let (dirty_load_inst, dirty_value) = alloc_load(func, dirty_addr, PtrLocation::Memory);
    exit_instructions.push(dirty_load_inst);
    // dirty_bool = ne dirty_word, 0
    let zero = func.alloc_value(Value::Immediate(Immediate::I256(U256::ZERO)));
    let (normalize, dirty_value) = func.alloc_value_inst(
        Instruction::new(InstKind::Ne(dirty_value, zero), Some(MirType::I1))
            .with_debug_info_dropped(),
    );
    exit_instructions.push(normalize);

    // dirty = mload dirty_addr
    // jumpi dirty, store_block, continuation !metadata(intentionally dropped)
    func.blocks[exit].instructions = exit_instructions;
    func.blocks[exit].set_generated_terminator(Terminator::Branch {
        condition: dirty_value,
        then_block: store_block,
        else_block: continuation,
    });

    let flush = alloc_flush(func, slot_value, temp_addr);
    func.blocks[store_block].predecessors.push(exit);
    func.blocks[store_block].instructions.extend(flush);
    func.blocks[store_block].set_generated_terminator(Terminator::Jump(continuation));

    func.blocks[continuation].predecessors.push(exit);
    func.blocks[continuation].predecessors.push(store_block);
    // continuation: remaining_instructions; old_terminator !metadata(exit)
    func.blocks[continuation].instructions = continuation_instructions;
    if let Some(terminator) = old_terminator {
        func.blocks[continuation].set_terminator(terminator, terminator_metadata);
    }

    redirect_successor_predecessors(func, exit, continuation);
}

// NOTE: The temporary is an absolute address in the entry's local region, and `lower-abi`
// copies it into `f.body` when the entry has internal callers. It still cannot overwrite a
// live word there:
// - Every entry places its spill area, and the shared region of static helper frames, above each
//   constant low-memory range in the functions it reaches, so above this address too.
// - The other words of an entry's local region are its own temporaries, live only inside the
//   call-free loops promoted here, and the inliner's return staging buffers, read right after the
//   inlined return. Neither is live across a call to `f.body`, and promoted loops get no code
//   inlined later because they had no calls to inline.
// - No inliner after this pass takes a callee with a frame: all check `internal_frame_size` or need
//   a single block, which a promoted loop never is. So the address never moves into a caller whose
//   frame rebasing, which rewrites only `internal_frame_addr`, would miss it.
fn allocate_temp_addr(func: &mut Function) -> ValueId {
    let frame_offset = func.internal_frame_size.max(func.external_static_return_size);
    let temp_addr = EvmMemoryLayout::HEAP_START + frame_offset;
    func.internal_frame_size = func.internal_frame_size.max(frame_offset + 32);
    func.alloc_value(Value::Immediate(Immediate::I256(U256::from(temp_addr))))
}

fn value_defined_in_loop(
    func: &Function,
    inst_blocks: &IndexVec<InstId, Option<BlockId>>,
    value: ValueId,
    loop_data: &Loop,
) -> bool {
    match func.value(value) {
        Value::Inst(inst_id) => {
            inst_blocks[*inst_id].is_some_and(|block_id| loop_data.blocks.contains(block_id))
        }
        Value::Undef(_) | Value::Error(_) => true,
        Value::Arg(_) | Value::Immediate(_) => false,
    }
}

//! Local dead memory optimization.
//!
//! This pass removes full-word `mstore` instructions that are overwritten by a
//! later full-word `mstore` to the same exact address within the same basic
//! block, before any operation can observe memory or gas. It also forwards
//! same-block `mload` instructions from the latest exact-address `mstore` when
//! no intervening operation can mutate memory.

use crate::{
    analysis::CfgInfo,
    mir::{
        BlockId, Function, Immediate, InstId, InstKind, Terminator, Value, ValueId,
        utils as mir_utils,
    },
    pass::FunctionPass,
};
use alloy_primitives::{U256, keccak256};
use solar_data_structures::map::{FxHashMap, FxHashSet};

/// Local dead memory optimization pass.
#[derive(Debug, Default)]
pub struct MemoryStoreEliminator {
    /// Number of memory instructions eliminated.
    pub eliminated_count: usize,
}

/// Function pass for local dead memory-store elimination.
pub struct MemoryDsePass;

impl FunctionPass for MemoryDsePass {
    fn name(&self) -> &str {
        "memory-dse"
    }

    fn run_on_function(&mut self, func: &mut Function) -> bool {
        MemoryStoreEliminator::new().run_to_fixpoint(func) != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum MemAddrKey {
    Const(u64),
    BaseOffset { base: ValueId, offset: u64 },
}

/// Above this many tracked live slots the backward DSE gives up precision and
/// treats all memory as live. Keeps the lattice height (and cost) bounded.
const MEM_LIVE_CAP: usize = 64;

/// Backward memory-liveness lattice over constant word-aligned slots.
///
/// `All` is the conservative top: any address may be observed. `Only` names the
/// exact slots that may be read before the next full-word overwrite; every
/// other slot is provably dead if overwritten.
#[derive(Clone, Debug, PartialEq, Eq)]
enum MemLive {
    All,
    Only(FxHashSet<u64>),
}

impl MemLive {
    fn contains(&self, addr: u64) -> bool {
        match self {
            Self::All => true,
            Self::Only(set) => set.contains(&addr),
        }
    }

    fn add_addr(&mut self, addr: u64) {
        if let Self::Only(set) = self {
            set.insert(addr);
            if set.len() > MEM_LIVE_CAP {
                *self = Self::All;
            }
        }
    }

    fn kill(&mut self, addr: u64) {
        if let Self::Only(set) = self {
            set.remove(&addr);
        }
    }

    fn join(&mut self, other: &Self) {
        match (&mut *self, other) {
            (Self::All, _) => {}
            (this, Self::All) => *this = Self::All,
            (Self::Only(a), Self::Only(b)) => {
                a.extend(b.iter().copied());
                if a.len() > MEM_LIVE_CAP {
                    *self = Self::All;
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ImmutableCopyKey {
    len: u64,
    offset: u64,
}

#[derive(Clone, Copy, Debug)]
struct CachedImmutableCopy {
    block: BlockId,
    index: usize,
    value: ValueId,
}

impl MemoryStoreEliminator {
    /// Creates a new memory optimization pass.
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs local memory optimization on a function.
    pub fn run(&mut self, func: &mut Function) -> usize {
        self.eliminated_count = 0;

        let needs_inst_results = func.blocks.iter().any(|block| {
            block.instructions.iter().any(|&inst_id| {
                matches!(
                    func.instructions[inst_id].kind,
                    InstKind::MLoad(_) | InstKind::Keccak256(_, _)
                )
            })
        });
        let inst_results =
            if needs_inst_results { func.inst_results() } else { FxHashMap::default() };

        self.reuse_redundant_immutable_copies(func, &inst_results);
        self.remove_unused_internal_frame_stores(func);

        let block_ids: Vec<BlockId> = func.blocks.indices().collect();
        let has_precise_reads = func.blocks.iter().any(|block| {
            block.instructions.iter().any(|&inst_id| {
                Self::constant_range_read(&func.instructions[inst_id].kind).is_some()
            })
        });
        if has_precise_reads {
            for block_id in block_ids {
                self.process_block::<true>(func, block_id, &inst_results);
            }
        } else {
            for block_id in block_ids {
                self.process_block::<false>(func, block_id, &inst_results);
            }
        }
        self.remove_cross_block_equal_const_stores(func);
        self.remove_cross_block_overwrites(func);
        self.remove_dead_memory_stores(func);

        self.eliminated_count
    }

    /// Removes full-word stores to a constant, word-aligned address that no
    /// path can observe before overwriting the same address.
    ///
    /// The block-local and single-edge passes above miss the dead default-init
    /// a boolean-returning entry stages into its return slot (`mstore(A, 0)`)
    /// when the real store (`mstore(A, 1)`) sits past a checked-arithmetic
    /// branch, in a different block. This is a backward memory-liveness
    /// dataflow over constant word-aligned slots: a slot is live where a later
    /// read may observe it before the next full-word overwrite.
    ///
    /// Soundness rests on modelling every way a stored value can still be read:
    /// an in-range constant read (`mload`/keccak/log/`returndata`/`revert`)
    /// keeps its slots live; anything that could observe or forward arbitrary
    /// memory — a symbolic address, a non-constant range, a call, a `return`
    /// (whose value may be a memory pointer the caller dereferences), a tail
    /// call, `msize` — widens to all-memory-live, which only ever keeps a
    /// store, never drops a live one.
    fn remove_dead_memory_stores(&mut self, func: &mut Function) {
        if !func.blocks.iter().any(|block| {
            block.instructions.iter().any(|&inst_id| {
                matches!(func.instructions[inst_id].kind, InstKind::MStore(addr, _) if Self::word_aligned_const(func, addr).is_some())
            })
        }) {
            return;
        }

        let block_ids: Vec<BlockId> = func.blocks.indices().collect();
        if block_ids.is_empty() {
            return;
        }

        // Backward fixpoint: live_in[b] = transfer(b, ∪ live_in[succ(b)]).
        // Liveness only grows, so stopping early would under-approximate it and
        // could mark a live store dead; require real convergence, else bail.
        let mut live_in: FxHashMap<BlockId, MemLive> =
            block_ids.iter().map(|&b| (b, MemLive::Only(FxHashSet::default()))).collect();
        let mut converged = false;
        for _ in 0..(block_ids.len() * 4 + 16) {
            let mut changed = false;
            for &block_id in block_ids.iter().rev() {
                let out = Self::live_out(func, block_id, &live_in);
                let new_in = Self::transfer_block(func, block_id, out, &mut None);
                if live_in.get(&block_id) != Some(&new_in) {
                    live_in.insert(block_id, new_in);
                    changed = true;
                }
            }
            if !changed {
                converged = true;
                break;
            }
        }
        if !converged {
            return;
        }

        // Collect dead stores using the stabilized live-out of each block.
        let mut dead: FxHashSet<InstId> = FxHashSet::default();
        for &block_id in &block_ids {
            let out = Self::live_out(func, block_id, &live_in);
            let mut collector = Some(&mut dead);
            Self::transfer_block(func, block_id, out, &mut collector);
        }

        if dead.is_empty() {
            return;
        }
        self.eliminated_count += dead.len();
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|id| !dead.contains(id));
        }
    }

    fn live_out(func: &Function, block: BlockId, live_in: &FxHashMap<BlockId, MemLive>) -> MemLive {
        let mut out = MemLive::Only(FxHashSet::default());
        if let Some(term) = func.blocks[block].terminator.as_ref() {
            for succ in term.successors() {
                if let Some(in_set) = live_in.get(&succ) {
                    out.join(in_set);
                }
            }
        }
        out
    }

    /// Runs the backward transfer over one block's terminator and instructions,
    /// returning the live set at block entry. When `dead` is `Some`, records the
    /// full-word constant stores found dead against the flowing live set.
    fn transfer_block(
        func: &Function,
        block: BlockId,
        mut live: MemLive,
        dead: &mut Option<&mut FxHashSet<InstId>>,
    ) -> MemLive {
        // Terminator first: it executes after every instruction in the block.
        match func.blocks[block].terminator.as_ref() {
            Some(Terminator::Revert { offset, size })
            | Some(Terminator::ReturnData { offset, size }) => {
                Self::mark_read(func, &mut live, *offset, *size);
            }
            // A `return` value may be a memory pointer the caller dereferences,
            // a tail call forwards memory to its callee, and a halt observes
            // nothing but is rare — keep all memory live rather than reason
            // about escape. `jump`/`branch`/`switch` read no memory; their
            // successors already contribute liveness via `live_out`.
            Some(Terminator::Return { .. })
            | Some(Terminator::TailCall { .. })
            | Some(Terminator::Stop)
            | Some(Terminator::Invalid)
            | Some(Terminator::SelfDestruct { .. }) => live = MemLive::All,
            Some(Terminator::Jump(_))
            | Some(Terminator::Branch { .. })
            | Some(Terminator::Switch { .. })
            | None => {}
        }

        for &inst_id in func.blocks[block].instructions.iter().rev() {
            match &func.instructions[inst_id].kind {
                InstKind::MStore(addr, _) => {
                    if let Some(slot) = Self::word_aligned_const(func, *addr) {
                        if !live.contains(slot)
                            && let Some(dead) = dead.as_mut()
                        {
                            dead.insert(inst_id);
                        }
                        // The store fully defines `slot`; nothing above it on
                        // this path can be observed here.
                        live.kill(slot);
                    }
                    // A symbolic store neither reads nor provably overwrites a
                    // tracked slot: leave the live set untouched.
                }
                InstKind::MLoad(addr) => match Self::word_aligned_const(func, *addr) {
                    Some(slot) => live.add_addr(slot),
                    None => live = MemLive::All,
                },
                InstKind::Keccak256(offset, size) | InstKind::Log0(offset, size) => {
                    Self::mark_read(func, &mut live, *offset, *size);
                }
                InstKind::Log1(offset, size, _) => {
                    Self::mark_read(func, &mut live, *offset, *size);
                }
                InstKind::Log2(offset, size, _, _) => {
                    Self::mark_read(func, &mut live, *offset, *size);
                }
                InstKind::Log3(offset, size, _, _, _) => {
                    Self::mark_read(func, &mut live, *offset, *size);
                }
                InstKind::Log4(offset, size, _, _, _, _) => {
                    Self::mark_read(func, &mut live, *offset, *size);
                }
                // Byte stores never fully define a word (so cannot make an
                // earlier store dead) and read nothing: leave the set as is.
                InstKind::MStore8(_, _) => {}
                // Anything that may read or alias memory we cannot model
                // precisely: assume it observes everything above.
                kind if Self::is_memory_or_gas_observer(kind) => live = MemLive::All,
                _ => {}
            }
        }

        live
    }

    /// Marks the word-aligned slots a constant memory read `[offset, offset +
    /// size)` may observe as live; a non-constant or oversized range widens to
    /// all-memory-live.
    fn mark_read(func: &Function, live: &mut MemLive, offset: ValueId, size: ValueId) {
        if matches!(live, MemLive::All) {
            return;
        }
        let (Some(offset), Some(size)) = (func.value_u64(offset), func.value_u64(size)) else {
            *live = MemLive::All;
            return;
        };
        if size == 0 {
            return;
        }
        let Some(end) = offset.checked_add(size) else {
            *live = MemLive::All;
            return;
        };
        let first = (offset / 32) * 32;
        // Bound the walk; a huge read is treated as observing all memory.
        if end.saturating_sub(first) > 32 * 256 {
            *live = MemLive::All;
            return;
        }
        let mut word = first;
        while word < end {
            live.add_addr(word);
            word += 32;
        }
    }

    /// Returns a constant, 32-byte-aligned memory address, or `None` otherwise.
    fn word_aligned_const(func: &Function, addr: ValueId) -> Option<u64> {
        match Self::mem_addr_key(func, addr) {
            Some(MemAddrKey::Const(a)) if a % 32 == 0 => Some(a),
            _ => None,
        }
    }

    /// Runs local memory optimization until no more instructions can be eliminated.
    pub fn run_to_fixpoint(&mut self, func: &mut Function) -> usize {
        let mut total = 0;
        loop {
            let eliminated = self.run(func);
            if eliminated == 0 {
                break;
            }
            total += eliminated;
        }
        total
    }

    fn reuse_redundant_immutable_copies(
        &mut self,
        func: &mut Function,
        inst_results: &FxHashMap<InstId, ValueId>,
    ) {
        let has_candidate = func.blocks.iter().any(|block| {
            block.instructions.windows(2).any(|window| {
                matches!(func.instructions[window[0]].kind, InstKind::CodeCopy(_, _, _))
                    && matches!(func.instructions[window[1]].kind, InstKind::MLoad(_))
            })
        });
        if !has_candidate {
            return;
        }

        let cfg = CfgInfo::new(func);
        let mut cached: FxHashMap<ImmutableCopyKey, CachedImmutableCopy> = FxHashMap::default();
        let mut replacements = FxHashMap::default();
        let mut dead = FxHashSet::default();

        for (block_id, block) in func.blocks.iter_enumerated() {
            let insts = block.instructions.clone();
            for (index, window) in insts.windows(2).enumerate() {
                let codecopy = window[0];
                let load = window[1];
                let InstKind::CodeCopy(dest, src, size) = func.instructions[codecopy].kind else {
                    continue;
                };
                if func.value_u64(size) != Some(32) {
                    continue;
                }
                let Some(key) = Self::immutable_copy_key(func, src) else {
                    continue;
                };
                let InstKind::MLoad(load_addr) = func.instructions[load].kind else {
                    continue;
                };
                if Self::mem_addr_key(func, dest) != Self::mem_addr_key(func, load_addr) {
                    continue;
                }
                let Some(&loaded_value) = inst_results.get(&load) else {
                    continue;
                };

                if let Some(cached_copy) = cached.get(&key).copied()
                    && Self::copy_dominates(cfg.dominators(), cached_copy, block_id, index)
                {
                    replacements.insert(loaded_value, cached_copy.value);
                    func.instructions[codecopy].kind = InstKind::MStore(dest, cached_copy.value);
                    dead.insert(load);
                    self.eliminated_count += 1;
                    continue;
                }

                cached.insert(
                    key,
                    CachedImmutableCopy { block: block_id, index, value: loaded_value },
                );
            }
        }

        if replacements.is_empty() && dead.is_empty() {
            return;
        }

        func.replace_uses_canonicalized(&replacements);
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|id| !dead.contains(id));
        }
    }

    fn remove_unused_internal_frame_stores(&mut self, func: &mut Function) {
        let has_candidate = func.blocks.iter().any(|block| {
            block.instructions.iter().any(|&inst_id| {
                matches!(func.instructions[inst_id].kind, InstKind::MStore(addr, _) if Self::internal_frame_offset(func, addr).is_some())
            })
        });
        if !has_candidate {
            return;
        }
        if Self::has_frame_observer(func) {
            return;
        }

        let Some(reads) = Self::internal_frame_read_ranges(func) else {
            return;
        };
        let mut dead = FxHashSet::default();

        for block in func.blocks.iter() {
            for &inst_id in &block.instructions {
                let InstKind::MStore(addr, _) = func.instructions[inst_id].kind else {
                    continue;
                };
                let Some(offset) = Self::internal_frame_offset(func, addr) else {
                    continue;
                };
                if !reads.iter().any(|&(read_offset, read_size)| {
                    mir_utils::ranges_overlap(offset, 32, read_offset, read_size)
                }) {
                    dead.insert(inst_id);
                }
            }
        }

        if dead.is_empty() {
            return;
        }

        self.eliminated_count += dead.len();
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|id| !dead.contains(id));
        }
    }

    fn process_block<const PRECISE_READS: bool>(
        &mut self,
        func: &mut Function,
        block_id: BlockId,
        inst_results: &FxHashMap<InstId, ValueId>,
    ) {
        let mut mstores = 0;
        let mut memory_writes = 0;
        let mut has_load = false;
        let mut has_keccak = false;
        for &inst_id in &func.blocks[block_id].instructions {
            match func.instructions[inst_id].kind {
                InstKind::MStore(_, _) => {
                    mstores += 1;
                    memory_writes += 1;
                }
                InstKind::CalldataCopy(_, _, _)
                | InstKind::CodeCopy(_, _, _)
                | InstKind::ReturnDataCopy(_, _, _)
                | InstKind::ExtCodeCopy(_, _, _, _) => memory_writes += 1,
                InstKind::MLoad(_) => has_load = true,
                InstKind::Keccak256(_, _) => has_keccak = true,
                _ => {}
            }
        }

        if has_keccak && mstores != 0 {
            self.fold_constant_keccak(func, block_id, inst_results);
        }
        if has_load && mstores != 0 {
            self.forward_loads(func, block_id, inst_results);
        }
        if mstores >= 2 {
            self.remove_equal_stores(func, block_id);
        }
        if memory_writes < 2 {
            return;
        }

        let inst_ids = func.blocks[block_id].instructions.clone();
        let mut overwritten: FxHashSet<MemAddrKey> = FxHashSet::default();
        let mut dead: FxHashSet<InstId> = FxHashSet::default();

        for &inst_id in inst_ids.iter().rev() {
            let inst = &func.instructions[inst_id];
            match &inst.kind {
                InstKind::MStore(addr, _) => {
                    if let Some(key) = Self::mem_addr_key(func, *addr) {
                        if overwritten.contains(&key) {
                            dead.insert(inst_id);
                            self.eliminated_count += 1;
                        } else {
                            overwritten.insert(key);
                        }
                    } else {
                        overwritten.clear();
                    }
                }
                InstKind::MLoad(addr) => {
                    if let Some(key) = Self::mem_addr_key(func, *addr) {
                        Self::remove_overlapping_set(&mut overwritten, key);
                    } else {
                        overwritten.clear();
                    }
                }
                InstKind::CalldataCopy(dest, _, size)
                | InstKind::CodeCopy(dest, _, size)
                | InstKind::ReturnDataCopy(dest, _, size) => {
                    Self::insert_or_clear_full_word_overwritten_range(
                        func,
                        &mut overwritten,
                        *dest,
                        *size,
                    );
                }
                InstKind::ExtCodeCopy(_, dest, _, size) => {
                    Self::insert_or_clear_full_word_overwritten_range(
                        func,
                        &mut overwritten,
                        *dest,
                        *size,
                    );
                }
                // Keccak and logs only *read* memory. A read over a constant
                // range observes only the stores that fall in it, so a later
                // overwrite of a disjoint slot still kills its earlier store.
                // Modelling the range (instead of clearing) lets a return-value
                // slot's dead default-init survive the mapping-hash keccaks and
                // event logs that sit between it and its real store.
                kind if PRECISE_READS
                    && let Some((offset, size)) = Self::constant_range_read(kind) =>
                {
                    Self::retain_overwritten_disjoint_from_read(
                        func,
                        &mut overwritten,
                        offset,
                        size,
                    );
                }
                kind if Self::is_memory_or_gas_observer(kind) => {
                    overwritten.clear();
                }
                _ => {}
            }
        }

        if dead.is_empty() {
            return;
        }

        func.blocks[block_id].instructions.retain(|id| !dead.contains(id));
    }

    fn constant_range_read(kind: &InstKind) -> Option<(ValueId, ValueId)> {
        match kind {
            InstKind::Keccak256(offset, size)
            | InstKind::Log0(offset, size)
            | InstKind::Log1(offset, size, _)
            | InstKind::Log2(offset, size, _, _)
            | InstKind::Log3(offset, size, _, _, _)
            | InstKind::Log4(offset, size, _, _, _, _) => Some((*offset, *size)),
            _ => None,
        }
    }

    /// Keeps only the overwritten slots a constant-range memory read cannot
    /// observe. A slot provably outside `[offset, offset + size)` survives; a
    /// non-constant range or a symbolic slot is assumed observed (dropped),
    /// which only ever keeps a store alive — never eliminates a live one.
    fn retain_overwritten_disjoint_from_read(
        func: &Function,
        overwritten: &mut FxHashSet<MemAddrKey>,
        offset: ValueId,
        size: ValueId,
    ) {
        let (Some(read_offset), Some(read_size)) = (func.value_u64(offset), func.value_u64(size))
        else {
            overwritten.clear();
            return;
        };
        if read_size == 0 {
            return;
        }
        overwritten.retain(|&key| match key {
            MemAddrKey::Const(addr) => !mir_utils::ranges_overlap(addr, 32, read_offset, read_size),
            MemAddrKey::BaseOffset { .. } => false,
        });
    }

    /// Removes constant stores made redundant by a constant store on the sole
    /// path into the block.
    ///
    /// Mapping-slot staging writes the slot constant to scratch `0x20` before
    /// every access; two accesses to the same mapping restage the identical
    /// constant, but the checked-arithmetic underflow branch between them puts
    /// the stores in separate blocks, out of the block-local pass's reach.
    /// Only constant address and constant value are tracked, so availability
    /// needs no SSA reasoning: a single-predecessor block inherits its
    /// predecessor's exit constants, and a store matching one is dead.
    fn remove_cross_block_equal_const_stores(&mut self, func: &mut Function) {
        if func
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|&&inst_id| matches!(func.instructions[inst_id].kind, InstKind::MStore(_, _)))
            .take(2)
            .count()
            < 2
        {
            return;
        }

        let const_store = |func: &Function, addr: ValueId, value: ValueId| {
            let (Value::Immediate(a), Value::Immediate(v)) = (func.value(addr), func.value(value))
            else {
                return None;
            };
            Some((a.as_u256()?.try_into().ok()?, v.as_u256()?))
        };

        let mut exit: FxHashMap<BlockId, FxHashMap<u64, U256>> = FxHashMap::default();
        let mut dead: FxHashSet<InstId> = FxHashSet::default();

        // Block index order approximates reverse postorder for this builder,
        // so a single predecessor is usually already computed; when it is not,
        // the block simply starts from no known constants.
        for block_id in func.blocks.indices() {
            let preds = &func.blocks[block_id].predecessors;
            let mut known: FxHashMap<u64, U256> = match preds.as_slice() {
                [pred] => exit.get(pred).cloned().unwrap_or_default(),
                _ => FxHashMap::default(),
            };

            for &inst_id in &func.blocks[block_id].instructions {
                match &func.instructions[inst_id].kind {
                    InstKind::MStore(addr, value) => match const_store(func, *addr, *value) {
                        Some((a, v)) => {
                            if known.get(&a) == Some(&v) {
                                dead.insert(inst_id);
                                self.eliminated_count += 1;
                            } else {
                                known.insert(a, v);
                            }
                        }
                        None => match Self::mem_addr_key(func, *addr) {
                            // A non-constant value written to a constant scratch
                            // slot makes its contents unknown.
                            Some(MemAddrKey::Const(a)) => {
                                known.remove(&a);
                            }
                            // An address we cannot pin could alias anything.
                            _ => known.clear(),
                        },
                    },
                    kind if Self::can_mutate_memory(kind) => known.clear(),
                    // A byte store may touch any slot.
                    InstKind::MStore8(_, _) => known.clear(),
                    // Loads and keccak read memory but never write it.
                    _ => {}
                }
            }

            exit.insert(block_id, known);
        }

        if dead.is_empty() {
            return;
        }
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|id| !dead.contains(id));
        }
    }

    fn remove_cross_block_overwrites(&mut self, func: &mut Function) {
        if func
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|&&inst_id| matches!(func.instructions[inst_id].kind, InstKind::MStore(_, _)))
            .take(2)
            .count()
            < 2
        {
            return;
        }

        let mut dead = FxHashSet::default();

        for pred in func.blocks.indices() {
            let Some(succ) = Self::single_jump_successor(func, pred) else {
                continue;
            };
            if func.blocks[succ].predecessors.as_slice() != [pred] {
                continue;
            }

            let Some((store, pred_key)) = self.last_cross_block_store_candidate(func, pred) else {
                continue;
            };
            let Some(succ_key) = self.first_cross_block_overwrite(func, succ) else {
                continue;
            };
            if pred_key == succ_key {
                dead.insert(store);
            }
        }

        if dead.is_empty() {
            return;
        }

        self.eliminated_count += dead.len();
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|id| !dead.contains(id));
        }
    }

    fn single_jump_successor(func: &Function, block: BlockId) -> Option<BlockId> {
        let Some(Terminator::Jump(target)) = func.blocks[block].terminator.as_ref() else {
            return None;
        };
        Some(*target)
    }

    fn last_cross_block_store_candidate(
        &self,
        func: &Function,
        block: BlockId,
    ) -> Option<(InstId, MemAddrKey)> {
        for &inst_id in func.blocks[block].instructions.iter().rev() {
            match func.instructions[inst_id].kind {
                InstKind::MStore(addr, _) => {
                    let key = Self::mem_addr_key(func, addr)?;
                    return Some((inst_id, key));
                }
                ref kind if Self::cross_block_memory_barrier(kind) => return None,
                _ => {}
            }
        }
        None
    }

    fn first_cross_block_overwrite(&self, func: &Function, block: BlockId) -> Option<MemAddrKey> {
        for &inst_id in &func.blocks[block].instructions {
            match func.instructions[inst_id].kind {
                InstKind::MStore(addr, _) => return Self::mem_addr_key(func, addr),
                ref kind if Self::cross_block_memory_barrier(kind) => return None,
                _ => {}
            }
        }
        None
    }

    fn fold_constant_keccak(
        &mut self,
        func: &mut Function,
        block_id: BlockId,
        inst_results: &FxHashMap<InstId, ValueId>,
    ) {
        let inst_ids = func.blocks[block_id].instructions.clone();
        let mut stored_words: FxHashMap<MemAddrKey, U256> = FxHashMap::default();
        let mut replacements: FxHashMap<ValueId, ValueId> = FxHashMap::default();
        let mut dead: FxHashSet<InstId> = FxHashSet::default();

        for &inst_id in &inst_ids {
            match &func.instructions[inst_id].kind {
                InstKind::MStore(addr, value) => {
                    let Some(key) = Self::mem_addr_key(func, *addr) else {
                        stored_words.clear();
                        continue;
                    };
                    Self::remove_overlapping_map(&mut stored_words, key);
                    if let Some(value) = func.value_u256(*value) {
                        stored_words.insert(key, value);
                    }
                }
                InstKind::Keccak256(offset, size) => {
                    let Some(bytes) =
                        Self::constant_memory_bytes(func, &stored_words, *offset, *size)
                    else {
                        continue;
                    };
                    let Some(&result) = inst_results.get(&inst_id) else {
                        continue;
                    };
                    let hash = keccak256(&bytes);
                    let replacement = func.alloc_value(Value::Immediate(Immediate::uint256(
                        U256::from_be_bytes(hash.0),
                    )));
                    replacements.insert(result, replacement);
                    dead.insert(inst_id);
                    self.eliminated_count += 1;
                }
                kind if Self::can_mutate_memory(kind) => {
                    stored_words.clear();
                }
                _ => {}
            }
        }

        if dead.is_empty() {
            return;
        }

        func.replace_uses_canonicalized(&replacements);
        func.blocks[block_id].instructions.retain(|id| !dead.contains(id));
    }

    fn remove_equal_stores(&mut self, func: &mut Function, block_id: BlockId) {
        let inst_ids = func.blocks[block_id].instructions.clone();
        let mut stored_values: FxHashMap<MemAddrKey, ValueId> = FxHashMap::default();
        let mut dead: FxHashSet<InstId> = FxHashSet::default();

        for &inst_id in &inst_ids {
            let inst = &func.instructions[inst_id];
            match &inst.kind {
                InstKind::MStore(addr, value) => {
                    let Some(key) = Self::mem_addr_key(func, *addr) else {
                        stored_values.clear();
                        continue;
                    };

                    if stored_values.get(&key).is_some_and(|&stored| stored == *value) {
                        dead.insert(inst_id);
                        self.eliminated_count += 1;
                        continue;
                    }

                    Self::remove_overlapping_map(&mut stored_values, key);
                    stored_values.insert(key, *value);
                }
                kind if Self::can_mutate_memory(kind) => {
                    stored_values.clear();
                }
                _ => {}
            }
        }

        if dead.is_empty() {
            return;
        }

        func.blocks[block_id].instructions.retain(|id| !dead.contains(id));
    }

    fn forward_loads(
        &mut self,
        func: &mut Function,
        block_id: BlockId,
        inst_results: &FxHashMap<InstId, ValueId>,
    ) {
        let inst_ids = func.blocks[block_id].instructions.clone();
        let mut stored_values: FxHashMap<MemAddrKey, ValueId> = FxHashMap::default();
        let mut replacements: FxHashMap<ValueId, ValueId> = FxHashMap::default();
        let mut dead: FxHashSet<InstId> = FxHashSet::default();

        for &inst_id in &inst_ids {
            let inst = &func.instructions[inst_id];
            match &inst.kind {
                InstKind::MStore(addr, value) => {
                    if let Some(key) = Self::mem_addr_key(func, *addr) {
                        if !Self::remove_overlapping_write_range(
                            func,
                            &mut stored_values,
                            *addr,
                            32,
                        ) {
                            stored_values.clear();
                            continue;
                        }
                        stored_values
                            .insert(key, mir_utils::resolve_replacement(*value, &replacements));
                    } else {
                        stored_values.clear();
                    }
                }
                InstKind::MLoad(addr) => {
                    let Some(key) = Self::mem_addr_key(func, *addr) else {
                        continue;
                    };
                    let Some(&stored_value) = stored_values.get(&key) else {
                        continue;
                    };
                    if let Some(&loaded_value) = inst_results.get(&inst_id) {
                        replacements.insert(loaded_value, stored_value);
                        dead.insert(inst_id);
                    }
                }
                InstKind::MStore8(addr, _)
                    if !Self::remove_overlapping_write_range(
                        func,
                        &mut stored_values,
                        *addr,
                        1,
                    ) =>
                {
                    stored_values.clear();
                }
                InstKind::CalldataCopy(dest, _, size)
                | InstKind::CodeCopy(dest, _, size)
                | InstKind::ReturnDataCopy(dest, _, size) => {
                    let Some(size) = func.value_u64(*size) else {
                        stored_values.clear();
                        continue;
                    };
                    if !Self::remove_overlapping_write_range(func, &mut stored_values, *dest, size)
                    {
                        stored_values.clear();
                    }
                }
                InstKind::ExtCodeCopy(_, dest, _, size) => {
                    let Some(size) = func.value_u64(*size) else {
                        stored_values.clear();
                        continue;
                    };
                    if !Self::remove_overlapping_write_range(func, &mut stored_values, *dest, size)
                    {
                        stored_values.clear();
                    }
                }
                kind if Self::can_mutate_memory(kind) => {
                    stored_values.clear();
                }
                _ => {}
            }
        }

        if dead.is_empty() {
            return;
        }

        func.replace_uses_canonicalized(&replacements);
        self.eliminated_count += dead.len();
        func.blocks[block_id].instructions.retain(|id| !dead.contains(id));
    }

    fn mem_addr_key(func: &Function, value: ValueId) -> Option<MemAddrKey> {
        Self::mem_addr_key_with_depth(func, value, 0)
    }

    fn mem_addr_key_with_depth(
        func: &Function,
        value: ValueId,
        depth: usize,
    ) -> Option<MemAddrKey> {
        if depth > 8 {
            return Some(MemAddrKey::BaseOffset { base: value, offset: 0 });
        }

        match &func.values[value] {
            Value::Immediate(imm) => {
                let addr = imm.as_u256()?;
                u64::try_from(addr).ok().map(MemAddrKey::Const)
            }
            Value::Inst(inst_id) => match func.instructions[*inst_id].kind {
                InstKind::Add(a, b) => Self::add_addr_offset(func, a, b, depth)
                    .or_else(|| Self::add_addr_offset(func, b, a, depth))
                    .or(Some(MemAddrKey::BaseOffset { base: value, offset: 0 })),
                _ => Some(MemAddrKey::BaseOffset { base: value, offset: 0 }),
            },
            Value::Arg { .. } => Some(MemAddrKey::BaseOffset { base: value, offset: 0 }),
            Value::Undef(_) | Value::Error(_) => None,
        }
    }

    fn add_addr_offset(
        func: &Function,
        base: ValueId,
        offset: ValueId,
        depth: usize,
    ) -> Option<MemAddrKey> {
        let offset = func.value_u64(offset)?;
        match Self::mem_addr_key_with_depth(func, base, depth + 1)? {
            MemAddrKey::Const(addr) => addr.checked_add(offset).map(MemAddrKey::Const),
            MemAddrKey::BaseOffset { base, offset: base_offset } => base_offset
                .checked_add(offset)
                .map(|offset| MemAddrKey::BaseOffset { base, offset }),
        }
    }

    fn immutable_copy_key(func: &Function, src: ValueId) -> Option<ImmutableCopyKey> {
        match func.values[src] {
            Value::Inst(inst_id) => match func.instructions[inst_id].kind {
                InstKind::Sub(code_size, len) if Self::is_codesize(func, code_size) => {
                    Some(ImmutableCopyKey { len: func.value_u64(len)?, offset: 0 })
                }
                InstKind::Add(base, offset) => {
                    Self::immutable_copy_key_with_offset(func, base, offset)
                        .or_else(|| Self::immutable_copy_key_with_offset(func, offset, base))
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn immutable_copy_key_with_offset(
        func: &Function,
        base: ValueId,
        offset: ValueId,
    ) -> Option<ImmutableCopyKey> {
        let mut key = Self::immutable_copy_key(func, base)?;
        key.offset = key.offset.checked_add(func.value_u64(offset)?)?;
        Some(key)
    }

    fn is_codesize(func: &Function, value: ValueId) -> bool {
        matches!(func.values[value], Value::Inst(inst_id) if matches!(func.instructions[inst_id].kind, InstKind::CodeSize))
    }

    fn copy_dominates(
        dominators: &crate::analysis::DominatorTree,
        cached: CachedImmutableCopy,
        block: BlockId,
        index: usize,
    ) -> bool {
        if cached.block == block {
            return cached.index < index;
        }
        dominators.dominates(cached.block, block)
    }

    fn constant_memory_bytes(
        func: &Function,
        stored_words: &FxHashMap<MemAddrKey, U256>,
        offset: ValueId,
        size: ValueId,
    ) -> Option<Vec<u8>> {
        let offset = func.value_u64(offset)?;
        let size = func.value_u64(size)?;
        if size > 4096 || size % 32 != 0 {
            return None;
        }

        let mut bytes = Vec::with_capacity(size as usize);
        for word_offset in (0..size).step_by(32) {
            let addr = offset.checked_add(word_offset)?;
            let word = stored_words.get(&MemAddrKey::Const(addr))?;
            bytes.extend_from_slice(&word.to_be_bytes::<32>());
        }
        Some(bytes)
    }

    fn overlaps(a: MemAddrKey, b: MemAddrKey) -> bool {
        match (a, b) {
            (MemAddrKey::Const(a), MemAddrKey::Const(b)) => mir_utils::ranges_overlap(a, 32, b, 32),
            (
                MemAddrKey::BaseOffset { base: a_base, offset: a_offset },
                MemAddrKey::BaseOffset { base: b_base, offset: b_offset },
            ) if a_base == b_base => mir_utils::ranges_overlap(a_offset, 32, b_offset, 32),
            _ => true,
        }
    }

    fn remove_overlapping_map<T>(map: &mut FxHashMap<MemAddrKey, T>, key: MemAddrKey) {
        map.retain(|&stored, _| !Self::overlaps(stored, key));
    }

    fn remove_overlapping_set(set: &mut FxHashSet<MemAddrKey>, key: MemAddrKey) {
        set.retain(|&stored| !Self::overlaps(stored, key));
    }

    fn remove_overlapping_write_range<T>(
        func: &Function,
        map: &mut FxHashMap<MemAddrKey, T>,
        dest: ValueId,
        size: u64,
    ) -> bool {
        let Some(write) = Self::mem_addr_key(func, dest) else {
            return false;
        };
        map.retain(|&stored, _| !Self::ranges_overlap_mem_keys(func, stored, 32, write, size));
        true
    }

    fn insert_full_word_overwritten_range(
        func: &Function,
        overwritten: &mut FxHashSet<MemAddrKey>,
        dest: ValueId,
        size: ValueId,
    ) -> bool {
        let Some(size) = func.value_u64(size) else {
            return false;
        };
        if size % 32 != 0 || size > 4096 {
            return false;
        }

        let Some(base) = Self::mem_addr_key(func, dest) else {
            return false;
        };
        for offset in (0..size).step_by(32) {
            let Some(key) = Self::offset_mem_addr_key(base, offset) else {
                return false;
            };
            overwritten.insert(key);
        }
        true
    }

    fn insert_or_clear_full_word_overwritten_range(
        func: &Function,
        overwritten: &mut FxHashSet<MemAddrKey>,
        dest: ValueId,
        size: ValueId,
    ) {
        if !Self::insert_full_word_overwritten_range(func, overwritten, dest, size) {
            overwritten.clear();
        }
    }

    fn offset_mem_addr_key(key: MemAddrKey, add: u64) -> Option<MemAddrKey> {
        match key {
            MemAddrKey::Const(offset) => offset.checked_add(add).map(MemAddrKey::Const),
            MemAddrKey::BaseOffset { base, offset } => {
                offset.checked_add(add).map(|offset| MemAddrKey::BaseOffset { base, offset })
            }
        }
    }

    fn ranges_overlap_mem_keys(
        func: &Function,
        read: MemAddrKey,
        read_size: u64,
        write: MemAddrKey,
        write_size: u64,
    ) -> bool {
        if (Self::is_scratch_const(read) && Self::is_fmp_heap_key(func, write))
            || (Self::is_fmp_heap_key(func, read) && Self::is_scratch_const(write))
        {
            return false;
        }

        match (read, write) {
            (MemAddrKey::Const(read), MemAddrKey::Const(write)) => {
                mir_utils::ranges_overlap(read, read_size, write, write_size)
            }
            (
                MemAddrKey::BaseOffset { base: read_base, offset: read },
                MemAddrKey::BaseOffset { base: write_base, offset: write },
            ) if read_base == write_base => {
                mir_utils::ranges_overlap(read, read_size, write, write_size)
            }
            _ => true,
        }
    }

    fn is_scratch_const(key: MemAddrKey) -> bool {
        matches!(key, MemAddrKey::Const(offset) if offset < 128)
    }

    fn is_fmp_heap_key(func: &Function, key: MemAddrKey) -> bool {
        let MemAddrKey::BaseOffset { base, .. } = key else {
            return false;
        };
        Self::is_fmp_heap_value(func, base, 0)
    }

    fn is_fmp_heap_value(func: &Function, value: ValueId, depth: usize) -> bool {
        if depth > 8 {
            return false;
        }
        let Value::Inst(inst_id) = func.values[value] else {
            return false;
        };
        match func.instructions[inst_id].kind {
            InstKind::MLoad(addr) => func.value_u64(addr) == Some(0x40),
            InstKind::Add(a, b) => {
                Self::is_fmp_heap_value(func, a, depth + 1)
                    || Self::is_fmp_heap_value(func, b, depth + 1)
            }
            _ => false,
        }
    }

    fn is_memory_or_gas_observer(kind: &InstKind) -> bool {
        matches!(
            kind,
            InstKind::MStore8(_, _)
                | InstKind::MSize
                | InstKind::MCopy(_, _, _)
                | InstKind::CalldataCopy(_, _, _)
                | InstKind::CodeCopy(_, _, _)
                | InstKind::ReturnDataCopy(_, _, _)
                | InstKind::ExtCodeCopy(_, _, _, _)
                | InstKind::Keccak256(_, _)
                | InstKind::Call { .. }
                | InstKind::StaticCall { .. }
                | InstKind::DelegateCall { .. }
                | InstKind::InternalCall { .. }
                | InstKind::Create(_, _, _)
                | InstKind::Create2(_, _, _, _)
                | InstKind::Log0(_, _)
                | InstKind::Log1(_, _, _)
                | InstKind::Log2(_, _, _, _)
                | InstKind::Log3(_, _, _, _, _)
                | InstKind::Log4(_, _, _, _, _, _)
                | InstKind::Gas
        )
    }

    fn has_frame_observer(func: &Function) -> bool {
        func.blocks.iter().any(|block| {
            block.instructions.iter().any(|&inst_id| {
                matches!(
                    func.instructions[inst_id].kind,
                    InstKind::Gas | InstKind::MSize | InstKind::InternalCall { .. }
                )
            })
        })
    }

    fn internal_frame_read_ranges(func: &Function) -> Option<Vec<(u64, u64)>> {
        let mut reads = Vec::new();

        for block in func.blocks.iter() {
            for &inst_id in &block.instructions {
                match func.instructions[inst_id].kind {
                    InstKind::MLoad(addr) => {
                        if let Some(offset) = Self::internal_frame_offset(func, addr) {
                            reads.push((offset, 32));
                        }
                    }
                    InstKind::Keccak256(offset, size)
                    | InstKind::Log0(offset, size)
                    | InstKind::Create(_, offset, size) => {
                        Self::push_frame_read(func, &mut reads, offset, size)?;
                    }
                    InstKind::Log1(offset, size, _) => {
                        Self::push_frame_read(func, &mut reads, offset, size)?;
                    }
                    InstKind::Log2(offset, size, _, _)
                    | InstKind::MCopy(_, offset, size)
                    | InstKind::Create2(_, offset, size, _) => {
                        Self::push_frame_read(func, &mut reads, offset, size)?;
                    }
                    InstKind::Log3(offset, size, _, _, _) => {
                        Self::push_frame_read(func, &mut reads, offset, size)?;
                    }
                    InstKind::Log4(offset, size, _, _, _, _) => {
                        Self::push_frame_read(func, &mut reads, offset, size)?;
                    }
                    InstKind::Call { args_offset, args_size, .. }
                    | InstKind::StaticCall { args_offset, args_size, .. }
                    | InstKind::DelegateCall { args_offset, args_size, .. } => {
                        Self::push_frame_read(func, &mut reads, args_offset, args_size)?;
                    }
                    _ => {}
                }
            }
        }

        for block in func.blocks.iter() {
            match block.terminator.as_ref() {
                Some(Terminator::ReturnData { offset, size })
                | Some(Terminator::Revert { offset, size }) => {
                    Self::push_frame_read(func, &mut reads, *offset, *size)?;
                }
                _ => {}
            }
        }

        Some(reads)
    }

    fn push_frame_read(
        func: &Function,
        reads: &mut Vec<(u64, u64)>,
        offset: ValueId,
        size: ValueId,
    ) -> Option<()> {
        if let Some(frame_offset) = Self::internal_frame_offset(func, offset) {
            reads.push((frame_offset, func.value_u64(size)?));
        }
        Some(())
    }

    fn internal_frame_offset(func: &Function, value: ValueId) -> Option<u64> {
        Self::internal_frame_offset_with_depth(func, value, 0)
    }

    fn internal_frame_offset_with_depth(
        func: &Function,
        value: ValueId,
        depth: usize,
    ) -> Option<u64> {
        if depth > 8 {
            return None;
        }

        match func.values[value] {
            Value::Inst(inst_id) => match func.instructions[inst_id].kind {
                InstKind::InternalFrameAddr(offset) => Some(offset),
                InstKind::Add(a, b) => Self::internal_frame_add_offset(func, a, b, depth)
                    .or_else(|| Self::internal_frame_add_offset(func, b, a, depth)),
                _ => None,
            },
            _ => None,
        }
    }

    fn internal_frame_add_offset(
        func: &Function,
        base: ValueId,
        offset: ValueId,
        depth: usize,
    ) -> Option<u64> {
        let base = Self::internal_frame_offset_with_depth(func, base, depth + 1)?;
        base.checked_add(func.value_u64(offset)?)
    }

    fn can_mutate_memory(kind: &InstKind) -> bool {
        matches!(
            kind,
            InstKind::MStore8(_, _)
                | InstKind::MCopy(_, _, _)
                | InstKind::CalldataCopy(_, _, _)
                | InstKind::CodeCopy(_, _, _)
                | InstKind::ReturnDataCopy(_, _, _)
                | InstKind::ExtCodeCopy(_, _, _, _)
                | InstKind::Call { .. }
                | InstKind::StaticCall { .. }
                | InstKind::DelegateCall { .. }
                | InstKind::InternalCall { .. }
        )
    }

    fn cross_block_memory_barrier(kind: &InstKind) -> bool {
        matches!(kind, InstKind::MLoad(_)) || Self::is_memory_or_gas_observer(kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{FunctionBuilder, FunctionId};
    use solar_interface::Ident;

    fn test_func() -> Function {
        Function::new(Ident::DUMMY)
    }

    #[test]
    fn removes_overwritten_store() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let zero = builder.imm_u64(0);
        let value = builder.imm_u64(42);
        builder.mstore(addr, zero);
        builder.mstore(addr, value);
        builder.stop();

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 1);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 1);
    }

    #[test]
    fn forwards_store_observed_only_by_load() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let zero = builder.imm_u64(0);
        let value = builder.imm_u64(42);
        builder.mstore(addr, zero);
        let loaded = builder.mload(addr);
        builder.mstore(addr, value);
        builder.ret(vec![loaded]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 2);

        let block = &func.blocks[func.entry_block];
        assert_eq!(block.instructions.len(), 1);
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(values.as_slice(), &[zero]);
    }

    #[test]
    fn dead_store_survives_disjoint_keccak_read() {
        // mstore(128, 0); keccak256(0, 64); mstore(128, 1) — the keccak reads
        // scratch [0, 64), disjoint from 128, so the default-init store is dead.
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let slot = builder.imm_u64(128);
        let zero = builder.imm_u64(0);
        let one = builder.imm_u64(1);
        let size = builder.imm_u64(64);
        builder.mstore(slot, zero);
        builder.keccak256(zero, size);
        builder.mstore(slot, one);
        builder.stop();

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 1);
        // The keccak and the surviving store remain.
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 2);
    }

    #[test]
    fn overlapping_keccak_read_blocks_store_elimination() {
        // The keccak reads [96, 160), which covers 128, so the earlier store is
        // observed and must be kept.
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let slot = builder.imm_u64(128);
        let read_start = builder.imm_u64(96);
        let zero = builder.imm_u64(0);
        let one = builder.imm_u64(1);
        let size = builder.imm_u64(64);
        builder.mstore(slot, zero);
        builder.keccak256(read_start, size);
        builder.mstore(slot, one);
        builder.stop();

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 0);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 3);
    }

    #[test]
    fn removes_cross_block_dead_store_over_branch() {
        // entry: mstore(128, 0); br cond, hot, cold
        // cold:  revert(0, 0)                  — never reads 128
        // hot:   mstore(128, 1); returndata(128, 32)
        // The default-init in entry is dead: every path either reverts (reads
        // only [0,0)) or overwrites 128 before the returndata reads it.
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let slot = builder.imm_u64(128);
        let zero = builder.imm_u64(0);
        let one = builder.imm_u64(1);
        let word = builder.imm_u64(32);
        let cond = builder.add_param(crate::mir::MirType::uint256());
        let hot = builder.create_block();
        let cold = builder.create_block();
        builder.mstore(slot, zero);
        builder.branch(cond, hot, cold);

        builder.switch_to_block(cold);
        builder.revert(zero, zero);

        builder.switch_to_block(hot);
        builder.mstore(slot, one);
        builder.ret_data(slot, word);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run_to_fixpoint(&mut func), 1);
        let entry_stores = func.blocks[func.entry_block]
            .instructions
            .iter()
            .filter(|&&id| matches!(func.instructions[id].kind, InstKind::MStore(_, _)))
            .count();
        assert_eq!(entry_stores, 0);
    }

    #[test]
    fn keeps_cross_block_store_read_on_one_path() {
        // entry: mstore(128, 7); br cond, reader, writer
        // reader: returndata(128, 32)          — observes the entry store
        // writer: mstore(128, 9); returndata(128, 32)
        // The entry store is live on the reader path and must survive.
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let slot = builder.imm_u64(128);
        let seven = builder.imm_u64(7);
        let nine = builder.imm_u64(9);
        let word = builder.imm_u64(32);
        let cond = builder.add_param(crate::mir::MirType::uint256());
        let reader = builder.create_block();
        let writer = builder.create_block();
        builder.mstore(slot, seven);
        builder.branch(cond, reader, writer);

        builder.switch_to_block(reader);
        builder.ret_data(slot, word);

        builder.switch_to_block(writer);
        builder.mstore(slot, nine);
        builder.ret_data(slot, word);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run_to_fixpoint(&mut func), 0);
        let entry_stores = func.blocks[func.entry_block]
            .instructions
            .iter()
            .filter(|&&id| matches!(func.instructions[id].kind, InstKind::MStore(_, _)))
            .count();
        assert_eq!(entry_stores, 1);
    }

    #[test]
    fn keeps_store_before_return_pointer() {
        // A value returned to an internal caller may be a memory pointer, so a
        // store cannot be assumed dead just because this function never reads it
        // back. `ret` (Terminator::Return) must keep memory live.
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let buffer = builder.imm_u64(160);
        let ptr = builder.imm_u64(160);
        let payload = builder.imm_u64(42);
        builder.mstore(buffer, payload);
        builder.ret(vec![ptr]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run_to_fixpoint(&mut func), 0);
        let stores = func.blocks[func.entry_block]
            .instructions
            .iter()
            .filter(|&&id| matches!(func.instructions[id].kind, InstKind::MStore(_, _)))
            .count();
        assert_eq!(stores, 1);
    }

    #[test]
    fn gas_is_a_barrier() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let zero = builder.imm_u64(0);
        let value = builder.imm_u64(42);
        builder.mstore(addr, zero);
        builder.gas();
        builder.mstore(addr, value);
        builder.stop();

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 0);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 3);
    }

    #[test]
    fn handles_distinct_immediate_values_for_same_address() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr1 = builder.imm_u64(128);
        let addr2 = builder.imm_u64(128);
        let zero = builder.imm_u64(0);
        let value = builder.imm_u64(42);
        builder.mstore(addr1, zero);
        builder.mstore(addr2, value);
        builder.stop();

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 1);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 1);
    }

    #[test]
    fn handles_equivalent_base_offset_addresses() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let base = builder.add_param(crate::mir::MirType::uint256());
        let offset = builder.imm_u64(32);
        let value = builder.imm_u64(42);
        let addr1 = builder.add(base, offset);
        builder.mstore(addr1, value);
        let addr2 = builder.add(base, offset);
        let loaded = builder.mload(addr2);
        builder.ret(vec![loaded]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 1);

        let block = &func.blocks[func.entry_block];
        assert_eq!(block.instructions.len(), 3);
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(values.as_slice(), &[value]);
    }

    #[test]
    fn removes_overwritten_store_to_equivalent_base_offset_address() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let base = builder.add_param(crate::mir::MirType::uint256());
        let offset = builder.imm_u64(32);
        let zero = builder.imm_u64(0);
        let value = builder.imm_u64(42);
        let addr1 = builder.add(base, offset);
        builder.mstore(addr1, zero);
        let addr2 = builder.add(base, offset);
        builder.mstore(addr2, value);
        builder.stop();

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 1);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 3);
    }

    #[test]
    fn removes_unused_internal_frame_store() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let frame = builder.internal_frame_addr(192);
        let zero = builder.imm_u64(0);
        builder.mstore(frame, zero);
        builder.ret(vec![zero]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run_to_fixpoint(&mut func), 1);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 1);
    }

    #[test]
    fn reuses_dominated_immutable_copy_load() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let dest = builder.imm_u64(0);
        let immutable_len = builder.imm_u64(64);
        let word_size = builder.imm_u64(32);
        let code_size = builder.codesize();
        let offset = builder.sub(code_size, immutable_len);
        builder.codecopy(dest, offset, word_size);
        let first = builder.mload(dest);
        let next = builder.create_block();
        builder.jump(next);

        builder.switch_to_block(next);
        let code_size = builder.codesize();
        let offset = builder.sub(code_size, immutable_len);
        builder.codecopy(dest, offset, word_size);
        let second = builder.mload(dest);
        let sum = builder.add(first, second);
        builder.ret(vec![sum]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run_to_fixpoint(&mut func), 1);

        let active_insts = func.blocks.iter().flat_map(|block| block.instructions.iter().copied());
        let mut code_copies = 0;
        let mut loads = 0;
        let mut stores = 0;
        for inst_id in active_insts {
            match func.instructions[inst_id].kind {
                InstKind::CodeCopy(_, _, _) => code_copies += 1,
                InstKind::MLoad(_) => loads += 1,
                InstKind::MStore(_, _) => stores += 1,
                _ => {}
            }
        }
        assert_eq!(code_copies, 1);
        assert_eq!(loads, 1);
        assert_eq!(stores, 1);

        let add = match &func.values[sum] {
            Value::Inst(inst_id) => &func.instructions[*inst_id].kind,
            _ => panic!("expected add instruction"),
        };
        let InstKind::Add(lhs, rhs) = *add else {
            panic!("expected add instruction");
        };
        assert_eq!(lhs, first);
        assert_eq!(rhs, first);
    }

    #[test]
    fn overlapping_load_blocks_overwritten_store_elimination() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let base = builder.imm_u64(128);
        let one = builder.imm_u64(1);
        let zero = builder.imm_u64(0);
        let value = builder.imm_u64(42);
        builder.mstore(base, zero);
        let overlap = builder.add(base, one);
        builder.mload(overlap);
        builder.mstore(base, value);
        builder.stop();

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 0);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 4);
    }

    #[test]
    fn forwards_load_from_store() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let value = builder.imm_u64(42);
        builder.mstore(addr, value);
        let loaded = builder.mload(addr);
        builder.ret(vec![loaded]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 1);

        let block = &func.blocks[func.entry_block];
        assert_eq!(block.instructions.len(), 1);
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(values.as_slice(), &[value]);
    }

    #[test]
    fn forwards_load_through_non_memory_operation() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let value = builder.imm_u64(42);
        let one = builder.imm_u64(1);
        builder.mstore(addr, value);
        builder.add(one, one);
        let loaded = builder.mload(addr);
        builder.ret(vec![loaded]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 1);

        let block = &func.blocks[func.entry_block];
        assert_eq!(block.instructions.len(), 2);
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(values.as_slice(), &[value]);
    }

    #[test]
    fn does_not_forward_load_across_memory_write() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let other_addr = builder.imm_u64(160);
        let value = builder.imm_u64(42);
        let len = builder.imm_u64(32);
        builder.mstore(addr, value);
        builder.mcopy(other_addr, addr, len);
        let loaded = builder.mload(addr);
        builder.ret(vec![loaded]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 0);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 3);
    }

    #[test]
    fn does_not_forward_load_across_internal_call() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let value = builder.imm_u64(42);
        builder.mstore(addr, value);
        builder.internal_call_void(FunctionId::from_usize(0), Vec::new(), 0);
        let loaded = builder.mload(addr);
        builder.ret(vec![loaded]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 0);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 3);
    }

    #[test]
    fn resolves_chained_forwarded_loads() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr1 = builder.imm_u64(128);
        let addr2 = builder.imm_u64(160);
        let value = builder.imm_u64(42);
        builder.mstore(addr1, value);
        let loaded1 = builder.mload(addr1);
        builder.mstore(addr2, loaded1);
        let loaded2 = builder.mload(addr2);
        builder.ret(vec![loaded2]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 2);

        let block = &func.blocks[func.entry_block];
        assert_eq!(block.instructions.len(), 2);
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(values.as_slice(), &[value]);
    }
}

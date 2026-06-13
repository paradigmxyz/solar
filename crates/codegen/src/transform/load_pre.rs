//! Dataflow-based redundancy elimination and PRE for memory-dependent reads.
//!
//! CSE reuses state-dependent reads only along the dominator tree. This pass removes the
//! join redundancies that dominance cannot see, driven by an available-expressions
//! dataflow over storage, transient-storage, memory, and keccak read keys:
//!
//! 1. **Full redundancy**: a read available at the end of every predecessor (computed in both arms
//!    of a diamond, or live around a loop backedge) and recomputed at the join becomes a phi over
//!    the per-predecessor values.
//! 2. **Store-to-load forwarding across joins**: a store in one arm makes the value known on that
//!    path, so the join reload merges stored and loaded values.
//! 3. **Partial redundancy**: a read available on some predecessors is inserted at the end of the
//!    jump-terminated remaining predecessors, then handled as a full redundancy.
//!
//! # Keys
//!
//! One key universe per function:
//! - `(Storage, alias)`: genned by `sload` (load result) and `sstore` (forwarded stored value);
//!   killed by may-aliasing `sstore` and by calls and creates. `STATICCALL` cannot mutate storage
//!   or transient storage and does not kill them.
//! - `(Transient, alias)`: same with `tload`/`tstore`.
//! - `(Memory, canonical address, width 32)`: genned by `mload` and by `mstore` at the same
//!   canonical address (forwarded stored value); killed by overlapping `mstore`/`mstore8`, by
//!   copies with an overlapping or unknown destination range, and by every call (including
//!   `STATICCALL`, which writes its return buffer).
//! - `(Keccak, canonical offset, size or dynamic-size value)`: genned by `keccak256`; killed like a
//!   memory read over its range. Reads with a non-constant size key the size operand so reads of
//!   different lengths never unify.
//!
//! # Availability dataflow
//!
//! Standard available expressions over the finite key universe:
//! `OUT(b) = (IN(b) - KILL(b)) | GEN(b)`, `IN(b)` = intersection of `OUT` over reachable
//! predecessors (the entry starts empty). `OUT` is optimistically initialized to the full
//! universe for non-entry blocks; this is required for loop headers, where the backedge
//! must not pessimistically erase availability before the fixpoint settles. The greatest
//! fixpoint of this system is the standard, sound all-paths solution.
//!
//! # Value location
//!
//! A key available at a predecessor's end is usable only if a concrete value can be
//! located: scan the predecessor backwards for a gen before any kill (load result or
//! forwarded store value). If the block neither gens nor kills the key, recurse to its
//! immediate dominator, provided the dataflow has the key in `IN(pred)` and no block on
//! any CFG path between the dominator and the predecessor may kill the key. The path
//! purity check is what keeps the walk sound: availability alone does not imply the value
//! is uniform, because a non-dominating path can kill and re-gen the key with a different
//! value (e.g. an `sstore` in one arm of a diamond between the dominator and the
//! predecessor). If no concrete value can be located (the value only exists as a
//! cross-path merge), the predecessor is treated as unavailable.
//!
//! # Safety of rewrites
//!
//! A join load is a candidate only if no kill of its key precedes it in the join block.
//! For that scan, `gas` is additionally treated as a kill in all spaces and `msize` as a
//! kill for memory and keccak keys: a partial-redundancy insertion moves the read to the
//! end of a predecessor, so everything in the join block above the original load executes
//! after the moved read, and a `gas`/`msize` read there would observe the moved load's gas
//! and memory-expansion effects early. Removal-only (fully redundant) rewrites do not move
//! reads, but we keep the single conservative scan for both cases for simplicity.
//!
//! An inserted load reads exactly the state the original would have read on that path: it
//! sits at the end of the predecessor (nothing follows it but the jump), and the join
//! prefix above the original load contains no kills of the key.
//!
//! # Termination
//!
//! The same discipline as [`pre`](super::pre):
//! 1. Instructions inserted by this run are never picked as rewrite candidates, so every rewrite
//!    retires a load that existed when the run started.
//! 2. A `(key, block)` pair never gets an insertion after an elimination in that block, preventing
//!    ping-pong between mutually-preceding joins.
//! 3. A function-size-derived rewrite budget backstops the above.

use crate::{
    analysis::{CfgInfo, DominatorTree},
    mir::{
        BlockId, Function, InstId, InstKind, Instruction, InstructionMetadata, MemoryRegion,
        MirType, StorageAlias, Terminator, Value, ValueId,
    },
    pass::FunctionPass,
    utils::repair_reachability_phis,
};
use alloy_primitives::U256;
use solar_data_structures::map::{FxHashMap, FxHashSet};

/// Statistics for load PRE.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoadPreStats {
    /// Number of join-block loads replaced by phis or available values.
    pub loads_eliminated: usize,
    /// Number of compensating loads inserted into predecessors.
    pub loads_inserted: usize,
}

impl LoadPreStats {
    /// Returns the total number of MIR edits made by this pass.
    pub const fn total(self) -> usize {
        self.loads_eliminated + self.loads_inserted
    }
}

/// Dataflow-based redundancy eliminator for memory-dependent reads.
#[derive(Debug, Default)]
pub struct LoadRedundancyEliminator {
    stats: LoadPreStats,
}

/// Function pass for load PRE.
pub struct LoadPrePass;

impl FunctionPass for LoadPrePass {
    fn name(&self) -> &str {
        "load-pre"
    }

    fn run_on_function(&mut self, func: &mut Function) -> bool {
        LoadRedundancyEliminator::new().run(func).total() != 0
    }
}

/// A normalized key for a state-dependent read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum LoadKey {
    Storage(StorageAlias),
    Transient(StorageAlias),
    /// A 32-byte memory read at a canonical address.
    Memory(MemAddr),
    Keccak(MemAddr, KeccakSize),
}

/// A canonical memory address: an optional symbolic base plus a known offset.
/// A `None` base is an absolute address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct MemAddr {
    region: MemoryRegion,
    base: Option<ValueId>,
    offset: u64,
}

/// The size of a keccak read: a known constant or the canonical size operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum KeccakSize {
    Const(u64),
    Dyn(ValueId),
}

/// Where a gen's value comes from.
enum GenSource {
    /// The instruction's own result (a load).
    LoadResult,
    /// The forwarded stored value (a store).
    Stored(ValueId),
}

/// A dense bitset over key-universe indices.
#[derive(Clone, Debug, PartialEq, Eq)]
struct KeySet {
    bits: Vec<u64>,
}

impl KeySet {
    fn empty(len: usize) -> Self {
        Self { bits: vec![0; len.div_ceil(64)] }
    }

    fn full(len: usize) -> Self {
        let mut bits = vec![!0u64; len.div_ceil(64)];
        let rem = len % 64;
        if rem != 0
            && let Some(last) = bits.last_mut()
        {
            *last = (1u64 << rem) - 1;
        }
        Self { bits }
    }

    fn insert(&mut self, idx: usize) {
        self.bits[idx / 64] |= 1 << (idx % 64);
    }

    fn remove(&mut self, idx: usize) {
        self.bits[idx / 64] &= !(1 << (idx % 64));
    }

    fn contains(&self, idx: usize) -> bool {
        self.bits[idx / 64] & (1 << (idx % 64)) != 0
    }

    fn is_empty(&self) -> bool {
        self.bits.iter().all(|word| *word == 0)
    }

    fn intersect_with(&mut self, other: &Self) {
        for (word, other) in self.bits.iter_mut().zip(&other.bits) {
            *word &= other;
        }
    }

    fn subtract(&mut self, other: &Self) {
        for (word, other) in self.bits.iter_mut().zip(&other.bits) {
            *word &= !other;
        }
    }

    fn union_with(&mut self, other: &Self) {
        for (word, other) in self.bits.iter_mut().zip(&other.bits) {
            *word |= other;
        }
    }
}

/// A join-block load rewrite: replace `inst` with a phi over `incoming`, after
/// inserting copies of `kind` at the end of the `insertions` predecessors.
struct Candidate {
    target: BlockId,
    inst: InstId,
    key: LoadKey,
    result: ValueId,
    result_ty: MirType,
    kind: InstKind,
    metadata: InstructionMetadata,
    incoming: Vec<(BlockId, ValueId)>,
    insertions: Vec<BlockId>,
}

/// Per-round analysis shared by candidate collection.
struct Analysis {
    keys: Vec<LoadKey>,
    key_index: FxHashMap<LoadKey, usize>,
    reachable: FxHashSet<BlockId>,
    /// Per-block keys killed at any point in the block; only blocks that kill
    /// something have an entry.
    kills: FxHashMap<BlockId, KeySet>,
    /// Availability at block entry, over all paths from the entry block.
    ins: FxHashMap<BlockId, KeySet>,
    /// Availability at block exit.
    outs: FxHashMap<BlockId, KeySet>,
    /// CFG path reachability for the value-location purity check; empty when
    /// no block kills any key.
    reach: FxHashMap<BlockId, FxHashSet<BlockId>>,
    dominators: DominatorTree,
    inst_results: FxHashMap<InstId, ValueId>,
    inst_blocks: FxHashMap<InstId, BlockId>,
}

impl Analysis {
    /// Returns true if any block on a CFG path from `from` to `to` (excluding
    /// `from` itself, whose effects the caller scans directly) may kill `key`.
    fn path_kills_key(&self, from: BlockId, to: BlockId, key_idx: usize) -> bool {
        if self.kills.is_empty() {
            return false;
        }
        let Some(reachable_from) = self.reach.get(&from) else { return true };
        for (&mid, kills) in &self.kills {
            if mid == from || !kills.contains(key_idx) {
                continue;
            }
            if reachable_from.contains(&mid)
                && self.reach.get(&mid).is_some_and(|reach| reach.contains(&to))
            {
                return true;
            }
        }
        false
    }
}

/// Mutable state threaded through one candidate-collection round.
struct CandidateCx<'a> {
    analysis: &'a Analysis,
    eliminated_keys: &'a FxHashSet<(LoadKey, BlockId)>,
    inserted_insts: &'a FxHashSet<InstId>,
    locate_cache: FxHashMap<(BlockId, usize), Option<ValueId>>,
}

impl LoadRedundancyEliminator {
    /// Creates a new load PRE pass.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns statistics from the most recent run.
    pub const fn stats(&self) -> LoadPreStats {
        self.stats
    }

    /// Runs load PRE to a fixed point under the rewrite budget.
    pub fn run(&mut self, func: &mut Function) -> LoadPreStats {
        self.stats = LoadPreStats::default();
        repair_reachability_phis(func);

        let rewrite_limit = func.instructions.len().saturating_mul(2).max(64);
        let mut rewrites = 0usize;
        let mut eliminated_keys: FxHashSet<(LoadKey, BlockId)> = FxHashSet::default();
        let mut inserted_insts: FxHashSet<InstId> = FxHashSet::default();

        while rewrites < rewrite_limit {
            let Some(analysis) = Self::compute_analysis(func) else { break };
            let mut cx = CandidateCx {
                analysis: &analysis,
                eliminated_keys: &eliminated_keys,
                inserted_insts: &inserted_insts,
                locate_cache: FxHashMap::default(),
            };
            let batch = self.collect_candidates(func, &mut cx, rewrite_limit - rewrites);
            if batch.is_empty() {
                break;
            }
            rewrites += batch.len();
            for candidate in batch {
                self.apply_candidate(func, candidate, &mut eliminated_keys, &mut inserted_insts);
            }
        }

        self.stats
    }

    /// Computes the key universe, the per-block gen/kill summaries, and the
    /// availability fixpoint. Returns `None` if no read is trackable.
    fn compute_analysis(func: &Function) -> Option<Analysis> {
        let mut cfg = CfgInfo::new(func);
        let rpo = cfg.rpo();

        // The key universe: every key genned in a reachable block.
        let mut keys = Vec::new();
        let mut key_index: FxHashMap<LoadKey, usize> = FxHashMap::default();
        for &block in rpo {
            for &inst_id in &func.blocks[block].instructions {
                if let Some((key, _)) = Self::gen_key_value(func, inst_id) {
                    key_index.entry(key).or_insert_with(|| {
                        keys.push(key);
                        keys.len() - 1
                    });
                }
            }
        }
        if keys.is_empty() {
            return None;
        }
        let key_count = keys.len();

        // Per-block summaries: GEN holds keys genned after the last kill, KILL
        // holds keys killed at any point.
        let mut gens = FxHashMap::default();
        let mut kills = FxHashMap::default();
        for &block in rpo {
            let mut gen_set = KeySet::empty(key_count);
            let mut kill_set = KeySet::empty(key_count);
            for &inst_id in &func.blocks[block].instructions {
                if func.instructions[inst_id].kind.has_side_effects() {
                    for (idx, &key) in keys.iter().enumerate() {
                        if Self::inst_kills_key(func, inst_id, key) {
                            kill_set.insert(idx);
                            gen_set.remove(idx);
                        }
                    }
                }
                // A store both kills aliases and gens its exact key; the gen
                // wins for the exact key because the slot then holds the
                // stored value.
                if let Some((key, _)) = Self::gen_key_value(func, inst_id)
                    && let Some(&idx) = key_index.get(&key)
                {
                    gen_set.insert(idx);
                }
            }
            if !kill_set.is_empty() {
                kills.insert(block, kill_set);
            }
            gens.insert(block, gen_set);
        }

        // Availability fixpoint with optimistic initialization.
        let mut ins: FxHashMap<BlockId, KeySet> = FxHashMap::default();
        let mut outs: FxHashMap<BlockId, KeySet> = rpo
            .iter()
            .map(|&block| {
                let out = if block == func.entry_block {
                    gens[&block].clone()
                } else {
                    KeySet::full(key_count)
                };
                (block, out)
            })
            .collect();
        loop {
            let mut changed = false;
            for &block in rpo {
                let in_set = if block == func.entry_block {
                    KeySet::empty(key_count)
                } else {
                    let mut acc: Option<KeySet> = None;
                    for &pred in &func.blocks[block].predecessors {
                        // Unreachable predecessors never execute and cannot
                        // contribute a path.
                        let Some(out) = outs.get(&pred) else { continue };
                        match &mut acc {
                            Some(acc) => acc.intersect_with(out),
                            None => acc = Some(out.clone()),
                        }
                    }
                    acc.unwrap_or_else(|| KeySet::empty(key_count))
                };
                let mut out = in_set.clone();
                if let Some(kill) = kills.get(&block) {
                    out.subtract(kill);
                }
                out.union_with(&gens[&block]);
                ins.insert(block, in_set);
                if outs.get(&block) != Some(&out) {
                    outs.insert(block, out);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let reach = if kills.is_empty() {
            FxHashMap::default()
        } else {
            cfg.transitive_reachability().clone()
        };

        Some(Analysis {
            keys,
            key_index,
            reachable: cfg.reachable().clone(),
            kills,
            ins,
            outs,
            reach,
            dominators: cfg.dominators().clone(),
            inst_results: Self::inst_results(func),
            inst_blocks: Self::inst_blocks(func),
        })
    }

    /// Collects non-interfering candidates from one analysis snapshot so they
    /// can be applied as a batch.
    fn collect_candidates(
        &self,
        func: &Function,
        cx: &mut CandidateCx<'_>,
        limit: usize,
    ) -> Vec<Candidate> {
        let mut batch = Vec::new();
        // Candidates whose analysis would be invalidated by an earlier
        // candidate in this batch are deferred to the next round.
        let mut modified_blocks: FxHashSet<BlockId> = FxHashSet::default();
        let mut eliminated_values: FxHashSet<ValueId> = FxHashSet::default();

        'targets: for target in func.blocks.indices() {
            if !cx.analysis.reachable.contains(&target) {
                continue;
            }
            let predecessors = Self::unique_predecessors(func, target);
            if predecessors.len() < 2
                || predecessors.iter().any(|pred| !cx.analysis.reachable.contains(pred))
            {
                continue;
            }

            for (inst, key_idx) in Self::first_loads(func, cx.analysis, target) {
                if batch.len() >= limit {
                    break 'targets;
                }
                // Termination rule 1: never rewrite a load this run inserted.
                if cx.inserted_insts.contains(&inst) {
                    continue;
                }
                let Some(candidate) =
                    self.candidate_for_load(func, cx, target, inst, key_idx, &predecessors)
                else {
                    continue;
                };
                if Self::interferes_with_batch(&candidate, &modified_blocks, &eliminated_values) {
                    continue;
                }
                modified_blocks.insert(candidate.target);
                modified_blocks.extend(candidate.insertions.iter().copied());
                eliminated_values.insert(candidate.result);
                batch.push(candidate);
            }
        }

        batch
    }

    /// Returns true if applying earlier candidates in the batch invalidates
    /// this candidate's analysis: its blocks were already rewritten, or it
    /// references a value whose defining load the batch removes.
    fn interferes_with_batch(
        candidate: &Candidate,
        modified_blocks: &FxHashSet<BlockId>,
        eliminated_values: &FxHashSet<ValueId>,
    ) -> bool {
        modified_blocks.contains(&candidate.target)
            || candidate.insertions.iter().any(|block| modified_blocks.contains(block))
            || candidate.incoming.iter().any(|(_, value)| eliminated_values.contains(value))
            || (!candidate.insertions.is_empty()
                && candidate
                    .kind
                    .operands()
                    .into_iter()
                    .any(|value| eliminated_values.contains(&value)))
    }

    /// Returns, in program order, the first load of each key in `target` that
    /// no kill of that key precedes.
    ///
    /// `gas` and `msize` conservatively end or restrict the scan: a
    /// partial-redundancy insertion moves the read to a predecessor's end, so
    /// it must not cross a `gas` (any space) or `msize` (memory and keccak)
    /// observation in the join prefix.
    fn first_loads(func: &Function, analysis: &Analysis, target: BlockId) -> Vec<(InstId, usize)> {
        let key_count = analysis.keys.len();
        let mut blocked = KeySet::empty(key_count);
        let mut taken: FxHashSet<usize> = FxHashSet::default();
        let mut found = Vec::new();

        for &inst_id in &func.blocks[target].instructions {
            if let Some((key, GenSource::LoadResult)) = Self::gen_key_value(func, inst_id) {
                if let Some(&idx) = analysis.key_index.get(&key)
                    && !blocked.contains(idx)
                    && taken.insert(idx)
                {
                    found.push((inst_id, idx));
                }
                continue;
            }
            let kind = &func.instructions[inst_id].kind;
            match kind {
                // `gas` blocks every space, so nothing after it can be a
                // candidate.
                InstKind::Gas => break,
                InstKind::MSize => {
                    for (idx, key) in analysis.keys.iter().enumerate() {
                        if matches!(key, LoadKey::Memory(_) | LoadKey::Keccak(_, _)) {
                            blocked.insert(idx);
                        }
                    }
                }
                _ if kind.has_side_effects() => {
                    // Kills block their keys; a store's own-key gen is also a
                    // kill here (the value differs from the predecessor-end
                    // state), which the may-alias check already covers.
                    for (idx, &key) in analysis.keys.iter().enumerate() {
                        if !blocked.contains(idx) && Self::inst_kills_key(func, inst_id, key) {
                            blocked.insert(idx);
                        }
                    }
                }
                _ => {}
            }
        }

        found
    }

    fn candidate_for_load(
        &self,
        func: &Function,
        cx: &mut CandidateCx<'_>,
        target: BlockId,
        inst: InstId,
        key_idx: usize,
        predecessors: &[BlockId],
    ) -> Option<Candidate> {
        let instruction = &func.instructions[inst];
        let result = *cx.analysis.inst_results.get(&inst)?;
        let result_ty = instruction.result_ty?;
        let key = cx.analysis.keys[key_idx];

        // The backend lowers phis through memory spill slots (one store per
        // predecessor plus a load at the use), which costs more than the
        // 3-gas MLOAD a rewrite would save. SLOAD/TLOAD (100) and KECCAK256
        // (30+) remain clear wins. Measured on the gas benchmark suite.
        if matches!(key, LoadKey::Memory(_)) {
            return None;
        }

        let mut incoming = Vec::with_capacity(predecessors.len());
        let mut insertions = Vec::new();
        for &pred in predecessors {
            if cx.analysis.outs.get(&pred).is_some_and(|out| out.contains(key_idx))
                && let Some(value) = self.locate_value(func, cx, pred, key_idx)
            {
                incoming.push((pred, value));
                continue;
            }
            // The key is unavailable on this predecessor; a compensating load
            // can only go on an edge that needs no splitting.
            if !Self::can_insert_on_edge(func, pred, target) {
                return None;
            }
            // Termination rule 2: never insert a key into a block it was
            // previously eliminated from in this run.
            if cx.eliminated_keys.contains(&(key, pred)) {
                return None;
            }
            if !Self::operands_dominate_block(func, &instruction.kind, pred, cx.analysis) {
                return None;
            }
            insertions.push(pred);
        }

        if !insertions.is_empty() {
            // The backend lowers phis through spill slots, so an insertion
            // always lengthens its own path by the phi copies while the
            // savings land on the other paths. Static counts cannot weigh
            // execution frequencies, so only loop-carried availability is
            // accepted: the value arrives over backedges (predecessors the
            // join dominates) and the compensating loads sit on the entry
            // edges, running once per loop entry while every iteration drops
            // a full load.
            let loop_carried = incoming
                .iter()
                .all(|&(pred, _)| cx.analysis.dominators.dominates(target, pred))
                && insertions.iter().all(|&pred| !cx.analysis.dominators.dominates(target, pred));
            if !loop_carried || insertions.len() > incoming.len() {
                return None;
            }
        }

        Some(Candidate {
            target,
            inst,
            key,
            result,
            result_ty,
            kind: instruction.kind.clone(),
            metadata: instruction.metadata.clone(),
            incoming,
            insertions,
        })
    }

    /// Locates the concrete value of `key` at the end of `block`, walking the
    /// dominator tree when the block is transparent for the key.
    fn locate_value(
        &self,
        func: &Function,
        cx: &mut CandidateCx<'_>,
        block: BlockId,
        key_idx: usize,
    ) -> Option<ValueId> {
        if let Some(&cached) = cx.locate_cache.get(&(block, key_idx)) {
            return cached;
        }
        let located = self.locate_value_uncached(func, cx, block, key_idx);
        cx.locate_cache.insert((block, key_idx), located);
        located
    }

    fn locate_value_uncached(
        &self,
        func: &Function,
        cx: &mut CandidateCx<'_>,
        block: BlockId,
        key_idx: usize,
    ) -> Option<ValueId> {
        let key = cx.analysis.keys[key_idx];
        for &inst_id in func.blocks[block].instructions.iter().rev() {
            if let Some((gen_key, source)) = Self::gen_key_value(func, inst_id)
                && gen_key == key
            {
                // A store's exact-key gen wins over its own kill: the slot
                // holds the stored value from this point on.
                return match source {
                    GenSource::LoadResult => cx.analysis.inst_results.get(&inst_id).copied(),
                    GenSource::Stored(value) => Some(value),
                };
            }
            if Self::inst_kills_key(func, inst_id, key) {
                return None;
            }
        }

        // The block is transparent for the key: the value at its end is the
        // value at its entry, which the dataflow must prove available on all
        // paths before the dominator walk may locate it.
        if !cx.analysis.ins.get(&block).is_some_and(|in_set| in_set.contains(key_idx)) {
            return None;
        }
        let idom = cx.analysis.dominators.idom(block)?;
        if idom == block {
            return None;
        }
        // Path purity: a non-dominating path between the dominator and this
        // block could kill and re-gen the key with a different value, which
        // availability alone does not rule out.
        if cx.analysis.path_kills_key(idom, block, key_idx) {
            return None;
        }
        self.locate_value(func, cx, idom, key_idx)
    }

    fn apply_candidate(
        &mut self,
        func: &mut Function,
        candidate: Candidate,
        eliminated_keys: &mut FxHashSet<(LoadKey, BlockId)>,
        inserted_insts: &mut FxHashSet<InstId>,
    ) {
        let Candidate {
            target,
            inst,
            key,
            result,
            result_ty,
            kind,
            metadata,
            mut incoming,
            insertions,
        } = candidate;

        eliminated_keys.insert((key, target));

        let fully_available = insertions.is_empty();
        for block in insertions {
            let new_inst = func.alloc_inst(Instruction {
                kind: kind.clone(),
                result_ty: Some(result_ty),
                metadata: metadata.clone(),
            });
            let value = func.alloc_value(Value::Inst(new_inst));
            func.blocks[block].instructions.push(new_inst);
            incoming.push((block, value));
            inserted_insts.insert(new_inst);
            self.stats.loads_inserted += 1;
        }
        incoming.sort_by_key(|(block, _)| block.index());

        // A fully-available key whose predecessors all locate the same value
        // needs no phi: that value's def dominates every predecessor and
        // therefore the join itself.
        let replacement = match incoming.first() {
            Some(&(_, first))
                if fully_available
                    && first != result
                    && incoming.iter().all(|&(_, value)| value == first) =>
            {
                first
            }
            _ => {
                let phi_inst =
                    func.alloc_inst(Instruction::new(InstKind::Phi(incoming), Some(result_ty)));
                let phi_value = func.alloc_value(Value::Inst(phi_inst));
                let phi_count = func.blocks[target]
                    .instructions
                    .iter()
                    .take_while(|&&inst_id| {
                        matches!(func.instructions[inst_id].kind, InstKind::Phi(_))
                    })
                    .count();
                func.blocks[target].instructions.insert(phi_count, phi_inst);
                phi_value
            }
        };

        Self::replace_uses(func, result, replacement);
        func.blocks[target].instructions.retain(|&inst_id| inst_id != inst);
        self.stats.loads_eliminated += 1;
    }

    // ----- Keys, gens, and kills -----

    /// Returns the key an instruction gens and where its value comes from.
    fn gen_key_value(func: &Function, inst_id: InstId) -> Option<(LoadKey, GenSource)> {
        match func.instructions[inst_id].kind {
            InstKind::SLoad(slot) => Some((
                LoadKey::Storage(Self::storage_alias(func, inst_id, slot)),
                GenSource::LoadResult,
            )),
            InstKind::SStore(slot, value) => Some((
                LoadKey::Storage(Self::storage_alias(func, inst_id, slot)),
                GenSource::Stored(value),
            )),
            InstKind::TLoad(slot) => Some((
                LoadKey::Transient(Self::storage_alias(func, inst_id, slot)),
                GenSource::LoadResult,
            )),
            InstKind::TStore(slot, value) => Some((
                LoadKey::Transient(Self::storage_alias(func, inst_id, slot)),
                GenSource::Stored(value),
            )),
            InstKind::MLoad(addr) => Self::mem_addr(func, inst_id, addr)
                .map(|addr| (LoadKey::Memory(addr), GenSource::LoadResult)),
            InstKind::MStore(addr, value) => Self::mem_addr(func, inst_id, addr)
                .map(|addr| (LoadKey::Memory(addr), GenSource::Stored(value))),
            InstKind::Keccak256(offset, size) => {
                let addr = Self::mem_addr(func, inst_id, offset)?;
                let size = match Self::const_u64(func, size) {
                    Some(size) => KeccakSize::Const(size),
                    None => KeccakSize::Dyn(size),
                };
                Some((LoadKey::Keccak(addr, size), GenSource::LoadResult))
            }
            _ => None,
        }
    }

    /// Returns true if an instruction may invalidate the value of `key`.
    fn inst_kills_key(func: &Function, inst_id: InstId, key: LoadKey) -> bool {
        let kind = &func.instructions[inst_id].kind;
        match key {
            LoadKey::Storage(alias) => match *kind {
                InstKind::SStore(slot, _) => {
                    Self::storage_alias(func, inst_id, slot).may_alias(alias)
                }
                // Calls and creates may re-enter and mutate storage;
                // STATICCALL cannot.
                _ => kind.may_mutate_storage(),
            },
            LoadKey::Transient(alias) => match *kind {
                InstKind::TStore(slot, _) => {
                    Self::storage_alias(func, inst_id, slot).may_alias(alias)
                }
                _ => kind.may_mutate_transient_storage(),
            },
            LoadKey::Memory(addr) => Self::memory_write_clobbers(func, inst_id, addr, Some(32)),
            LoadKey::Keccak(addr, size) => {
                let size = match size {
                    KeccakSize::Const(size) => Some(size),
                    KeccakSize::Dyn(_) => None,
                };
                Self::memory_write_clobbers(func, inst_id, addr, size)
            }
        }
    }

    /// Returns true if a memory-writing instruction may overlap the read
    /// range; reads with an unknown size are clobbered by any write that the
    /// region split cannot rule out.
    fn memory_write_clobbers(
        func: &Function,
        inst_id: InstId,
        read: MemAddr,
        read_size: Option<u64>,
    ) -> bool {
        let kind = &func.instructions[inst_id].kind;
        let (dest, write_size) = match *kind {
            InstKind::MStore(dest, _) => (dest, Some(32)),
            InstKind::MStore8(dest, _) => (dest, Some(1)),
            InstKind::MCopy(dest, _, size)
            | InstKind::CalldataCopy(dest, _, size)
            | InstKind::CodeCopy(dest, _, size)
            | InstKind::ReturnDataCopy(dest, _, size) => (dest, Self::const_u64(func, size)),
            InstKind::ExtCodeCopy(_, dest, _, size) => (dest, Self::const_u64(func, size)),
            // Every call clobbers tracked memory, including STATICCALL: its
            // return buffer write is a memory effect even in a static context.
            _ => return kind.may_mutate_memory(),
        };

        let write_region = func.instructions[inst_id]
            .metadata
            .memory_region
            .unwrap_or_else(|| Self::memory_region_for_addr(func, dest));
        if read.region != MemoryRegion::Unknown
            && write_region != MemoryRegion::Unknown
            && read.region != write_region
        {
            return false;
        }
        let (write_base, write_offset) = Self::memory_addr_base_offset(func, dest);
        if read.base != write_base {
            return true;
        }
        let (Some(read_size), Some(write_offset), Some(write_size)) =
            (read_size, write_offset, write_size)
        else {
            return true;
        };
        Self::ranges_overlap(read.offset, read_size, write_offset, write_size)
    }

    fn storage_alias(func: &Function, inst_id: InstId, slot: ValueId) -> StorageAlias {
        func.instructions[inst_id]
            .metadata
            .storage_alias
            .unwrap_or_else(|| StorageAlias::for_value(func, slot))
    }

    fn mem_addr(func: &Function, inst_id: InstId, addr: ValueId) -> Option<MemAddr> {
        let region = func.instructions[inst_id]
            .metadata
            .memory_region
            .unwrap_or_else(|| Self::memory_region_for_addr(func, addr));
        let (base, offset) = Self::memory_addr_base_offset(func, addr);
        Some(MemAddr { region, base, offset: offset? })
    }

    fn memory_region_for_addr(func: &Function, addr: ValueId) -> MemoryRegion {
        match func.value(addr) {
            Value::Immediate(imm)
                if imm.as_u256().is_some_and(|value| value < U256::from(0x80)) =>
            {
                MemoryRegion::Scratch
            }
            _ => MemoryRegion::Unknown,
        }
    }

    fn memory_addr_base_offset(func: &Function, addr: ValueId) -> (Option<ValueId>, Option<u64>) {
        if let Some((base, offset)) = Self::offset_chain(func, addr, 0) {
            if let Some(offset) = Self::u256_to_u64(offset) {
                return (Some(base), Some(offset));
            }
            return (Some(addr), Some(0));
        }
        match func.value(addr) {
            Value::Immediate(imm) => (None, imm.as_u256().and_then(Self::u256_to_u64)),
            Value::Arg { .. } | Value::Inst(_) | Value::Undef(_) => (Some(addr), Some(0)),
        }
    }

    /// Splits `value` into a symbolic base plus a constant offset by walking
    /// constant `add`/`sub` chains, so syntactically different addresses of
    /// the same location unify.
    fn offset_chain(func: &Function, value: ValueId, depth: usize) -> Option<(ValueId, U256)> {
        if depth >= 4 {
            return None;
        }
        match func.value(value) {
            Value::Immediate(_) => None,
            Value::Arg { .. } | Value::Undef(_) => Some((value, U256::ZERO)),
            Value::Inst(inst_id) => match func.instructions[*inst_id].kind {
                InstKind::Add(a, b) => {
                    if let Some(offset) = Self::const_u256(func, b) {
                        let (base, existing) = Self::offset_chain(func, a, depth + 1)?;
                        Some((base, existing.wrapping_add(offset)))
                    } else if let Some(offset) = Self::const_u256(func, a) {
                        let (base, existing) = Self::offset_chain(func, b, depth + 1)?;
                        Some((base, existing.wrapping_add(offset)))
                    } else {
                        Some((value, U256::ZERO))
                    }
                }
                InstKind::Sub(a, b) => {
                    let offset = Self::const_u256(func, b)?;
                    let (base, existing) = Self::offset_chain(func, a, depth + 1)?;
                    Some((base, existing.wrapping_sub(offset)))
                }
                _ => Some((value, U256::ZERO)),
            },
        }
    }

    fn ranges_overlap(a_start: u64, a_size: u64, b_start: u64, b_size: u64) -> bool {
        let Some(a_end) = a_start.checked_add(a_size) else { return true };
        let Some(b_end) = b_start.checked_add(b_size) else { return true };
        a_start < b_end && b_start < a_end
    }

    fn const_u256(func: &Function, value: ValueId) -> Option<U256> {
        let Value::Immediate(imm) = func.value(value) else { return None };
        imm.as_u256()
    }

    fn const_u64(func: &Function, value: ValueId) -> Option<u64> {
        Self::const_u256(func, value).and_then(Self::u256_to_u64)
    }

    fn u256_to_u64(value: U256) -> Option<u64> {
        value.try_into().ok()
    }

    // ----- CFG helpers -----

    fn can_insert_on_edge(func: &Function, pred: BlockId, target: BlockId) -> bool {
        matches!(func.blocks[pred].terminator, Some(Terminator::Jump(jump_target)) if jump_target == target)
    }

    fn operands_dominate_block(
        func: &Function,
        kind: &InstKind,
        block: BlockId,
        analysis: &Analysis,
    ) -> bool {
        kind.operands().into_iter().all(|value| match func.value(value) {
            Value::Immediate(_) | Value::Arg { .. } | Value::Undef(_) => true,
            Value::Inst(inst_id) => analysis
                .inst_blocks
                .get(inst_id)
                .is_some_and(|def_block| analysis.dominators.dominates(*def_block, block)),
        })
    }

    fn unique_predecessors(func: &Function, block: BlockId) -> Vec<BlockId> {
        let mut predecessors = Vec::new();
        for &pred in &func.blocks[block].predecessors {
            if !predecessors.contains(&pred) {
                predecessors.push(pred);
            }
        }
        predecessors
    }

    fn inst_results(func: &Function) -> FxHashMap<InstId, ValueId> {
        func.values
            .iter_enumerated()
            .filter_map(|(value_id, value)| {
                if let Value::Inst(inst_id) = value { Some((*inst_id, value_id)) } else { None }
            })
            .collect()
    }

    fn inst_blocks(func: &Function) -> FxHashMap<InstId, BlockId> {
        let mut inst_blocks = FxHashMap::default();
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                inst_blocks.insert(inst_id, block_id);
            }
        }
        inst_blocks
    }

    // ----- Rewriting -----

    fn replace_uses(func: &mut Function, from: ValueId, to: ValueId) {
        for inst in func.instructions.iter_mut() {
            let mut changed = false;
            inst.kind.visit_operands_mut(|value| {
                if *value == from {
                    *value = to;
                    changed = true;
                }
            });
            if !changed {
                continue;
            }
            // Operand-derived metadata is stale once the operand changes.
            if Self::is_memory_inst(&inst.kind) {
                inst.metadata.memory_region = None;
            }
            if matches!(
                inst.kind,
                InstKind::SLoad(_)
                    | InstKind::SStore(_, _)
                    | InstKind::TLoad(_)
                    | InstKind::TStore(_, _)
            ) {
                inst.metadata.storage_alias = None;
            }
        }

        for block in func.blocks.iter_mut() {
            if let Some(term) = &mut block.terminator {
                Self::replace_terminator_uses(term, from, to);
            }
        }
    }

    fn replace_terminator_uses(term: &mut Terminator, from: ValueId, to: ValueId) {
        let replace = |value: &mut ValueId| {
            if *value == from {
                *value = to;
            }
        };

        match term {
            Terminator::Jump(_) | Terminator::Stop | Terminator::Invalid => {}
            Terminator::Branch { condition, .. } => replace(condition),
            Terminator::Switch { value, cases, .. } => {
                replace(value);
                for (case, _) in cases {
                    replace(case);
                }
            }
            Terminator::Return { values } => {
                for value in values {
                    replace(value);
                }
            }
            Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                replace(offset);
                replace(size);
            }
            Terminator::SelfDestruct { recipient } => replace(recipient),
        }
    }

    fn is_memory_inst(kind: &InstKind) -> bool {
        matches!(
            kind,
            InstKind::MLoad(_)
                | InstKind::MStore(_, _)
                | InstKind::MStore8(_, _)
                | InstKind::MCopy(_, _, _)
                | InstKind::CalldataCopy(_, _, _)
                | InstKind::CodeCopy(_, _, _)
                | InstKind::ReturnDataCopy(_, _, _)
                | InstKind::ExtCodeCopy(_, _, _, _)
                | InstKind::Keccak256(_, _)
        )
    }
}

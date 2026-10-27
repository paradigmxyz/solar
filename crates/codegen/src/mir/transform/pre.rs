//! Partial redundancy elimination for pure expressions and memory-dependent reads.
//!
//! CSE reuses a value only along the dominator tree. This pass removes the join redundancies that
//! dominance cannot see: a join recomputes a value that is already available on some or all of its
//! incoming edges. One engine serves every key family, and a [`Pre`] pass selects the families it
//! tracks:
//! - `pre` tracks pure expressions. It runs early, so CSE and the later reads see the merged
//!   values.
//! - `load-pre` tracks storage, transient-storage, memory, and keccak reads. It runs after CSE has
//!   unified the dominated reads. Both families in one run, or pure expressions after CSE, left the
//!   corpus larger.
//!
//! Every rewrite replaces a join-block instruction with a phi over the per-predecessor values:
//!
//! 1. **Full redundancy**: a key available at the end of every predecessor (computed in both arms
//!    of a diamond, or live around a loop backedge) and recomputed at the join.
//! 2. **Store-to-load forwarding across joins**: a store in one arm makes the value known on that
//!    path, so the join reload merges stored and loaded values.
//! 3. **Partial redundancy**: a key available on some predecessors is inserted at the end of the
//!    remaining predecessor edges, then handled as a full redundancy. A jump-terminated predecessor
//!    receives the instruction directly; a branch- or switch-terminated one ends a critical edge,
//!    which is split first so the instruction runs only on the path to the join. Reads charge the
//!    transfer overhead before selecting the rewrite, and size mode leaves their critical edges
//!    unsplit until physical phi/transfer size is known. A pure expression needs one available
//!    predecessor per insertion, and at most [`MAX_PURE_INSERTIONS`] insertions.
//! 4. **Non-local full redundancy**: a read in a block reached from a join only through
//!    single-predecessor blocks, with no kill of its key on that chain or in the join, reads the
//!    value the join's entry holds. When that value is available on every predecessor, the read
//!    becomes the join's phi. A loop body reading a word that only the preheader writes, such as
//!    the length of an array the loop fills, then reads the preheader's stored value. This case
//!    never inserts reads.
//!
//! # Keys
//!
//! Read keys form one universe per function for the dataflow below. Pure keys need none:
//! - `Pure(expression)`: an instruction that computes a word from its operands alone, so it can
//!   move to any point where its operands are available. Immediate operands become placeholders,
//!   commutative operands share a key, and a swapped signed or unsigned comparison shares the key
//!   of its mirror predicate. Cheap casts that emit no code are not candidates. Nothing kills a
//!   pure key, so a def in a predecessor or any of its dominators is available at the predecessor's
//!   end, with no dataflow. The join's phi operands translate through each incoming edge before the
//!   lookup, so `add(phi(a, b), 1)` is available where `add(a, 1)` is.
//! - `(slot)`, a storage or transient-storage slot: genned by `sload`/`tload` (load result) and
//!   `sstore`/`tstore` (forwarded stored value); killed by may-aliasing stores to the same space
//!   and by calls and creates. `STATICCALL` cannot mutate storage or transient storage and does not
//!   kill them.
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
//! Standard available expressions over the finite universe of read keys:
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
//! For words at allocation bases, prefer an equivalent constant or a value already live at the
//! predecessor's exit. A fully redundant reload stays in place if forwarding would extend a
//! value's lifetime across the join solely to save this cheap load.
//!
//! # Safety of rewrites
//!
//! Pure expressions never read or write state, so moving them crosses nothing. The rest of this
//! section is about reads.
//!
//! A join load is never removed or moved after a gas observation on any incoming
//! CFG path, including loop backedges and observations in transitive callees.
//! A join load is a candidate only if no kill of its key precedes it in the join block.
//! For that scan, gas observations and calls block all spaces: a partial-redundancy
//! insertion moves the read to the end of a predecessor, so everything in the join block
//! above the original load executes after the moved read, and a `gas` read there would
//! observe the moved load's gas early. A call can also observe storage-access warming even when
//! it cannot change stored values. Removal-only (fully redundant) rewrites do not move reads, but
//! we keep the single conservative scan for these cases for simplicity.
//!
//! Before inserting a read, also reject a join prefix that may revert, terminate, or diverge:
//! the new read could trap or consume gas on a path that did not execute it. Full redundancy
//! can still reuse values computed on every incoming path, without inserting an access.
//!
//! An inserted load reads exactly the state the original would have read on that path: it
//! sits at the end of the predecessor or a new edge block (only a jump follows), and the join
//! prefix above the original load contains no kills of the key.
//!
//! # Termination
//!
//! Joins that are mutual predecessors can ping-pong a key between each other forever: each rewrite
//! is net-zero and re-creates a candidate in the other block. Three rules guarantee termination:
//! 1. Instructions inserted by this run are never picked as rewrite candidates, so every rewrite
//!    retires an instruction that existed when the run started, bounding rewrites by the initial
//!    instruction count.
//! 2. A `(key, block)` pair never gets an insertion after an elimination in that block, preventing
//!    ping-pong between mutually-preceding joins.
//! 3. A function-size-derived rewrite budget backstops the above.
//!
//! Edge splitting does not weaken these rules: split blocks have a single predecessor, so they are
//! never join targets, and the only instructions they hold are inserted this run and excluded by
//! rule 1.

use crate::{
    backend::evm::op,
    mir::{
        BlockId, EffectKind, Function, Immediate, InstId, InstKind, Instruction,
        InstructionMetadata, MirType, Module, PtrLocation, StorageAlias, Terminator, Value,
        ValueId,
        analysis::{
            Access, AddressSpace, AliasAnalysis, CfgInfo, DominatorTree, GasObservations, Liveness,
            Location, LocationSize, MemoryAddress, MemoryBase, MemoryCallSummaries, MemoryLocation,
            ModRef,
        },
        pass::{MirPass, run_function_pass_with_alias_and_cfg, run_function_pass_with_cfg},
        utils as mir_utils,
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use std::{cell::OnceCell, collections::BTreeMap, mem, rc::Rc, sync::Arc};

/// Function pass for PRE, over one family of keys.
pub(crate) enum Pre {
    /// Pure expressions, before the reads they depend on are unified.
    Pure,
    /// Storage, transient-storage, memory, and keccak reads.
    Reads,
}

impl MirPass for Pre {
    fn name(&self) -> &'static str {
        match self {
            Self::Pure => "pre",
            Self::Reads => "load-pre",
        }
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        match self {
            Self::Pure => run_function_pass_with_cfg(module, analyses, |func, analyses| {
                let target = analyses.target(gcx);
                Eliminator::new(&target, Some(Rc::clone(analyses.cfg())), None).run(func)
            }),
            Self::Reads => {
                let summaries = analyses.call_summaries(module);
                run_function_pass_with_alias_and_cfg(module, analyses, |func, analyses| {
                    let target = analyses.target(gcx);
                    let reads = ReadAnalyses {
                        alias: Rc::clone(analyses.alias()),
                        summaries: Arc::clone(&summaries),
                    };
                    Eliminator::new(&target, Some(Rc::clone(analyses.cfg())), Some(reads)).run(func)
                })
            }
        }
    }
}

/// Statistics for PRE.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PreStats {
    /// Number of join-block instructions replaced by phis or available values.
    eliminated: usize,
    /// Number of compensating instructions inserted into predecessors.
    inserted: usize,
}

impl PreStats {
    /// Returns the total number of MIR edits made by this pass.
    const fn total(self) -> usize {
        self.eliminated + self.inserted
    }
}

/// Redundancy eliminator for pure expressions and memory-dependent reads.
#[derive(Debug)]
struct Eliminator<'a> {
    /// The read analyses. A pass tracks reads when they are present and pure expressions when
    /// they are absent.
    reads: Option<ReadAnalyses>,
    liveness: OnceCell<Liveness>,
    gas: OnceCell<GasObservations>,
    /// Shared CFG snapshot for the availability dataflow.
    cfg: Option<Rc<CfgInfo>>,
    stats: PreStats,
    target: &'a Target,
}

/// State for the read keys: the alias analysis and the call summaries.
#[derive(Debug)]
struct ReadAnalyses {
    alias: Rc<AliasAnalysis>,
    /// Call summaries for the alias snapshot rebuilt after edge splitting.
    summaries: Arc<MemoryCallSummaries>,
}

/// A normalized key for a state-dependent read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum LoadKey {
    /// A storage or transient-storage slot.
    Slot(Location),
    /// A 32-byte memory read at a canonical address.
    Memory(MemoryAddress),
    Keccak(MemoryAddress, KeccakSize),
}

/// The most copies of a pure expression one rewrite inserts; this bounds code growth at joins with
/// many predecessors.
const MAX_PURE_INSERTIONS: usize = 2;

/// What a rewrite names: a state-dependent read, or a pure expression that nothing kills.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Key {
    Read(LoadKey),
    Pure(ExprKey),
}

/// A pure expression with immediate operands replaced by placeholders, the immediates in operand
/// order, and the result type, which casts take from the instruction rather than from their
/// operands.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ExprKey {
    kind: InstKind,
    immediates: SmallVec<[Immediate; 2]>,
    ty: MirType,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum OperandKey {
    Value(ValueId),
    Immediate(Immediate),
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
type KeySet = DenseBitSet<usize>;

/// A join-block rewrite: replace `insts` with a phi over `incoming`, after inserting the
/// instruction of each `insertions` entry at the end of its predecessor.
struct Candidate {
    target: BlockId,
    /// The block holding `insts`: `target` itself, or, for reads, a single-predecessor descendant
    /// of it.
    block: BlockId,
    key: Key,
    result_ty: MirType,
    metadata: InstructionMetadata,
    /// The instructions the rewrite removes, with their results. All compute `key`.
    insts: Vec<(InstId, ValueId)>,
    incoming: Vec<(BlockId, ValueId)>,
    insertions: Vec<(BlockId, InstKind)>,
}

/// A small static profitability model for read insertions.
///
/// The model intentionally works in approximate dynamic path cost rather than
/// MIR instruction count: a join-block reload executes on every predecessor
/// path, while a compensating load only executes on the paths where the value
/// was unavailable.
#[derive(Clone, Copy, Debug)]
struct PreCostModel<'a> {
    target: &'a Target,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PreCost {
    saved: i64,
    inserted: i64,
    phi: i64,
    operands: i64,
}

impl PreCost {
    const fn is_profitable(self) -> bool {
        self.saved > self.inserted + self.phi + self.operands
    }
}

struct PreCostInput<'a> {
    func: &'a Function,
    key: LoadKey,
    kind: &'a InstKind,
    predecessors: &'a [BlockId],
    loads: &'a [(InstId, ValueId)],
    insertions: &'a [BlockId],
    loop_carried: bool,
    needs_phi: bool,
}

impl PreCostModel<'_> {
    fn estimate(&self, input: PreCostInput<'_>) -> PreCost {
        let read = self.read_cost(input.key);
        let saved = read * input.loads.len() as i64 * input.predecessors.len() as i64;
        let splits =
            input.insertions.iter().filter(|&&pred| splits_edge(input.func, pred)).count() as i64;
        let transfer = self
            .target
            .opcode(op::PUSH2)
            .plus(self.target.opcode(op::JUMP))
            .plus(self.target.opcode(op::JUMPDEST));
        let inserted = read * input.insertions.len() as i64 + splits * i64::from(transfer.gas);
        let phi = if !input.needs_phi || input.loop_carried {
            0
        } else {
            // One `DUP` per edge.
            i64::from(self.target.dup().gas) * input.predecessors.len() as i64
        };
        let operands = self.inserted_operand_cost(input.func, input.kind, input.insertions);
        PreCost { saved, inserted, phi, operands }
    }

    fn read_cost(&self, key: LoadKey) -> i64 {
        match key {
            // A repeated storage read is warm, whatever the first access cost.
            LoadKey::Slot(location) => {
                self.target.repeated_read_gas(location.address_space()) as i64
            }
            LoadKey::Memory(_) => self.target.repeated_read_gas(AddressSpace::Memory) as i64,
            LoadKey::Keccak(_, size) => {
                // A dynamic size is priced as a two-word mapping key.
                let size = match size {
                    KeccakSize::Const(size) => size,
                    KeccakSize::Dyn(_) => 64,
                };
                let arguments = [None, Some(U256::from(size))];
                self.target.opcode_with_immediates(op::KECCAK256, &arguments).gas as i64
            }
        }
    }

    fn inserted_operand_cost(
        &self,
        func: &Function,
        kind: &InstKind,
        insertions: &[BlockId],
    ) -> i64 {
        if insertions.is_empty() {
            return 0;
        }
        let cross_block_operands = kind
            .operands()
            .into_iter()
            .filter(|&value| matches!(func.value(value), Value::Inst(_) | Value::Undef(_)))
            .count();
        // One `DUP` to reach each operand.
        i64::from(self.target.dup().gas) * cross_block_operands as i64 * insertions.len() as i64
    }
}

/// Returns whether inserting at the end of `pred` splits its edge to the join: only a
/// jump-terminated predecessor owns its single outgoing edge, so any other terminator makes the
/// edge critical.
fn splits_edge(func: &Function, pred: BlockId) -> bool {
    !matches!(func.blocks[pred].terminator, Some(Terminator::Jump(_)))
}

/// Per-round analysis shared by candidate collection.
struct Analysis {
    keys: Vec<LoadKey>,
    key_index: FxHashMap<LoadKey, usize>,
    kill_index: KillIndex,
    cfg: Rc<CfgInfo>,
    availability: Availability,
    inst_blocks: IndexVec<InstId, Option<BlockId>>,
}

/// Availability of the read keys, empty when no read is trackable.
#[derive(Default)]
struct Availability {
    /// Per-block keys killed at any point in the block; only blocks that kill
    /// something have an entry.
    kills: FxHashMap<BlockId, KeySet>,
    /// Availability at block entry, over all paths from the entry block.
    ins: FxHashMap<BlockId, KeySet>,
    /// Availability at block exit.
    outs: FxHashMap<BlockId, KeySet>,
}

#[derive(Default)]
struct KillIndex {
    /// The slot keys of each space in [`AddressSpace::SLOTS`].
    slots: [StorageKillIndex; AddressSpace::SLOTS.len()],
    memory: Vec<usize>,
    memory_words: FxHashMap<MemoryAddress, BTreeMap<u64, usize>>,
    keccak: Vec<usize>,
}

#[derive(Default)]
struct StorageKillIndex {
    /// Every key of the space.
    all: Vec<usize>,
    slots: FxHashMap<U256, usize>,
    symbolic: FxHashMap<ValueId, FxHashMap<U256, usize>>,
}

impl StorageKillIndex {
    fn insert(&mut self, alias: StorageAlias, idx: usize) {
        self.all.push(idx);
        match alias {
            StorageAlias::Slot(slot) => {
                self.slots.insert(slot, idx);
            }
            StorageAlias::Symbolic(base) => {
                self.symbolic.entry(base).or_default().insert(U256::ZERO, idx);
            }
            StorageAlias::Offset { base, offset } => {
                self.symbolic.entry(base).or_default().insert(offset, idx);
            }
        }
    }

    fn for_each_alias(&self, written: StorageAlias, mut kill: impl FnMut(usize)) {
        match written {
            StorageAlias::Slot(slot) => {
                if let Some(&idx) = self.slots.get(&slot) {
                    kill(idx);
                }
                for group in self.symbolic.values() {
                    for &idx in group.values() {
                        kill(idx);
                    }
                }
            }
            StorageAlias::Symbolic(base) => {
                self.for_each_symbolic_alias(base, U256::ZERO, &mut kill);
            }
            StorageAlias::Offset { base, offset } => {
                self.for_each_symbolic_alias(base, offset, &mut kill);
            }
        }
    }

    fn for_each_symbolic_alias(&self, base: ValueId, offset: U256, kill: &mut impl FnMut(usize)) {
        for &idx in self.slots.values() {
            kill(idx);
        }
        for (&key_base, group) in &self.symbolic {
            if key_base == base {
                if let Some(&idx) = group.get(&offset) {
                    kill(idx);
                }
            } else {
                for &idx in group.values() {
                    kill(idx);
                }
            }
        }
    }
}

impl KillIndex {
    fn new(keys: &[LoadKey]) -> Self {
        let mut this = Self::default();
        for (idx, &key) in keys.iter().enumerate() {
            match key {
                LoadKey::Slot(location) => {
                    let alias = location.slot_alias().expect("a slot key names a slot");
                    this.slots_mut(location.address_space()).insert(alias, idx);
                }
                LoadKey::Memory(mut address) => {
                    this.memory.push(idx);
                    let offset = address.offset;
                    address.offset = 0;
                    this.memory_words.entry(address).or_default().insert(offset, idx);
                }
                LoadKey::Keccak(_, _) => {
                    this.memory.push(idx);
                    this.keccak.push(idx);
                }
            }
        }
        this
    }

    fn space(&self, space: AddressSpace) -> &[usize] {
        match space {
            AddressSpace::Storage | AddressSpace::Transient => &self.slots(space).all,
            AddressSpace::Memory => &self.memory,
            AddressSpace::Immutable => &[],
        }
    }

    fn slots(&self, space: AddressSpace) -> &StorageKillIndex {
        &self.slots[space.slot_index().expect("a slot space")]
    }

    fn slots_mut(&mut self, space: AddressSpace) -> &mut StorageKillIndex {
        &mut self.slots[space.slot_index().expect("a slot space")]
    }
}

impl Analysis {
    /// Returns true if any block on a CFG path from `from` to `to` (excluding
    /// `from` itself, whose effects the caller scans directly) may kill `key`.
    fn path_kills_key(&self, from: BlockId, to: BlockId, key_idx: usize) -> bool {
        let block_kills = &self.availability.kills;
        if block_kills.is_empty() {
            return false;
        }
        let Some(reachable_from) = self.cfg.transitive_reachability(from) else { return true };
        for (&mid, kills) in block_kills {
            if mid == from || !kills.contains(key_idx) {
                continue;
            }
            if reachable_from.contains(mid)
                && self
                    .cfg
                    .transitive_reachability(mid)
                    .is_some_and(|reachable| reachable.contains(to))
            {
                return true;
            }
        }
        false
    }
}

/// The candidates of one collection round. A candidate whose analysis an earlier one would
/// invalidate waits for the next round.
struct Batch {
    candidates: Vec<Candidate>,
    /// Blocks an accepted candidate rewrites.
    modified_blocks: DenseBitSet<BlockId>,
    /// Values whose defining instruction an accepted candidate removes.
    eliminated_values: DenseBitSet<ValueId>,
}

impl Batch {
    fn new(func: &Function) -> Self {
        Self {
            candidates: Vec::new(),
            modified_blocks: DenseBitSet::new_empty(func.blocks.len()),
            eliminated_values: DenseBitSet::new_empty(func.num_values()),
        }
    }

    /// Accepts `candidate` unless an accepted one invalidates its analysis: it shares a block
    /// with it or references a value it removes.
    fn push(&mut self, candidate: Candidate) -> bool {
        if self.interferes(&candidate) {
            return false;
        }
        self.modified_blocks.insert(candidate.target);
        self.modified_blocks.insert(candidate.block);
        for &(block, _) in &candidate.insertions {
            self.modified_blocks.insert(block);
        }
        for &(_, value) in &candidate.insts {
            self.eliminated_values.insert(value);
        }
        self.candidates.push(candidate);
        true
    }

    fn interferes(&self, candidate: &Candidate) -> bool {
        self.modified_blocks.contains(candidate.target)
            || self.modified_blocks.contains(candidate.block)
            || candidate.insertions.iter().any(|&(block, _)| self.modified_blocks.contains(block))
            || candidate.incoming.iter().any(|&(_, value)| self.eliminated_values.contains(value))
            || candidate.insts.iter().any(|&(_, value)| self.eliminated_values.contains(value))
            || candidate.insertions.iter().any(|(_, kind)| {
                kind.operands().into_iter().any(|value| self.eliminated_values.contains(value))
            })
    }
}

/// Mutable state threaded through one candidate-collection round.
struct CandidateCx<'a> {
    analysis: &'a Analysis,
    eliminated_keys: &'a FxHashSet<(Key, BlockId)>,
    inserted_insts: &'a GrowableBitSet<InstId>,
    locate_cache: FxHashMap<(BlockId, usize), Option<ValueId>>,
}

/// Keys a forward scan from a join's entry has seen killed or loaded.
#[derive(Clone)]
struct LoadScan {
    /// Keys whose value may differ from the one at the join's entry.
    blocked: KeySet,
    /// Keys whose first load the scan has already found.
    taken: KeySet,
}

impl<'a> Eliminator<'a> {
    fn new(target: &'a Target, cfg: Option<Rc<CfgInfo>>, reads: Option<ReadAnalyses>) -> Self {
        Self {
            reads,
            liveness: OnceCell::new(),
            gas: OnceCell::new(),
            cfg,
            stats: PreStats::default(),
            target,
        }
    }

    /// Runs PRE to a fixed point under the rewrite budget. Returns whether it changed `func`.
    fn run(&mut self, func: &mut Function) -> bool {
        if self.reads.is_none() && !self.has_pure_candidate(func) {
            return false;
        }

        let rewrite_limit = func.num_insts().saturating_mul(2).max(64);
        let mut rewrites = 0usize;
        let mut eliminated_keys = FxHashSet::default();
        let mut inserted_insts = GrowableBitSet::with_capacity(func.num_insts());

        while rewrites < rewrite_limit {
            self.liveness.take();
            self.gas.take();
            let Some(analysis) = self.compute_analysis(func) else { break };
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
            let blocks_before = func.blocks.len();
            for candidate in batch {
                self.apply_candidate(func, candidate, &mut eliminated_keys, &mut inserted_insts);
            }
            let split = func.blocks.len() != blocks_before;
            if split {
                self.cfg = None;
            }
            if let Some(reads) = &mut self.reads {
                if split {
                    reads.alias = Rc::new(AliasAnalysis::with_call_summaries(
                        func,
                        Arc::clone(&reads.summaries),
                    ));
                } else {
                    reads.alias.clear_cached_addresses();
                }
            }
        }

        self.stats.total() != 0
    }

    fn alias(&self) -> &AliasAnalysis {
        &self.reads.as_ref().expect("read keys need alias analysis").alias
    }

    /// Returns the gas observations of this round's CFG.
    fn gas(&self, func: &Function, cfg: &CfgInfo) -> &GasObservations {
        self.gas.get_or_init(|| GasObservations::new(func, cfg, self.alias()))
    }

    /// Computes the read key universe and, when it is not empty, the availability fixpoint.
    /// Returns `None` if there is nothing to rewrite.
    fn compute_analysis(&self, func: &Function) -> Option<Analysis> {
        let cfg = self.cfg.as_ref().map_or_else(|| Rc::new(CfgInfo::new(func)), Rc::clone);
        let (keys, key_index) = self.read_keys(func, &cfg);
        if keys.is_empty() && self.reads.is_some() {
            return None;
        }
        let kill_index = KillIndex::new(&keys);
        let availability = if keys.is_empty() {
            Availability::default()
        } else {
            self.availability(func, &cfg, &keys, &key_index, &kill_index)
        };
        Some(Analysis {
            keys,
            key_index,
            kill_index,
            cfg,
            availability,
            inst_blocks: func.inst_block_table(),
        })
    }

    /// Returns the key universe: only keys a rewrite could ever name. A candidate is
    /// always a load in a block with at least two reachable predecessors
    /// (the join whose redundancy dominance cannot see) or in a block that
    /// such a join reaches through single-predecessor blocks alone, so a key
    /// never loaded in either can produce no candidate. Every key's
    /// gen/kill/availability is computed independently of the others, so
    /// narrowing the universe leaves the retained keys' dataflow unchanged
    /// while dropping the summaries that no rewrite could consult. This is
    /// what keeps the pass off straight-line code, where the key universe
    /// would otherwise scale with the whole function.
    fn read_keys(
        &self,
        func: &Function,
        cfg: &CfgInfo,
    ) -> (Vec<LoadKey>, FxHashMap<LoadKey, usize>) {
        let mut keys = Vec::new();
        let mut key_index: FxHashMap<LoadKey, usize> = FxHashMap::default();
        if self.reads.is_none() {
            return (keys, key_index);
        }
        for &block in cfg.rpo() {
            if Self::join_above(func, cfg, block).is_none() {
                continue;
            }
            for &inst_id in &func.blocks[block].instructions {
                if let Some((key, GenSource::LoadResult)) = self.gen_key_value(func, inst_id) {
                    key_index.entry(key).or_insert_with(|| {
                        keys.push(key);
                        keys.len() - 1
                    });
                }
            }
        }
        (keys, key_index)
    }

    /// Solves the availability of `keys` over the CFG.
    fn availability(
        &self,
        func: &Function,
        cfg: &CfgInfo,
        keys: &[LoadKey],
        key_index: &FxHashMap<LoadKey, usize>,
        kill_index: &KillIndex,
    ) -> Availability {
        let rpo = cfg.rpo();
        let key_count = keys.len();

        // Per-block summaries: GEN holds keys genned after the last kill, KILL
        // holds keys killed at any point.
        let mut gens = FxHashMap::default();
        let mut kills = FxHashMap::default();
        for &block in rpo {
            let mut gen_set = KeySet::new_empty(key_count);
            let mut kill_set = KeySet::new_empty(key_count);
            for &inst_id in &func.blocks[block].instructions {
                if func.inst(inst_id).kind.has_side_effects() {
                    let effects = self.alias().instruction_mod_ref(func, inst_id);
                    self.for_each_killed_key(&effects, keys, kill_index, |idx| {
                        kill_set.insert(idx);
                        gen_set.remove(idx);
                    });
                }
                // A store both kills aliases and gens its exact key; the gen
                // wins for the exact key because the slot then holds the
                // stored value.
                if let Some((key, _)) = self.gen_key_value(func, inst_id)
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
                let out = if func.blocks[block].predecessors.is_empty() {
                    gens[&block].clone()
                } else {
                    KeySet::new_filled(key_count)
                };
                (block, out)
            })
            .collect();
        loop {
            let mut changed = false;
            for &block in rpo {
                let mut acc: Option<KeySet> = None;
                for &pred in &func.blocks[block].predecessors {
                    // Unreachable predecessors never execute and cannot
                    // contribute a path.
                    let Some(out) = outs.get(&pred) else { continue };
                    match &mut acc {
                        Some(acc) => {
                            acc.intersect(out);
                        }
                        None => acc = Some(out.clone()),
                    }
                }
                let in_set = acc.unwrap_or_else(|| KeySet::new_empty(key_count));
                let mut out = in_set.clone();
                if let Some(kill) = kills.get(&block) {
                    out.subtract(kill);
                }
                out.union(&gens[&block]);
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

        Availability { kills, ins, outs }
    }

    /// Collects non-interfering candidates from one analysis snapshot so they
    /// can be applied as a batch.
    fn collect_candidates(
        &self,
        func: &Function,
        cx: &mut CandidateCx<'_>,
        limit: usize,
    ) -> Vec<Candidate> {
        let mut batch = Batch::new(func);
        'targets: for target in func.blocks.indices() {
            let predecessors = &func.blocks[target].predecessors;
            if !cx.analysis.cfg.is_reachable(target) || predecessors.len() < 2 {
                continue;
            }

            if self.reads.is_none() {
                for &inst in &func.blocks[target].instructions {
                    if batch.candidates.len() >= limit {
                        break 'targets;
                    }
                    // Termination rule 1: never rewrite an instruction this run inserted.
                    if cx.inserted_insts.contains(inst) {
                        continue;
                    }
                    let Some(candidate) = self.pure_candidate(func, cx, target, inst) else {
                        continue;
                    };
                    if batch.push(candidate) {
                        continue 'targets;
                    }
                }
                continue;
            }

            if predecessors.iter().any(|&pred| !cx.analysis.cfg.is_reachable(pred))
                || self.gas(func, &cx.analysis.cfg).at_entry(target)
            {
                continue;
            }

            let loads = self
                .first_loads(func, cx.analysis, target)
                .into_iter()
                .map(|(inst, key_idx)| (target, inst, key_idx))
                .chain(self.descendant_loads(func, cx.analysis, target));
            for (load_block, inst, key_idx) in loads {
                if batch.candidates.len() >= limit {
                    break 'targets;
                }
                // Termination rule 1: never rewrite a load this run inserted.
                if cx.inserted_insts.contains(inst) {
                    continue;
                }
                let Some(candidate) =
                    self.candidate_for_load(func, cx, target, load_block, inst, key_idx)
                else {
                    continue;
                };
                let splits = candidate.insertions.iter().any(|&(pred, _)| splits_edge(func, pred));
                if batch.push(candidate) {
                    // Rebuild CFG and availability before collecting another
                    // candidate against predecessor lists changed by edge splitting.
                    if splits {
                        break 'targets;
                    }
                    continue 'targets;
                }
            }
        }

        batch.candidates
    }

    /// Returns, in program order, the first load of each key in `target` that
    /// no kill of that key precedes.
    ///
    /// Gas observations and calls end or restrict the scan: a
    /// partial-redundancy insertion moves the read to a predecessor's end, so
    /// it must not cross a gas observation or call (any space) in the join prefix.
    fn first_loads(
        &self,
        func: &Function,
        analysis: &Analysis,
        target: BlockId,
    ) -> Vec<(InstId, usize)> {
        let key_count = analysis.keys.len();
        let mut scan =
            LoadScan { blocked: KeySet::new_empty(key_count), taken: KeySet::new_empty(key_count) };
        let mut found = Vec::new();
        self.scan_first_loads(func, analysis, target, &mut scan, &mut found);
        found
    }

    /// Returns the first load of each key in the blocks below `target` that `target` reaches
    /// through single-predecessor blocks alone, as `(block, load, key)`, when no kill of that key
    /// precedes it on the way from `target`'s entry and `target` holds no load of it.
    ///
    /// Such a load reads the value the key has at `target`'s entry, so it is fully redundant
    /// when that value is available on every predecessor. The walk applies the scan of
    /// [`Self::first_loads`] to every block: it crosses no gas observation or call, and it visits
    /// at most [`Self::MAX_DESCENDANT_BLOCKS`] blocks.
    fn descendant_loads(
        &self,
        func: &Function,
        analysis: &Analysis,
        target: BlockId,
    ) -> Vec<(BlockId, InstId, usize)> {
        let key_count = analysis.keys.len();
        let mut scan =
            LoadScan { blocked: KeySet::new_empty(key_count), taken: KeySet::new_empty(key_count) };
        let mut found = Vec::new();
        if !self.scan_first_loads(func, analysis, target, &mut scan, &mut Vec::new()) {
            return Vec::new();
        }
        let mut stack = vec![(target, scan)];
        let mut visited = 0;
        while let Some((block, scan)) = stack.pop() {
            let Some(terminator) = &func.blocks[block].terminator else { continue };
            for successor in terminator.successors() {
                if visited >= Self::MAX_DESCENDANT_BLOCKS {
                    return found;
                }
                if successor == target
                    || func.blocks[successor].predecessors.as_slice() != [block]
                    || self.gas(func, &analysis.cfg).at_entry(successor)
                {
                    continue;
                }
                visited += 1;
                let mut scan = scan.clone();
                let mut loads = Vec::new();
                let transparent =
                    self.scan_first_loads(func, analysis, successor, &mut scan, &mut loads);
                found.extend(loads.into_iter().map(|(inst, key)| (successor, inst, key)));
                if transparent {
                    stack.push((successor, scan));
                }
            }
        }
        found
    }

    /// The most single-predecessor blocks [`Self::descendant_loads`] scans below one join.
    const MAX_DESCENDANT_BLOCKS: usize = 16;

    /// Returns the join whose candidates may include the loads in `block`: `block` itself when
    /// it has two or more predecessors, all reachable, or the first such block above a chain of
    /// at most [`Self::MAX_DESCENDANT_BLOCKS`] single-predecessor blocks ending at `block`.
    fn join_above(func: &Function, cfg: &CfgInfo, block: BlockId) -> Option<BlockId> {
        let mut current = block;
        for _ in 0..=Self::MAX_DESCENDANT_BLOCKS {
            let predecessors = &func.blocks[current].predecessors;
            match predecessors.as_slice() {
                [] => return None,
                &[pred] if pred != block => current = pred,
                [_] => return None,
                _ if predecessors.iter().all(|&pred| cfg.is_reachable(pred)) => {
                    return Some(current);
                }
                _ => return None,
            }
        }
        None
    }

    /// Appends to `found` each load in `block` of a key that `scan` neither blocks nor has taken,
    /// updating `scan` with the block's kills. Returns whether the whole block was scanned,
    /// rather than stopping at a gas observation or a call.
    fn scan_first_loads(
        &self,
        func: &Function,
        analysis: &Analysis,
        block: BlockId,
        scan: &mut LoadScan,
        found: &mut Vec<(InstId, usize)>,
    ) -> bool {
        let LoadScan { blocked, taken } = scan;
        for &inst_id in &func.blocks[block].instructions {
            if self.gas(func, &analysis.cfg).observes(inst_id) {
                return false;
            }
            if let Some((key, GenSource::LoadResult)) = self.gen_key_value(func, inst_id) {
                if let Some(&idx) = analysis.key_index.get(&key)
                    && !blocked.contains(idx)
                    && taken.insert(idx)
                {
                    found.push((inst_id, idx));
                }
                continue;
            }
            let kind = &func.inst(inst_id).kind;
            match kind {
                // Gas observations and calls block motion in every space.
                _ if Self::blocks_motion(kind) => return false,
                _ if kind.has_side_effects() => {
                    // Kills block their keys; a store's own-key gen is also a
                    // kill here (the value differs from the predecessor-end
                    // state), which the may-alias check already covers.
                    let effects = self.alias().instruction_mod_ref(func, inst_id);
                    self.for_each_killed_key(
                        &effects,
                        &analysis.keys,
                        &analysis.kill_index,
                        |idx| {
                            blocked.insert(idx);
                        },
                    );
                }
                _ => {}
            }
        }
        true
    }

    fn blocks_motion(kind: &InstKind) -> bool {
        kind.observes_gas()
            || matches!(
                kind.effect_kind(),
                EffectKind::ExternalCall | EffectKind::ICall | EffectKind::Create
            )
    }

    fn same_key_loads_in_target(
        &self,
        func: &Function,
        target: BlockId,
        first_inst: InstId,
        key: LoadKey,
    ) -> Vec<(InstId, ValueId)> {
        let mut loads = Vec::new();
        let mut past_first = false;

        for &inst_id in &func.blocks[target].instructions {
            if inst_id == first_inst {
                past_first = true;
            }
            if !past_first {
                continue;
            }

            if inst_id != first_inst {
                let kind = &func.inst(inst_id).kind;
                if Self::blocks_motion(kind) {
                    break;
                }
                if kind.has_side_effects() && self.inst_kills_key(func, inst_id, key) {
                    break;
                }
            }

            if let Some((load_key, GenSource::LoadResult)) = self.gen_key_value(func, inst_id)
                && load_key == key
                && let Some(value) = func.inst_result_value(inst_id)
            {
                loads.push((inst_id, value));
            }
        }

        loads
    }

    fn candidate_for_load(
        &self,
        func: &Function,
        cx: &mut CandidateCx<'_>,
        target: BlockId,
        load_block: BlockId,
        inst: InstId,
        key_idx: usize,
    ) -> Option<Candidate> {
        let predecessors = func.blocks[target].predecessors.as_slice();
        let instruction = func.inst(inst);
        let result = func.inst_result_value(inst)?;
        let result_ty = instruction.result_ty?;
        let key = cx.analysis.keys[key_idx];
        let loads = self.same_key_loads_in_target(func, load_block, inst, key);
        if loads.is_empty() {
            return None;
        }

        let mut incoming = Vec::with_capacity(predecessors.len());
        let mut insertions = Vec::new();
        for &pred in predecessors {
            if cx.analysis.availability.outs.get(&pred).is_some_and(|out| out.contains(key_idx))
                && let Some(value) = self.locate_value(func, cx, pred, key_idx)
            {
                incoming.push((pred, value));
                continue;
            }
            // A load below the join is only removed, never moved to the join's predecessors.
            if load_block != target {
                return None;
            }
            // The key is unavailable here; insert only on an explicit CFG edge.
            if !Self::can_insert_on_edge(func, pred, target) {
                return None;
            }
            if self.target.optimization().is_size() && splits_edge(func, pred) {
                return None;
            }
            // Termination rule 2: never insert a key into a block it was
            // previously eliminated from in this run.
            if cx.eliminated_keys.contains(&(Key::Read(key), pred)) {
                return None;
            }
            if !Self::operands_dominate_block(func, &instruction.kind, pred, cx.analysis) {
                return None;
            }
            insertions.push(pred);
        }

        let loop_carried = Self::is_loop_carried_rewrite(
            cx.analysis.cfg.dominators(),
            target,
            &incoming,
            &insertions,
        );
        let needs_phi = !insertions.is_empty()
            || incoming.first().is_none_or(|&(_, first)| {
                first == result || incoming.iter().any(|&(_, value)| value != first)
            });
        if !needs_phi
            && matches!(key, LoadKey::Memory(address)
                if address.is_allocation_base())
            && incoming.first().is_some_and(|&(_, value)| {
                !matches!(func.value(value), Value::Immediate(_))
                    && !self
                        .liveness
                        .get_or_init(|| Liveness::compute_live_sets(func))
                        .live_in(target)
                        .contains(value)
            })
        {
            return None;
        }
        let model = PreCostModel { target: self.target };
        let cost = model.estimate(PreCostInput {
            func,
            key,
            kind: &instruction.kind,
            predecessors,
            loads: &loads,
            insertions: &insertions,
            loop_carried,
            needs_phi,
        });
        if !cost.is_profitable() {
            return None;
        }

        if !insertions.is_empty() {
            if func.blocks[target]
                .instructions
                .iter()
                .take_while(|&&prefix| prefix != inst)
                .any(|&prefix| func.inst(prefix).kind.effects().control.any())
            {
                return None;
            }
            // Insertions must be structurally safe; profitability is decided
            // by `PreCostModel` above.
            let loop_insertion = incoming
                .iter()
                .all(|&(pred, _)| cx.analysis.cfg.dominators().dominates(target, pred))
                && insertions
                    .iter()
                    .all(|&pred| !cx.analysis.cfg.dominators().dominates(target, pred));
            let diamond_insertion = Self::is_non_cyclic_diamond_insertion(
                cx.analysis.cfg.dominators(),
                target,
                &incoming,
                &insertions,
            );
            if !(loop_insertion || diamond_insertion) || insertions.len() > incoming.len() {
                return None;
            }
        }

        Some(Candidate {
            target,
            block: load_block,
            key: Key::Read(key),
            result_ty,
            metadata: instruction.metadata.clone(),
            insts: loads,
            incoming,
            insertions: insertions
                .into_iter()
                .map(|pred| (pred, instruction.kind.clone()))
                .collect(),
        })
    }

    fn is_loop_carried_rewrite(
        dominators: &DominatorTree,
        target: BlockId,
        incoming: &[(BlockId, ValueId)],
        insertions: &[BlockId],
    ) -> bool {
        let has_backedge_incoming =
            incoming.iter().any(|&(pred, _)| dominators.dominates(target, pred));
        let has_entry_edge = incoming.iter().any(|&(pred, _)| !dominators.dominates(target, pred))
            || !insertions.is_empty();
        has_backedge_incoming
            && has_entry_edge
            && insertions.iter().all(|&pred| !dominators.dominates(target, pred))
    }

    fn is_non_cyclic_diamond_insertion(
        dominators: &DominatorTree,
        target: BlockId,
        incoming: &[(BlockId, ValueId)],
        insertions: &[BlockId],
    ) -> bool {
        !incoming.is_empty()
            && !insertions.is_empty()
            && incoming.iter().all(|&(pred, _)| !dominators.dominates(target, pred))
            && insertions.iter().all(|&pred| !dominators.dominates(target, pred))
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
        let prefer_live = matches!(key, LoadKey::Memory(address)
            if address.is_allocation_base());
        let mut found = None;
        for &inst_id in func.blocks[block].instructions.iter().rev() {
            if let Some((gen_key, source)) = self.gen_key_value(func, inst_id)
                && gen_key == key
            {
                // A store's exact-key gen wins over its own kill: the slot
                // holds the stored value from this point on.
                let value = match source {
                    GenSource::LoadResult => func.inst_result_value(inst_id)?,
                    GenSource::Stored(value) => value,
                };
                if !prefer_live
                    || matches!(func.value(value), Value::Immediate(_))
                    || self
                        .liveness
                        .get_or_init(|| Liveness::compute_live_sets(func))
                        .live_out(block)
                        .contains(value)
                {
                    return Some(value);
                }
                found.get_or_insert(value);
            }
            if self.inst_kills_key(func, inst_id, key) {
                return found;
            }
        }
        if found.is_some() {
            return found;
        }

        // The block is transparent for the key: the value at its end is the
        // value at its entry, which the dataflow must prove available on all
        // paths before the dominator walk may locate it.
        if !cx.analysis.availability.ins.get(&block).is_some_and(|in_set| in_set.contains(key_idx))
        {
            return None;
        }
        let idom = cx.analysis.cfg.dominators().idom(block)?;
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
        eliminated_keys: &mut FxHashSet<(Key, BlockId)>,
        inserted_insts: &mut GrowableBitSet<InstId>,
    ) {
        let Candidate { target, block, key, result_ty, metadata, insts, mut incoming, insertions } =
            candidate;

        eliminated_keys.insert((key, target));

        let fully_available = insertions.is_empty();
        for (pred, kind) in insertions {
            // A critical edge is split so the computation runs only on the edge into the join.
            // The split block sits on that edge, so the per-edge phi translation that held for
            // `pred` holds for it too.
            //
            // pred -> edge: inst; jump target
            // target: phi [..., edge: inserted_value, ...]
            let edge = if splits_edge(func, pred) {
                mir_utils::split_edge(func, pred, target)
            } else {
                pred
            };
            let mut instruction = Instruction::new(kind, Some(result_ty));
            instruction.metadata = metadata.clone();
            let (new_inst, value) = func.alloc_value_inst(instruction);
            func.blocks[edge].instructions.push(new_inst);
            incoming.push((edge, value));
            inserted_insts.insert(new_inst);
            self.stats.inserted += 1;
        }
        incoming.sort_by_key(|(block, _)| block.index());

        // A fully-available key whose predecessors all locate the same value
        // needs no phi: that value's def dominates every predecessor and
        // therefore the join itself.
        let first_result = insts[0].1;
        let replacement = match incoming.first() {
            Some(&(_, first))
                if fully_available
                    && first != first_result
                    && incoming.iter().all(|&(_, value)| value == first) =>
            {
                first
            }
            _ => {
                let (phi_inst, phi_value) = func.alloc_value_inst(
                    Instruction::new(InstKind::Phi(incoming), Some(result_ty))
                        .with_debug_info_dropped(),
                );
                let phi_count = func.blocks[target]
                    .instructions
                    .iter()
                    .take_while(|&&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
                    .count();
                func.blocks[target].instructions.insert(phi_count, phi_inst);
                phi_value
            }
        };

        func.replace_uses(&insts.iter().map(|&(_, result)| (result, replacement)).collect());
        let removed = insts.iter().map(|&(inst_id, _)| inst_id).collect::<FxHashSet<_>>();
        func.blocks[block].instructions.retain(|inst_id| !removed.contains(inst_id));
        self.stats.eliminated += insts.len();
    }

    // ----- Keys, gens, and kills -----

    /// Returns the key an instruction gens and where its value comes from.
    fn gen_key_value(&self, func: &Function, inst_id: InstId) -> Option<(LoadKey, GenSource)> {
        let aa = self.alias();
        match func.inst(inst_id).kind {
            InstKind::Load(slot, location @ (PtrLocation::Storage | PtrLocation::Transient)) => {
                Location::slot(location, aa.storage_alias(func, inst_id, slot))
                    .map(|slot| (LoadKey::Slot(slot), GenSource::LoadResult))
            }
            InstKind::Store(
                slot,
                value,
                location @ (PtrLocation::Storage | PtrLocation::Transient),
            ) => Location::slot(location, aa.storage_alias(func, inst_id, slot))
                .map(|slot| (LoadKey::Slot(slot), GenSource::Stored(value))),
            // Stored words are integers, so a pointer load keeps its own type.
            InstKind::Load(addr, PtrLocation::Memory)
                if func.inst(inst_id).result_ty == Some(MirType::I256) =>
            {
                self.mem_addr(func, inst_id, addr, LocationSize::Const(32))
                    .map(|addr| (LoadKey::Memory(addr), GenSource::LoadResult))
            }
            InstKind::Fmp => Some((LoadKey::Memory(Self::fmp_addr()), GenSource::LoadResult)),
            InstKind::Store(addr, value, PtrLocation::Memory) => self
                .mem_addr(func, inst_id, addr, LocationSize::Const(32))
                .map(|addr| (LoadKey::Memory(addr), GenSource::Stored(value))),
            InstKind::Keccak256(offset, size) => {
                let addr = self.mem_addr(func, inst_id, offset, aa.location_size(func, size))?;
                let size = match func.value_u64(size) {
                    Some(size) => KeccakSize::Const(size),
                    None => KeccakSize::Dyn(size),
                };
                Some((LoadKey::Keccak(addr, size), GenSource::LoadResult))
            }
            _ => None,
        }
    }

    /// Returns true if an instruction may invalidate the value of `key`.
    #[must_use]
    fn inst_kills_key(&self, func: &Function, inst_id: InstId, key: LoadKey) -> bool {
        let effects = self.alias().instruction_mod_ref(func, inst_id);
        self.effects_kill_key(&effects, key)
    }

    #[must_use]
    fn effects_kill_key(&self, effects: &ModRef, key: LoadKey) -> bool {
        if effects.observes_gas() {
            return true;
        }
        let aa = self.alias();
        match key {
            LoadKey::Slot(location) => effects.may_write(aa, location),
            LoadKey::Memory(address) => effects.may_write(
                aa,
                Location::Memory(MemoryLocation::new(address, LocationSize::Const(32))),
            ),
            LoadKey::Keccak(address, size) => {
                let size = match size {
                    KeccakSize::Const(size) => LocationSize::Const(size),
                    KeccakSize::Dyn(size) => LocationSize::Dynamic(size),
                };
                effects.may_write(aa, Location::Memory(MemoryLocation::new(address, size)))
            }
        }
    }

    fn for_each_killed_key(
        &self,
        effects: &ModRef,
        keys: &[LoadKey],
        index: &KillIndex,
        mut kill: impl FnMut(usize),
    ) {
        for &write in effects.writes() {
            match write {
                Access::Any(space) => {
                    for &idx in index.space(space) {
                        kill(idx);
                    }
                }
                Access::Location(
                    location @ (Location::Storage(written) | Location::Transient(written)),
                ) => {
                    index.slots(location.address_space()).for_each_alias(written, &mut kill);
                }
                Access::Location(Location::Memory(written)) => {
                    self.for_each_killed_memory_key(written, keys, index, &mut kill);
                }
                Access::Location(Location::Immutable(_)) => {}
            }
        }
    }

    fn for_each_killed_memory_key(
        &self,
        written: MemoryLocation,
        keys: &[LoadKey],
        index: &KillIndex,
        kill: &mut impl FnMut(usize),
    ) {
        if matches!(written.size, LocationSize::Const(0)) {
            return;
        }
        for (&group, words) in &index.memory_words {
            // An access beyond the extent of a unique allocation starts at its offset in the
            // allocation's own words, so the two names share one offset space.
            let mut written_group = written.address;
            if let (MemoryBase::Allocation(site), MemoryBase::FreeMemory(fresh))
            | (MemoryBase::FreeMemory(site), MemoryBase::Allocation(fresh)) =
                (group.base, written.address.base)
                && site == fresh
            {
                written_group.base = group.base;
            }
            let written = MemoryLocation::new(written_group, written.size);
            written_group.offset = 0;
            if group == written_group {
                // Alias analysis conservatively treats a word whose end
                // overflows `u64` as overlapping every access on this base.
                for &idx in words.range((u64::MAX - 31)..).map(|(_, idx)| idx) {
                    kill(idx);
                }
                let start = written.address.offset.saturating_sub(31);
                if let LocationSize::Const(size) = written.size
                    && let Some(end) = written.address.offset.checked_add(size)
                {
                    for &idx in words.range(start..end).map(|(_, idx)| idx) {
                        kill(idx);
                    }
                    continue;
                }
                // A write of unknown extent begins at its offset and only
                // extends forward; words that end before it survive.
                for &idx in words.range(start..).map(|(_, idx)| idx) {
                    kill(idx);
                }
                continue;
            }
            if group.base == written.address.base {
                for &idx in words.values() {
                    kill(idx);
                }
                continue;
            }
            let probe = MemoryLocation::new(group, LocationSize::Const(1));
            if AliasAnalysis::memory_alias_locations(written, probe).may_alias() {
                for &idx in words.values() {
                    kill(idx);
                }
            }
        }
        for &idx in &index.keccak {
            let key = match keys[idx] {
                LoadKey::Keccak(address, KeccakSize::Const(size)) => {
                    MemoryLocation::new(address, LocationSize::Const(size))
                }
                LoadKey::Keccak(address, KeccakSize::Dyn(size)) => {
                    MemoryLocation::new(address, LocationSize::Dynamic(size))
                }
                _ => unreachable!(),
            };
            if AliasAnalysis::memory_alias_locations(written, key).may_alias() {
                kill(idx);
            }
        }
    }

    fn mem_addr(
        &self,
        func: &Function,
        inst_id: InstId,
        addr: ValueId,
        size: LocationSize,
    ) -> Option<MemoryAddress> {
        self.alias().memory_location(func, inst_id, addr, size).map(|location| location.address)
    }

    fn fmp_addr() -> MemoryAddress {
        AliasAnalysis::fmp_location().address
    }

    // ----- CFG helpers -----

    fn can_insert_on_edge(func: &Function, pred: BlockId, target: BlockId) -> bool {
        func.blocks[pred].terminator.as_ref().is_some_and(|term| term.has_successor(target))
    }

    fn operands_dominate_block(
        func: &Function,
        kind: &InstKind,
        block: BlockId,
        analysis: &Analysis,
    ) -> bool {
        kind.operands().into_iter().all(|value| match func.value(value) {
            Value::Immediate(_) | Value::Arg(_) | Value::Undef(_) | Value::Error(_) => true,
            Value::Inst(inst_id) => analysis
                .inst_blocks
                .get(*inst_id)
                .copied()
                .flatten()
                .is_some_and(|def_block| analysis.cfg.dominators().dominates(def_block, block)),
        })
    }

    // ----- Pure expressions -----

    /// Returns whether a join block holds a pure expression worth moving.
    fn has_pure_candidate(&self, func: &Function) -> bool {
        func.blocks.iter().any(|block| {
            block.predecessors.len() >= 2
                && block.instructions.iter().any(|&inst| {
                    self.is_pure_candidate(func.inst(inst))
                        && func.inst_result_value(inst).is_some()
                })
        })
    }

    /// Returns the rewrite that replaces the pure expression `inst` in the join `target` with a
    /// phi: the expression is already available at the end of at least as many predecessors as
    /// need a copy.
    fn pure_candidate(
        &self,
        func: &Function,
        cx: &CandidateCx<'_>,
        target: BlockId,
        inst: InstId,
    ) -> Option<Candidate> {
        let instruction = func.inst(inst);
        if !self.is_pure_candidate(instruction) {
            return None;
        }
        let result_ty = instruction.result_ty?;
        let result = func.inst_result_value(inst)?;
        let candidate_key = Self::expr_key(func, &instruction.kind, result_ty)?;
        let analysis = cx.analysis;
        let predecessors = &func.blocks[target].predecessors;
        let mut incoming = Vec::with_capacity(predecessors.len());
        let mut insertions = Vec::new();
        let mut available = 0usize;

        for &pred in predecessors {
            let translated = Self::translate_for_predecessor(
                func,
                &instruction.kind,
                target,
                pred,
                &analysis.inst_blocks,
            )?;
            if !Self::operands_dominate_block(func, &translated, pred, analysis) {
                return None;
            }
            let key = Self::expr_key(func, &translated, result_ty)?;
            if let Some(value) =
                Self::available_value_at_end(func, analysis.cfg.dominators(), pred, &key)
            {
                available += 1;
                incoming.push((pred, value));
                continue;
            }

            // Termination rule 2: never insert an expression into a block it
            // was previously eliminated from, which would ping-pong it between
            // mutually-preceding join blocks.
            if cx.eliminated_keys.contains(&(Key::Pure(key), pred)) {
                return None;
            }
            insertions.push((pred, translated));
        }

        // Every insertion must be paid for by a predecessor where the
        // expression is already available, so no path computes it more often
        // than before; paths through available predecessors compute it
        // strictly less often. The constant bounds code growth at joins with
        // many predecessors.
        if insertions.len() > available || insertions.len() > MAX_PURE_INSERTIONS {
            return None;
        }

        Some(Candidate {
            target,
            block: target,
            key: Key::Pure(candidate_key),
            result_ty,
            metadata: instruction.metadata.clone(),
            insts: vec![(inst, result)],
            incoming,
            insertions,
        })
    }

    /// Returns `kind` with the operands that are phis of `target` replaced by their value on the
    /// edge from `pred`.
    fn translate_for_predecessor(
        func: &Function,
        kind: &InstKind,
        target: BlockId,
        pred: BlockId,
        inst_blocks: &IndexVec<InstId, Option<BlockId>>,
    ) -> Option<InstKind> {
        let mut translated = kind.clone();
        let mut ok = true;
        translated.visit_operands_mut(|value| {
            if let Value::Inst(inst) = func.value(*value)
                && inst_blocks.get(*inst).copied().flatten() == Some(target)
                && let InstKind::Phi(incoming) = &func.inst(*inst).kind
            {
                match incoming.iter().find(|&&(incoming_pred, _)| incoming_pred == pred) {
                    Some(&(_, incoming_value)) => *value = incoming_value,
                    None => ok = false,
                }
            }
        });
        ok.then_some(translated)
    }

    /// Finds the expression in `block` or any of its dominators; a def in a dominator is
    /// available at `block`'s end with no further checks.
    fn available_value_at_end(
        func: &Function,
        dominators: &DominatorTree,
        block: BlockId,
        key: &ExprKey,
    ) -> Option<ValueId> {
        dominators
            .self_and_dominators(block)
            .into_iter()
            .find_map(|block| Self::available_value_in_block(func, block, key))
    }

    fn available_value_in_block(func: &Function, block: BlockId, key: &ExprKey) -> Option<ValueId> {
        func.blocks[block].instructions.iter().rev().find_map(|&inst| {
            let instruction = func.inst(inst);
            let same_op = match (&instruction.kind, &key.kind) {
                (InstKind::Gt(..), InstKind::Lt(..)) | (InstKind::SGt(..), InstKind::SLt(..)) => {
                    true
                }
                (kind, key_kind) => mem::discriminant(kind) == mem::discriminant(key_kind),
            };
            if !same_op
                || instruction.result_ty != Some(key.ty)
                || !Self::is_pure_expression(instruction)
            {
                return None;
            }
            let candidate_key = Self::expr_key(func, &instruction.kind, instruction.result_ty?)?;
            (candidate_key == *key).then(|| func.inst_result_value(inst)).flatten()
        })
    }

    /// Returns whether moving `instruction` can save work: a pure expression that emits code,
    /// since replacing a free cast with a phi only adds stack traffic.
    fn is_pure_candidate(&self, instruction: &Instruction) -> bool {
        Self::is_pure_expression(instruction)
            && self.target.op(&instruction.kind.op(), |_| None) != Cost::ZERO
    }

    /// Returns whether `instruction` computes a word from its operands alone, so it can move to
    /// any point where its operands are available.
    fn is_pure_expression(instruction: &Instruction) -> bool {
        let kind = &instruction.kind;
        !matches!(kind, InstKind::Phi(_))
            && kind.effect_kind() == EffectKind::Pure
            && kind.effects().can_speculate()
            && kind.any_operand(|_| true)
            && instruction.result_ty.is_some_and(MirType::is_word)
    }

    /// Returns the key of a pure expression: commutative operands in canonical order, and a
    /// greater-than comparison as the swapped less-than.
    fn expr_key(func: &Function, kind: &InstKind, ty: MirType) -> Option<ExprKey> {
        let operand = |value| match func.value(value) {
            Value::Immediate(imm) => OperandKey::Immediate(imm.clone()),
            _ => OperandKey::Value(value),
        };
        let mut kind = match *kind {
            InstKind::Gt(a, b) => InstKind::Lt(b, a),
            InstKind::SGt(a, b) => InstKind::SLt(b, a),
            _ => kind.op().canonicalize_commutative_by_key(operand).into_kind()?,
        };
        let mut immediates = SmallVec::new();
        kind.visit_operands_mut(|value| {
            if let Value::Immediate(imm) = func.value(*value) {
                immediates.push(imm.clone());
                *value = ValueId::MAX;
            }
        });
        Some(ExprKey { kind, immediates, ty })
    }
}

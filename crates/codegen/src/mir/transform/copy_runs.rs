//! Replaces a run of contiguous word copies with one bulk copy.
//!
//! ABI encoding and decoding, and struct copies, move several adjacent words one at a
//! time: `mstore(d + 32 i, calldataload(s + 32 i))` for `i = 0..n`, or the same with `mload`.
//! After word lowering and address simplification, the run is a list of load/store pairs
//! whose addresses share one base each and step by 32:
//!
//! ```text
//! mstore d, calldataload s;  mstore d + 32, calldataload s + 32  ==>  calldatacopy d, s, 64
//! mstore d, mload s;         mstore d + 32, mload s + 32         ==>  mcopy d, s, 64
//! ```
//!
//! The pass sits in the physical pipeline after `byte-run-loads`, once the egraph has put
//! addresses in the form below. Under `-Osize` it is the cheapest form of the run; under `-Ogas`
//! the cost model finds it cheaper in gas as well, so both objectives take it.
//!
//! # Matching
//!
//! A pair is a `mstore` whose value is the result of a `calldataload` or `mload`. Both addresses
//! are flattened as in `byte-run-loads`, so `b + 32`, `(b + 8) + 24` and `b + 56 - 24` meet as
//! one base at the same constant offset. Pairs of one block that share both bases and the source
//! location are chained in program order while both offsets advance by exactly one word. A chain
//! of two or more pairs is a run.
//!
//! # Safety
//!
//! The copy replaces the run's first store, and the run's other stores go. Every instruction
//! between the run's first access and its last store must either belong to the run or be a pure
//! computation that observes neither memory nor gas, so no read sees memory between the stores
//! and no other store lands between two of them. A word loaded by an instruction that has other
//! users keeps that load and loses only its store.
//!
//! Calldata cannot change, so its loads may sit anywhere that dominates the stores:
//! `calldatacopy` reads the same bytes, zero-fills past the end of calldata as `calldataload`
//! does, and writes the same destination range. The two differ where the source range wraps past
//! `2^256`: the loads wrap to the start of calldata and the copy reads zeros. A run is rewritten
//! only when the last word's address provably cannot wrap. That holds for a constant source, and
//! otherwise for what `check-elim`'s `facts_at` shows at the first store:
//!
//! - `start + reach < start` is false, from an upper bound of `start`: the ranges of its terms, the
//!   bound `calldatasize` gives a value a passed check keeps below it, and sums, products and
//!   shifts of those. A loop cursor that steps by a constant, or an index times a stride, is
//!   bounded by its entry value plus the step over the target's trip limit, because every iteration
//!   burns gas and each step below the bound cannot wrap;
//! - the run ends inside calldata: `x <= calldatasize` and `calldatasize - x >= c + reach` for
//!   `start = x + c`, through the difference some check of the function already computes. A chain
//!   of nested offsets, each checked against the room behind the pointer it adds to, proves one
//!   link at a time;
//! - a parameter of an internal function is bounded or below `calldatasize` at every call, which
//!   `argument_facts` takes from the checks that dominate each call, round by round through the
//!   callers. Only functions that are called directly qualify. The same walk gives each function a
//!   lower bound of `calldatasize`, the least the checks before each call, or before the dispatch
//!   `entry` tail-calls an ABI wrapper, establish. With that bound `calldatasize - c` cannot wrap,
//!   so a nested decoder's offset check against it keeps `base + offset + c` inside calldata, and
//!   so does the pointer it passes on.
//!
//! Other runs keep their loads.
//!
//! `mcopy` reads its whole source before it writes, where the word pairs interleave loads and
//! stores. Memory loads must lie in the run's block with no write to the source before the
//! last store. The two forms agree when every load comes before the first store, when the
//! destination starts at or below the source on one base so that no store reaches a word still
//! to load, or when alias analysis proves the ranges disjoint. Runs the pass cannot prove
//! keep their word copies, as do all runs for a target without `MCOPY`. Memory addresses do not
//! need a wrap proof: an access at `2^256 - 32` or above runs out of gas in either form.
//!
//! # Profitability
//!
//! The target's cost model prices each run: the load, the store and the address arithmetic of
//! every word, against one copy and its three operands (`Target::bulk_word_copy_improves`).
//!
//! # Limitations
//!
//! A run must lie in one block, as `byte-run-loads` requires, so a decoder that validates each
//! word in its own block is left alone. A pointer that an internal call returns, or one that a
//! signed offset check lets wrap, has no bound here, and its run keeps its loads.

use super::{
    byte_run::{address_key, sweep_dead},
    check_elim::{ParamFacts, argument_facts, calldatasize_value, facts_at},
    egraph::use_counts,
    lower_mcopy::{CopyDirection, copy_direction_of},
};
use crate::{
    mir::{
        BlockId, EffectKind, Function, Immediate, InstId, InstKind, Instruction, MirType, Module,
        PtrLocation, Value, ValueId,
        analysis::{AliasAnalysis, LocationSize},
        memory::EvmMemoryLayout,
        pass::{MirPass, ModuleAnalyses, run_function_pass_with_alias},
    },
    target::Target,
};
use alloy_primitives::U256;
use solar_data_structures::{
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use solar_sema::Gcx;

/// Bytes in one word.
const WORD: u64 = 32;

pub(crate) struct CopyRuns;

impl MirPass for CopyRuns {
    fn name(&self) -> &'static str {
        "copy-runs"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        let target = Target::new(gcx);
        // What every call establishes about the parameters, once a block holds a calldata run.
        let has_runs = |func: &Function| {
            func.blocks.indices().any(|block| {
                let pairs = collect_pairs(func, block);
                pairs.iter().filter(|pair| pair.source == PtrLocation::Calldata).count() >= 2
            })
        };
        let arguments = if module.functions.iter().any(has_runs) {
            argument_facts(module)
        } else {
            Default::default()
        };
        run_function_pass_with_alias(module, analyses, |func, analyses| {
            let params = arguments.get(&analyses.function()).cloned().unwrap_or_default();
            run_function(func, analyses.alias(), &target, &params)
        })
    }
}

/// An address split into its non-constant addends and their constant sum.
struct Address {
    base: Vec<ValueId>,
    offset: u64,
    value: ValueId,
}

/// One `mstore` of a `calldataload` or an `mload`.
struct Pair {
    store: InstId,
    /// Index of the store in its block.
    position: usize,
    /// The load and its result.
    load: InstId,
    value: ValueId,
    /// Index of the load in the store's block, if it lies there. Only memory runs need it.
    load_position: Option<usize>,
    source: PtrLocation,
    dest: Address,
    src: Address,
}

/// A run worth replacing: the first store becomes the copy and `dropped` go.
struct Candidate {
    block: BlockId,
    store: InstId,
    dest: ValueId,
    src: ValueId,
    source: PtrLocation,
    dropped: Vec<InstId>,
}

impl Candidate {
    fn words(&self) -> u64 {
        self.dropped.len() as u64 + 1
    }
}

fn run_function(
    func: &mut Function,
    alias: &AliasAnalysis,
    target: &Target,
    params: &ParamFacts,
) -> bool {
    let mut uses = None;
    let mut candidates = Vec::new();
    for block in func.blocks.indices().collect::<Vec<_>>() {
        let pairs = collect_pairs(func, block);
        if pairs.len() < 2 {
            continue;
        }
        for run in chain_runs(&pairs) {
            let uses = uses.get_or_insert_with(|| use_counts(func));
            if let Some(candidate) = price_run(func, alias, block, &pairs, &run, uses, target) {
                candidates.push(candidate);
            }
        }
    }
    if candidates.is_empty() {
        return false;
    }
    let exact = non_wrapping_calldata_runs(func, &candidates, params);
    let mut removed = FxHashMap::<BlockId, FxHashSet<InstId>>::default();
    for candidate in candidates {
        // Memory addresses cannot wrap: a word at 2^256 - 32 or above runs out of gas.
        if candidate.source == PtrLocation::Calldata && !exact.contains(&candidate.store) {
            continue;
        }
        let len = U256::from(WORD * candidate.words());
        let len = func.alloc_value(Value::Immediate(Immediate::I256(len)));
        // calldatacopy d, s, 32 * n
        // mcopy d, s, 32 * n
        func.inst_mut(candidate.store).replace_kind(InstKind::Copy(
            candidate.dest,
            candidate.src,
            len,
            candidate.source,
        ));
        for &dropped in &candidate.dropped {
            let metadata = func.inst(dropped).metadata.clone();
            func.inst_mut(candidate.store).metadata.merge_debug_context(&metadata);
        }
        removed.entry(candidate.block).or_default().extend(candidate.dropped);
    }
    let changed = !removed.is_empty();
    for (block, removed) in removed {
        func.blocks[block].instructions.retain(|inst| !removed.contains(inst));
    }
    if changed {
        sweep_dead(func);
    }
    changed
}

/// The first stores of the calldata runs whose last word's address cannot wrap.
///
/// A source that can reach the top of the address space wraps in the loads and not in the copy.
/// The copy is exact for a constant source below it, or where `start + reach < start` is false
/// at the first store, which a dominating guard shows directly or as the run ending inside
/// calldata. With `start = x + c`, that is `x <= calldatasize` and
/// `calldatasize - x >= c + reach`, through the difference some check of the function already
/// computes.
fn non_wrapping_calldata_runs(
    func: &mut Function,
    candidates: &[Candidate],
    params: &ParamFacts,
) -> FxHashSet<InstId> {
    let mut exact = FxHashSet::default();
    let mut unproven = Vec::new();
    for candidate in candidates.iter().filter(|c| c.source == PtrLocation::Calldata) {
        match func.value_u256(candidate.src) {
            Some(start) => {
                if start.checked_add(U256::from(WORD * candidate.words())).is_some() {
                    exact.insert(candidate.store);
                }
            }
            None => unproven.push(candidate),
        }
    }
    if unproven.is_empty() {
        return exact;
    }
    let size = calldatasize_value(func);
    // The difference `calldatasize - x` that a check of the function computes, for each `x`.
    let mut rooms = FxHashMap::<ValueId, ValueId>::default();
    if let Some(size) = size {
        for inst in func.instructions() {
            if let InstKind::Sub(minuend, x) = func.inst(inst).kind
                && minuend == size
                && let Some(room) = func.inst_result_value(inst)
            {
                rooms.entry(x).or_insert(room);
            }
        }
    }
    // The conditions belong to no block, so nothing refers to them past the query.
    let mark = func.allocation_mark();
    let mut queries = Vec::new();
    let mut conditions = Vec::new();
    for candidate in unproven {
        let reach = U256::from(WORD * (candidate.words() - 1));
        let reach_value = func.alloc_value(Value::Immediate(Immediate::I256(reach)));
        // wraps = lt (add start, reach), start
        let end = alloc_inst(func, InstKind::Add(candidate.src, reach_value), MirType::I256);
        let wraps = alloc_inst(func, InstKind::Lt(end, candidate.src), MirType::I1);
        queries.push((candidate.store, wraps));
        let mut proofs = vec![wraps];
        let (x, constant) = split_constant(func, candidate.src);
        if let Some(size) = size
            && let Some(&room) = rooms.get(&x)
        {
            let needed = U256::from(constant) + reach;
            let needed = func.alloc_value(Value::Immediate(Immediate::I256(needed)));
            // outside = or (gt x, calldatasize), (lt (sub calldatasize, x), c + reach)
            let past = alloc_inst(func, InstKind::Gt(x, size), MirType::I1);
            let short = alloc_inst(func, InstKind::Lt(room, needed), MirType::I1);
            let outside = alloc_inst(func, InstKind::Or(past, short), MirType::I1);
            queries.push((candidate.store, outside));
            proofs.push(outside);
        }
        conditions.push((candidate.store, proofs));
    }
    let refuted = facts_at(func, size, params, &queries);
    func.truncate_to(mark);
    exact.extend(
        conditions
            .into_iter()
            .filter(|(_, proofs)| proofs.iter().any(|proof| refuted.contains(proof)))
            .map(|(store, _)| store),
    );
    exact
}

/// Allocates an instruction that no block holds, for a query to evaluate.
fn alloc_inst(func: &mut Function, kind: InstKind, ty: MirType) -> ValueId {
    func.alloc_value_inst(Instruction::new(kind, Some(ty))).1
}

/// Splits `value` into `x + c` for the largest run of constant addends `c`, or `value + 0`.
fn split_constant(func: &Function, mut value: ValueId) -> (ValueId, u64) {
    let mut total = 0u64;
    while let Value::Inst(inst) = *func.value(value)
        && let InstKind::Add(a, b) = func.inst(inst).kind
    {
        let (next, constant) = match (func.value_u64(a), func.value_u64(b)) {
            (_, Some(constant)) => (a, constant),
            (Some(constant), _) => (b, constant),
            _ => break,
        };
        let Some(sum) = total.checked_add(constant) else { break };
        total = sum;
        value = next;
    }
    (value, total)
}

fn collect_pairs(func: &Function, block: BlockId) -> Vec<Pair> {
    let instructions = &func.blocks[block].instructions;
    let mut pairs = Vec::new();
    for (position, &inst) in instructions.iter().enumerate() {
        let InstKind::Store(dest, value, PtrLocation::Memory) = func.inst(inst).kind else {
            continue;
        };
        let Value::Inst(load) = *func.value(value) else { continue };
        let InstKind::Load(src, source @ (PtrLocation::Calldata | PtrLocation::Memory)) =
            func.inst(load).kind
        else {
            continue;
        };
        let (Some((dest_base, dest_offset)), Some((src_base, src_offset))) =
            (address_key(func, dest), address_key(func, src))
        else {
            continue;
        };
        pairs.push(Pair {
            store: inst,
            position,
            load,
            value,
            load_position: None,
            source,
            dest: Address { base: dest_base, offset: dest_offset, value: dest },
            src: Address { base: src_base, offset: src_offset, value: src },
        });
    }
    if pairs.len() >= 2 && pairs.iter().any(|pair| pair.source == PtrLocation::Memory) {
        let positions: FxHashMap<InstId, usize> =
            instructions.iter().enumerate().map(|(position, &inst)| (inst, position)).collect();
        for pair in &mut pairs {
            pair.load_position = positions.get(&pair.load).copied();
        }
    }
    pairs
}

/// Groups pairs into maximal chains, as indices into `pairs` in program order.
fn chain_runs(pairs: &[Pair]) -> Vec<Vec<usize>> {
    let mut runs: Vec<Vec<usize>> = Vec::new();
    for (index, pair) in pairs.iter().enumerate() {
        let extends = |run: &Vec<usize>| {
            let last = &pairs[*run.last().unwrap()];
            last.source == pair.source
                && last.dest.base == pair.dest.base
                && last.src.base == pair.src.base
                && last.dest.offset.checked_add(WORD) == Some(pair.dest.offset)
                && last.src.offset.checked_add(WORD) == Some(pair.src.offset)
        };
        match runs.iter_mut().find(|run| extends(run)) {
            Some(run) => run.push(index),
            None => runs.push(vec![index]),
        }
    }
    runs.retain(|run| run.len() >= 2);
    runs
}

fn price_run(
    func: &Function,
    alias: &AliasAnalysis,
    block: BlockId,
    pairs: &[Pair],
    run: &[usize],
    uses: &IndexVec<ValueId, u32>,
    target: &Target,
) -> Option<Candidate> {
    let first = &pairs[run[0]];
    let last = &pairs[*run.last().unwrap()];
    let source = first.source;
    let from_memory = source == PtrLocation::Memory;
    if from_memory && !target.evm_version().has_mcopy() {
        return None;
    }
    // Only the run's own stores and loads and pure computations may sit between its first access
    // and its last store. Memory loads must sit in the block, so nothing writes the source
    // between a load and the copy; calldata loads may sit anywhere.
    let mut own: FxHashSet<InstId> = run.iter().map(|&index| pairs[index].store).collect();
    let mut start = first.position;
    if from_memory {
        for &index in run {
            start = start.min(pairs[index].load_position?);
            own.insert(pairs[index].load);
        }
    }
    let instructions = &func.blocks[block].instructions;
    if !instructions[start..=last.position].iter().all(|&inst| {
        own.contains(&inst) || {
            let kind = &func.inst(inst).kind;
            !kind.has_side_effects()
                && !kind.effects().observes_execution()
                && matches!(kind.effect_kind(), EffectKind::Pure | EffectKind::EnvironmentRead)
        }
    }) {
        return None;
    }
    let shared = |index: &usize| uses.get(pairs[*index].value).copied().unwrap_or(0) > 1;
    let kept_loads = run.iter().filter(|&index| shared(index)).count();
    if from_memory {
        // The loads that follow a multi-word call and read its extra results through the buffer
        // belong to the call's stack protocol, which the backend recognizes only as loads.
        if run.iter().any(|&index| {
            pairs[index].src.base.iter().any(|&base| reads_multi_return_buffer_pointer(func, base))
        }) {
            return None;
        }
        // A load that stays must not read after the copy has written.
        if run.iter().any(|index| {
            shared(index) && pairs[*index].load_position.is_some_and(|at| at > first.position)
        }) {
            return None;
        }
        // Word-by-word copying is `mcopy` when every load comes first, or when it goes forward.
        let loads_first = run
            .iter()
            .all(|&index| pairs[index].load_position.is_some_and(|at| at < first.position));
        let size = LocationSize::Const(WORD * run.len() as u64);
        if !loads_first
            && copy_direction_of(func, alias, first.dest.value, first.src.value, size)
                != CopyDirection::Forward
        {
            return None;
        }
    }
    if !target.bulk_word_copy_improves(
        run.len() as u32,
        kept_loads as u32,
        source,
        func.value_u64(first.dest.value),
        func.value_u64(first.src.value),
    ) {
        return None;
    }
    Some(Candidate {
        block,
        store: first.store,
        dest: first.dest.value,
        src: first.src.value,
        source,
        dropped: run[1..].iter().map(|&index| pairs[index].store).collect(),
    })
}

/// Whether `value` is the buffer pointer a multi-word call's caller reads to find the extra
/// results.
fn reads_multi_return_buffer_pointer(func: &Function, value: ValueId) -> bool {
    matches!(*func.value(value), Value::Inst(inst)
        if matches!(func.inst(inst).kind, InstKind::Load(slot, PtrLocation::Memory)
            if func.value_u64(slot) == Some(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT)))
}

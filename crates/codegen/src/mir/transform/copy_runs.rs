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
//! `2^256`: the loads wrap to the start of calldata and the copy reads zeros. A run is
//! rewritten only when the last word's address provably cannot wrap, which is a constant source,
//! a dominating guard (`check-elim` evaluates `start + reach < start` as false, from a range
//! bound or from a bound by `calldatasize`), or a start that a passed check keeps inside calldata.
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
//! word in its own block is left alone. A source that loops over an array has a cursor whose
//! bound needs an induction proof, which this pass does not attempt.

use super::{
    byte_run::{address_key, sweep_dead},
    check_elim::conditions_false,
    egraph::use_counts,
};
use crate::{
    mir::{
        BlockId, EffectKind, Function, Immediate, InstId, InstKind, Instruction, MirType, Module,
        PtrLocation, Value, ValueId,
        analysis::{AliasAnalysis, AliasResult, LocationSize},
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
        run_function_pass_with_alias(module, analyses, |func, analyses| {
            run_function(func, analyses.alias(), &target)
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
    load: ValueId,
    /// Index of the load in the store's block, if it lies there.
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
    words: usize,
    source: PtrLocation,
    dropped: Vec<InstId>,
}

fn run_function(func: &mut Function, alias: &AliasAnalysis, target: &Target) -> bool {
    let uses = use_counts(func);
    let mut candidates = Vec::new();
    for block in func.blocks.indices().collect::<Vec<_>>() {
        let pairs = collect_pairs(func, block);
        if pairs.len() < 2 {
            continue;
        }
        for run in chain_runs(&pairs) {
            if let Some(candidate) = price_run(func, alias, block, &pairs, &run, &uses, target) {
                candidates.push(candidate);
            }
        }
    }
    let bounded = non_wrapping_calldata_runs(func, &candidates);
    let mut removed = FxHashMap::<BlockId, FxHashSet<InstId>>::default();
    let mut changed = false;
    for candidate in candidates {
        // Memory addresses cannot wrap: a word at 2^256 - 32 or above runs out of gas.
        if candidate.source == PtrLocation::Calldata
            && !func.value_u256(candidate.src).is_some_and(|start| {
                start.checked_add(U256::from(WORD * candidate.words as u64)).is_some()
            })
            && !bounded.contains(&candidate.store)
        {
            continue;
        }
        // calldatacopy d, s, 32 * n
        // mcopy d, s, 32 * n
        let len = func.alloc_value(Value::Immediate(Immediate::I256(U256::from(
            WORD * candidate.words as u64,
        ))));
        func.inst_mut(candidate.store).replace_kind(InstKind::Copy(
            candidate.dest,
            candidate.src,
            len,
            candidate.source,
        ));
        removed.entry(candidate.block).or_default().extend(candidate.dropped);
        changed = true;
    }
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
/// The copy is exact where `start + reach < start` is false at the first store, which a
/// dominating guard shows directly, or as the run ending inside calldata. With
/// `start = x + c`, that is `x <= calldatasize` and `calldatasize - x >= c + reach`, through the
/// difference some check of the function already computes.
fn non_wrapping_calldata_runs(func: &mut Function, candidates: &[Candidate]) -> FxHashSet<InstId> {
    let mark = func.allocation_mark();
    let size = func.instructions().find_map(|inst| {
        matches!(func.inst(inst).kind, InstKind::CalldataSize)
            .then(|| func.inst_result_value(inst))
            .flatten()
    });
    let mut queries = Vec::new();
    let mut conditions = Vec::new();
    for candidate in candidates {
        if candidate.source != PtrLocation::Calldata || func.value_u256(candidate.src).is_some() {
            continue;
        }
        let reach = WORD * (candidate.words as u64 - 1);
        let reach_value = func.alloc_value(Value::Immediate(Immediate::I256(U256::from(reach))));
        let (_, end) = func.alloc_value_inst(Instruction::new(
            InstKind::Add(candidate.src, reach_value),
            Some(MirType::I256),
        ));
        let (_, wraps) = func.alloc_value_inst(Instruction::new(
            InstKind::Lt(end, candidate.src),
            Some(MirType::I1),
        ));
        queries.push((candidate.store, wraps));
        let mut proofs = vec![wraps];
        if let Some(size) = size {
            let (x, constant) = split_constant(func, candidate.src);
            let room = func.instructions().find_map(|inst| {
                matches!(func.inst(inst).kind, InstKind::Sub(a, b) if a == size && b == x)
                    .then(|| func.inst_result_value(inst))
                    .flatten()
            });
            if let Some(room) = room {
                let needed = func
                    .alloc_value(Value::Immediate(Immediate::I256(U256::from(constant + reach))));
                let (_, past) = func
                    .alloc_value_inst(Instruction::new(InstKind::Gt(x, size), Some(MirType::I1)));
                let (_, short) = func.alloc_value_inst(Instruction::new(
                    InstKind::Lt(room, needed),
                    Some(MirType::I1),
                ));
                let (_, outside) = func.alloc_value_inst(Instruction::new(
                    InstKind::Or(past, short),
                    Some(MirType::I1),
                ));
                queries.push((candidate.store, outside));
                proofs.push(outside);
            }
        }
        conditions.push((candidate.store, proofs));
    }
    let refuted =
        if queries.is_empty() { FxHashSet::default() } else { conditions_false(func, &queries) };
    // The conditions belong to no block, so nothing refers to them past the query.
    func.truncate_to(mark);
    conditions
        .into_iter()
        .filter(|(_, proofs)| proofs.iter().any(|proof| refuted.contains(proof)))
        .map(|(store, _)| store)
        .collect()
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
    let positions: FxHashMap<InstId, usize> =
        instructions.iter().enumerate().map(|(position, &inst)| (inst, position)).collect();
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
            load: value,
            load_position: positions.get(&load).copied(),
            source,
            dest: Address { base: dest_base, offset: dest_offset, value: dest },
            src: Address { base: src_base, offset: src_offset, value: src },
        });
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
    // Memory loads must sit in the block, so nothing writes the source between a load and the
    // copy. Calldata loads may sit anywhere.
    let first_load = if from_memory {
        let positions = run.iter().map(|&index| pairs[index].load_position);
        positions.collect::<Option<Vec<_>>>()?.into_iter().min()?
    } else {
        first.position
    };
    let start = first.position.min(first_load);
    // Only the run's own stores and loads and pure computations may sit between its first access
    // and its last store.
    let mut own: FxHashSet<InstId> = run.iter().map(|&index| pairs[index].store).collect();
    if from_memory {
        own.extend(run.iter().filter_map(|&index| {
            let Value::Inst(load) = *func.value(pairs[index].load) else { return None };
            Some(load)
        }));
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
    let shared = |index: &usize| uses.get(pairs[*index].load).copied().unwrap_or(0) > 1;
    let kept_loads = run.iter().filter(|&index| shared(index)).count();
    if from_memory {
        // A load that stays must not read after the copy has written.
        if run.iter().any(|index| {
            shared(index) && pairs[*index].load_position.is_some_and(|at| at > first.position)
        }) {
            return None;
        }
        let bytes = WORD * run.len() as u64;
        let loads_first = run
            .iter()
            .all(|&index| pairs[index].load_position.is_some_and(|at| at < first.position));
        if !loads_first && !forward_copy_is_move(func, alias, first, bytes) {
            return None;
        }
    }
    if !target.bulk_word_copy_improves(
        run.len() as u32,
        kept_loads as u32,
        !from_memory,
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
        words: run.len(),
        source,
        dropped: run[1..].iter().map(|&index| pairs[index].store).collect(),
    })
}

/// Whether copying word by word, upward, gives the result of `mcopy`: the ranges are disjoint,
/// or the destination starts at or below the source, so no store reaches a word still to load.
fn forward_copy_is_move(func: &Function, alias: &AliasAnalysis, first: &Pair, bytes: u64) -> bool {
    if first.dest.base == first.src.base {
        let (dest, src) = (first.dest.offset, first.src.offset);
        return dest <= src || dest >= src.saturating_add(bytes);
    }
    let size = LocationSize::Const(bytes);
    let (Some(dest), Some(src)) = (
        alias.bare_memory_location(func, first.dest.value, size),
        alias.bare_memory_location(func, first.src.value, size),
    ) else {
        return false;
    };
    alias.memory_alias(dest, src) == AliasResult::NoAlias
}

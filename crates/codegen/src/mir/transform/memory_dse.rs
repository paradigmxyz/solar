//! Dead memory-store elimination and store-to-load forwarding.
//!
//! Each round of the pass runs these rewrites until none applies:
//!
//! - Immutable copies: a 32-byte `codecopy` from a constant offset of the code tail followed by an
//!   `mload` of the same word, as assembly reading clone arguments does, reuses the word an
//!   identical dominating copy loaded and becomes an `mstore` of it.
//! - Frame stores: a store to an internal-frame word that no instruction or terminator of the
//!   function can read is removed, unless the function makes internal calls or observes gas.
//! - Within each block: `keccak256` over words that constant stores fully define folds to its hash
//!   when that is cheaper: the hash is a `PUSH32` at each use, where the result stays on the stack,
//!   so the fold must repay those bytes with the stores it leaves dead, found by running the
//!   liveness dataflow below without the hash; an `mload` reads the value of the latest store to
//!   the same canonical word; a store of the value the word already holds is removed; and a
//!   backward scan removes a full-word store that a later full-word store, `memory_zero`, or copy
//!   into a constant range overwrites before any read or gas observation; a small word-sized zero
//!   fill or calldata copy whose every word such a write overwrites goes too. Returndata copies
//!   stay because their source bounds check can revert. Reads over constant ranges, such as hashes,
//!   logs, and call inputs, only keep the words they cover. A reload of the free-memory-pointer
//!   word forwards across every instruction that cannot write it, such as a write through a heap
//!   pointer.
//! - Store narrowing: when a later full-word store overwrites the last 31 bytes of a word store
//!   before any read or gas observation, only the first byte of the earlier store is observable, so
//!   the backward scan shortens the store to an `mstore8` of that byte. `mstore8` demands only the
//!   low 8 bits of its operand, so the operand needs the top byte of the stored word in its low
//!   bits: that is `v` for `shl v, 248` over a 256-bit `v`, and `c >> 248` for a nonzero constant
//!   `c` whose low 248 bits are zero. Other values stay whole, since a `shr` would cost what the
//!   shift saves, and so does a store whose tail is read or never overwritten, such as a
//!   one-character string literal. The later store already expands memory past the narrowed one, so
//!   the size and the final contents do not change. This follows the partial-overwrite
//!   classification of LLVM's dead store elimination, which only shortens `memset` and `memcpy`
//!   because a narrower scalar store needs a new value; `mstore8` is as cheap as `mstore` and the
//!   operand already exists. Deliberate limits: it needs one later store at the next byte, not the
//!   union of several later overwrites, and it leaves a store whose only surviving byte is the
//!   last. A byte store kills nothing across blocks, so narrowing can hide an earlier dead store of
//!   the same word in another block from the cross-block rewrites.
//! - Dead allocation stores: a constant-offset `mstore` or `mstore8` into an `alloc` whose pointer
//!   does not escape is removed when no read reachable after it overlaps its bytes. The pass
//!   derives the object's pointers through `ptrtoint`, `inttoptr`, and constant `add` and `ptradd`;
//!   a pointer moved by a runtime index may land anywhere in the object, since address arithmetic
//!   wraps, so reads through it cover all of it. Exact loads, hashes, logs, creations, call inputs,
//!   memory copies, and the data of `revert` and `returndata` are the reads. A range with a runtime
//!   length covers the bytes from its start on, so a `bytes` length word dies once forwarding has
//!   replaced its loads and the call or terminator reads only the data after it. Any other use of a
//!   derived pointer (a stored value, a phi, an internal call argument, a `ret`, a `sub`, a
//!   runtime-offset `add`) leaves the object alone, as does a heap object that follows a
//!   free-memory pointer reset and so may lie over an earlier one, as does every exposed object of
//!   a function that writes through a runtime index, which may wrap and rewind the pointer. Other
//!   allocations do not overlap. Assembly can still rebuild a pointer into a heap object from the
//!   free-memory pointer, so every other read that may touch heap memory, such as a load through an
//!   unrelated pointer, `msize`, or a callee with unknown reads, reads the whole object. A `ret` or
//!   tail call reads no object whose pointer it does not receive. A read that may leave its own
//!   allocation reaches the objects allocated after it, not the ones before. A write-only deferred
//!   object, which static placement keeps unrelated to the free-memory pointer, ignores them. A
//!   store that may reach past the end of its object may write over the next one and stays: the
//!   size of an infallible allocation, whose arithmetic cannot wrap, is bounded below through sums
//!   and word-rounding masks, and other runtime sizes have no bound. Functions observing gas keep
//!   their stores, since the stores pay for memory expansion. Writes the object receives from
//!   copies and calls stay: only stores are candidates.
//! - Write-only allocations: an allocation that nothing reads loses every store and copy that fills
//!   it, which the dead-allocation-store rule above leaves when they are copies, fills, or indexed
//!   stores. Alias analysis shows that the allocation does not escape, no read may alias it, and
//!   its writes stay inside it. Its proof scans the whole function, so only `memory-dse-unread`,
//!   the run after builtin expansion, applies it. See `write_only`.
//! - Across edges: a constant store equal to the constant a sole predecessor left in the word is
//!   removed, and a store that the single jump successor overwrites first, with no memory or gas
//!   observer in between, is removed.
//! - Across the CFG: a backward liveness dataflow over constant, word-aligned addresses removes a
//!   full-word store that no path reads before overwriting it. Writes keep nothing live. A read of
//!   unknown extent keeps every word from its address on when an allocation, a frame, or a read of
//!   the free memory pointer bounds that address from below through constant offsets, as the heap
//!   and the frames lie above the scratch space and the free memory pointer. Constant additions
//!   need a no-wrap proof; without one, a fixed-size read keeps both the wrapped low range and the
//!   non-wrapped heap range live. Unknown ranges keep all memory live: a runtime index past a
//!   length that assembly forged can wrap a read around to low memory. A word loaded from the free
//!   memory pointer's slot bounds a read only when nothing in the module can leave a word below the
//!   heap there, as a copy into the slot can, and a decoded value only when it is a memory object;
//!   an encoding or decoding that allocates reads the free memory pointer. The pointer an internal
//!   call returns is bounded the same way when every return of the callee is. An internal call
//!   keeps what its callee's memory summary may read, and a call or tail call to a function that
//!   never returns and calls nothing, as a shared revert helper is, keeps what that function reads
//!   before writing. A return keeps what the callers may read right after their calls when the
//!   module shows every call site, joined over them to a fixpoint, so a helper that returns a fresh
//!   string to a wrapper that only encodes it loses its scratch stores and its free memory pointer
//!   bump. External entries, functions nothing calls, and functions with several results, whose
//!   tail the backend passes back through memory, keep all memory at their returns, as does any
//!   other tail call; revert and return data keep only the words they cover, and halts such as
//!   `stop` keep none.
//!
//! Unbounded raw reads retain their exact address and size until a full overwrite of the same
//! range discharges them. Unmatched reads keep every slot live; their value identities are
//! forgotten before crossing definitions or function boundaries, so loop iterations and callees
//! cannot be confused. Partial writes never discharge them.
//!
//! Keys come from alias analysis, which canonicalizes an address as a base plus a constant offset.
//! Allocation-based keys require the complete access range to fit; wider accesses may overlap
//! adjacent allocations. Every other instruction is classified through its shared ModRef effects:
//! a possible write invalidates the remembered words it may overlap, and only the explicit writes
//! above count as definite overwrites. The pass runs late in semantic optimization and again in
//! lowering, after ABI and aggregate lowering expose word stores. Another run follows the CSE that
//! forwards the length words of `bytes` objects, while allocations are still atomic, so the dead
//! allocation stores go before allocation lowering turns the object into free-memory-pointer
//! traffic. A final run after allocation lowering removes fills that only become explicit there,
//! before memory-zero lowering turns them into copies or loops, and forwards the free-memory
//! pointer and the stores that allocation lowering exposes to the loads that read them. It runs
//! under both objectives, since the forwarded words shrink the code.

use crate::{
    mir::{
        AllocationFailure, BasicBlock, BlockId, Callee, EffectKind, Function, FunctionId,
        Immediate, InstId, InstKind, MangledSymbol, MemoryRegion, MirModuleTarget, MirType, Module,
        PtrLocation, RawMemoryAccess, Terminator, Value, ValueId,
        analysis::{
            Access, AddressSpace, AliasAnalysis, CfgInfo, GasObservations, Location, LocationSize,
            MemoryAddress, MemoryBase, MemoryCallSummaries, MemoryLocation, ModRef, ReadBounds,
            integers::no_wrap_with,
        },
        memory::EvmMemoryLayout,
        pass::{MirPass, ModuleAnalyses, run_selected_function_pass_with_alias_and_cfg},
        utils::{self as mir_utils, IndexLists},
    },
    target::Target,
};
use alloy_primitives::{U256, keccak256};
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet},
};
use std::{
    collections::{BTreeMap, VecDeque},
    mem,
    rc::Rc,
    sync::Arc,
};

mod write_only;

/// Function pass for local dead memory-store elimination.
pub(crate) enum MemoryDse {
    /// Every rewrite except the removal of writes into unread allocations.
    Standard,
    /// Every rewrite, including the removal of writes into unread allocations.
    WithUnreadAllocations,
}

impl MirPass for MemoryDse {
    fn name(&self) -> &'static str {
        match self {
            Self::Standard => "memory-dse",
            Self::WithUnreadAllocations => "memory-dse-unread",
        }
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut ModuleAnalyses,
    ) -> bool {
        let unread_allocations = matches!(self, Self::WithUnreadAllocations);
        let mut selected = DenseBitSet::new_empty(module.functions.len());
        for (func_id, func) in module.functions.iter_enumerated() {
            if has_memory_writes(func) {
                selected.insert(func_id);
            }
        }
        if selected.is_empty() {
            return false;
        }
        let free_pointer = free_pointer_trusted(module);
        let target = module.target(gcx);
        let terminal = Arc::new(terminal_liveness(module, analyses, target, free_pointer));
        let summaries = analyses.call_summaries(module);
        let returns =
            Arc::new(return_facts(module, analyses, &summaries, &terminal, target, free_pointer));
        run_selected_function_pass_with_alias_and_cfg(
            module,
            analyses,
            &selected,
            |func, analyses| {
                let mut eliminator = MemoryStoreEliminator::new(
                    Rc::clone(analyses.alias()),
                    Rc::clone(analyses.cfg()),
                    analyses.target(gcx),
                    free_pointer,
                    unread_allocations,
                );
                eliminator.terminal = Arc::clone(&terminal);
                eliminator.returns = Arc::clone(&returns);
                eliminator.run_to_fixpoint(func) != 0
            },
        )
    }
}

/// The memory each function that never returns and calls nothing reads before writing it, as a
/// shared revert helper reads only the payload it stores first. A call or tail call to such a
/// function keeps no other word live.
fn terminal_liveness(
    module: &Module,
    analyses: &mut ModuleAnalyses,
    target: Target,
    free_pointer: bool,
) -> FxHashMap<FunctionId, MemLive> {
    let mut terminal = FxHashMap::default();
    for (id, func) in module.functions.iter_enumerated() {
        let returns = |block: &BasicBlock| {
            matches!(
                block.terminator,
                None | Some(Terminator::Return { .. } | Terminator::TailCall { .. })
            )
        };
        let calls = |inst| {
            matches!(func.inst(inst).kind, InstKind::ICall { function: Callee::Function(_), .. })
        };
        if func.blocks.is_empty()
            || func.blocks.iter().any(returns)
            || func.instructions().any(calls)
        {
            continue;
        }
        let eliminator = MemoryStoreEliminator::new(
            Rc::new(AliasAnalysis::new(func)),
            analyses.cfg(id, func),
            target,
            free_pointer,
            false,
        );
        let mut entry = eliminator.liveness(func).swap_remove(BlockId::ENTRY);
        MemoryStoreEliminator::summarize_live(&mut entry);
        if !entry.is_all() {
            terminal.insert(id, entry);
        }
    }
    terminal
}

/// What callers read once a call returns, and where the pointers functions return point; see
/// [`return_facts`].
#[derive(Default)]
struct ReturnFacts {
    /// For each function whose every call site the module shows, the memory live after any of
    /// them. The returns of every other function keep all memory live.
    live: FxHashMap<MangledSymbol, MemLive>,
    /// The least address the single value each function returns can hold.
    bounds: FxHashMap<FunctionId, u64>,
}

/// Bounds of the iterations that join the memory live after calls into their callees' returns.
/// Each round only grows the sets; a module that still changes after this many keeps all memory
/// live at every return.
const RETURN_LIVENESS_ROUNDS: usize = 8;

/// Computes [`ReturnFacts`].
///
/// A function's result is bounded once every `ret` returns a value that
/// [`AliasAnalysis::read_lower_bound`] bounds and every tail call that can return reaches a
/// function whose result is bounded, so a caller's read through a returned heap pointer keeps
/// only the heap. The memory live at a function's returns joins, over its call sites, what each
/// caller keeps live right after the call, and for a tail call what the caller's own returns
/// keep. It starts empty and grows to a fixpoint, as recursion feeds a function's returns back
/// into its own call sites. External entries, the dispatcher, functions no instruction calls,
/// and functions with several results, whose tail the backend passes back through memory, keep
/// all memory live.
fn return_facts(
    module: &Module,
    analyses: &mut ModuleAnalyses,
    summaries: &Arc<MemoryCallSummaries>,
    terminal: &Arc<FxHashMap<FunctionId, MemLive>>,
    target: Target,
    free_pointer: bool,
) -> ReturnFacts {
    let mut facts = ReturnFacts::default();
    let mut calls = index_vec![Vec::new(); module.functions.len()];
    let mut tails = index_vec![Vec::new(); module.functions.len()];
    let mut pending = DenseBitSet::new_empty(module.functions.len());
    for (caller, func) in module.functions.iter_enumerated() {
        if func.return_components().len() == 1 {
            pending.insert(caller);
        }
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst in &block.instructions {
                if let InstKind::ICall { function: Callee::Function(callee), .. } =
                    func.inst(inst).kind
                {
                    calls[callee].push((caller, block_id, inst));
                }
            }
            if let Some(Terminator::TailCall { function, .. }) = block.terminator {
                tails[function].push(caller);
            }
        }
    }
    // A tail call to a function that never returns hands back no result.
    let returning = module.returning_functions();
    // A bound found once never changes. A failed proof can change only when a callee gains a
    // bound, so revisit just those callers, with every round still using the previous facts.
    let mut found = Vec::new();
    while !pending.is_empty() {
        found.clear();
        for id in pending.iter() {
            let func = &module.functions[id];
            let call = |function: FunctionId| facts.bounds.get(&function).copied();
            let mut bound = Some(u64::MAX);
            let mut returns = false;
            for block in &func.blocks {
                let returned = match &block.terminator {
                    Some(Terminator::Return { values }) => values.first().and_then(|&value| {
                        AliasAnalysis::read_lower_bound(func, value, &call, free_pointer)
                    }),
                    Some(Terminator::TailCall { function, .. })
                        if returning.contains(*function) =>
                    {
                        call(*function)
                    }
                    None => None,
                    Some(_) => continue,
                };
                returns = true;
                bound = bound.zip(returned).map(|(bound, returned)| bound.min(returned));
                if bound.is_none() {
                    break;
                }
            }
            if returns && let Some(bound) = bound {
                found.push((id, bound));
            }
        }
        pending.clear();
        facts.bounds.extend(found.iter().copied());
        for &(callee, _) in &found {
            for caller in calls[callee]
                .iter()
                .map(|&(caller, _, _)| caller)
                .chain(tails[callee].iter().copied())
            {
                if module.functions[caller].return_components().len() == 1
                    && !facts.bounds.contains_key(&caller)
                {
                    pending.insert(caller);
                }
            }
        }
    }
    // The pass finds a function's facts by its name, so a shared name keeps all memory live.
    let mut names = FxHashMap::default();
    for (id, func) in module.functions.iter_enumerated() {
        names.entry(func.name).and_modify(|unique| *unique = None).or_insert(Some(id));
    }
    let mut tracked = DenseBitSet::new_empty(module.functions.len());
    for (id, func) in module.functions.iter_enumerated() {
        if !func.blocks.is_empty()
            && !func.is_external_entry()
            && module.dispatch_entry() != Some(id)
            && func.return_components().len() <= 1
            && !(calls[id].is_empty() && tails[id].is_empty())
            && names[&func.name] == Some(id)
        {
            tracked.insert(id);
        }
    }
    // The call sites of tracked functions, by caller.
    let mut sites = FxHashMap::<FunctionId, Vec<_>>::default();
    for callee in tracked.iter() {
        for &(caller, block, inst) in &calls[callee] {
            sites.entry(caller).or_default().push((callee, block, inst));
        }
    }
    // The function bodies, CFGs, alias facts, and returned-pointer bounds do not change between
    // rounds. Keep their analyses, and group calls by block so each suffix is scanned only once.
    let mut callers = sites
        .into_iter()
        .map(|(caller, mut sites)| {
            let func = &module.functions[caller];
            let mut eliminator = MemoryStoreEliminator::new(
                Rc::new(AliasAnalysis::empty_with_summaries(Arc::clone(summaries))),
                analyses.cfg(caller, func),
                target,
                free_pointer,
                false,
            );
            eliminator.terminal = Arc::clone(terminal);
            sites.sort_unstable_by_key(|&(_, block, _)| block);
            let blocks = sites
                .chunk_by(|a, b| a.1 == b.1)
                .map(|group| (group[0].1, group.len()))
                .filter(|&(block, _)| eliminator.cfg.is_reachable(block))
                .collect::<Vec<_>>();
            let mut callees = sites.into_iter().map(|(callee, _, _)| callee).collect::<Vec<_>>();
            callees.sort_unstable();
            callees.dedup();
            // Only blocks reachable after a tracked call can contribute to its live-out.
            // Keep the successor closure, including loop backedges, so every dependency of a
            // retained block is present in the restricted liveness solve.
            let mut live_blocks = DenseBitSet::new_empty(func.blocks.len());
            let mut worklist = Vec::new();
            for &(block, _) in &blocks {
                if live_blocks.insert(block) {
                    worklist.push(block);
                }
            }
            while let Some(block) = worklist.pop() {
                for &successor in eliminator.cfg.successors(block) {
                    if live_blocks.insert(successor) {
                        worklist.push(successor);
                    }
                }
            }
            (caller, eliminator, blocks, callees, live_blocks)
        })
        .filter(|(_, _, blocks, _, _)| !blocks.is_empty())
        .collect::<Vec<_>>();

    let mut live = FxHashMap::default();
    for id in tracked.iter() {
        live.insert(id, MemLive::empty());
    }
    let mut next = FxHashMap::default();
    for round in 0..RETURN_LIVENESS_ROUNDS {
        let snapshot = Arc::new(ReturnFacts {
            live: live
                .iter()
                .map(|(&id, live)| (module.functions[id].name, live.clone()))
                .collect(),
            bounds: facts.bounds.clone(),
        });
        next.clone_from(&live);
        for (caller, eliminator, blocks, callees, live_blocks) in &mut callers {
            let func = &module.functions[*caller];
            // Only this caller's return liveness can change its transfer between rounds. Its
            // previous contributions remain in `next`, so an unchanged boundary needs no scan.
            // Nor can another contribution change a callee that already keeps all memory live.
            if round != 0
                && (eliminator.returns.live.get(&func.name) == snapshot.live.get(&func.name)
                    || callees.iter().all(|callee| live[callee].is_all()))
            {
                continue;
            }
            eliminator.returns = Arc::clone(&snapshot);
            let live_in = eliminator.liveness_in(func, live_blocks);
            eliminator.join_call_liveness(func, blocks, &live_in, &mut next);
        }
        for callee in tracked.iter() {
            for &caller in &tails[callee] {
                let returned = live.get(&caller).cloned().unwrap_or_else(MemLive::all);
                next.get_mut(&callee).expect("tracked callee").join(&returned);
            }
        }
        if next == live {
            facts.live = snapshot.live.clone();
            return facts;
        }
        mem::swap(&mut live, &mut next);
    }
    facts
}

/// Returns whether the function contains a store, fill, or copy this pass can remove or forward
/// from, or an immutable copy it can reuse: every rewrite starts from one of them.
fn has_memory_writes(func: &Function) -> bool {
    func.instructions().any(|inst_id| {
        matches!(
            func.inst(inst_id).kind,
            InstKind::Store(.., PtrLocation::Memory)
                | InstKind::Store8(..)
                | InstKind::CodeCopy(..)
                | InstKind::MemoryZero(..)
                | InstKind::Copy(.., PtrLocation::Calldata | PtrLocation::Memory)
                | InstKind::DataCopy(..)
                | InstKind::ExtCodeCopy(..)
        )
    })
}

/// Whether nothing in the module can leave a word below the heap in the free memory pointer's
/// slot, so that a word loaded from it bounds the reads through it; see
/// [`AliasAnalysis::read_lower_bound`]. A whole-word store keeps a pointer there only when its
/// low bytes retain a proven heap floor. This pass can run without heap-floor, and raw slot words
/// need not be clamped even in the normal pipeline. The compiler's own updates keep one too, as do
/// stores that reach only the slot's high bytes, where no pointer below 2^64 has bits, as Solady
/// writes call data across them. A copy, a byte store, or a store across the low bytes, as assembly
/// may write, can leave any word there, unless its block halts the call with no memory read after
/// it, as a revert's scratch payload does.
fn free_pointer_trusted(module: &Module) -> bool {
    module.functions.iter().all(|func| {
        let alias = AliasAnalysis::new(func);
        func.blocks.iter().all(|block| {
            let halts = matches!(
                block.terminator,
                Some(
                    Terminator::Revert { .. }
                        | Terminator::RevertReturndata
                        | Terminator::ReturnData { .. }
                        | Terminator::Stop
                        | Terminator::SelfDestruct { .. }
                        | Terminator::Invalid
                )
            );
            block.instructions.iter().enumerate().all(|(index, &inst)| {
                !forges_free_pointer(func, &alias, inst)
                    || halts
                        && block.instructions[index + 1..]
                            .iter()
                            .all(|&later| alias.instruction_mod_ref(func, later).reads().is_empty())
            })
        })
    })
}

/// Whether `inst` may write the low bytes of the free memory pointer's slot, other than with
/// a proven heap pointer or one of the compiler's own updates; see [`free_pointer_trusted`].
fn forges_free_pointer(func: &Function, alias: &AliasAnalysis, inst: InstId) -> bool {
    if func.inst(inst).kind.effect_kind().is_read_only() {
        return false;
    }
    match func.inst(inst).kind {
        InstKind::Store(address, value, PtrLocation::Memory)
            if func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT) =>
        {
            if let Some(value) = func.value_u256(value) {
                return value.as_limbs()[0] < EvmMemoryLayout::HEAP_START;
            }
            let Value::Inst(stored) = *func.value(value) else { return true };
            let stored = func.inst(stored);
            // Allocator results preserve the heap pointer; reloading the slot preserves the
            // invariant too. An arbitrary word may lose its floor when a high-byte write follows.
            return !stored.metadata.alloc_ptr()
                && !matches!(stored.kind, InstKind::Load(slot, PtrLocation::Memory)
                    if func.value_u64(slot) == Some(EvmMemoryLayout::FMP_SLOT));
        }
        // Internal callees are checked separately by the module-wide scan.
        InstKind::SetFmp(_)
        | InstKind::Alloc { .. }
        | InstKind::ICall { function: Callee::Function(_), .. } => return false,
        _ => {}
    }
    // The last eight bytes of the slot hold every pointer below 2^64.
    let low = EvmMemoryLayout::FMP_SLOT + EvmMemoryLayout::WORD_SIZE - 8;
    let slot_end = EvmMemoryLayout::FMP_SLOT + EvmMemoryLayout::WORD_SIZE;
    alias.instruction_mod_ref(func, inst).writes().iter().any(|access| match access {
        Access::Location(Location::Memory(location))
            if matches!(location.address.base, MemoryBase::Absolute) =>
        {
            let start = location.address.offset;
            let end = match location.size {
                LocationSize::Const(size) => start.saturating_add(size),
                LocationSize::Dynamic(_) | LocationSize::Unknown => u64::MAX,
            };
            start < slot_end && end > low
        }
        Access::Location(Location::Memory(location)) => {
            location.size != LocationSize::Const(0)
                && AliasAnalysis::address_lower_bound(func, location.address, location.size)
                    .is_none_or(|bound| bound < slot_end)
        }
        Access::Any(AddressSpace::Memory) => true,
        _ => false,
    })
}

/// Removes the instructions in `dead` from every block and returns how many there are.
fn remove_dead(func: &mut Function, dead: &DenseBitSet<InstId>) -> usize {
    let count = dead.count();
    if count != 0 {
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|&id| !dead.contains(id));
        }
    }
    count
}

/// Whether `func` has at least two memory stores.
fn has_two_memory_stores(func: &Function) -> bool {
    func.instructions()
        .filter(|&inst| matches!(func.inst(inst).kind, InstKind::Store(.., PtrLocation::Memory)))
        .nth(1)
        .is_some()
}

/// Local dead memory optimization.
struct MemoryStoreEliminator {
    /// Shared CFG snapshot for memory liveness and immutable-copy reuse.
    cfg: Rc<CfgInfo>,
    /// Number of memory instructions eliminated.
    eliminated_count: usize,
    alias: Rc<AliasAnalysis>,
    target: Target,
    /// Memory live at the entry of each block as of the start of the round, built when the first
    /// fold candidate needs it. Rewrites of the round only remove reads, so it stays conservative.
    live_in: Option<IndexVec<BlockId, MemLive>>,
    /// Operand uses of every value, built when a fold candidate needs them and dropped when a
    /// rewrite may add uses.
    uses: Option<IndexVec<ValueId, u32>>,
    /// Whether to remove the writes into allocations nothing reads.
    unread_allocations: bool,
    /// The memory functions that never return read first; see [`terminal_liveness`].
    terminal: Arc<FxHashMap<FunctionId, MemLive>>,
    /// What callers read once calls return; see [`return_facts`].
    returns: Arc<ReturnFacts>,
    /// Whether a word loaded from the free memory pointer's slot bounds reads through it; see
    /// [`free_pointer_trusted`].
    free_pointer: bool,
    /// Non-wrap proofs for the current immutable function snapshot.
    read_bounds: ReadBounds,
}

/// Above this many tracked live slots the backward DSE gives up precision and
/// treats all memory as live. Keeps the lattice height (and cost) bounded.
const MEM_LIVE_CAP: usize = 64;

/// Live slot set, kept sorted ascending. `MEM_LIVE_CAP` bounds the length, so a
/// sorted vector beats hashing on every operation the dataflow performs:
/// membership and insertion are binary searches, and the join is a sort and
/// dedup of two short runs. Almost all block-entry sets have zero to four
/// slots; larger sets spill without changing the analysis limit.
type LiveSlots = SmallVec<[u64; 4]>;

/// Backward memory-liveness lattice over constant word-aligned slots.
///
/// `slots` names the exact slots that may be read before the next full-word overwrite, and
/// `from`, when set, every word that reaches it or lies past it: what a read of unknown extent
/// may observe when a compiler-owned base bounds its address from below. Every other slot is
/// provably dead if overwritten. A `from` of zero is the conservative top: any address may be
/// observed. Slots `from` covers are not listed, so equal sets compare equal.
#[derive(Clone, Debug, PartialEq, Eq)]
struct MemLive {
    slots: LiveSlots,
    from: Option<u64>,
    /// Exact reads without a useful address bound. A matching full overwrite discharges them;
    /// until then, they may observe any slot. These IDs belong to the current function only.
    ranges: SmallVec<[(ValueId, ValueId); 2]>,
}

impl MemLive {
    fn empty() -> Self {
        Self { slots: LiveSlots::new(), from: None, ranges: SmallVec::new() }
    }

    fn all() -> Self {
        Self { slots: LiveSlots::new(), from: Some(0), ranges: SmallVec::new() }
    }

    /// Makes every address live, reusing the allocations.
    fn set_all(&mut self) {
        self.slots.clear();
        self.ranges.clear();
        self.from = Some(0);
    }

    fn is_all(&self) -> bool {
        self.from == Some(0)
    }

    /// Whether the word at `addr` reaches `from`.
    fn covers(&self, addr: u64) -> bool {
        self.from.is_some_and(|from| addr.saturating_add(EvmMemoryLayout::WORD_SIZE) > from)
    }

    fn contains(&self, addr: u64) -> bool {
        !self.ranges.is_empty() || self.covers(addr) || self.slots.binary_search(&addr).is_ok()
    }

    fn add_addr(&mut self, addr: u64) {
        if self.covers(addr) {
            return;
        }
        if let Err(index) = self.slots.binary_search(&addr) {
            if self.slots.len() >= MEM_LIVE_CAP {
                self.set_all();
                return;
            }
            self.slots.insert(index, addr);
        }
    }

    /// Marks every word that reaches `from` or lies past it live.
    fn add_from(&mut self, from: u64) {
        if self.from.is_some_and(|old| old <= from) {
            return;
        }
        self.from = Some(from);
        self.slots.retain(|addr| addr.saturating_add(EvmMemoryLayout::WORD_SIZE) <= from);
        if from == 0 {
            self.ranges.clear();
        }
    }

    /// Kills a fully overwritten slot. A word `from` covers stays live: the representation has no
    /// holes, which only ever keeps a store.
    fn kill(&mut self, addr: u64) {
        if let Ok(index) = self.slots.binary_search(&addr) {
            self.slots.remove(index);
        }
    }

    fn join(&mut self, other: &Self) {
        if self.is_all() {
            return;
        }
        if let Some(from) = other.from {
            self.add_from(from);
        }
        for &(offset, size) in &other.ranges {
            self.add_range(offset, size);
        }
        if other.slots.is_empty() {
            return;
        }
        // Merge, so the lattice value has one canonical sorted representation and equality is a
        // plain comparison.
        let mut merged = mem::take(&mut self.slots);
        merged.extend(other.slots.iter().copied());
        merged.sort_unstable();
        merged.dedup();
        merged.retain(|&mut addr| !self.covers(addr));
        if merged.len() > MEM_LIVE_CAP {
            self.set_all();
        } else {
            self.slots = merged;
        }
    }

    /// Keeps a bounded, canonical set so joins converge independently of predecessor order.
    fn add_range(&mut self, offset: ValueId, size: ValueId) {
        if self.is_all() {
            return;
        }
        if let Err(index) = self.ranges.binary_search(&(offset, size)) {
            if self.ranges.len() == 8 {
                self.set_all();
            } else {
                self.ranges.insert(index, (offset, size));
            }
        }
    }
}

/// One `(region, base)` group of 32-byte slots, ordered by offset.
#[derive(Debug)]
struct SlotBucket<T> {
    region: MemoryRegion,
    base: MemoryBase,
    slots: BTreeMap<u64, T>,
}

/// A map from 32-byte memory slots to values that invalidates by alias, without
/// rescanning every live entry on each write.
///
/// [`AliasAnalysis::memory_alias_locations`] settles most pairs from region and
/// base alone: two distinct allocation sites never overlap, two distinct
/// non-`Unknown` regions never overlap, and the offsets are compared only when
/// the bases are equal. Grouping entries by `(region, base)` therefore lets a
/// single test retire or drop a whole group, and ordering each group by offset
/// narrows the equal-base case to the window the write actually covers. A flat
/// map has to run the alias test against every entry for every store, which is
/// quadratic in the number of live slots — the shape that dominated this pass on
/// large functions.
#[derive(Debug)]
struct SlotMap<T> {
    /// Groups whose base is an allocation site, indexed by that site. Writes to
    /// one site can never invalidate another, so a write with an allocation
    /// base skips every other site outright.
    by_alloc: FxHashMap<InstId, SmallVec<[SlotBucket<T>; 1]>>,
    /// Groups whose base is not an allocation site. Every write has to consider
    /// these, but a write to a non-allocation base also invalidates nearly
    /// everything, so they stay few.
    unindexed: SmallVec<[SlotBucket<T>; 1]>,
}

impl<T> Default for SlotMap<T> {
    fn default() -> Self {
        Self { by_alloc: FxHashMap::default(), unindexed: SmallVec::new() }
    }
}

impl<T> SlotMap<T> {
    fn clear(&mut self) {
        self.by_alloc.clear();
        self.unindexed.clear();
    }

    /// The allocation site of `base`, if it has one. Mirrors the private
    /// `AliasAnalysis::allocation_base`.
    fn alloc_site(base: MemoryBase) -> Option<InstId> {
        match base {
            MemoryBase::Allocation(id) | MemoryBase::DynamicAllocation(id) => Some(id),
            _ => None,
        }
    }

    fn bucket_mut(&mut self, key: MemoryAddress) -> &mut SlotBucket<T> {
        let (region, base) = (key.region, key.base);
        let buckets = match Self::alloc_site(base) {
            Some(site) => self.by_alloc.entry(site).or_default(),
            None => &mut self.unindexed,
        };
        if let Some(index) = buckets.iter().position(|b| b.region == region && b.base == base) {
            return &mut buckets[index];
        }
        let index = buckets.len();
        buckets.push(SlotBucket { region, base, slots: BTreeMap::new() });
        &mut buckets[index]
    }

    fn get(&self, key: MemoryAddress) -> Option<&T> {
        let (region, base) = (key.region, key.base);
        let buckets = match Self::alloc_site(base) {
            Some(site) => self.by_alloc.get(&site)?.as_slice(),
            None => self.unindexed.as_slice(),
        };
        buckets.iter().find(|b| b.region == region && b.base == base)?.slots.get(&key.offset)
    }

    fn insert(&mut self, key: MemoryAddress, value: T) {
        self.bucket_mut(key).slots.insert(key.offset, value);
    }

    /// Drops every entry a `size`-byte write at `write` may alias.
    ///
    /// Stored entries are 32 bytes wide, matching the `mstore` slots this map
    /// tracks.
    fn invalidate(&mut self, write: MemoryAddress, size: u64) {
        let write_site = Self::alloc_site(write.base);
        if let Some(site) = write_site {
            // A write into one allocation site cannot reach another, so only
            // this site's groups and the non-allocation groups can be affected.
            if let Some(group) = self.by_alloc.get_mut(&site) {
                group.retain(|bucket| Self::invalidate_bucket(bucket, write, size));
                if group.is_empty() {
                    self.by_alloc.remove(&site);
                }
            }
        } else {
            self.by_alloc.retain(|_, group| {
                group.retain(|bucket| Self::invalidate_bucket(bucket, write, size));
                !group.is_empty()
            });
        }
        self.unindexed.retain_mut(|bucket| Self::invalidate_bucket(bucket, write, size));
    }

    /// Applies a write to one group, returning whether the group survives.
    ///
    /// The group shares a base, so the allocation rules of
    /// [`AliasAnalysis::memory_alias_locations`] settle the whole group at once;
    /// only a write onto that same base reaches the offset comparison.
    fn invalidate_bucket(bucket: &mut SlotBucket<T>, write: MemoryAddress, size: u64) -> bool {
        let bucket_site = Self::alloc_site(bucket.base);
        let write_site = Self::alloc_site(write.base);
        if let (Some(bucket_site), Some(write_site)) = (bucket_site, write_site) {
            // Distinct allocation sites are disjoint; the same site is only
            // separable by offset when neither access is a loop instance.
            if bucket_site != write_site {
                return true;
            }
            if matches!(bucket.base, MemoryBase::DynamicAllocation(_))
                || matches!(write.base, MemoryBase::DynamicAllocation(_))
            {
                bucket.slots.clear();
                return false;
            }
        } else if bucket_site.is_some() || write_site.is_some() {
            // A loop-instance allocation may alias any other base.
            if matches!(bucket.base, MemoryBase::DynamicAllocation(_))
                || matches!(write.base, MemoryBase::DynamicAllocation(_))
            {
                bucket.slots.clear();
                return false;
            }
        }

        // Different bases that neither rule separated may alias at any offset.
        if bucket.base != write.base {
            bucket.slots.clear();
            return false;
        }

        // A stored slot `[offset, offset + 32)` overlaps `[write, write + size)`
        // exactly when `write - 32 < offset < write + size`.
        let Some(write_end) = write.offset.checked_add(size) else {
            bucket.slots.clear();
            return false;
        };
        let first = write.offset.saturating_sub(31);
        let doomed: SmallVec<[u64; 8]> =
            bucket.slots.range(first..write_end).map(|(&offset, _)| offset).collect();
        for offset in doomed {
            bucket.slots.remove(&offset);
        }
        !bucket.slots.is_empty()
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

struct BlockScratch {
    overwritten: FxHashSet<MemoryAddress>,
    stored_values: SlotMap<ValueId>,
    stored_words: SlotMap<U256>,
    replacements: FxHashMap<ValueId, ValueId>,
    dead: DenseBitSet<InstId>,
    /// Stores that the hashes folded so far in the block leave dead, and, while pricing a
    /// candidate, those they leave dead with it.
    fold_dead: DenseBitSet<InstId>,
    fold_dead_with_candidate: DenseBitSet<InstId>,
    /// Whether `fold_dead` describes the block being folded.
    fold_dead_ready: bool,
    /// Word stores of the current block that the backward scan narrows: store, address, and byte.
    narrowed: Vec<(InstId, ValueId, TopByte)>,
}

/// The byte a word store writes to its first address, whose other 31 bytes are zero.
#[derive(Clone, Copy)]
enum TopByte {
    /// The store value is `shl byte, 248`.
    Shifted(ValueId),
    /// The store value is a constant with only its top byte set.
    Constant(U256),
}

impl BlockScratch {
    fn new(func: &Function) -> Self {
        Self {
            overwritten: FxHashSet::default(),
            stored_values: SlotMap::default(),
            stored_words: SlotMap::default(),
            replacements: FxHashMap::default(),
            dead: DenseBitSet::new_empty(func.num_insts()),
            fold_dead: DenseBitSet::new_empty(func.num_insts()),
            fold_dead_with_candidate: DenseBitSet::new_empty(func.num_insts()),
            fold_dead_ready: false,
            narrowed: Vec::new(),
        }
    }
}

impl MemoryStoreEliminator {
    /// Creates a new memory optimization pass.
    fn new(
        alias: Rc<AliasAnalysis>,
        cfg: Rc<CfgInfo>,
        target: Target,
        free_pointer: bool,
        unread_allocations: bool,
    ) -> Self {
        Self {
            cfg,
            eliminated_count: 0,
            alias,
            target,
            live_in: None,
            uses: None,
            unread_allocations,
            terminal: Arc::default(),
            returns: Arc::default(),
            free_pointer,
            read_bounds: ReadBounds::default(),
        }
    }

    fn run_with_scratch(&mut self, func: &mut Function, scratch: &mut BlockScratch) -> usize {
        self.eliminated_count = 0;
        self.live_in = None;
        self.uses = None;

        // Every rewrite needs a store or an immutable copy to act on; functions without one skip
        // the whole scan and never build the alias snapshot.
        if !has_memory_writes(func) {
            return 0;
        }

        // Reuse one provenance snapshot across fixpoint iterations: removing
        // stores keeps the allocation facts conservative, so only the
        // value-address memo is dropped per iteration.
        self.reuse_redundant_immutable_copies(func, scratch);
        self.alias.clear_cached_addresses();
        self.remove_unused_internal_frame_stores(func, scratch);
        self.read_bounds.clear();

        // Liveness runs before block-local forwarding: forwarding replaces a reloaded pointer by
        // the stored word, whose bound the alias analysis may not know.
        self.remove_dead_memory_stores(func, scratch);
        self.read_bounds.clear();

        let block_ids = func.blocks.indices();
        for block_id in block_ids {
            let before = self.eliminated_count;
            self.process_block(func, block_id, scratch);
            // An unchanged block leaves every cached proof valid, including access positions
            // in other blocks. Rebuild only after a rewrite removes or changes instructions.
            if self.eliminated_count != before {
                self.alias.clear_cached_addresses();
                self.read_bounds.clear();
            }
        }
        self.remove_dead_allocation_stores(func, scratch);
        self.remove_cross_block_equal_const_stores(func, scratch);
        self.remove_cross_block_overwrites(func, scratch);

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
    /// keeps its slots live; a read of unknown extent keeps every word from a
    /// lower bound of its address on, or all memory without one; a call keeps what its callee may
    /// read; a `return` keeps what the callers may read after their calls; and gas observations
    /// keep all memory. Each only ever keeps a store, never drops a live one.
    fn remove_dead_memory_stores(&mut self, func: &mut Function, scratch: &mut BlockScratch) {
        if !func.instructions().any(|inst_id| {
            matches!(func.inst(inst_id).kind, InstKind::Store(addr, _, PtrLocation::Memory) if self.word_aligned_const(func, addr).is_some())
        }) {
            return;
        }

        self.eliminated_count +=
            mir_utils::invalidate_unreachable_blocks(func, self.cfg.reachable());
        let live_in = self.liveness(func);

        // Collect dead stores using the stabilized live-out of each block.
        let dead = &mut scratch.dead;
        dead.clear();
        for &block_id in self.cfg.rpo() {
            let out = Self::live_out(func, block_id, &live_in);
            let mut collector = Some(&mut *dead);
            self.transfer_block(func, block_id, out, &mut collector, None);
        }

        self.eliminated_count += remove_dead(func, dead);
    }

    /// The memory live into each block, by a backward fixpoint:
    /// `live_in[b] = transfer(b, ∪ live_in[succ(b)])`.
    ///
    /// The transfer is monotone over a lattice of bounded height (`MEM_LIVE_CAP` slots and a
    /// lowering bound, topped by all memory), so a worklist seeded in postorder reaches the same
    /// least fixpoint as a full round-robin sweep while touching each block only when one of its
    /// successors actually moved. Postorder puts successors before predecessors, which is the
    /// direction information flows here.
    fn liveness(&self, func: &Function) -> IndexVec<BlockId, MemLive> {
        self.liveness_in(func, self.cfg.reachable())
    }

    /// Solves liveness in a successor-closed subset of the reachable CFG. Return summaries
    /// need only blocks after their tracked calls; earlier blocks contribute to no call site.
    fn liveness_in(
        &self,
        func: &Function,
        blocks: &DenseBitSet<BlockId>,
    ) -> IndexVec<BlockId, MemLive> {
        let cfg = &*self.cfg;
        let mut live_in = index_vec![MemLive::empty(); func.blocks.len()];
        let mut worklist = cfg
            .rpo()
            .iter()
            .rev()
            .copied()
            .filter(|&block| blocks.contains(block))
            .collect::<VecDeque<_>>();
        let mut queued = DenseBitSet::new_empty(func.blocks.len());
        for &block_id in &worklist {
            queued.insert(block_id);
        }
        while let Some(block_id) = worklist.pop_front() {
            queued.remove(block_id);
            let out = Self::live_out(func, block_id, &live_in);
            let new_in = self.transfer_block(func, block_id, out, &mut None, None);
            if live_in[block_id] != new_in {
                live_in[block_id] = new_in;
                for &pred in &func.blocks[block_id].predecessors {
                    if blocks.contains(pred) && queued.insert(pred) {
                        worklist.push_back(pred);
                    }
                }
            }
        }
        live_in
    }

    fn live_out(func: &Function, block: BlockId, live_in: &IndexVec<BlockId, MemLive>) -> MemLive {
        let mut out = MemLive::empty();
        if let Some(term) = func.blocks[block].terminator.as_ref() {
            for succ in term.successors() {
                out.join(&live_in[succ]);
            }
        }
        out
    }

    /// Runs the backward transfer over one block's terminator and instructions,
    /// returning the live set at block entry. When `dead` is `Some`, records the
    /// full-word constant stores found dead against the flowing live set. The hashes in `folded`
    /// read nothing.
    fn transfer_block(
        &self,
        func: &Function,
        block: BlockId,
        mut live: MemLive,
        dead: &mut Option<&mut DenseBitSet<InstId>>,
        folded: Option<&DenseBitSet<InstId>>,
    ) -> MemLive {
        // Terminator first: it executes after every instruction in the block.
        self.transfer_terminator(func, block, &mut live);
        for &inst_id in func.blocks[block].instructions.iter().rev() {
            if folded.is_some_and(|folded| folded.contains(inst_id))
                && matches!(func.inst(inst_id).kind, InstKind::Keccak256(..))
            {
                continue;
            }
            self.transfer_inst(func, inst_id, &mut live, dead);
        }
        live
    }

    /// Joins memory live after tracked calls into their callees, walking each block suffix once.
    /// Each block carries its tracked call count so the scan stops at its first tracked call.
    fn join_call_liveness(
        &self,
        func: &Function,
        blocks: &[(BlockId, usize)],
        live_in: &IndexVec<BlockId, MemLive>,
        returned: &mut FxHashMap<FunctionId, MemLive>,
    ) {
        for &(block, mut remaining) in blocks {
            let mut live = Self::live_out(func, block, live_in);
            self.transfer_terminator(func, block, &mut live);
            for &inst in func.blocks[block].instructions.iter().rev() {
                if let InstKind::ICall { function: Callee::Function(callee), .. } =
                    func.inst(inst).kind
                    && let Some(returned) = returned.get_mut(&callee)
                {
                    let mut after = live.clone();
                    Self::summarize_live(&mut after);
                    returned.join(&after);
                    remaining -= 1;
                    if remaining == 0 {
                        break;
                    }
                }
                self.transfer_inst(func, inst, &mut live, &mut None);
            }
            debug_assert_eq!(remaining, 0, "all tracked calls were visited");
        }
    }

    fn transfer_terminator(&self, func: &Function, block: BlockId, live: &mut MemLive) {
        match func.blocks[block].terminator.as_ref() {
            Some(Terminator::Revert { offset, size })
            | Some(Terminator::ReturnData { offset, size }) => {
                self.mark_read(func, live, *offset, *size, None);
            }
            // A `return` value may be a memory pointer the caller dereferences: keep
            // what the callers read after their calls when the module shows them all,
            // and all memory otherwise. A tail call forwards memory to its callee,
            // which keeps only what it reads first when it never returns.
            Some(Terminator::Return { .. }) => *live = self.return_liveness(func),
            Some(Terminator::TailCall { function, .. }) => *live = self.callee_liveness(*function),
            // Halts end the call and discard its memory without reading it.
            // `jump`/`branch`/`switch` read no memory; their successors already
            // contribute liveness via `live_out`.
            Some(Terminator::RevertReturndata)
            | Some(Terminator::Stop)
            | Some(Terminator::Invalid)
            | Some(Terminator::SelfDestruct { .. })
            | Some(Terminator::Jump(_))
            | Some(Terminator::Branch { .. })
            | Some(Terminator::Switch { .. })
            | None => {}
        }
    }

    fn transfer_inst(
        &self,
        func: &Function,
        inst_id: InstId,
        live: &mut MemLive,
        dead: &mut Option<&mut DenseBitSet<InstId>>,
    ) {
        // No ordinary transfer can refine top, which has no exact ranges: constant kills
        // leave its open range intact. A known non-returning call is the exception, since
        // its callee replaces the suffix.
        if live.is_all()
            && !matches!(func.inst(inst_id).kind,
                InstKind::ICall { function: Callee::Function(function), .. }
                if self.terminal.contains_key(&function))
        {
            return;
        }
        // A value defined in a loop can name different addresses on successive iterations.
        // Forget exact identities before crossing their definitions, including phis.
        if let Some(value) = func.inst_result_value(inst_id)
            && live.ranges.iter().any(|&(offset, size)| value == offset || value == size)
        {
            Self::summarize_live(live);
        }
        // Pure definitions can end an exact range's lifetime, but do not otherwise transfer
        // memory liveness. `gas` is not pure, so it still reaches the effect transfer below.
        if func.inst(inst_id).kind.effect_kind() == EffectKind::Pure {
            return;
        }
        // Transfer writes before reads, as a memory copy reads its source before writing its
        // destination. Word stores first test their own liveness before killing the range.
        if !matches!(func.inst(inst_id).kind, InstKind::Store(.., PtrLocation::Memory)) {
            self.kill_written_ranges(func, inst_id, live);
        }
        match &func.inst(inst_id).kind {
            InstKind::Store(addr, _, PtrLocation::Memory) => {
                if let Some(slot) = self.word_aligned_const(func, *addr) {
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
            InstKind::Load(addr, PtrLocation::Memory) => match self.word_aligned_const(func, *addr)
            {
                Some(slot) => live.add_addr(slot),
                None => self.mark_from(func, live, *addr, Some(32), Some(inst_id)),
            },
            InstKind::Fmp | InstKind::Alloc { .. } => live.add_addr(EvmMemoryLayout::FMP_SLOT),
            InstKind::SetFmp(_) => live.kill(EvmMemoryLayout::FMP_SLOT),
            InstKind::Keccak256(offset, size)
            | InstKind::Log { offset, size, .. }
            | InstKind::Copy(_, offset, size, PtrLocation::Memory) => {
                // Keep the original address: canonical offsets discard local no-wrap proofs.
                self.mark_read(func, live, *offset, *size, Some(inst_id));
            }
            // Byte stores never fully define a word (so cannot make an
            // earlier store dead) and read nothing: leave the set as is.
            InstKind::Store8(..) => {}
            // A call to a function that never returns leaves only what that
            // function reads first.
            InstKind::ICall { function: Callee::Function(function), .. }
                if self.terminal.contains_key(function) =>
            {
                *live = self.callee_liveness(*function);
            }
            _ => self.mark_effects(func, inst_id, live),
        }
        if matches!(func.inst(inst_id).kind, InstKind::Store(.., PtrLocation::Memory)) {
            self.kill_written_ranges(func, inst_id, live);
        }
    }

    /// The memory a call or tail call to `function` keeps live: what it reads first when it never
    /// returns, and all memory otherwise.
    fn callee_liveness(&self, function: FunctionId) -> MemLive {
        self.terminal.get(&function).cloned().unwrap_or_else(MemLive::all)
    }

    /// The memory the callers of `func` may read once it returns: what they keep live right
    /// after their calls when the module shows every call site, and all memory otherwise.
    fn return_liveness(&self, func: &Function) -> MemLive {
        self.returns.live.get(&func.name).cloned().unwrap_or_else(MemLive::all)
    }

    /// The least address a read through `value` can reach; see
    /// [`AliasAnalysis::read_lower_bound`].
    fn read_bound(&self, func: &Function, value: ValueId) -> Option<u64> {
        self.read_bounds.read_lower_bound(
            func,
            value,
            &|function| self.returns.bounds.get(&function).copied(),
            self.free_pointer,
        )
    }

    /// Marks the memory an instruction may read live: a constant range word by word, and any
    /// other from a lower bound of its address on. Writes keep nothing live, so a later store
    /// that fully overwrites a word still finds an earlier one dead. Reading the gas left observes
    /// all memory.
    fn mark_effects(&self, func: &Function, inst_id: InstId, live: &mut MemLive) {
        let effects = self.alias.instruction_mod_ref(func, inst_id);
        if effects.observes_gas() {
            live.set_all();
            return;
        }
        for &access in effects.reads() {
            match access {
                Access::Any(AddressSpace::Memory) => {
                    live.set_all();
                    return;
                }
                Access::Location(Location::Memory(location)) => {
                    self.mark_location(func, live, location);
                }
                Access::Any(_) | Access::Location(_) => {}
            }
        }
    }

    /// Marks a memory location live: a constant range word by word, and any other from its
    /// start on when a compiler-owned base bounds that start from below. Internal frames and
    /// allocations lie above the reserved words; see [`AliasAnalysis::frame_lower_bound`]. A
    /// location of unknown extent may come from a runtime index, which a forged length can wrap
    /// around to low memory, so it keeps all memory live.
    fn mark_location(&self, func: &Function, live: &mut MemLive, location: MemoryLocation) {
        let offset = location.address.offset;
        if location.size == LocationSize::Unknown {
            live.set_all();
            return;
        }
        let bound = match location.address.base {
            MemoryBase::Absolute => {
                if let LocationSize::Const(size) = location.size {
                    Self::mark_range(live, offset, size);
                    return;
                }
                Some(offset)
            }
            MemoryBase::InternalFrame => Some(AliasAnalysis::frame_lower_bound(offset)),
            MemoryBase::Allocation(_)
            | MemoryBase::UnboundedAllocation(_)
            | MemoryBase::DynamicAllocation(_) => EvmMemoryLayout::HEAP_START.checked_add(offset),
            MemoryBase::FreeMemory(read) => func
                .inst_result_value(read)
                .filter(|&value| self.read_bounds.constant_offset_no_wrap(func, value, offset))
                .and_then(|value| self.read_bound(func, value))
                .and_then(|bound| bound.checked_add(offset)),
            MemoryBase::Value(value) => self
                .read_bounds
                .constant_offset_no_wrap(func, value, offset)
                .then(|| self.read_bound(func, value)?.checked_add(offset))
                .flatten(),
        };
        match bound {
            Some(bound) => live.add_from(bound),
            None => live.set_all(),
        }
    }

    /// Marks every word from a read's proven lower bound on live. An unproven constant addition
    /// keeps both possible ranges live; an address or extent without useful bounds keeps all.
    fn mark_from(
        &self,
        func: &Function,
        live: &mut MemLive,
        address: ValueId,
        size: Option<u64>,
        user: Option<InstId>,
    ) {
        let bound = match user {
            Some(user) => self.read_bounds.read_lower_bound_at(
                func,
                address,
                &|function| self.returns.bounds.get(&function).copied(),
                self.free_pointer,
                user,
            ),
            None => self.read_bound(func, address),
        };
        match bound {
            Some(bound) => live.add_from(bound),
            None => {
                if !self.mark_wrapping_read(func, live, address, size) {
                    live.set_all();
                }
            }
        }
    }

    /// Marks the word-aligned slots a memory read `[offset, offset + size)` may
    /// observe as live: those of a constant range, and for any other every word
    /// from a lower bound of `offset` on.
    fn mark_read(
        &self,
        func: &Function,
        live: &mut MemLive,
        offset: ValueId,
        size: ValueId,
        user: Option<InstId>,
    ) {
        if live.is_all() || func.value_u64(size) == Some(0) {
            return;
        }
        match (func.value_u64(offset), func.value_u64(size)) {
            (Some(offset), Some(size)) => Self::mark_range(live, offset, size),
            _ => {
                let mut read = MemLive::empty();
                self.mark_from(func, &mut read, offset, func.value_u64(size), user);
                if read.is_all() {
                    live.add_range(AliasAnalysis::memory_address_value(func, offset), size);
                } else {
                    live.join(&read);
                }
            }
        }
    }

    /// Marks the word-aligned slots of the constant range `[offset, offset + size)`
    /// live; an oversized range keeps every word from its start on.
    fn mark_range(live: &mut MemLive, offset: u64, size: u64) {
        if size == 0 {
            return;
        }
        let first = (offset / 32) * 32;
        // Bound the walk; a huge read keeps every word from its start on.
        match offset.checked_add(size) {
            Some(end) if end - first <= 32 * 256 => {
                // `step_by` stops before the word cursor can wrap at the address-space end.
                for word in (first..end).step_by(32) {
                    live.add_addr(word);
                }
            }
            _ => live.add_from(first),
        }
    }

    /// Returns a constant, 32-byte-aligned memory address, or `None` otherwise.
    fn word_aligned_const(&self, func: &Function, addr: ValueId) -> Option<u64> {
        self.mem_addr_key(func, addr, 32)?.as_absolute().filter(|address| address % 32 == 0)
    }

    /// Runs local memory optimization until no more instructions can be eliminated.
    fn run_to_fixpoint(&mut self, func: &mut Function) -> usize {
        let mut total = 0;
        let mut scratch = BlockScratch::new(func);
        loop {
            let mut eliminated = self.run_with_scratch(func, &mut scratch);
            // Its proof scans the whole function, so it runs once the cheaper rewrites settle.
            if eliminated == 0 && self.unread_allocations {
                eliminated = self.remove_write_only_allocation_writes(func, &mut scratch);
            }
            if eliminated == 0 {
                break;
            }
            self.alias.clear_cached_addresses();
            total += eliminated;
        }
        total
    }

    fn reuse_redundant_immutable_copies(
        &mut self,
        func: &mut Function,
        scratch: &mut BlockScratch,
    ) {
        let has_candidate = func.blocks.iter().any(|block| {
            block.instructions.windows(2).any(|window| {
                matches!(func.inst(window[0]).kind, InstKind::CodeCopy(_, _, _))
                    && matches!(func.inst(window[1]).kind, InstKind::Load(_, PtrLocation::Memory))
            })
        });
        if !has_candidate {
            return;
        }

        let cfg = &*self.cfg;
        let mut cached: FxHashMap<ImmutableCopyKey, CachedImmutableCopy> = FxHashMap::default();
        let replacements = &mut scratch.replacements;
        replacements.clear();
        let dead = &mut scratch.dead;
        dead.clear();

        let block_ids = func.blocks.indices();
        for block_id in block_ids {
            let insts = func.blocks[block_id].instructions.clone();
            for (index, window) in insts.windows(2).enumerate() {
                let codecopy = window[0];
                let load = window[1];
                let InstKind::CodeCopy(dest, src, size) = func.inst(codecopy).kind else {
                    continue;
                };
                if func.value_u64(size) != Some(32) {
                    continue;
                }
                let Some(key) = Self::immutable_copy_key(func, src) else {
                    continue;
                };
                let InstKind::Load(load_addr, PtrLocation::Memory) = func.inst(load).kind else {
                    continue;
                };
                if self.mem_addr_key(func, dest, 32) != self.mem_addr_key(func, load_addr, 32) {
                    continue;
                }
                let Some(loaded_value) = func.inst_result_value(load) else {
                    continue;
                };

                if let Some(cached_copy) = cached.get(&key).copied()
                    && Self::copy_dominates(cfg.dominators(), cached_copy, block_id, index)
                {
                    replacements.insert(loaded_value, cached_copy.value);
                    func.inst_mut(codecopy).kind =
                        InstKind::Store(dest, cached_copy.value, PtrLocation::Memory);
                    dead.insert(load);
                    self.eliminated_count += 1;
                } else {
                    cached.insert(
                        key,
                        CachedImmutableCopy { block: block_id, index, value: loaded_value },
                    );
                }
            }
        }

        func.replace_uses_canonicalized(replacements);
        remove_dead(func, dead);
    }

    fn remove_unused_internal_frame_stores(
        &mut self,
        func: &mut Function,
        scratch: &mut BlockScratch,
    ) {
        let has_candidate = func.instructions().any(|inst_id| {
            matches!(func.inst(inst_id).kind, InstKind::Store(addr, _, PtrLocation::Memory) if self.internal_frame_offset(func, addr).is_some())
        });
        if !has_candidate {
            return;
        }
        if Self::has_frame_observer(func) {
            return;
        }

        let Some(reads) = self.internal_frame_read_ranges(func) else {
            return;
        };
        let dead = &mut scratch.dead;
        dead.clear();

        for inst_id in func.instructions() {
            let InstKind::Store(addr, _, PtrLocation::Memory) = func.inst(inst_id).kind else {
                continue;
            };
            let Some(offset) = self.internal_frame_offset(func, addr) else {
                continue;
            };
            if !reads.iter().any(|&(read_offset, read_size)| {
                self.alias
                    .memory_alias(
                        func,
                        MemoryLocation::new(
                            MemoryAddress::internal_frame(offset),
                            LocationSize::Const(32),
                        ),
                        MemoryLocation::new(
                            MemoryAddress::internal_frame(read_offset),
                            LocationSize::Const(read_size),
                        ),
                    )
                    .may_alias()
            }) {
                dead.insert(inst_id);
            }
        }

        self.eliminated_count += remove_dead(func, dead);
    }

    fn process_block(
        &mut self,
        func: &mut Function,
        block_id: BlockId,
        scratch: &mut BlockScratch,
    ) {
        let mut mstores = 0;
        let mut memory_writes = 0;
        let mut has_load = false;
        let mut has_keccak = false;
        for &inst_id in &func.blocks[block_id].instructions {
            if func.inst(inst_id).kind.effect_kind() == EffectKind::Pure {
                continue;
            }
            match func.inst(inst_id).kind {
                InstKind::Store(.., PtrLocation::Memory) => {
                    mstores += 1;
                    memory_writes += 1;
                }
                InstKind::Load(_, PtrLocation::Memory) => has_load = true,
                InstKind::Keccak256(_, _) => has_keccak = true,
                _ if self.can_mutate_memory(func, inst_id) => {
                    memory_writes += 1;
                }
                _ => {}
            }
        }

        if has_keccak && mstores != 0 {
            self.fold_constant_keccak(func, block_id, scratch);
        }
        if has_load && mstores != 0 {
            self.forward_loads(func, block_id, scratch);
        }
        if mstores >= 2 {
            self.remove_equal_stores(func, block_id, scratch);
        }
        if memory_writes < 2 {
            return;
        }

        scratch.overwritten.clear();
        scratch.dead.clear();

        for &inst_id in func.blocks[block_id].instructions.iter().rev() {
            let inst = func.inst(inst_id);
            match &inst.kind {
                InstKind::Store(addr, value, PtrLocation::Memory) => {
                    if let Some(key) = self.mem_addr_key(func, *addr, 32) {
                        if scratch.overwritten.contains(&key) {
                            scratch.dead.insert(inst_id);
                            self.eliminated_count += 1;
                        } else {
                            // The store writes byte `key`, and a later store overwrites the
                            // other 31 bytes before any read.
                            if key.checked_add(1).is_some_and(|tail| {
                                Self::contains_overwritten_word(&scratch.overwritten, tail)
                            }) && let Some(byte) = Self::top_byte(func, *value)
                            {
                                scratch.narrowed.push((inst_id, *addr, byte));
                            }
                            scratch.overwritten.insert(key);
                        }
                    } else {
                        scratch.overwritten.clear();
                    }
                }
                InstKind::Load(addr, PtrLocation::Memory) => {
                    if let Some(key) = self.mem_addr_key(func, *addr, 32) {
                        Self::remove_overlapping_set(&mut scratch.overwritten, key);
                    } else {
                        scratch.overwritten.clear();
                    }
                }
                InstKind::Fmp | InstKind::Alloc { .. } => {
                    Self::remove_overlapping_set(
                        &mut scratch.overwritten,
                        AliasAnalysis::fmp_location().address,
                    );
                }
                InstKind::SetFmp(_) => {
                    scratch.overwritten.insert(AliasAnalysis::fmp_location().address);
                }
                InstKind::MemoryZero(dest, size)
                | InstKind::Copy(dest, _, size, PtrLocation::Calldata | PtrLocation::Returndata)
                | InstKind::DataCopy(_, dest, size)
                | InstKind::CodeCopy(dest, _, size)
                | InstKind::ExtCodeCopy(_, dest, _, size) => {
                    if self.insert_full_word_overwritten_range(
                        func,
                        &mut scratch.overwritten,
                        *dest,
                        *size,
                    ) && matches!(
                        inst.kind,
                        InstKind::MemoryZero(..) | InstKind::Copy(.., PtrLocation::Calldata)
                    ) {
                        scratch.dead.insert(inst_id);
                        self.eliminated_count += 1;
                    }
                }
                InstKind::Call { args_offset, args_size, .. }
                | InstKind::CallCode { args_offset, args_size, .. }
                | InstKind::StaticCall { args_offset, args_size, .. }
                | InstKind::DelegateCall { args_offset, args_size, .. } => {
                    // A call reads its whole input, but it only copies
                    // `min(ret_size, returndatasize())` bytes into the output
                    // area. The rest remains unchanged, so the output cannot
                    // prove that an earlier store is dead.
                    self.retain_overwritten_disjoint_from_read(
                        func,
                        &mut scratch.overwritten,
                        *args_offset,
                        *args_size,
                    );
                }
                // Keccak and logs only *read* memory. A read over a constant
                // range observes only the stores that fall in it, so a later
                // overwrite of a disjoint slot still kills its earlier store.
                // Modelling the range (instead of clearing) lets a return-value
                // slot's dead default-init survive the mapping-hash keccaks and
                // event logs that sit between it and its real store.
                kind if let Some((offset, size)) = Self::constant_range_read(kind) => {
                    self.retain_overwritten_disjoint_from_read(
                        func,
                        &mut scratch.overwritten,
                        offset,
                        size,
                    );
                }
                _ => self.apply_memory_effects(func, inst_id, &mut scratch.overwritten),
            }
        }

        for (inst, addr, byte) in scratch.narrowed.drain(..) {
            let byte = match byte {
                TopByte::Shifted(byte) => byte,
                TopByte::Constant(byte) => {
                    func.alloc_value(Value::Immediate(Immediate::I256(byte)))
                }
            };
            // mstore addr, byte << 248 -> mstore8 addr, byte
            func.inst_mut(inst).replace_kind(InstKind::Store8(addr, byte));
            self.eliminated_count += 1;
        }
        if scratch.dead.is_empty() {
            return;
        }

        func.blocks[block_id].instructions.retain(|&id| !scratch.dead.contains(id));
    }

    /// Returns the byte a word store of `value` writes first when the other bytes are zero:
    /// `shl byte, 248` or a nonzero constant with only its top byte set.
    fn top_byte(func: &Function, value: ValueId) -> Option<TopByte> {
        const TOP_BYTE_SHIFT: usize = 248;
        if let Some(word) = func.value_u256(value) {
            return (!word.is_zero() && word.trailing_zeros() >= TOP_BYTE_SHIFT)
                .then(|| TopByte::Constant(word >> TOP_BYTE_SHIFT));
        }
        let Value::Inst(shl) = func.value(value) else { return None };
        let InstKind::Shl(shift, byte) = func.inst(*shl).kind else { return None };
        (func.value_u256(shift) == Some(U256::from(TOP_BYTE_SHIFT))
            && func.value_ty(byte) == Some(MirType::I256))
        .then_some(TopByte::Shifted(byte))
    }

    fn apply_memory_effects(
        &self,
        func: &Function,
        inst_id: InstId,
        overwritten: &mut FxHashSet<MemoryAddress>,
    ) {
        if func.inst(inst_id).kind.effect_kind() == EffectKind::Pure {
            return;
        }
        let effects = self.alias.instruction_mod_ref(func, inst_id);
        if effects.observes_gas()
            || effects.reads_anywhere(AddressSpace::Memory)
            || effects.writes_anywhere(AddressSpace::Memory)
        {
            overwritten.clear();
            return;
        }

        // ModRef describes possible writes, not definite overwrites. Only
        // the unconditional writes handled by process_block can kill stores.
        for &access in effects.reads() {
            if let Access::Location(Location::Memory(location)) = access {
                overwritten.retain(|key| {
                    !self
                        .alias
                        .memory_alias(
                            func,
                            MemoryLocation::new(*key, LocationSize::Const(32)),
                            location,
                        )
                        .may_alias()
                });
            }
        }
    }

    fn constant_range_read(kind: &InstKind) -> Option<(ValueId, ValueId)> {
        match kind {
            InstKind::Keccak256(offset, size) | InstKind::Log { offset, size, .. } => {
                Some((*offset, *size))
            }
            _ => None,
        }
    }

    /// Keeps only the overwritten slots a constant-range memory read cannot
    /// observe. A slot provably outside `[offset, offset + size)` survives; a
    /// non-constant range or a symbolic slot is assumed observed (dropped),
    /// which only ever keeps a store alive — never eliminates a live one.
    fn retain_overwritten_disjoint_from_read(
        &self,
        func: &Function,
        overwritten: &mut FxHashSet<MemoryAddress>,
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
        overwritten.retain(|key| {
            key.as_absolute().is_some_and(|_| {
                !self
                    .alias
                    .memory_alias(
                        func,
                        MemoryLocation::new(*key, LocationSize::Const(32)),
                        MemoryLocation::new(
                            MemoryAddress::absolute(read_offset),
                            LocationSize::Const(read_size),
                        ),
                    )
                    .may_alias()
            })
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
    fn remove_cross_block_equal_const_stores(
        &mut self,
        func: &mut Function,
        scratch: &mut BlockScratch,
    ) {
        if !has_two_memory_stores(func) {
            return;
        }

        let const_store = |func: &Function, addr: ValueId, value: ValueId| {
            func.value_u64(addr).zip(func.value_u256(value))
        };
        let invalidate_overlapping_words = |known: &mut FxHashMap<u64, U256>, address: u64| {
            known.retain(|known_address, _| {
                known_address.abs_diff(address) >= EvmMemoryLayout::WORD_SIZE
            });
        };

        let mut exit: FxHashMap<BlockId, FxHashMap<u64, U256>> = FxHashMap::default();
        let dead = &mut scratch.dead;
        dead.clear();

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
                match &func.inst(inst_id).kind {
                    InstKind::Store(addr, value, PtrLocation::Memory) => {
                        match const_store(func, *addr, *value) {
                            Some((a, v)) => {
                                if known.get(&a) == Some(&v) {
                                    dead.insert(inst_id);
                                    self.eliminated_count += 1;
                                } else {
                                    // store a, old; store b, value; store a, old
                                    // => retain the restore when the words at a and b overlap
                                    invalidate_overlapping_words(&mut known, a);
                                    known.insert(a, v);
                                }
                            }
                            None => {
                                match self
                                    .mem_addr_key(func, *addr, 32)
                                    .and_then(|key| key.as_absolute())
                                {
                                    // A non-constant word written at a known address
                                    // invalidates every remembered overlapping word.
                                    Some(a) => {
                                        invalidate_overlapping_words(&mut known, a);
                                    }
                                    // An address we cannot pin could alias anything.
                                    _ => known.clear(),
                                }
                            }
                        }
                    }
                    // A byte store invalidates the remembered words that contain its byte.
                    InstKind::Store8(addr, _) => {
                        match self.mem_addr_key(func, *addr, 1).and_then(|key| key.as_absolute()) {
                            Some(a) => known.retain(|&word, _| {
                                a < word || a - word >= EvmMemoryLayout::WORD_SIZE
                            }),
                            None => known.clear(),
                        }
                    }
                    _ if self.can_mutate_memory(func, inst_id) => known.clear(),
                    // Loads and keccak read memory but never write it.
                    _ => {}
                }
            }

            exit.insert(block_id, known);
        }

        remove_dead(func, dead);
    }

    fn remove_cross_block_overwrites(&mut self, func: &mut Function, scratch: &mut BlockScratch) {
        if !has_two_memory_stores(func) {
            return;
        }

        let dead = &mut scratch.dead;
        dead.clear();

        for pred in func.blocks.indices() {
            let Some(Terminator::Jump(succ)) = func.blocks[pred].terminator else { continue };
            if func.blocks[succ].predecessors.as_slice() != [pred] {
                continue;
            }

            let pred_insts = func.blocks[pred].instructions.iter().rev();
            if let Some((store, key)) = self.first_cross_block_store(func, pred_insts)
                && self
                    .first_cross_block_store(func, func.blocks[succ].instructions.iter())
                    .is_some_and(|(_, succ_key)| succ_key == key)
            {
                dead.insert(store);
            }
        }

        self.eliminated_count += remove_dead(func, dead);
    }

    /// The first full-word memory store reached from one end of a block, with its address, unless
    /// a memory or gas observer comes first.
    fn first_cross_block_store<'a>(
        &self,
        func: &Function,
        mut insts: impl Iterator<Item = &'a InstId>,
    ) -> Option<(InstId, MemoryAddress)> {
        insts.find_map(|&inst_id| match func.inst(inst_id).kind {
            InstKind::Store(addr, _, PtrLocation::Memory) => {
                Some(self.mem_addr_key(func, addr, 32).map(|key| (inst_id, key)))
            }
            _ => {
                let effects = self.alias.instruction_mod_ref(func, inst_id);
                (effects.reads_space(AddressSpace::Memory)
                    || effects.writes_space(AddressSpace::Memory)
                    || effects.observes_gas())
                .then_some(None)
            }
        })?
    }

    fn fold_constant_keccak(
        &mut self,
        func: &mut Function,
        block_id: BlockId,
        scratch: &mut BlockScratch,
    ) {
        scratch.stored_words.clear();
        scratch.replacements.clear();
        scratch.dead.clear();
        scratch.fold_dead_ready = false;

        for index in 0..func.blocks[block_id].instructions.len() {
            let inst_id = func.blocks[block_id].instructions[index];
            match &func.inst(inst_id).kind {
                InstKind::Store(addr, value, PtrLocation::Memory) => {
                    let Some(key) = self.mem_addr_key(func, *addr, 32) else {
                        scratch.stored_words.clear();
                        continue;
                    };
                    scratch.stored_words.invalidate(key, 32);
                    if let Some(value) = func.value_u256(*value) {
                        scratch.stored_words.insert(key, value);
                    }
                }
                InstKind::Store8(addr, _) => {
                    if let Some(key) = self.mem_addr_key(func, *addr, 1) {
                        scratch.stored_words.invalidate(key, 1);
                    } else {
                        scratch.stored_words.clear();
                    }
                }
                InstKind::Keccak256(offset, size) => {
                    let Some(bytes) =
                        Self::constant_memory_bytes(func, &scratch.stored_words, *offset, *size)
                    else {
                        continue;
                    };
                    let Some(result) = func.inst_result_value(inst_id) else {
                        continue;
                    };
                    let hash = U256::from_be_bytes(keccak256(&bytes).0);
                    if !self.folds_keccak(func, block_id, inst_id, result, hash, scratch) {
                        continue;
                    }
                    let replacement = func.alloc_value(Value::Immediate(Immediate::I256(hash)));
                    scratch.replacements.insert(result, replacement);
                    scratch.dead.insert(inst_id);
                    self.eliminated_count += 1;
                }
                _ if self.can_mutate_memory(func, inst_id) => {
                    scratch.stored_words.clear();
                }
                _ => {}
            }
        }

        if scratch.dead.is_empty() {
            return;
        }

        func.replace_uses_canonicalized(&scratch.replacements);
        func.blocks[block_id].instructions.retain(|&id| !scratch.dead.contains(id));
    }

    /// Whether the hash `keccak` computes, which equals the constant `hash`, is cheaper as an
    /// immediate. The stores it leaves dead are credited: the stores that no longer have a reader
    /// once `keccak` and the hashes already folded in the block go. The stores the folded hashes
    /// leave dead are scanned once per block and again with each candidate.
    fn folds_keccak(
        &mut self,
        func: &Function,
        block: BlockId,
        keccak: InstId,
        result: ValueId,
        hash: U256,
        scratch: &mut BlockScratch,
    ) -> bool {
        let live_in = self.live_in.take().unwrap_or_else(|| self.liveness(func));
        let out = Self::live_out(func, block, &live_in);
        self.live_in = Some(live_in);
        if !scratch.fold_dead_ready {
            scratch.fold_dead.clear();
            let dead = &mut Some(&mut scratch.fold_dead);
            self.transfer_block(func, block, out.clone(), dead, Some(&scratch.dead));
            scratch.fold_dead_ready = true;
        }
        scratch.dead.insert(keccak);
        scratch.fold_dead_with_candidate.clear();
        let dead = &mut Some(&mut scratch.fold_dead_with_candidate);
        self.transfer_block(func, block, out, dead, Some(&scratch.dead));
        scratch.dead.remove(keccak);
        let stores = scratch
            .fold_dead_with_candidate
            .iter()
            .filter(|&store| !scratch.fold_dead.contains(store))
            .map(|store| self.target.instruction_estimate(func, store))
            .sum();
        let keccak_cost = self.target.instruction_estimate(func, keccak);
        let uses = self.uses.get_or_insert_with(|| super::egraph::use_counts(func))[result];
        let in_loop = self.cfg.cyclic_blocks().contains(block);
        let folds = self.target.folds_constant_keccak(hash, keccak_cost, stores, uses, in_loop);
        if folds {
            mem::swap(&mut scratch.fold_dead, &mut scratch.fold_dead_with_candidate);
        }
        folds
    }

    fn remove_equal_stores(
        &mut self,
        func: &mut Function,
        block_id: BlockId,
        scratch: &mut BlockScratch,
    ) {
        scratch.stored_values.clear();
        scratch.dead.clear();

        for &inst_id in &func.blocks[block_id].instructions {
            let inst = func.inst(inst_id);
            match &inst.kind {
                InstKind::Store(addr, value, PtrLocation::Memory) => {
                    let Some(key) = self.mem_addr_key(func, *addr, 32) else {
                        scratch.stored_values.clear();
                        continue;
                    };

                    if scratch.stored_values.get(key).is_some_and(|&stored| stored == *value) {
                        scratch.dead.insert(inst_id);
                        self.eliminated_count += 1;
                        continue;
                    }

                    scratch.stored_values.invalidate(key, 32);
                    scratch.stored_values.insert(key, *value);
                }
                _ if self.can_mutate_memory(func, inst_id) => {
                    scratch.stored_values.clear();
                }
                _ => {}
            }
        }

        if scratch.dead.is_empty() {
            return;
        }

        func.blocks[block_id].instructions.retain(|&id| !scratch.dead.contains(id));
    }

    fn forward_loads(
        &mut self,
        func: &mut Function,
        block_id: BlockId,
        scratch: &mut BlockScratch,
    ) {
        scratch.stored_values.clear();
        scratch.replacements.clear();
        scratch.dead.clear();

        // The free-memory-pointer word lives outside the slot map: a write through a heap pointer
        // cannot reach it, so only a write that may touch it drops the word.
        let fmp_key = AliasAnalysis::fmp_location().address;
        let mut fmp_word = None;

        for &inst_id in &func.blocks[block_id].instructions {
            let inst = func.inst(inst_id);
            if inst.kind.effect_kind() == EffectKind::Pure {
                continue;
            }
            if fmp_word.is_some()
                && self.alias.instruction_may_write_fmp(func, inst_id, &self.read_bounds)
            {
                fmp_word = None;
            }
            match &inst.kind {
                InstKind::Store(addr, value, PtrLocation::Memory) => {
                    if let Some(key) = self.mem_addr_key(func, *addr, 32) {
                        scratch.stored_values.invalidate(key, 32);
                        let value = mir_utils::resolve_replacement(*value, &scratch.replacements);
                        if key == fmp_key {
                            fmp_word = Some(value);
                        } else {
                            scratch.stored_values.insert(key, value);
                        }
                    } else {
                        scratch.stored_values.clear();
                    }
                }
                InstKind::Load(addr, PtrLocation::Memory) => {
                    let Some(key) = self.mem_addr_key(func, *addr, 32) else {
                        continue;
                    };
                    let stored_value = if key == fmp_key {
                        fmp_word
                    } else {
                        scratch.stored_values.get(key).copied()
                    };
                    let Some(stored_value) = stored_value else {
                        continue;
                    };
                    // A pointer load of a stored word keeps its own type.
                    if let Some(loaded_value) = func.inst_result_value(inst_id)
                        && func.value_ty(loaded_value) == func.value_ty(stored_value)
                    {
                        scratch.replacements.insert(loaded_value, stored_value);
                        scratch.dead.insert(inst_id);
                    }
                }
                InstKind::MemoryZero(dest, size)
                | InstKind::Copy(dest, _, size, PtrLocation::Calldata | PtrLocation::Returndata)
                | InstKind::DataCopy(_, dest, size)
                | InstKind::CodeCopy(dest, _, size)
                | InstKind::ExtCodeCopy(_, dest, _, size) => {
                    let Some(size) = func.value_u64(*size) else {
                        scratch.stored_values.clear();
                        continue;
                    };
                    if let Some(write) = self.mem_addr_key(func, *dest, size) {
                        scratch.stored_values.invalidate(write, size);
                    } else {
                        scratch.stored_values.clear();
                    }
                }
                _ if self.can_mutate_memory(func, inst_id) => {
                    scratch.stored_values.clear();
                }
                _ => {}
            }
        }

        if scratch.dead.is_empty() {
            return;
        }

        func.replace_uses_canonicalized(&scratch.replacements);
        self.uses = None;
        self.eliminated_count += scratch.dead.count();
        func.blocks[block_id].instructions.retain(|&id| !scratch.dead.contains(id));
    }

    fn mem_addr_key(&self, func: &Function, value: ValueId, size: u64) -> Option<MemoryAddress> {
        self.alias
            .bare_memory_location(func, value, LocationSize::Const(size))
            .map(|location| location.address)
    }

    fn immutable_copy_key(func: &Function, src: ValueId) -> Option<ImmutableCopyKey> {
        match func.value(src) {
            Value::Inst(inst_id) => match func.inst(*inst_id).kind {
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
        matches!(func.value(value), Value::Inst(inst_id) if matches!(func.inst(*inst_id).kind, InstKind::CodeSize))
    }

    fn copy_dominates(
        dominators: &crate::mir::analysis::DominatorTree,
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
        stored_words: &SlotMap<U256>,
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
            let word = stored_words.get(MemoryAddress::absolute(addr))?;
            bytes.extend_from_slice(&word.to_be_bytes::<32>());
        }
        Some(bytes)
    }

    fn remove_overlapping_set(set: &mut FxHashSet<MemoryAddress>, key: MemoryAddress) {
        set.retain(|&stored| {
            !AliasAnalysis::memory_alias_locations(
                MemoryLocation::new(stored, LocationSize::Const(32)),
                MemoryLocation::new(key, LocationSize::Const(32)),
            )
            .may_alias()
        });
    }

    /// Marks the words of a constant range of whole words overwritten, and returns whether the
    /// range is nonempty and every word was already overwritten. Any other range clears the set.
    fn insert_full_word_overwritten_range(
        &self,
        func: &Function,
        overwritten: &mut FxHashSet<MemoryAddress>,
        dest: ValueId,
        size: ValueId,
    ) -> bool {
        func.value_u64(size)
            .filter(|&size| size % 32 == 0 && size <= 4096)
            .and_then(|size| {
                let base = self.mem_addr_key(func, dest, size)?;
                let mut fully_overwritten = size != 0;
                for offset in (0..size).step_by(32) {
                    fully_overwritten &= !overwritten.insert(base.checked_add(offset)?);
                }
                Some(fully_overwritten)
            })
            .unwrap_or_else(|| {
                overwritten.clear();
                false
            })
    }

    /// Finds an exact overwritten word, including when the access crosses its allocation's
    /// end. Losing allocation disjointness does not change that allocation's pointer value;
    /// compare only the same allocation and offset, leaving all other alias relationships intact.
    fn contains_overwritten_word(
        overwritten: &FxHashSet<MemoryAddress>,
        mut key: MemoryAddress,
    ) -> bool {
        if overwritten.contains(&key) {
            return true;
        }
        key.base = match key.base {
            MemoryBase::Allocation(alloc) => MemoryBase::UnboundedAllocation(alloc),
            MemoryBase::UnboundedAllocation(alloc) => MemoryBase::Allocation(alloc),
            _ => return false,
        };
        overwritten.contains(&key)
    }

    fn has_frame_observer(func: &Function) -> bool {
        func.instructions().any(|inst_id| {
            func.inst(inst_id).kind.observes_gas()
                || matches!(
                    func.inst(inst_id).kind,
                    InstKind::ICall { function: Callee::Function(_), .. }
                )
        })
    }

    fn internal_frame_read_ranges(&self, func: &Function) -> Option<Vec<(u64, u64)>> {
        let mut reads = Vec::new();

        for inst_id in func.instructions() {
            Self::push_frame_reads(
                &mut reads,
                self.alias.instruction_mod_ref(func, inst_id).reads(),
            )?;
        }

        for block in func.blocks.iter() {
            if let Some(terminator) = &block.terminator {
                Self::push_frame_reads(
                    &mut reads,
                    self.alias.terminator_mod_ref(func, terminator).reads(),
                )?;
            }
        }

        Some(reads)
    }

    fn push_frame_reads(reads: &mut Vec<(u64, u64)>, accesses: &[Access]) -> Option<()> {
        for access in accesses {
            match *access {
                Access::Any(AddressSpace::Memory) => return None,
                Access::Location(Location::Memory(location)) => {
                    if let Some(offset) = location.address.as_internal_frame_offset() {
                        reads.push((offset, location.size.as_const()?));
                    } else if matches!(
                        location.address.region,
                        MemoryRegion::InternalFrame | MemoryRegion::Unknown
                    ) {
                        return None;
                    }
                }
                Access::Location(Location::Storage(_))
                | Access::Location(Location::Transient(_))
                | Access::Location(Location::Immutable(_))
                | Access::Any(AddressSpace::Storage)
                | Access::Any(AddressSpace::Transient)
                | Access::Any(AddressSpace::Immutable) => {}
            }
        }
        Some(())
    }

    fn internal_frame_offset(&self, func: &Function, value: ValueId) -> Option<u64> {
        self.alias.memory_address(func, value)?.as_internal_frame_offset()
    }

    fn can_mutate_memory(&self, func: &Function, inst_id: InstId) -> bool {
        !func.inst(inst_id).kind.effect_kind().is_read_only()
            && self.alias.instruction_mod_ref(func, inst_id).writes_space(AddressSpace::Memory)
    }

    /// Removes the stores into an allocation that no later read can observe.
    ///
    /// An `alloc` whose pointer does not escape is addressed only through the values derived from
    /// it. The derivation follows `ptrtoint`, `inttoptr`, and `add` or `ptradd` of a constant,
    /// which keep an exact offset, and `ptradd` of a runtime index, which can land anywhere in the
    /// object and so reads all of it. A derived value may address a load, store, fill, copy, hash,
    /// log, creation or call range, or the data of `revert` and `returndata`. Any other use, such
    /// as storing it as a value, passing it to an internal call, joining it in a phi, or
    /// subtracting from it, leaves the allocation untouched.
    ///
    /// A constant-offset `mstore` or `mstore8` inside the object is dead when no read that is
    /// reachable after it overlaps the bytes it writes. A range read from offset `a` with a
    /// runtime length covers the bytes from `a` on, which is how the length word of a `bytes`
    /// object goes once forwarding has replaced its loads and the call or terminator reads only
    /// the data after it. Other allocations do not overlap, so reads inside them never count,
    /// unless a free-memory-pointer reset precedes the object, which then keeps its stores.
    /// Assembly can still reach a heap object through a pointer rebuilt from the free-memory
    /// pointer, so every other read that may touch heap memory reads all of it. A `ret` or tail
    /// call reads no object whose pointer it does not receive. A read that may leave its own
    /// allocation reaches the objects allocated after it, not the ones before. A write-only
    /// deferred object, which static placement keeps unrelated to the free-memory pointer,
    /// ignores them.
    /// The allocation stays: later passes size and place it, and the address arithmetic feeding
    /// the store dies with it.
    ///
    /// Removing a write can lower memory expansion, so functions observing gas keep their stores.
    fn remove_dead_allocation_stores(&mut self, func: &mut Function, scratch: &mut BlockScratch) {
        let allocations: Vec<_> = func
            .instructions()
            .filter(|&inst| matches!(func.inst(inst).kind, InstKind::Alloc { .. }))
            .filter_map(|inst| func.inst_result_value(inst).map(|value| (inst, value)))
            .collect();
        if allocations.is_empty() {
            return;
        }
        let mut uses = Vec::new();
        let mut positions = FxHashMap::default();
        for (block_id, block) in func.blocks.iter_enumerated() {
            for (index, &inst) in block.instructions.iter().enumerate() {
                if matches!(func.inst(inst).kind, InstKind::Alloc { .. }) {
                    positions.insert(inst, (block_id, index));
                }
                func.inst(inst).kind.visit_operands(|operand| {
                    uses.push((operand, AllocUse::Inst(inst, block_id, index)));
                });
            }
            if let Some(terminator) = &block.terminator {
                terminator.visit_operands(|operand| {
                    uses.push((operand, AllocUse::Terminator(block_id)));
                });
            }
        }
        let users = IndexLists::new(func.num_values(), uses.iter().copied());
        let candidates: Vec<_> = allocations
            .into_iter()
            .filter_map(|(alloc, pointer)| {
                let access = Self::allocation_accesses(func, &users, alloc, pointer)?;
                // Static placement keeps a write-only deferred object unrelated to the heap. One
                // that is read is placed by the free-memory pointer, so it stays heap-visible.
                let exposed =
                    !func.inst(alloc).metadata.deferred_alloc() || !access.reads.is_empty();
                // After a free-memory-pointer reset, an object may lie over an earlier one.
                if exposed && self.allocation_site(func, pointer) != Some(alloc) {
                    return None;
                }
                (!access.stores.is_empty()).then_some((alloc, access, exposed))
            })
            .collect();
        if candidates.is_empty() {
            return;
        }

        let cfg = &*self.cfg;
        if GasObservations::any(func, cfg, &self.alias) {
            return;
        }

        let mut heap_reads = Vec::new();
        let mut wrapping_writes = false;
        if candidates.iter().any(|&(_, _, exposed)| exposed) {
            'reads: for (block_id, block) in func.blocks.iter_enumerated() {
                for (index, &inst) in block.instructions.iter().enumerate() {
                    if func.inst(inst).kind.effect_kind() == EffectKind::Pure {
                        continue;
                    }
                    if self.alias.instruction_may_wrap_write(func, inst) {
                        // Every exposed object stays once a wrapping write appears; deferred,
                        // unexposed objects ignore these heap reads entirely.
                        wrapping_writes = true;
                        break 'reads;
                    }
                    let effects = self.alias.instruction_mod_ref(func, inst);
                    self.collect_heap_reads(func, &effects, (block_id, index), &mut heap_reads);
                }
                if let Some(terminator) = &block.terminator {
                    let effects = self.alias.terminator_mod_ref(func, terminator);
                    self.collect_heap_reads(
                        func,
                        &effects,
                        (block_id, usize::MAX),
                        &mut heap_reads,
                    );
                }
            }
        }

        let dead = &mut scratch.dead;
        dead.clear();
        for (alloc, access, exposed) in candidates {
            let (alloc_block, alloc_index) = positions[&alloc];
            // A write that wraps may rewind the free-memory pointer and put any object over an
            // earlier one, which allocation uniqueness ignores.
            if exposed && wrapping_writes {
                continue;
            }
            // An out-of-bounds read of an object that precedes this one can reach it.
            let heap_reads = heap_reads.iter().filter(|read| {
                exposed
                    && read.beyond.is_none_or(|object| {
                        // Semantic ABI allocations also have allocation IDs, but only explicit
                        // `alloc`s have positions here. An untracked read may reach any object.
                        positions.get(&object).is_none_or(|&(block, index)| {
                            object != alloc
                                && (block == alloc_block && index < alloc_index
                                    || Self::block_reaches(cfg, block, alloc_block))
                        })
                    })
            });
            let heap_reads = heap_reads.collect::<Vec<_>>();
            for store in &access.stores {
                if !access.reads.iter().any(|read| Self::read_reaches(cfg, store, read))
                    && !heap_reads.iter().any(|read| {
                        Self::position_reaches(cfg, (store.block, store.index), read.position)
                    })
                {
                    dead.insert(store.inst);
                }
            }
        }

        self.eliminated_count += remove_dead(func, dead);
    }

    /// Records the reads in `effects` that may touch a heap object other than through the
    /// pointer of one allocation, which the derivation of that pointer covers.
    fn collect_heap_reads(
        &self,
        func: &Function,
        effects: &ModRef,
        position: (BlockId, usize),
        heap_reads: &mut Vec<HeapRead>,
    ) {
        for access in effects.reads() {
            let beyond = match *access {
                Access::Any(AddressSpace::Memory) => None,
                // A runtime-indexed access wraps, and so may land in any object.
                Access::Location(Location::Memory(location))
                    if location.size == LocationSize::Unknown =>
                {
                    None
                }
                Access::Location(Location::Memory(location)) => match location.address.base {
                    // An access within the bounds of an allocation.
                    MemoryBase::Allocation(_) | MemoryBase::DynamicAllocation(_) => continue,
                    // A slot of the frame below the heap. A longer range may run into the heap.
                    MemoryBase::InternalFrame
                        if location
                            .size
                            .as_const()
                            .is_some_and(|size| size <= EvmMemoryLayout::WORD_SIZE) =>
                    {
                        continue;
                    }
                    MemoryBase::InternalFrame => None,
                    MemoryBase::Absolute => {
                        // The reserved words below the heap.
                        let reserved = match location.size.as_const() {
                            Some(0) => true,
                            Some(size) => location
                                .address
                                .offset
                                .checked_add(size)
                                .is_some_and(|end| EvmMemoryLayout::is_reserved(end - 1)),
                            None => false,
                        };
                        if reserved {
                            continue;
                        }
                        None
                    }
                    // An access that may leave its allocation names it by its pointer, or by
                    // `FreeMemory` for a unique one, and reaches only the objects above it.
                    MemoryBase::Value(pointer) => self.allocation_site(func, pointer),
                    MemoryBase::FreeMemory(site)
                        if matches!(func.inst(site).kind, InstKind::Alloc { .. }) =>
                    {
                        Some(site)
                    }
                    MemoryBase::UnboundedAllocation(alloc) => Some(alloc),
                    // A read at the free-memory pointer may reach any object allocated after it.
                    MemoryBase::FreeMemory(_) => None,
                },
                Access::Any(_) | Access::Location(_) => continue,
            };
            heap_reads.push(HeapRead { position, beyond });
        }
    }

    /// Returns the allocation `pointer` is the result of, if it has a fresh provenance.
    fn allocation_site(&self, func: &Function, pointer: ValueId) -> Option<InstId> {
        let Value::Inst(inst) = func.value(pointer) else { return None };
        (matches!(func.inst(*inst).kind, InstKind::Alloc { .. })
            && self.alias.memory_address(func, pointer).is_some_and(|address| {
                matches!(
                    address.base,
                    MemoryBase::Allocation(site) | MemoryBase::DynamicAllocation(site)
                        if site == *inst
                )
            }))
        .then_some(*inst)
    }

    /// Collects the constant-offset stores into the allocation `alloc` and every read of its
    /// bytes, or returns `None` when its pointer is used in a way this analysis does not model.
    fn allocation_accesses(
        func: &Function,
        users: &IndexLists<ValueId, AllocUse>,
        alloc: InstId,
        pointer: ValueId,
    ) -> Option<AllocAccesses> {
        let InstKind::Alloc { size, semantics } = func.inst(alloc).kind else { return None };
        let mut walk = AllocWalk {
            func,
            size: if semantics.failure == AllocationFailure::Infallible {
                AllocWalk::size_lower_bound(func, size, 4)
            } else {
                func.value_u64(size).unwrap_or(0)
            },
            derived: FxHashMap::from_iter([(pointer, AllocPointer::START)]),
            worklist: vec![pointer],
            accesses: AllocAccesses::default(),
        };
        while let Some(value) = walk.worklist.pop() {
            let at = walk.derived[&value];
            for &user in users.get(value) {
                // `value` may only appear as an address; any other operand position is an escape.
                let (addresses, operands) = match user {
                    AllocUse::Inst(inst, block, index) => {
                        let mut operands = 0;
                        func.inst(inst)
                            .kind
                            .visit_operands(|operand| operands += usize::from(operand == value));
                        (walk.inst_addresses(value, at, inst, block, index)?, operands)
                    }
                    AllocUse::Terminator(block) => {
                        let terminator = func.blocks[block].terminator.as_ref()?;
                        let mut operands = 0;
                        terminator
                            .visit_operands(|operand| operands += usize::from(operand == value));
                        (walk.terminator_addresses(value, at, block, terminator), operands)
                    }
                };
                if addresses != operands {
                    return None;
                }
            }
        }
        Some(walk.accesses)
    }

    /// Returns whether `read` may observe bytes `store` wrote: it overlaps them and runs after
    /// the store.
    fn read_reaches(cfg: &CfgInfo, store: &AllocStore, read: &AllocRead) -> bool {
        read.range.overlaps(store.range)
            && Self::position_reaches(cfg, (store.block, store.index), (read.block, read.index))
    }

    /// Returns whether `to` runs after `from`: later in its block or in a block `from`'s block
    /// can reach.
    fn position_reaches(cfg: &CfgInfo, from: (BlockId, usize), to: (BlockId, usize)) -> bool {
        from.0 == to.0 && from.1 < to.1 || Self::block_reaches(cfg, from.0, to.0)
    }

    /// Returns whether a path of at least one edge leads from `from` to `to`.
    fn block_reaches(cfg: &CfgInfo, from: BlockId, to: BlockId) -> bool {
        cfg.transitive_reachability(from).is_none_or(|reachable| reachable.contains(to))
    }

    /// Marks both possible ranges of a constant-offset read whose base has a lower bound but
    /// no upper bound. A wrapped sum lies below the offset; a non-wrapped sum retains the
    /// base's floor. Unknown lengths cannot leave a gap between those ranges.
    fn mark_wrapping_read(
        &self,
        func: &Function,
        live: &mut MemLive,
        address: ValueId,
        size: Option<u64>,
    ) -> bool {
        let Some(size) = size else { return false };
        let Some(address) = self.alias.memory_address(func, address) else { return false };
        let base = match address.base {
            MemoryBase::Value(base) => base,
            MemoryBase::FreeMemory(read) => {
                let Some(base) = func.inst_result_value(read) else { return false };
                base
            }
            _ => return false,
        };
        let Some(bound) = self.read_bound(func, base) else { return false };
        let Some(end) = address.offset.checked_add(size) else { return false };
        Self::mark_range(live, 0, end);
        live.add_from(bound.checked_add(address.offset).unwrap_or(bound));
        true
    }

    /// Function-local value identities cannot escape into a callee's liveness summary.
    /// Unresolved exact reads have no useful bound, so forgetting them keeps every slot live.
    fn summarize_live(live: &mut MemLive) {
        if !live.ranges.is_empty() {
            live.set_all();
        }
    }

    /// Discharges exact reads only after a complete overwrite of that same runtime range.
    /// This needs no assumption about pointer provenance or non-wrapping address arithmetic.
    fn kill_written_ranges(&self, func: &Function, inst: InstId, live: &mut MemLive) {
        if live.ranges.is_empty() {
            return;
        }
        func.inst(inst).kind.visit_raw_memory(|access, address, size| {
            if access == RawMemoryAccess::Write {
                let address = AliasAnalysis::memory_address_value(func, address);
                live.ranges.retain(|&mut (offset, length)| {
                    address != offset
                        || !(size == crate::mir::RawMemorySize::Value(length)
                            || size
                                .constant(func)
                                .is_some_and(|n| func.value_u64(length) == Some(n)))
                });
            }
        });
    }
}

/// A read that may touch any heap object, or only the objects above the allocation `beyond`.
struct HeapRead {
    position: (BlockId, usize),
    beyond: Option<InstId>,
}

/// A use of a value: an instruction at an index of its block, or a block's terminator.
#[derive(Clone, Copy)]
enum AllocUse {
    Inst(InstId, BlockId, usize),
    Terminator(BlockId),
}

/// A pointer derived from an allocation: its constant offset from the object start, and whether
/// a runtime index has moved it by an unknown amount.
#[derive(Clone, Copy)]
struct AllocPointer {
    offset: u64,
    runtime_index: bool,
}

impl AllocPointer {
    const START: Self = Self { offset: 0, runtime_index: false };

    /// The object bytes an access of `len` bytes from this pointer covers: everything from the
    /// pointer on when the length is not constant, nothing for an empty access, and the whole
    /// object when a runtime index moved the pointer, since address arithmetic wraps and an index
    /// can land anywhere in it.
    fn range(self, len: Option<u64>) -> Option<AllocRange> {
        if self.runtime_index {
            return Some(AllocRange { start: 0, end: None });
        }
        let end = match len {
            Some(0) => return None,
            Some(len) => self.offset.checked_add(len),
            None => None,
        };
        Some(AllocRange { start: self.offset, end })
    }
}

/// A byte range of an allocation, open-ended when `end` is `None`.
#[derive(Clone, Copy)]
struct AllocRange {
    start: u64,
    end: Option<u64>,
}

impl AllocRange {
    fn overlaps(self, other: Self) -> bool {
        self.end.is_none_or(|end| end > other.start) && other.end.is_none_or(|end| end > self.start)
    }
}

struct AllocStore {
    inst: InstId,
    block: BlockId,
    index: usize,
    range: AllocRange,
}

struct AllocRead {
    block: BlockId,
    /// `usize::MAX` for a terminator, which runs after every instruction.
    index: usize,
    range: AllocRange,
}

#[derive(Default)]
struct AllocAccesses {
    stores: Vec<AllocStore>,
    reads: Vec<AllocRead>,
}

/// The walk over the values derived from one allocation pointer.
struct AllocWalk<'a> {
    func: &'a Function,
    /// A lower bound of the allocation's byte size.
    size: u64,
    derived: FxHashMap<ValueId, AllocPointer>,
    worklist: Vec<ValueId>,
    accesses: AllocAccesses,
}

impl AllocWalk<'_> {
    /// Records the accesses `inst` makes through `value`, derived pointer `at`, and returns the
    /// number of its operand positions that hold `value` as an address, or `None` when the
    /// derivation overflows.
    fn inst_addresses(
        &mut self,
        value: ValueId,
        at: AllocPointer,
        inst: InstId,
        block: BlockId,
        index: usize,
    ) -> Option<usize> {
        let func = self.func;
        let is = |address| usize::from(address == value);
        Some(match func.inst(inst).kind {
            InstKind::Load(address, PtrLocation::Memory) => {
                self.read(address == value, at, block, index, Some(32));
                is(address)
            }
            InstKind::Keccak256(address, len)
            | InstKind::Log { offset: address, size: len, .. }
            | InstKind::Create(_, address, len)
            | InstKind::Create2(_, address, len, _) => {
                self.read(address == value, at, block, index, func.value_u64(len));
                is(address)
            }
            InstKind::Call { args_offset, args_size, ret_offset, .. }
            | InstKind::CallCode { args_offset, args_size, ret_offset, .. }
            | InstKind::StaticCall { args_offset, args_size, ret_offset, .. }
            | InstKind::DelegateCall { args_offset, args_size, ret_offset, .. } => {
                self.read(args_offset == value, at, block, index, func.value_u64(args_size));
                is(args_offset) + is(ret_offset)
            }
            InstKind::Copy(dest, source, len, PtrLocation::Memory) => {
                self.read(source == value, at, block, index, func.value_u64(len));
                is(dest) + is(source)
            }
            InstKind::Store(address, _, PtrLocation::Memory) => {
                self.store(address == value, at, inst, block, index, 32);
                is(address)
            }
            InstKind::Store8(address, _) => {
                self.store(address == value, at, inst, block, index, 1);
                is(address)
            }
            InstKind::MemoryZero(address, _)
            | InstKind::Copy(address, _, _, PtrLocation::Calldata | PtrLocation::Returndata)
            | InstKind::DataCopy(_, address, _)
            | InstKind::CodeCopy(address, ..)
            | InstKind::ExtCodeCopy(_, address, ..) => is(address),
            InstKind::PtrAdd(base, offset) if base == value => {
                let next = match func.value_u64(offset) {
                    Some(offset) => AllocPointer { offset: at.offset.checked_add(offset)?, ..at },
                    None => AllocPointer { runtime_index: true, ..at },
                };
                self.derive(inst, next);
                1
            }
            InstKind::Add(base, offset) | InstKind::Add(offset, base)
                if base == value
                    && let Some(offset) = func.value_u64(offset) =>
            {
                self.derive(inst, AllocPointer { offset: at.offset.checked_add(offset)?, ..at });
                1
            }
            InstKind::PtrToInt(operand, 256) | InstKind::IntToPtr(operand) if operand == value => {
                self.derive(inst, at);
                1
            }
            _ => 0,
        })
    }

    /// As [`Self::inst_addresses`], for a terminator.
    fn terminator_addresses(
        &mut self,
        value: ValueId,
        at: AllocPointer,
        block: BlockId,
        terminator: &Terminator,
    ) -> usize {
        match *terminator {
            Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                // A terminator runs after every instruction of its block.
                self.read(offset == value, at, block, usize::MAX, self.func.value_u64(size));
                usize::from(offset == value)
            }
            _ => 0,
        }
    }

    fn read(
        &mut self,
        addresses: bool,
        at: AllocPointer,
        block: BlockId,
        index: usize,
        len: Option<u64>,
    ) {
        if addresses && let Some(range) = at.range(len) {
            self.accesses.reads.push(AllocRead { block, index, range });
        }
    }

    /// Records a `width`-byte store at the exact offset `at` of the object, if it fits. A store
    /// past the end of the object may write over the next one, which it then leaves alone.
    fn store(
        &mut self,
        addresses: bool,
        at: AllocPointer,
        inst: InstId,
        block: BlockId,
        index: usize,
        width: u64,
    ) {
        if addresses
            && let Some(range) = at.range(Some(width))
            && range.end.is_some_and(|end| end <= self.size)
        {
            self.accesses.stores.push(AllocStore { inst, block, index, range });
        }
    }

    /// A lower bound of the size of an infallible allocation: constants, sums that provably do not
    /// wrap, and masks that clear low bits, such as the rounding of a length to whole words.
    fn size_lower_bound(func: &Function, size: ValueId, depth: u32) -> u64 {
        if let Some(size) = func.value_u256(size) {
            return crate::mir::utils::u256_to_u64(size).unwrap_or(u64::MAX);
        }
        let (Value::Inst(inst), Some(depth)) = (func.value(size), depth.checked_sub(1)) else {
            return 0;
        };
        let bound = |value| Self::size_lower_bound(func, value, depth);
        match func.inst(*inst).kind {
            // A sum is at least the sum of its bounds only when it does not wrap: a wrapped
            // subtraction, such as `add x, 2^256 - 32` for `x - 32` or `add (sub 0, n), 32`,
            // lowers it.
            InstKind::Add(a, b)
                if no_wrap_with(func, size, &|value| Self::environment_size(func, value)) =>
            {
                bound(a).saturating_add(bound(b))
            }
            // `and x, mask` is monotonic in `x` when the mask only clears low bits.
            InstKind::And(x, mask) | InstKind::And(mask, x)
                if let Some(mask) = func.value_u256(mask)
                    && (!mask & (!mask).wrapping_add(U256::from(1))).is_zero() =>
            {
                crate::mir::utils::u256_to_u64(U256::from(bound(x)) & mask).unwrap_or(u64::MAX)
            }
            _ => 0,
        }
    }

    /// The range of `value` when it reads the size of calldata, return data, or code, which gas
    /// keeps far below 2^64 bytes.
    fn environment_size(func: &Function, value: ValueId) -> Option<(U256, U256)> {
        let Value::Inst(inst) = func.value(value) else { return None };
        matches!(
            func.inst(*inst).kind,
            InstKind::CalldataSize | InstKind::ReturnDataSize | InstKind::CodeSize
        )
        .then_some((U256::ZERO, U256::from(u64::MAX)))
    }

    /// Follows the result of `inst`, a pointer derived from `at`.
    fn derive(&mut self, inst: InstId, next: AllocPointer) {
        if let Some(result) = self.func.inst_result_value(inst)
            && self.derived.insert(result, next).is_none()
        {
            self.worklist.push(result);
        }
    }
}

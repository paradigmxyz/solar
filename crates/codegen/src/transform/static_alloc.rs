//! Static placement of provably local heap allocations.
//!
//! When a constant-size `alloc` executes at most once per call and its pointer
//! never escapes the function, the allocation does not need a runtime
//! free-memory-pointer bump. This module proves those properties for the EVM
//! backend and retains a conservative MIR rewrite for the explicit
//! `static-alloc` pass.
//!
//! Safety contract:
//! - external entries only: their locals are absolute low-memory addresses;
//! - the block cannot re-execute, so the reused static region cannot expose a previous iteration's
//!   contents where fresh zeroed memory was expected;
//! - every use of the pointer is an in-bounds address derivation into exact loads, stores, hashes,
//!   copies, logs, or external-data terminators — the pointer value never escapes into stored data,
//!   call arguments, or unbounded arithmetic;
//! - functions observing `msize` are skipped: eliding a bump changes the high-water mark.

use crate::{
    analysis::{
        AliasAnalysis, MemoryCallSummaries, StaticAllocCandidate, eligible_static_allocations,
        has_msize, is_static_alloc_entry,
    },
    memory::EvmMemoryLayout,
    mir::{Function, Immediate, Module, Value},
    pass::MirPass,
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;
use std::sync::Arc;

/// Pass that places provably local allocations statically.
pub(crate) struct StaticAlloc;

impl MirPass for StaticAlloc {
    fn name(&self) -> &'static str {
        "static-alloc"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        // Every entry's locals share the same low-memory region — only one
        // entry runs per call — so the tallest entry's frame top is a shadow
        // the others can grow into without moving the shared static-frame
        // region or any spill base above it. Placements stay inside it.
        let shadow = module
            .functions
            .iter()
            .filter(|func| is_static_alloc_entry(func))
            .map(|func| {
                EvmMemoryLayout::HEAP_START
                    + func.internal_frame_size.max(func.external_static_return_size)
            })
            .max()
            .unwrap_or(EvmMemoryLayout::HEAP_START);

        let summaries = Arc::new(MemoryCallSummaries::new(module));
        let mut changed = false;
        for func in module.functions.iter_mut() {
            if !is_static_alloc_entry(func) || has_msize(func) {
                continue;
            }
            let aa = AliasAnalysis::with_call_summaries(func, Arc::clone(&summaries));
            changed |= run_on_entry(func, shadow, &aa);
        }
        changed
    }
}

/// Pass that defers eligible allocations until exact backend layout is known.
pub(crate) struct DeferAlloc;

impl MirPass for DeferAlloc {
    fn name(&self) -> &'static str {
        "defer-alloc"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        let summaries = Arc::new(MemoryCallSummaries::new(module));
        let mut candidates = Vec::new();
        for (func_id, func) in module.functions.iter_enumerated() {
            let aa = AliasAnalysis::with_call_summaries(func, Arc::clone(&summaries));
            candidates.extend(
                eligible_static_allocations(func, &aa)
                    .into_iter()
                    .map(|candidate| (func_id, candidate.alloc)),
            );
        }

        let mut changed = false;
        for (func_id, alloc) in candidates {
            let metadata = &mut module.functions[func_id].inst_mut(alloc).metadata;
            if !metadata.deferred_alloc() {
                metadata.set_deferred_alloc();
                changed = true;
            }
        }
        changed
    }
}

fn run_on_entry(func: &mut Function, shadow: u64, aa: &AliasAnalysis) -> bool {
    let mut changed = false;
    for cand in eligible_static_allocations(func, aa) {
        changed |= apply_candidate(func, &cand, shadow);
    }
    changed
}

/// Rewrites an eligible allocation using the conservative placement retained
/// for the explicit `static-alloc` MIR pass.
fn apply_candidate(func: &mut Function, cand: &StaticAllocCandidate, shadow: u64) -> bool {
    // The region lives past the locals and the static return
    // buffer. It must stay inside the tallest entry's shadow — growing past
    // it pushes the shared static-frame region and can widen every helper
    // and spill push behind it — and must not drag this entry's own spill
    // base across the one-byte address boundary.
    let base = EvmMemoryLayout::HEAP_START
        + func.internal_frame_size.max(func.external_static_return_size);
    if base + cand.size > shadow || (base < 0x100 && base + cand.size > 0x100) {
        return false;
    }
    func.internal_frame_size = (base - EvmMemoryLayout::HEAP_START) + cand.size;
    let replacement = func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(base))));
    let mut replacements = FxHashMap::default();
    replacements.insert(cand.ptr, replacement);
    func.replace_uses_canonicalized(&replacements);
    let block = &mut func.blocks[cand.block];
    block.instructions.retain(|&inst| inst != cand.alloc);
    true
}

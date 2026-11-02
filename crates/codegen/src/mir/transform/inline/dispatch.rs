//! Expose scalar external bodies in the MIR dispatcher.
//!
//! After ABI and memory lowering, a bounded set of frameless word-only wrappers
//! can be spliced into the selector switch without an internal call protocol.
//! All routes must qualify: narrow arguments, fallback/receive handlers,
//! libraries, allocations, calls, and frame-relative memory retain the existing
//! wrapper boundary. The bound limits growth of per-function analyses; this
//! transformation moves bodies rather than duplicating them.
//!
//! External exits remain external exits. Debug context is copied for cloned
//! instructions. This pass precedes the final e-graph and CFG cleanup, and runs only with
//! optimization enabled. Dynamic inputs qualify only when their lowered body
//! needs word operations alone; general dynamic ABI and memory copying retain
//! the wrapper boundary. Returns must use the bounded ABI buffer or fit below the
//! heap start without reading an uninitialized free-memory-pointer slot, and reverts must fit
//! below the slot. Constant writes must stay below the heap start or in the route's static ABI
//! return buffer. Arbitrary assembly
//! memory accesses could otherwise observe or overwrite spills introduced by
//! the combined dispatcher, or depend on a wrapper's free-memory initialization.

use crate::{
    backend::evm::select::opcode_lowering,
    mir::{
        EffectKind, Function, InstKind, Module, PtrLocation, Terminator, analysis::CallGraphInfo,
        memory::EvmMemoryLayout,
    },
};

const MAX_DISPATCH_INSTRUCTIONS: usize = 512;

pub(super) fn inline_dispatch(module: &mut Module) -> bool {
    let Some(entry_id) = module.dispatch_entry() else { return false };
    if module.is_library {
        return false;
    }
    let routes = module.runtime_entries().iter().collect::<Vec<_>>();
    if !(2..=16).contains(&routes.len())
        || routes.iter().any(|&id| !eligible(module.function(id)))
        || routes.iter().map(|&id| module.function(id).blocks.len()).sum::<usize>() > 128
        || routes.iter().map(|&id| module.function(id).instructions().count()).sum::<usize>()
            > MAX_DISPATCH_INSTRUCTIONS
    {
        return false;
    }
    // Wrappers are moved only when the dispatcher is their sole caller.
    let graph = CallGraphInfo::new(module);
    let other_callees =
        graph.reachable_callees_from(module.functions.indices().filter(|&id| id != entry_id));
    if routes.iter().any(|&id| other_callees.contains(id)) {
        return false;
    }

    let mut entry = module.function(entry_id).clone();
    let mut switches = entry.blocks.iter().filter_map(|data| match &data.terminator {
        Some(Terminator::Switch { cases, .. }) => Some(cases),
        _ => None,
    });
    let (Some(cases), None) = (switches.next(), switches.next()) else { return false };
    // A selector route is a switch case; `receive` and `fallback` keep their wrapper boundary.
    let case_blocks = cases.iter().map(|&(_, block)| block).collect::<Vec<_>>();
    let sites = entry
        .blocks
        .iter_enumerated()
        .filter_map(|(block, data)| {
            if let Some(Terminator::TailCall { function, args }) = &data.terminator
                && case_blocks.contains(&block)
                && routes.contains(function)
                && args.is_empty()
            {
                Some((block, *function))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if sites.len() != routes.len()
        || routes.iter().any(|id| sites.iter().filter(|(_, callee)| callee == id).count() != 1)
    {
        return false;
    }
    for (block, id) in sites {
        let callee = module.function(id);
        if super::inline_dispatch_route(&mut entry, block, callee).is_none() {
            return false;
        }
        entry.external_static_return_size =
            entry.external_static_return_size.max(callee.external_static_return_size);
    }
    // entry: tail_call @wrapper => entry: cloned wrapper body
    // @wrapper is no longer an external root and function DCE removes it
    *module.function_mut(entry_id) = entry;
    true
}

fn eligible(func: &Function) -> bool {
    // A wrapper with an objective of its own keeps its code out of the dispatcher's.
    !func.attributes.no_inline
        && func.attributes.optimization.is_none()
        && func.internal_frame_size == 0
        && func.params.is_empty()
        && func.return_components().is_empty()
        && func.instructions().all(|id| {
            let kind = &func.inst(id).kind;
            match kind {
                InstKind::Store(ptr, _, PtrLocation::Memory) => {
                    safe_constant_memory_write(func, *ptr)
                }
                // Full-width pointer casts emit no code.
                InstKind::Phi(_)
                | InstKind::IntToPtr(_)
                | InstKind::PtrToInt(_, 256)
                | InstKind::Load(_, PtrLocation::Calldata)
                | InstKind::CalldataSize => true,
                _ => {
                    kind.effect_kind() == EffectKind::Pure && opcode_lowering(&kind.op()).is_some()
                }
            }
        })
        && func.blocks.iter().all(|block| match &block.terminator {
            Some(Terminator::ReturnData { offset, size }) => {
                func.value_u64(*offset).zip(func.value_u64(*size)).is_some_and(|(offset, size)| {
                    (offset == EvmMemoryLayout::HEAP_START
                        && size <= MAX_DISPATCH_INSTRUCTIONS as u64 * EvmMemoryLayout::WORD_SIZE)
                        || offset.checked_add(size).is_some_and(|end| {
                            end <= EvmMemoryLayout::HEAP_START
                                && (end <= EvmMemoryLayout::FMP_SLOT
                                    || offset >= EvmMemoryLayout::ZERO_SLOT
                                    || func.block_stores_fmp_slot(block))
                        })
                })
            }
            Some(Terminator::Revert { offset, size }) => func
                .value_u64(*offset)
                .zip(func.value_u64(*size))
                .and_then(|(offset, size)| offset.checked_add(size))
                .is_some_and(|end| end <= EvmMemoryLayout::FMP_SLOT),
            Some(Terminator::Return { values }) => values.is_empty(),
            Some(
                Terminator::Jump(_)
                | Terminator::Branch { .. }
                | Terminator::Switch { .. }
                | Terminator::Stop
                | Terminator::Invalid,
            ) => true,
            _ => false,
        })
}

/// Returns whether a constant word write stays within memory owned by the route.
fn safe_constant_memory_write(func: &Function, pointer: crate::mir::ValueId) -> bool {
    let Some(start) = func.value_u64(pointer) else { return false };
    let Some(end) = start.checked_add(EvmMemoryLayout::WORD_SIZE) else { return false };
    if end <= EvmMemoryLayout::HEAP_START {
        return true;
    }
    let terminal_return_size = func
        .blocks
        .iter()
        .filter_map(|block| {
            let Some(Terminator::ReturnData { offset, size }) = block.terminator else {
                return None;
            };
            (func.value_u64(offset) == Some(EvmMemoryLayout::HEAP_START))
                .then(|| func.value_u64(size))
                .flatten()
        })
        .max()
        .unwrap_or(0);
    let return_size = func.external_static_return_size.max(terminal_return_size);
    let Some(return_end) = EvmMemoryLayout::HEAP_START.checked_add(return_size) else {
        return false;
    };
    start >= EvmMemoryLayout::HEAP_START && end <= return_end
}

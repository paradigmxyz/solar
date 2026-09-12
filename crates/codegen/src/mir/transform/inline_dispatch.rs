//! Expose scalar external bodies and their shared inputs in the MIR dispatcher.
//!
//! After ABI and memory lowering, a bounded set of frameless word-only wrappers
//! can be spliced into the selector switch without an internal call protocol.
//! All routes must qualify: narrow arguments, fallback/receive handlers,
//! libraries, allocations, calls, and frame-relative memory retain the existing
//! wrapper boundary. The bound limits growth of per-function analyses; this
//! transformation moves bodies rather than duplicating them.
//!
//! Each ABI input word is materialized once before the switch and reused by
//! the cloned bodies. Calldata reads cannot trap, mutate memory, or change
//! across a call. Ordinary scalar passes can therefore see shared definitions;
//! the backend may still rematerialize them when that avoids stack traffic.
//! External exits remain external exits. Debug context is copied for cloned
//! instructions, while shared input loads have deliberately generated origins.
//! This pass precedes the final e-graph and CFG cleanup, and runs only with
//! optimization enabled. Dynamic inputs qualify only when their lowered body
//! needs word operations alone; general dynamic ABI and memory copying retain
//! the wrapper boundary. Returns must use the bounded ABI buffer and reverts
//! must fit below the free-memory-pointer slot. Arbitrary assembly memory exits
//! could otherwise observe spills introduced by the combined dispatcher, or
//! depend on a wrapper's free-memory initialization.

use crate::{
    backend::evm::select::opcode_lowering,
    mir::{
        EffectKind, Function, FunctionBuilder, InstKind, MirPhase, MirType, Module, Terminator,
        Value,
        analysis::CallGraphInfo,
        memory::EvmMemoryLayout,
        pass::{MirPass, ModuleAnalyses},
    },
};
use solar_ast::StateMutability;
use solar_sema::Gcx;

const MAX_DISPATCH_INSTRUCTIONS: usize = 512;

pub(crate) struct InlineDispatch;

impl MirPass for InlineDispatch {
    fn name(&self) -> &'static str {
        "inline-dispatch"
    }

    fn is_enabled(&self, gcx: Gcx<'_>, module: &Module) -> bool {
        gcx.sess.opts.optimization != solar_config::OptimizationMode::None
            && module.phase == MirPhase::MemoryLowered
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        if !inline_dispatch(module) {
            return false;
        }
        let _ = super::cfg_simplify::FunctionDce.run_pass(gcx, module, analyses);
        true
    }
}

fn inline_dispatch(module: &mut Module) -> bool {
    let Some(entry_id) = module.dispatch_entry() else { return false };
    if module.is_library
        || module
            .functions
            .iter()
            .any(|func| func.attributes.is_fallback || func.attributes.is_receive)
    {
        return false;
    }
    let routes = module
        .functions
        .iter_enumerated()
        .filter(|(_, func)| func.selector.is_some())
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
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
    let switches = entry
        .blocks
        .iter_enumerated()
        .filter_map(|(block, data)| {
            matches!(data.terminator, Some(Terminator::Switch { .. })).then_some(block)
        })
        .collect::<Vec<_>>();
    let [switch] = switches.as_slice() else { return false };
    let sites = entry
        .blocks
        .iter_enumerated()
        .filter_map(|(block, data)| {
            if let Some(Terminator::TailCall { function, args }) = &data.terminator
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
    let argument_count =
        routes.iter().map(|&id| module.function(id).arg_indices().count()).max().unwrap_or(0);
    // switch selector, cases
    //   => input0 = calldataload 4; input1 = calldataload 36; ...
    //      switch selector, cases[inputN/argN]
    let args = {
        let mut builder = FunctionBuilder::new(&mut entry);
        builder.switch_to_block(*switch);
        (0..argument_count)
            .map(|index| {
                let offset = builder.imm(4 + index as u64 * EvmMemoryLayout::WORD_SIZE);
                builder.calldataload(offset)
            })
            .collect::<Box<[_]>>()
    };
    for (block, id) in sites {
        let callee = module.function(id);
        if super::inline::inline_dispatch_route(&mut entry, block, callee, args.clone()).is_none() {
            return false;
        }
        entry.external_static_return_size =
            entry.external_static_return_size.max(callee.external_static_return_size);
    }
    // entry: tail_call @wrapper => entry: cloned wrapper body
    // @wrapper is no longer an external root and function DCE removes it
    *module.function_mut(entry_id) = entry;
    for id in routes {
        module.function_mut(id).selector = None;
    }
    true
}

fn eligible(func: &Function) -> bool {
    !func.attributes.no_inline
        && func.attributes.state_mutability == StateMutability::Pure
        && func.internal_frame_size == 0
        && func.params.is_empty()
        && func.returns.is_empty()
        && func.arg_indices().count() <= 8
        && func.arg_indices().all(|index| func.arg_ty(index) == MirType::uint256())
        && func.instructions().all(|id| {
            let kind = &func.inst(id).kind;
            match kind {
                InstKind::MStore(ptr, _) => matches!(func.value(*ptr), Value::Immediate(_)),
                InstKind::Phi(_) | InstKind::CalldataLoad(_) | InstKind::CalldataSize => true,
                _ => {
                    kind.effect_kind() == EffectKind::Pure && opcode_lowering(&kind.op()).is_some()
                }
            }
        })
        && func.blocks.iter().all(|block| match &block.terminator {
            Some(Terminator::ReturnData { offset, size }) => {
                func.value_u64(*offset) == Some(EvmMemoryLayout::HEAP_START)
                    && func.value_u64(*size).is_some_and(|size| {
                        size <= MAX_DISPATCH_INSTRUCTIONS as u64 * EvmMemoryLayout::WORD_SIZE
                    })
            }
            Some(Terminator::Revert { offset, size }) => func
                .value_u64(*offset)
                .zip(func.value_u64(*size))
                .and_then(|(offset, size)| offset.checked_add(size))
                .is_some_and(|end| end <= EvmMemoryLayout::FMP_SLOT),
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

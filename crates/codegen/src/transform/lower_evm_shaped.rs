//! EVM-shaped phase lowering: prepare control flow for the EVM backend.
//!
//! Real lowered external bodies keep their encode fused: they terminate with
//! `RETURN`/`REVERT` and never return to a caller. After the ABI and dispatch
//! phases, wrappers still reach such bodies through `icall`, which
//! models a returning edge that does not exist — the same dishonesty the
//! dispatch phase removed from its own case blocks.
//!
//! This pass rewrites a resultless `icall` to a callee that cannot
//! return (no reachable `ret` or `stop` terminator) into a
//! [`Terminator::TailCall`], dropping the dead remainder of the block. The
//! module comes out in the `evm-shaped` phase: every call edge either returns
//! or is an explicit tail call, which is the control-flow shape the backend
//! consumes.
//!
//! Arguments ride along: the backend stores them at the callee's compile-time
//! frame addresses and jumps, pushing no return address. That addressing only
//! exists for callees the backend gives a static frame (bodied, selectorless,
//! non-recursive), so calls to any other callee are left as ordinary calls.
//!
//! The backend also eliminates phis by copying each incoming value at the end of its predecessor.
//! When a phi's previous value remains live on a sibling edge, that copy must run after the branch
//! selects the phi successor. This pass isolates only those copies in a single-successor block.

use crate::{
    analysis::{CallGraphInfo, CfgInfo, Liveness},
    mir::{
        Function, InstKind, MirPhase, Module, Terminator,
        utils::{repair_reachability_phis, split_edge},
    },
    pass::MirPass,
    transform::cfg_simplify::remove_unreachable_blocks,
};
use solar_data_structures::bit_set::DenseBitSet;

/// EVM-shaped phase lowering pass.
pub(crate) struct LowerEvmShaped;

impl MirPass for LowerEvmShaped {
    fn name(&self) -> &'static str {
        "lower-evm-shaped"
    }

    fn is_enabled(&self, _gcx: solar_sema::Gcx<'_>, module: &Module) -> bool {
        module.phase == MirPhase::MemoryLowered
            && module.functions.iter().all(|func| {
                func.instructions().all(|inst_id| {
                    let inst = func.inst(inst_id);
                    match inst.kind {
                        InstKind::MakeSlice { .. }
                        | InstKind::SlicePtr(_)
                        | InstKind::SliceLen(_)
                        | InstKind::Fmp
                        | InstKind::SetFmp(_)
                        | InstKind::StoreImmutable(..) => false,
                        InstKind::Alloc { .. } => inst.metadata.deferred_alloc(),
                        _ => true,
                    }
                })
            })
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        lower_evm_shaped(module)
    }
}

fn lower_evm_shaped(module: &mut Module) -> bool {
    if module.phase != MirPhase::MemoryLowered {
        return false;
    }

    // Entry routing already uses explicit tail calls. Most modules have no
    // resultless internal call left to reshape, so avoid building a call
    // graph and classifying every function in that common case.
    let has_candidate = module.functions.iter().any(|func| {
        func.instructions().any(|inst_id| {
            let inst = func.inst(inst_id);
            inst.result_ty.is_none() && matches!(inst.kind, InstKind::ICall { .. })
        })
    });
    if has_candidate {
        let call_graph = CallGraphInfo::new(module);
        let mut tail_callable = DenseBitSet::new_empty(module.functions.len());
        for (func_id, func) in module.functions.iter_enumerated() {
            if function_cannot_return(func)
                && func.selector.is_none()
                && !func.attributes.is_receive
                && !func.attributes.is_fallback
                && !call_graph.is_recursive(func_id)
            {
                tail_callable.insert(func_id);
            }
        }

        // The deployment path emits constructor-reachable bodies without static
        // frames, so an argument-carrying tail call has no compile-time
        // argument addresses there. Keep those calls ordinary; argument-less
        // rewrites need no frame addressing and stay valid on both paths.
        let mut constructor_reachable = call_graph.reachable_callees_from(
            module
                .functions
                .iter_enumerated()
                .filter_map(|(id, func)| func.attributes.is_constructor.then_some(id)),
        );
        for (id, func) in module.functions.iter_enumerated() {
            if func.attributes.is_constructor {
                constructor_reachable.insert(id);
            }
        }

        for (func_id, func) in module.functions.iter_mut_enumerated() {
            let mut function_changed = false;
            for block_id in (0..func.blocks.len()).map(crate::mir::BlockId::from_usize) {
                let insts = &func.blocks[block_id].instructions;
                let Some((position, function, args)) =
                    insts.iter().enumerate().find_map(|(position, &inst_id)| {
                        let inst = func.inst(inst_id);
                        if inst.result_ty.is_none()
                            && let InstKind::ICall { function, args, .. } = &inst.kind
                            && tail_callable.contains(*function)
                            && (args.is_empty() || !constructor_reachable.contains(func_id))
                        {
                            Some((position, *function, args.iter().copied().collect()))
                        } else {
                            None
                        }
                    })
                else {
                    continue;
                };

                let metadata = func.inst(insts[position]).metadata.debug_context();
                // icall callee, args !metadata(call) -> tail_call callee, args !metadata(call)
                // Everything after the non-returning call is dead.
                func.blocks[block_id].instructions.truncate(position);
                func.blocks[block_id]
                    .set_terminator(Terminator::TailCall { function, args }, metadata);
                function_changed = true;
            }
            if function_changed {
                let _ = repair_reachability_phis(func);
                let _ = remove_unreachable_blocks(func);
            }
        }
    }
    for func in &mut module.functions {
        split_clobbering_phi_edges(func);
    }

    module.advance_phase(MirPhase::EvmShaped);
    true
}

fn split_clobbering_phi_edges(func: &mut Function) {
    let phi_successors =
        func.blocks.indices().filter(|&block| func.block_has_phi(block)).collect::<Vec<_>>();
    if phi_successors.is_empty() {
        return;
    }

    let liveness = Liveness::compute(func);
    let mut edges = Vec::new();

    for successor in phi_successors {
        let block = &func.blocks[successor];
        for &predecessor in &block.predecessors {
            let Some(terminator) = &func.blocks[predecessor].terminator else { continue };
            let successors = terminator.successors();
            if !successors.iter().any(|&sibling| sibling != successor) {
                continue;
            }

            let terminator_operands = terminator.operands();
            let copy_clobbers_live_value = block
                .instructions
                .iter()
                .take_while(|&&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)))
                .filter(|&&inst| {
                    let InstKind::Phi(incoming) = &func.inst(inst).kind else { unreachable!() };
                    incoming.iter().any(|&(block, _)| block == predecessor)
                })
                .filter_map(|&inst| func.inst_result_value(inst))
                .any(|destination| {
                    terminator_operands.contains(&destination)
                        || successors.iter().any(|&sibling| {
                            sibling != successor && liveness.live_in(sibling).contains(destination)
                        })
                });
            if copy_clobbers_live_value {
                edges.push((predecessor, successor));
            }
        }
    }

    edges.sort_unstable_by_key(|(predecessor, successor)| (predecessor.index(), successor.index()));
    edges.dedup();
    for (predecessor, successor) in edges {
        split_edge(func, predecessor, successor);
    }
}

/// Whether a function can never return to an internal caller: its reachable CFG
/// has no `ret` or `stop` terminator (`stop` is the internal return of a void
/// function).
fn function_cannot_return(func: &Function) -> bool {
    if func.blocks.is_empty() {
        return false;
    }
    let cfg = CfgInfo::new(func);
    !cfg.reachable().iter().any(|block| {
        matches!(func.blocks[block].terminator, Some(Terminator::Return { .. } | Terminator::Stop))
    })
}

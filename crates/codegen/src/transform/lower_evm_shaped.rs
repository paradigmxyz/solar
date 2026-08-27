//! EVM-shaped phase lowering: make non-returning call edges explicit.
//!
//! Real lowered external bodies keep their encode fused: they terminate with
//! `RETURN`/`REVERT` and never return to a caller. After the ABI and dispatch
//! phases, wrappers still reach such bodies through `internal_call`, which
//! models a returning edge that does not exist — the same dishonesty the
//! dispatch phase removed from its own case blocks.
//!
//! This pass rewrites a resultless `internal_call` to a callee that cannot
//! return (no reachable `ret` or `stop` terminator) into a
//! [`Terminator::TailCall`], dropping the dead remainder of the block. The
//! module comes out in the `evm-shaped` phase: every statically frame-eligible
//! call to a non-returning callee is an explicit tail call. Other calls retain
//! the backend's return protocol.
//!
//! Arguments ride along: the backend stores them at the callee's compile-time
//! frame addresses and jumps, pushing no return address. That addressing only
//! exists for callees the backend gives a static frame (bodied, selectorless,
//! non-recursive), so calls to any other callee are left as ordinary calls.

use crate::{
    analysis::{CallGraphInfo, CfgInfo},
    mir::{Function, InstKind, MirPhase, Module, Terminator, utils::repair_reachability_phis},
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

    fn is_enabled(&self, gcx: solar_sema::Gcx<'_>, module: &Module) -> bool {
        module.phase == MirPhase::TargetLowered
            && super::lower_intrinsics::intrinsics_are_lowered(module)
            && super::lower_target::target_operations_are_lowered(
                module,
                gcx.sess.opts.evm_version.has_mcopy(),
            )
    }

    fn is_required(&self) -> bool {
        true
    }

    fn output_phase(&self) -> Option<MirPhase> {
        Some(MirPhase::EvmShaped)
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        LowerEvmShapedCx::default().run(gcx, module)
    }
}

/// Statistics from EVM-shape lowering.
#[derive(Clone, Debug, Default)]
struct LowerEvmShapedStats {
    /// Number of internal calls rewritten into tail calls.
    tail_calls: usize,
}

#[derive(Debug, Default)]
struct LowerEvmShapedCx {
    stats: LowerEvmShapedStats,
}

impl LowerEvmShapedCx {
    fn run(&mut self, gcx: solar_sema::Gcx<'_>, module: &mut Module) -> bool {
        self.stats = LowerEvmShapedStats::default();
        if module.phase >= MirPhase::EvmShaped {
            return false;
        }
        if module.phase != MirPhase::TargetLowered {
            return false;
        }

        // Entry routing already uses explicit tail calls. Most modules have no
        // resultless internal call left to reshape, so avoid building a call
        // graph and classifying every function in that common case.
        let has_candidate = module.functions.iter().any(|func| {
            func.instructions().any(|inst_id| {
                let inst = func.inst(inst_id);
                inst.result_ty.is_none() && matches!(inst.kind, InstKind::InternalCall { .. })
            })
        });
        if !has_candidate {
            let changed = module.phase != MirPhase::EvmShaped;
            module.advance_phase(MirPhase::EvmShaped);
            return changed;
        }

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

        let mut changed = false;
        let function_ids: Vec<_> = module.functions.indices().collect();
        for func_id in function_ids {
            let func = &mut module.functions[func_id];
            let mut function_changed = false;
            for block_id in (0..func.blocks.len()).map(crate::mir::BlockId::from_usize) {
                let insts = &func.blocks[block_id].instructions;
                let Some(position) = insts.iter().position(|&inst_id| {
                    let inst = func.inst(inst_id);
                    inst.result_ty.is_none()
                        && matches!(
                            &inst.kind,
                            InstKind::InternalCall { function, args, .. }
                                if tail_callable.contains(*function)
                                    && (args.is_empty()
                                        || !constructor_reachable.contains(func_id))
                        )
                }) else {
                    continue;
                };

                let inst_id = func.blocks[block_id].instructions[position];
                let InstKind::InternalCall { function, args, .. } = &func.inst(inst_id).kind else {
                    unreachable!("position matched an internal call");
                };
                let (function, args) = (*function, args.iter().copied().collect());

                // Control never comes back: everything after the call is dead.
                func.blocks[block_id].instructions.truncate(position);
                func.blocks[block_id].terminator = Some(Terminator::TailCall { function, args });
                self.stats.tail_calls += 1;
                function_changed = true;
            }
            if function_changed {
                function_changed |= repair_reachability_phis(func);
                function_changed |= remove_unreachable_blocks(func) != 0;
            }
            changed |= function_changed;
        }

        if !evm_shape_is_lowered(module) {
            gcx.dcx().err("failed to make non-returning calls EVM-shaped").emit();
            return changed;
        }

        let phase_changed = module.phase != MirPhase::EvmShaped;
        module.advance_phase(MirPhase::EvmShaped);
        changed || phase_changed
    }
}

/// Returns whether every call eligible for static tail-call framing is explicit.
pub(crate) fn evm_shape_is_lowered(module: &Module) -> bool {
    let function_count = module.functions.len();
    if module.functions.iter().any(|func| {
        func.instructions().any(|inst_id| {
            matches!(func.inst(inst_id).kind, InstKind::InternalCall { function, .. } if function.index() >= function_count)
        }) || func.blocks.iter().any(|block| {
            matches!(block.terminator, Some(Terminator::TailCall { function, .. }) if function.index() >= function_count)
        })
    }) {
        return false;
    }

    let call_graph = CallGraphInfo::new(module);
    let mut tail_callable = DenseBitSet::new_empty(function_count);
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

    module.functions.iter_enumerated().all(|(func_id, func)| {
        func.instructions().all(|inst_id| {
            let inst = func.inst(inst_id);
            !matches!(
                &inst.kind,
                InstKind::InternalCall { function, args, .. }
                    if inst.result_ty.is_none()
                        && tail_callable.contains(*function)
                        && (args.is_empty() || !constructor_reachable.contains(func_id))
            )
        })
    })
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

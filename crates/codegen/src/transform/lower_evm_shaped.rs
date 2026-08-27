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
    analysis::{CallGraphInfo, TailCallEligibility},
    mir::{InstKind, MirPhase, Module, Terminator, utils::repair_reachability_phis},
    pass::MirPass,
    transform::cfg_simplify::remove_unreachable_blocks,
};

/// EVM-shaped phase lowering pass.
pub(crate) struct LowerEvmShaped;

impl MirPass for LowerEvmShaped {
    fn name(&self) -> &'static str {
        "lower-evm-shaped"
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
        lower_evm_shape(module)
    }
}

fn lower_evm_shape(module: &mut Module) -> bool {
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
        module.advance_phase(MirPhase::EvmShaped);
        return true;
    }

    let mut eligibility = TailCallEligibility::new(module);
    loop {
        let function_ids = eligibility.callee_first().to_vec();
        let mut round_changed = false;
        let mut graph_changed = false;
        for func_id in function_ids {
            let (function_changed, function_graph_changed) = {
                let function_count = module.functions.len();
                let func = &mut module.functions[func_id];
                let mut callees_before = None;
                let mut function_changed = false;
                for block_id in (0..func.blocks.len()).map(crate::mir::BlockId::from_usize) {
                    let insts = &func.blocks[block_id].instructions;
                    let Some(position) = insts.iter().position(|&inst_id| {
                        let inst = func.inst(inst_id);
                        inst.result_ty.is_none()
                            && matches!(
                                &inst.kind,
                                InstKind::InternalCall { function, args, .. }
                                    if eligibility.contains(func_id, *function)
                            )
                    }) else {
                        continue;
                    };
                    callees_before
                        .get_or_insert_with(|| CallGraphInfo::direct_callees(func, function_count));

                    let inst_id = func.blocks[block_id].instructions[position];
                    let InstKind::InternalCall { function, args, .. } = &func.inst(inst_id).kind
                    else {
                        unreachable!("position matched an internal call");
                    };
                    let (function, args) = (*function, args.iter().copied().collect());

                    // Control never comes back: everything after the call is dead.
                    func.blocks[block_id].instructions.truncate(position);
                    func.blocks[block_id].terminator =
                        Some(Terminator::TailCall { function, args });
                    function_changed = true;
                }
                if function_changed {
                    let _ = remove_unreachable_blocks(func);
                    let _ = repair_reachability_phis(func);
                }
                let function_graph_changed = callees_before.is_some_and(|callees_before| {
                    callees_before != CallGraphInfo::direct_callees(func, function_count)
                });
                (function_changed, function_graph_changed)
            };
            if function_changed {
                eligibility.refresh_callee(module, func_id);
                round_changed = true;
                graph_changed |= function_graph_changed;
            }
        }
        if !round_changed || !graph_changed {
            break;
        }

        let next = TailCallEligibility::new(module);
        if eligibility.same_eligible_calls(&next) {
            break;
        }
        eligibility = next;
    }

    module.advance_phase(MirPhase::EvmShaped);
    true
}

//! EVM-shaped phase lowering: make non-returning call edges explicit.
//!
//! Real lowered external bodies keep their encode fused: they terminate with
//! `RETURN`/`REVERT` and never return to a caller. After the ABI and dispatch
//! phases, wrappers still reach such bodies through `internal_call`, which
//! models a returning edge that does not exist — the same dishonesty the
//! dispatch phase removed from its own case blocks.
//!
//! This pass rewrites a resultless `internal_call` to a callee that cannot
//! return (no `ret` and no `stop` terminator anywhere in it) into a
//! [`Terminator::TailCall`], dropping the dead remainder of the block. The
//! module comes out in the `evm-shaped` phase: every call edge either returns
//! or is an explicit tail call, which is the control-flow shape the backend
//! consumes.
//!
//! Arguments ride along: the backend stores them at the callee's compile-time
//! frame addresses and jumps, pushing no return address. That addressing only
//! exists for callees the backend gives a static frame (bodied, selectorless,
//! non-recursive), so calls to any other callee are left as ordinary calls.

use crate::{
    analysis::CallGraphInfo,
    mir::{Function, InstKind, MirPhase, Module, Terminator},
    pass::MirPass,
};
use solar_data_structures::bit_set::DenseBitSet;

/// EVM-shaped phase lowering pass.
pub(crate) struct LowerEvmShapedPass;

impl MirPass for LowerEvmShapedPass {
    fn name(&self) -> &'static str {
        "lower-evm-shaped"
    }

    fn is_enabled(&self, _gcx: solar_sema::Gcx<'_>, module: &Module) -> bool {
        module.phase == MirPhase::Dispatch
    }

    fn run_pass(&self, _gcx: solar_sema::Gcx<'_>, module: &mut Module) -> bool {
        LowerEvmShaped::default().run(module)
    }

    fn is_required(&self) -> bool {
        true
    }
}

/// Statistics from EVM-shape lowering.
#[derive(Clone, Debug, Default)]
pub(crate) struct LowerEvmShapedStats {
    /// Number of internal calls rewritten into tail calls.
    pub tail_calls: usize,
}

#[derive(Debug, Default)]
struct LowerEvmShaped {
    stats: LowerEvmShapedStats,
}

impl LowerEvmShaped {
    fn run(&mut self, module: &mut Module) -> bool {
        self.stats = LowerEvmShapedStats::default();
        if module.phase >= MirPhase::EvmShaped {
            return false;
        }

        // Dispatch already uses explicit tail calls. Most modules have no
        // resultless internal call left to reshape, so avoid building a call
        // graph and classifying every function in that common case.
        let has_candidate = module.functions.iter().any(|func| {
            func.instructions.iter().any(|inst| {
                inst.result_ty.is_none() && matches!(inst.kind, InstKind::InternalCall { .. })
            })
        });
        if !has_candidate {
            module.advance_phase(MirPhase::EvmShaped);
            return false;
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

        for func in module.functions.iter_mut() {
            for block_id in (0..func.blocks.len()).map(crate::mir::BlockId::from_usize) {
                let insts = &func.blocks[block_id].instructions;
                let Some(position) = insts.iter().position(|&inst_id| {
                    let inst = &func.instructions[inst_id];
                    inst.result_ty.is_none()
                        && matches!(
                            &inst.kind,
                            InstKind::InternalCall { function, .. }
                                if tail_callable.contains(*function)
                        )
                }) else {
                    continue;
                };

                let inst_id = func.blocks[block_id].instructions[position];
                let InstKind::InternalCall { function, args, .. } =
                    &func.instructions[inst_id].kind
                else {
                    unreachable!("position matched an internal call");
                };
                let (function, args) = (*function, args.iter().copied().collect());

                // Control never comes back: everything after the call is dead.
                func.blocks[block_id].instructions.truncate(position);
                func.blocks[block_id].terminator = Some(Terminator::TailCall { function, args });
                self.stats.tail_calls += 1;
            }
        }

        module.advance_phase(MirPhase::EvmShaped);
        self.stats.tail_calls != 0
    }
}

/// Whether a function can never return to an internal caller: it has no `ret`
/// and no `stop` terminator (`stop` is the internal return of a void function).
fn function_cannot_return(func: &Function) -> bool {
    !func.blocks.is_empty()
        && !func.blocks.iter().any(|block| {
            matches!(block.terminator, Some(Terminator::Return { .. } | Terminator::Stop))
        })
}

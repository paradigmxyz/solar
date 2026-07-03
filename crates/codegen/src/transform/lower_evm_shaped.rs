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
//! expects to consume.

use crate::{
    mir::{Function, InstKind, MirPhase, Module, Terminator},
    pass::ModulePass,
};

/// Statistics from EVM-shape lowering.
#[derive(Clone, Debug, Default)]
pub struct LowerEvmShapedStats {
    /// Number of internal calls rewritten into tail calls.
    pub tail_calls: usize,
}

/// EVM-shaped phase lowering pass.
#[derive(Debug, Default)]
pub struct LowerEvmShapedPass {
    stats: LowerEvmShapedStats,
}

impl LowerEvmShapedPass {
    /// Returns statistics for the most recent run.
    #[must_use]
    pub const fn stats(&self) -> &LowerEvmShapedStats {
        &self.stats
    }

    fn run(&mut self, module: &mut Module) -> bool {
        if module.phase >= MirPhase::EvmShaped {
            return false;
        }

        let cannot_return: Vec<bool> =
            module.functions.iter().map(function_cannot_return).collect();

        for func in module.functions.iter_mut() {
            for block_id in (0..func.blocks.len()).map(crate::mir::BlockId::from_usize) {
                let insts = &func.blocks[block_id].instructions;
                let Some(position) = insts.iter().position(|&inst_id| {
                    let inst = &func.instructions[inst_id];
                    inst.result_ty.is_none()
                        && matches!(
                            &inst.kind,
                            InstKind::InternalCall { function, .. }
                                if cannot_return[function.index()]
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

impl ModulePass for LowerEvmShapedPass {
    fn name(&self) -> &str {
        "lower-evm-shaped"
    }

    fn run(&mut self, module: &mut Module) -> bool {
        Self::run(self, module)
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

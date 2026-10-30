//! Lower semantic memory zeroing to an EVM calldata copy.
//!
//! Reading beyond the end of calldata yields zero bytes, so copying from
//! `calldatasize()` implements an arbitrary-length zero fill without a loop. A 32-byte fill is
//! one zero word store instead, which needs fewer instructions than the copy.
//!
//! This pass runs after `lower-alloc` and the memory DSE that follows it, which can overwrite the
//! zero fills allocation lowering leaves, and before `lower-integers`.

use super::utils::expand_block;
use crate::mir::{Function, FunctionBuilder, InstKind, Module, PtrLocation, pass::MirPass};
use solar_sema::Gcx;

/// Lowers `memory_zero` instructions to `calldatasize` and `calldatacopy`.
pub(crate) struct LowerMemoryZero;

impl MirPass for LowerMemoryZero {
    fn name(&self) -> &'static str {
        "lower-memory-zero"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        _gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let mut changed = false;
        for func in module.functions.iter_mut() {
            changed |= lower_function(func);
        }
        changed
    }
}

fn lower_function(func: &mut Function) -> bool {
    if !func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::MemoryZero(_, _))) {
        return false;
    }

    for block in func.blocks.indices() {
        let mut builder = FunctionBuilder::new(func);
        expand_block(&mut builder, block, |builder, inst| {
            let InstKind::MemoryZero(dest, size) = builder.func().inst(inst).kind else {
                return false;
            };
            // destination = ptrtoint destination to i256
            let dest = builder.cast(dest, crate::mir::MirType::I256);
            if builder.func().value_u64(size) == Some(32) {
                let zero = builder.imm(0);
                let instruction = builder.func_mut().inst_mut(inst);
                instruction.kind = InstKind::Store(dest, zero, PtrLocation::Memory);
                instruction.metadata.set_memory_region(None);
                instruction.metadata.set_effect(Some(instruction.kind.effect_kind()));
            } else {
                let calldata_end = builder.calldatasize();
                builder.func_mut().inst_mut(inst).kind =
                    InstKind::Copy(dest, calldata_end, size, PtrLocation::Calldata);
            }
            false
        });
    }
    true
}

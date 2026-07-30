//! Lower semantic memory zeroing to an EVM calldata copy.
//!
//! Reading beyond the end of calldata yields zero bytes, so copying from
//! `calldatasize()` implements an arbitrary-length zero fill without a loop.

use crate::{
    mir::{BlockId, Function, FunctionBuilder, InstKind, Module},
    pass::MirPass,
};
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
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        let mut changed = false;
        for func in module.functions.iter_mut() {
            if !func.blocks.is_empty() {
                changed |= lower_function(func);
            }
        }
        changed
    }
}

fn lower_function(func: &mut Function) -> bool {
    if !func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::MemoryZero(_, _))) {
        return false;
    }

    let blocks: Vec<BlockId> = func.blocks.indices().collect();
    for block in blocks {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        for inst in instructions {
            let zero = match builder.func().inst(inst).kind {
                InstKind::MemoryZero(dest, size) => Some((dest, size)),
                _ => None,
            };
            if let Some((dest, size)) = zero {
                let calldata_end = builder.calldatasize();
                builder.func_mut().inst_mut(inst).kind =
                    InstKind::CalldataCopy(dest, calldata_end, size);
            }
            builder.func_mut().blocks[block].instructions.push(inst);
        }
    }
    true
}

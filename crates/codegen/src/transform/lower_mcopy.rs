//! Lower `mcopy` for EVM versions that predate Cancun.

use crate::{
    mir::{BlockId, Function, FunctionBuilder, InstKind, Module},
    pass::MirPass,
    transform::utils::redirect_successor_predecessors,
};
use solar_config::EvmVersion;
use solar_sema::Gcx;

/// Lowers `mcopy` to the identity precompile when the target has no `MCOPY`
/// opcode.
pub(crate) struct LowerMCopy;

impl MirPass for LowerMCopy {
    fn name(&self) -> &'static str {
        "lower-mcopy"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        if gcx.sess.opts.evm_version.has_mcopy() {
            return false;
        }

        let evm_version = gcx.sess.opts.evm_version;
        let mut changed = false;
        for func in module.functions.iter_mut() {
            if !func.blocks.is_empty() {
                changed |= lower_function(func, evm_version);
            }
        }
        changed
    }
}

fn lower_function(func: &mut Function, evm_version: EvmVersion) -> bool {
    let mut changed = false;
    let mut block_index = 0;
    while block_index < func.blocks.len() {
        let block = BlockId::from_usize(block_index);
        let mcopy = func.blocks[block]
            .instructions
            .iter()
            .copied()
            .enumerate()
            .find(|(_, inst)| matches!(func.inst(*inst).kind, InstKind::MCopy(_, _, _)));
        if let Some((position, inst)) = mcopy {
            lower_mcopy(func, block, position, inst, evm_version);
            changed = true;
        }
        block_index += 1;
    }
    changed
}

fn lower_mcopy(
    func: &mut Function,
    block: BlockId,
    position: usize,
    inst: crate::mir::InstId,
    evm_version: EvmVersion,
) {
    let InstKind::MCopy(dest, src, len) = func.inst(inst).kind else { unreachable!() };

    let mut instructions = std::mem::take(&mut func.blocks[block].instructions);
    let tail = instructions.split_off(position + 1);
    let removed = instructions.pop();
    debug_assert_eq!(removed, Some(inst));
    func.blocks[block].instructions = instructions;
    let old_terminator = func.blocks[block].terminator.take();

    let continuation = func.alloc_block();
    func.blocks[continuation].instructions = tail;
    func.blocks[continuation].terminator = old_terminator;
    redirect_successor_predecessors(func, block, continuation);

    let failure = func.alloc_block();
    let mut builder = FunctionBuilder::new(func);
    builder.switch_to_block(block);
    let gas = crate::utils::precompile_gas(&mut builder, evm_version);
    let identity = builder.imm_u64(4);
    let ok = if evm_version.has_static_call() {
        builder.staticcall(gas, identity, src, len, dest, len)
    } else {
        let value = builder.imm_u64(0);
        builder.call(gas, identity, value, src, len, dest, len)
    };
    let failed = builder.iszero(ok);
    builder.branch(failed, failure, continuation);

    builder.switch_to_block(failure);
    let zero = builder.imm_u64(0);
    builder.revert(zero, zero);
}

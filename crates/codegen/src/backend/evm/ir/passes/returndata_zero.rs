//! Zero from `RETURNDATASIZE` while the return data buffer is empty.
//!
//! Before Shanghai there is no `PUSH0`, and `PUSH1 0` costs two bytes and three gas. From
//! Byzantium, `RETURNDATASIZE` pushes the length of the return data buffer, which is zero until
//! the frame runs a call or create, and costs one byte and two gas. The pass rewrites a literal
//! zero push into it wherever the buffer is provably empty, so that each rewrite is cheaper in
//! bytes and gas at every optimization level. The cost model confirms that before the pass
//! rewrites anything; on Shanghai and later `PUSH0` costs the same, and nothing changes.
//!
//! The analysis is a must-analysis over the control-flow graph, from the entry block, which
//! `Verifier::blocks_after_returndata_set` computes with the stack verifier's edge model. A block
//! is tainted when any path to it runs a `CALL`, `CALLCODE`, `DELEGATECALL`, `STATICCALL`,
//! `CREATE` or `CREATE2`, including a path through an internal function that runs one before it
//! returns to its continuation. A block entered through a dynamic jump that no edge models is
//! tainted too. Within a clean block, only the zero pushes before its first call or create are
//! rewritten, since a later one sees the buffer that operation set. Reads of the buffer, such as
//! `RETURNDATACOPY` after a call, are not affected: they sit in tainted blocks or after the call.
//!
//! The pass runs after every pass that reads `push 0` as a concrete immediate, at the end of the
//! pipeline, so no later transform has to treat the instruction as a constant, and it also runs
//! at `-O none`. A rewritten instruction keeps its metadata. Block layout does not change.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Instruction, Module, ModuleTarget, verify::Verifier},
    op,
};
use alloy_primitives::U256;
use solar_sema::Gcx;

pub(super) struct ReturndataZero;

impl EvmPass for ReturndataZero {
    fn name(&self) -> &'static str {
        "returndata-zero"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        returndata_zero(gcx, module)
    }
}

fn returndata_zero(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let target = module.runtime_target(gcx);
    if !gcx.sess.opts.evm_version.supports_returndata()
        || !target.dominates(target.opcode(op::RETURNDATASIZE), target.encoded_push(U256::ZERO))
    {
        return false;
    }

    let after = Verifier::blocks_after_returndata_set(module);
    let mut changed = false;
    for (block_id, block) in module.blocks.iter_mut_enumerated() {
        if after.contains(block_id) {
            continue;
        }
        for inst in &mut block.instructions {
            if inst.as_evm_opcode().is_some_and(op::sets_returndata) {
                break;
            }
            // push 0 => returndatasize
            if inst.concrete_immediate() == Some(U256::ZERO) {
                inst.replace_preserving_metadata(Instruction::opcode(op::RETURNDATASIZE));
                changed = true;
            }
        }
    }
    changed
}

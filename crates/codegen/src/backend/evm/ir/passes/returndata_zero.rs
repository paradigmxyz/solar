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
//! rewritten, since a later one sees the buffer that operation set. A `DUP` of a word that the
//! block's own prefix pushed as zero becomes `RETURNDATASIZE` too, which costs a gas less, as
//! far as swaps and pops leave the word where the pass can follow it. Reads of the buffer, such as
//! `RETURNDATACOPY` after a call, are not affected: they sit in tainted blocks or after the call.
//!
//! The pass runs after every pass that reads `push 0` as a concrete immediate, at the end of the
//! pipeline, so no later transform has to treat the instruction as a constant, and it also runs
//! at `-O none`. A rewritten instruction keeps its metadata. Block layout does not change.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Instruction, Module, ModuleTarget, verify::Verifier},
    op::{self, StackOp},
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
    let returndatasize = target.opcode(op::RETURNDATASIZE);
    if !gcx.sess.opts.evm_version.supports_returndata()
        || !target.dominates(returndatasize, target.encoded_push(U256::ZERO))
    {
        return false;
    }
    let dup_to_zero = target.dominates(returndatasize, target.opcode(op::DUP1));

    let after = Verifier::blocks_after_returndata_set(module);
    let mut changed = false;
    // Whether each of the top words of the stack is a known zero, from the block's entry.
    // Words below the tracked ones are unknown.
    let mut zeros = Vec::new();
    for (block_id, block) in module.blocks.iter_mut_enumerated() {
        if after.contains(block_id) {
            continue;
        }
        zeros.clear();
        for inst in &mut block.instructions {
            if inst.as_evm_opcode().is_some_and(op::sets_returndata) {
                break;
            }
            match inst.as_stack_op() {
                Some(StackOp::Dup(n)) => {
                    let zero = zeros.len().checked_sub(usize::from(n)).is_some_and(|at| zeros[at]);
                    // dup n of a zero => returndatasize
                    if zero && dup_to_zero {
                        inst.replace_preserving_metadata(Instruction::opcode(op::RETURNDATASIZE));
                        changed = true;
                    }
                    zeros.push(zero);
                }
                Some(StackOp::Swap(n)) => swap_zeros(&mut zeros, 0, usize::from(n)),
                Some(StackOp::Exchange(n, m)) => {
                    swap_zeros(&mut zeros, usize::from(n), usize::from(m));
                }
                Some(StackOp::Pop) => {
                    zeros.pop();
                }
                None => {
                    // push 0 => returndatasize
                    let zero = inst.concrete_immediate() == Some(U256::ZERO);
                    if zero {
                        inst.replace_preserving_metadata(Instruction::opcode(op::RETURNDATASIZE));
                        changed = true;
                    }
                    let effect = inst.stack_effect();
                    zeros.truncate(zeros.len().saturating_sub(usize::from(effect.inputs)));
                    let known = zero || inst.as_evm_opcode() == Some(op::RETURNDATASIZE);
                    zeros.extend((0..effect.outputs).map(|_| known));
                }
            }
        }
    }
    changed
}

/// Swaps the flags of the words `a` and `b` below the top of the stack, forgetting all of them
/// when either lies below the tracked ones.
fn swap_zeros(zeros: &mut Vec<bool>, a: usize, b: usize) {
    let top = zeros.len();
    match (top.checked_sub(1 + a), top.checked_sub(1 + b)) {
        (Some(a), Some(b)) => zeros.swap(a, b),
        _ => zeros.clear(),
    }
}

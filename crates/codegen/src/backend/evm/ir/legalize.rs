//! Completes physical instruction lowering for older EVM targets.
//!
//! SHL and SHR use powers of two, whose wrapping exponentiation naturally yields
//! zero for shifts of at least 256. SAR complements negative inputs before the
//! unsigned division and complements the result afterward, giving floor rounding
//! and sign extension even when the divisor wraps to zero. This required pass
//! runs after target optimizations and before primitive assembly. It introduces
//! only local physical stack operations and never touches memory or CFG edges.
//! The final encoding adapter also maps REVERT to INVALID before Byzantium,
//! preserving exceptional termination on targets without revert-data support.
//! This mandatory conversion is independent of the selected optimization passes.

use super::{EvmPass, InstKind, Module, TerminatorKind};
use crate::backend::evm::op;
use alloy_primitives::U256;
use solar_sema::Gcx;

pub(super) struct LegalizeShifts;

impl EvmPass for LegalizeShifts {
    fn name(&self) -> &'static str {
        "legalize-shifts"
    }
    fn is_required(&self) -> bool {
        true
    }
    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        if gcx.sess.opts.evm_version.has_bitwise_shifting() {
            return false;
        }
        let mut changed = false;
        for id in module.block_ids().collect::<Vec<_>>() {
            let block = &mut module.blocks[id];
            let input = std::mem::take(&mut block.insts);
            for inst in input {
                match inst.kind {
                    // x shift -> x shift 2 -> x (2 ** shift) -> result
                    InstKind::Op(op::SHL) => {
                        block.insts.extend(
                            [
                                InstKind::Push(U256::from(2)),
                                InstKind::Op(op::EXP),
                                InstKind::Op(op::MUL),
                            ]
                            .map(Into::into),
                        );
                        changed = true;
                    }
                    // x shift -> x (2 ** shift) -> (2 ** shift) x -> result
                    InstKind::Op(op::SHR) => {
                        block.insts.extend(
                            [
                                InstKind::Push(U256::from(2)),
                                InstKind::Op(op::EXP),
                                InstKind::Swap(1),
                                InstKind::Op(op::DIV),
                            ]
                            .map(Into::into),
                        );
                        changed = true;
                    }
                    // x shift
                    // x shift x 0 -> x shift (x <s 0)
                    // x shift mask -> mask shift x mask
                    // mask (x xor mask) shift
                    // mask (2 ** shift) (x xor mask)
                    // mask quotient -> quotient xor mask
                    InstKind::Op(op::SAR) => {
                        block.insts.extend(
                            [
                                InstKind::Dup(2),
                                InstKind::Push(U256::ZERO),
                                InstKind::Swap(1),
                                InstKind::Op(op::SLT),
                                InstKind::Push(U256::ZERO),
                                InstKind::Op(op::SUB),
                                InstKind::Swap(2),
                                InstKind::Dup(3),
                                InstKind::Op(op::XOR),
                                InstKind::Swap(1),
                                InstKind::Push(U256::from(2)),
                                InstKind::Op(op::EXP),
                                InstKind::Swap(1),
                                InstKind::Op(op::DIV),
                                InstKind::Op(op::XOR),
                            ]
                            .map(Into::into),
                        );
                        changed = true;
                    }
                    // instruction -> instruction
                    _ => block.insts.push(inst),
                }
            }
        }
        changed
    }
}

/// Preserves exceptional termination where the target has no REVERT opcode.
pub(super) fn lower_unavailable_reverts(gcx: Gcx<'_>, module: &mut Module) {
    if op::available(op::REVERT, gcx.sess.opts.evm_version) {
        return;
    }
    for id in module.block_ids().collect::<Vec<_>>() {
        let block = &mut module.blocks[id];
        if block.terminator.kind == TerminatorKind::Revert {
            // revert offset, size -> invalid
            block.terminator = TerminatorKind::Invalid.into();
        }
    }
}

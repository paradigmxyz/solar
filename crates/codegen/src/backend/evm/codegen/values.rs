//! Late gas reads: a call's gas operand read right before the call.

use super::{
    EvmCodegen, Function, InstId, InstKind, SmallVec, U256, Value, ValueId, index_vec, op,
};

impl<'gcx> EvmCodegen<'gcx> {
    /// Emits the gas left minus `reserve`, for a call's gas operand.
    pub(super) fn emit_gas_minus(&mut self, reserve: U256) {
        // push <reserve>
        // gas !metadata(keep_with_next)
        // sub !metadata(keep_with_next)
        //
        // Before EIP-150 a call asking for more gas than is left throws, so the reserve only
        // keeps solc's 10-gas margin while nothing but the `SUB` runs between the `GAS` and
        // the call. Keeping both with the next instruction stops every backend transform from
        // making that boundary a block boundary, and with it from inserting a jump.
        let keep_with_call = !self.gcx.sess.opts.evm_version.can_overcharge_gas_for_call();
        self.asm.emit_push(reserve);
        self.asm.emit_op(op::GAS);
        if keep_with_call {
            self.asm.keep_last_with_next();
        }
        self.asm.emit_op(op::SUB);
        if keep_with_call {
            self.asm.keep_last_with_next();
        }
    }
}

/// A call's gas operand read right before the call.
pub(super) struct LateGasRead {
    pub(super) gas: ValueId,
    /// The `gas` read and, with a reserve, the subtraction.
    pub(super) insts: SmallVec<[InstId; 2]>,
}

/// Finds the gas operands of calls that are a `gas` read, possibly minus a constant reserve,
/// computed in the call's block and read only by the call.
pub(super) fn late_gas_reads(func: &Function) -> Vec<LateGasRead> {
    let mut use_counts = index_vec![0u32; func.num_values()];
    for block in &func.blocks {
        for &inst_id in &block.instructions {
            func.inst(inst_id).kind.visit_operands(|operand| use_counts[operand] += 1);
        }
        if let Some(terminator) = &block.terminator {
            terminator.visit_operands(|operand| use_counts[operand] += 1);
        }
    }

    let mut reads = Vec::new();
    for block in &func.blocks {
        for &inst_id in &block.instructions {
            let (InstKind::Call { gas, .. }
            | InstKind::CallCode { gas, .. }
            | InstKind::StaticCall { gas, .. }
            | InstKind::DelegateCall { gas, .. }) = func.inst(inst_id).kind
            else {
                continue;
            };
            if use_counts[gas] != 1 {
                continue;
            }
            let Value::Inst(operand) = *func.value(gas) else { continue };
            let insts: SmallVec<[InstId; 2]> = match func.inst(operand).kind {
                InstKind::Gas => SmallVec::from_slice(&[operand]),
                InstKind::Sub(lhs, _) => {
                    let Some((reading, _)) = gas_minus(func, gas) else { continue };
                    if use_counts[lhs] != 1 {
                        continue;
                    }
                    SmallVec::from_slice(&[reading, operand])
                }
                _ => continue,
            };
            if insts.iter().all(|inst| block.instructions.contains(inst)) {
                reads.push(LateGasRead { gas, insts });
            }
        }
    }
    reads
}

/// Matches `sub (gas), reserve` with a constant reserve, returning the `gas` read and the
/// reserve.
pub(super) fn gas_minus(func: &Function, value: ValueId) -> Option<(InstId, U256)> {
    let Value::Inst(inst) = *func.value(value) else { return None };
    let InstKind::Sub(lhs, reserve) = func.inst(inst).kind else { return None };
    let Value::Inst(reading) = *func.value(lhs) else { return None };
    if !matches!(func.inst(reading).kind, InstKind::Gas) {
        return None;
    }
    Some((reading, func.value_u256(reserve)?))
}

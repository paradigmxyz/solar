//! Pre-Constantinople shift legalization.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Instruction, Module},
    op,
};
use alloy_primitives::U256;
use solar_sema::Gcx;

pub(super) struct LegalizeShifts;

impl EvmPass for LegalizeShifts {
    fn name(&self) -> &'static str {
        "legalize-shifts"
    }

    fn is_enabled(&self, gcx: Gcx<'_>, _module: &Module) -> bool {
        !gcx.sess.opts.evm_version.has_bitwise_shifting()
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        legalize_shifts(gcx, module)
    }
}

pub(in crate::backend::evm) fn legalize_shifts(gcx: Gcx<'_>, module: &mut Module) -> bool {
    if gcx.sess.opts.evm_version.has_bitwise_shifting() {
        return false;
    }

    let mut changed = false;
    for block in &mut module.blocks {
        let mut instructions = Vec::with_capacity(block.instructions.len());
        for inst in std::mem::take(&mut block.instructions) {
            match inst.opcode {
                op::SHL => {
                    instructions.extend([
                        Instruction::push_value(U256::from(2)),
                        Instruction::opcode(op::EXP),
                        Instruction::opcode(op::MUL),
                    ]);
                    changed = true;
                }
                op::SHR => {
                    instructions.extend([
                        Instruction::push_value(U256::from(2)),
                        Instruction::opcode(op::EXP),
                        Instruction::opcode(op::SWAP1),
                        Instruction::opcode(op::DIV),
                    ]);
                    changed = true;
                }
                op::SAR => {
                    append_sar(&mut instructions);
                    changed = true;
                }
                _ => instructions.push(inst),
            }
        }
        block.instructions = instructions;
    }
    changed
}

/// Replaces `sar(shift, value)` with unsigned division of the value or its complement.
fn append_sar(out: &mut Vec<Instruction>) {
    let op = |opcode| Instruction::opcode(opcode);
    out.extend([
        op(op::DUP2),
        op(op::DUP2),
        Instruction::push_value(U256::from(2)),
        op(op::EXP),
        op(op::SWAP2),
        op(op::POP),
        op(op::DUP2),
        op(op::DUP2),
        op(op::DIV),
        op(op::DUP3),
        op(op::DUP3),
        op(op::NOT),
        op(op::DIV),
        op(op::NOT),
        op(op::DUP3),
        Instruction::push_value(U256::ZERO),
        op(op::SWAP1),
        op(op::SLT),
        op(op::DUP1),
        op(op::DUP3),
        op(op::MUL),
        op(op::DUP2),
        Instruction::push_value(U256::from(1)),
        op(op::SUB),
        op(op::DUP5),
        op(op::MUL),
        op(op::ADD),
        op(op::SWAP1),
        op(op::POP),
        op(op::SWAP1),
        op(op::POP),
        op(op::SWAP1),
        op(op::POP),
        op(op::SWAP1),
        op(op::POP),
        op(op::SWAP1),
        op(op::POP),
        op(op::SWAP1),
        op(op::POP),
    ]);
}

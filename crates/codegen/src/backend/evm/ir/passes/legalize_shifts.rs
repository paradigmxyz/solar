//! Legacy EVM opcode legalization.

use super::{EvmPass, utils::StackDepths};
use crate::backend::evm::{
    ir::{Instruction, Module, TerminatorKind},
    op,
};
use alloy_primitives::U256;
use solar_sema::Gcx;

/// Maximum transient stack growth of a legacy shift replacement.
pub(in crate::backend::evm) const LEGACY_SHIFT_STACK_HEADROOM: usize = 7;

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
    let evm_version = gcx.sess.opts.evm_version;
    let legalize_shifts = !evm_version.has_bitwise_shifting();
    let legalize_revert = !evm_version.supports_returndata();
    if !legalize_shifts && !legalize_revert {
        return false;
    }

    if legalize_shifts {
        let Some(depths) = StackDepths::new(module) else {
            gcx.dcx().err("cannot analyze stack depth for legacy shift legalization").emit();
            return false;
        };
        for (block_id, block) in module.blocks.iter_enumerated() {
            for (index, inst) in block.instructions.iter().enumerate() {
                let growth = match inst.opcode {
                    op::SHL | op::SHR => 1,
                    op::SAR => LEGACY_SHIFT_STACK_HEADROOM,
                    _ => continue,
                };
                if !depths.has_headroom(block_id, index, growth) {
                    gcx.dcx()
                        .err("legacy shift legalization would exceed the EVM stack limit")
                        .emit();
                    return false;
                }
            }
        }
    }

    let mut changed = false;
    for block in &mut module.blocks {
        let mut instructions = Vec::with_capacity(block.instructions.len());
        for inst in std::mem::take(&mut block.instructions) {
            match inst.opcode {
                op::SHL if legalize_shifts => {
                    instructions.extend([
                        Instruction::push_value(U256::from(2)),
                        Instruction::opcode(op::EXP),
                        Instruction::opcode(op::MUL),
                    ]);
                    changed = true;
                }
                op::SHR if legalize_shifts => {
                    instructions.extend([
                        Instruction::push_value(U256::from(2)),
                        Instruction::opcode(op::EXP),
                        Instruction::stack_op(op::StackOp::Swap(1)),
                        Instruction::opcode(op::DIV),
                    ]);
                    changed = true;
                }
                op::SAR if legalize_shifts => {
                    append_sar(&mut instructions);
                    changed = true;
                }
                _ => instructions.push(inst),
            }
        }
        block.instructions = instructions;
        if legalize_revert
            && let Some(terminator) = &mut block.terminator
            && matches!(terminator.kind, TerminatorKind::Op(op::REVERT))
        {
            terminator.kind = TerminatorKind::Op(op::INVALID);
            terminator.metadata.stack = None;
            changed = true;
        }
    }
    changed
}

/// Replaces `sar(shift, value)` with unsigned division of the value or its complement.
fn append_sar(out: &mut Vec<Instruction>) {
    let op = |opcode| {
        op::StackOp::from_legacy_opcode(opcode)
            .map_or_else(|| Instruction::opcode(opcode), Instruction::stack_op)
    };
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

//! Legalize opcodes that older EVM targets lack.
//!
//! Before Constantinople, this pass expands `SHL`, `SHR`, and `SAR` into arithmetic and physical
//! stack operations with the same signedness and operand order. `SHL` and `SHR` become a
//! multiplication or division by `2**shift`; when a literal push supplies the shift, the pass
//! pushes the factor itself instead of computing it with `EXP` if the cost model prefers the wider
//! push. Shifts of 256 or more fold to a zero factor, which matches the wrapped `EXP` result.
//! Before Byzantium, it replaces `REVERT` with `INVALID`, the closest available terminating
//! behavior. Newer targets skip the pass entirely.
//!
//! Codegen runs this pass after the whole EVM IR pipeline, so no peephole sees its output.

use super::EvmPass;
use crate::{
    backend::evm::{
        ir::{Instruction, Module, TerminatorKind},
        op,
    },
    target::Target,
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

pub(in crate::backend) fn legalize_shifts(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let evm_version = gcx.sess.opts.evm_version;
    let legalize_shifts = !evm_version.has_bitwise_shifting();
    let legalize_revert = !evm_version.supports_returndata();
    if !legalize_shifts && !legalize_revert {
        return false;
    }

    let code_follows = module.code_follows;
    let mut changed = false;
    for block in &mut module.blocks {
        let target = block.target(gcx, code_follows);
        let mut instructions = Vec::with_capacity(block.instructions.len());
        for inst in std::mem::take(&mut block.instructions) {
            let metadata = inst.metadata.clone();
            let start = instructions.len();
            match inst.opcode {
                op::SHL | op::SHR if legalize_shifts => {
                    append_shift(target, &mut instructions, inst.opcode == op::SHR);
                }
                op::SAR if legalize_shifts => append_sar(&mut instructions),
                _ => {
                    instructions.push(inst);
                    continue;
                }
            }
            changed = true;
            for replacement in &mut instructions[start..] {
                replacement.metadata.copy_source_debug_from(&metadata);
            }
        }
        block.instructions = instructions;
        if legalize_revert
            && let Some(terminator) = &mut block.terminator
            && matches!(terminator.kind, TerminatorKind::Op(op::REVERT))
        {
            terminator.kind = TerminatorKind::Op(op::INVALID);
            changed = true;
        }
    }
    changed
}

/// Replaces `shl(shift, value)` with `value * 2**shift` and `shr(shift, value)` with
/// `value / 2**shift`, folding a literal shift push into the factor when that is cheaper.
fn append_shift(target: Target, out: &mut Vec<Instruction>, shr: bool) {
    let two = U256::from(2);
    let factor = out
        .last()
        .and_then(Instruction::concrete_immediate)
        .and_then(|shift| target.legacy_shift_factor(shift));
    match factor {
        // push 2**shift
        Some(factor) => {
            out.last_mut().unwrap().replace_preserving_metadata(Instruction::push_value(factor))
        }
        // push 2
        // exp
        None => out.extend([Instruction::push_value(two), Instruction::opcode(op::EXP)]),
    }
    if shr {
        // swap1
        // div
        out.extend([Instruction::stack_op(op::StackOp::Swap(1)), Instruction::opcode(op::DIV)]);
    } else {
        // mul
        out.push(Instruction::opcode(op::MUL));
    }
}

/// Replaces `sar(shift, value)` with unsigned division of the value or its complement.
fn append_sar(out: &mut Vec<Instruction>) {
    let op = Instruction::opcode;
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

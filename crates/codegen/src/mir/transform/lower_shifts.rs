//! Lower constant shifts to multiplication and division on EVM versions without shift opcodes.
//!
//! Before Constantinople, `SHL` and `SHR` do not exist. The EVM IR pass `legalize-shifts` rewrites
//! them to `MUL` and `DIV` by `2**shift`, but it runs after stack scheduling and sees only the
//! scheduled instruction stream. The shift's operands then sit in shift-first order, so a
//! right shift needs an extra `SWAP1` before its `DIV` that the division's own operand order
//! would not.
//!
//! This pass runs before scheduling and rewrites `shl(c, x)` to `mul(x, 2**c)` and `shr(c, x)` to
//! `div(x, 2**c)`, with a zero factor when `c` is 256 or more, as the wrapped `EXP` result is.
//! The scheduler then orders the operands. A shift is rewritten only when
//! [`Target::legacy_shift_factor`](crate::target::Target::legacy_shift_factor) prefers pushing the
//! factor, which is the same query the EVM IR pass makes; otherwise `legalize-shifts` computes the
//! factor with `EXP`. Shifts by a variable amount and `SAR` stay for that pass, which remains the
//! backstop for every shift left in the code.
//!
//! The pass is disabled on targets with shifts, so it never changes their output. It runs last in
//! the lowered pipeline, after every pass that matches shifts, because nothing downstream
//! recognizes the products and quotients it creates as shifts again.

use crate::mir::{
    Immediate, InstKind, MirType, Module, Value,
    pass::{MirPass, ModuleAnalyses, run_function_pass},
};
use solar_sema::Gcx;

/// Rewrites constant `shl` and `shr` to `mul` and `div` by the factor `2**shift`.
pub(crate) struct LowerShifts;

impl MirPass for LowerShifts {
    fn name(&self) -> &'static str {
        "lower-shifts"
    }

    fn is_enabled(&self, gcx: Gcx<'_>, _module: &Module) -> bool {
        !gcx.sess.opts.evm_version.has_bitwise_shifting()
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        run_function_pass(module, analyses, |func, analyses| {
            let shifts = func
                .instructions()
                .filter_map(|inst| match func.inst(inst).kind {
                    InstKind::Shl(shift, value) => Some((inst, shift, value, false)),
                    InstKind::Shr(shift, value) => Some((inst, shift, value, true)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if shifts.is_empty() {
                return false;
            }
            let target = analyses.target(gcx);
            let mut changed = false;
            for (inst, shift, value, right) in shifts {
                let word = |value| func.value_ty(value) == Some(MirType::I256);
                if !word(value) || !word(shift) || !func.inst_result_value(inst).is_some_and(word) {
                    continue;
                }
                let Some(factor) =
                    func.value_u256(shift).and_then(|shift| target.legacy_shift_factor(shift))
                else {
                    continue;
                };
                // v = mul value, 2**shift (shl)
                // v = div value, 2**shift (shr)
                let factor = func.alloc_value(Value::Immediate(Immediate::I256(factor)));
                let kind =
                    if right { InstKind::Div(value, factor) } else { InstKind::Mul(value, factor) };
                func.inst_mut(inst).replace_kind(kind);
                changed = true;
            }
            changed
        })
    }
}

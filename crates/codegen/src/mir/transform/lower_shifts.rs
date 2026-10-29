//! Lower shifts to arithmetic on EVM versions without shift opcodes.
//!
//! Before Constantinople, `SHL`, `SHR`, and `SAR` do not exist. This pass runs at the end of the
//! lowered pipeline, just before stack scheduling, and leaves no shift behind: later code,
//! including the EVM IR pipeline, never sees an illegal one, and the backend rejects a module that
//! still has one.
//!
//! A left shift becomes `x * 2**s` and a logical right shift `x / 2**s`. The factor is an
//! immediate for a constant `s` when [`Target::legacy_shift_factor`] prefers pushing it, and
//! `exp(2, s)` otherwise, which also covers variable shifts. A shift of 256 or more gives a zero
//! factor, as the wrapped `EXP` result does: `MUL` by zero clears the word, and so does `DIV`.
//! Doing this before scheduling lets the scheduler order the factor and the value, which a rewrite
//! after scheduling cannot do without an extra `SWAP1` for each right shift.
//!
//! An arithmetic right shift divides the value or its complement, whichever is nonnegative, and
//! complements the quotient back: with `m` the sign mask (all ones for a negative value, else
//! zero), `sar(s, x) = ((x ^ m) / 2**s) ^ m`. A zero factor then gives `m`, which is the result
//! for any shift of 256 or more.
//!
//! The pass is disabled on targets with shifts, so it never changes their output. It must run
//! after every pass that creates or matches shifts, because nothing downstream recognizes the
//! products and quotients it creates as shifts again.

use super::utils::expand_block;
use crate::{
    mir::{
        FunctionBuilder, InstId, InstKind, MirType, Module, ValueId,
        pass::{MirPass, ModuleAnalyses, run_function_pass_with_cfg},
    },
    target::Target,
};
use solar_data_structures::map::FxHashMap;
use solar_sema::Gcx;

/// Rewrites `shl`, `shr`, and `sar` to arithmetic.
pub(crate) struct LowerShifts;

impl MirPass for LowerShifts {
    fn name(&self) -> &'static str {
        "lower-shifts"
    }

    fn is_enabled(&self, gcx: Gcx<'_>, _module: &Module) -> bool {
        !gcx.sess.opts.evm_version.has_bitwise_shifting()
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        run_function_pass_with_cfg(module, analyses, |func, analyses| {
            let blocks = func
                .blocks
                .iter_enumerated()
                .filter(|(_, block)| {
                    block.instructions.iter().any(|&inst| func.inst(inst).kind.is_shift())
                })
                .map(|(id, _)| id)
                .collect::<Vec<_>>();
            if blocks.is_empty() {
                return false;
            }
            // A block outside every loop of creation-only code runs once, and is priced as creation
            // code as the EVM IR passes price it.
            let once = analyses.loop_aware_target(gcx);
            let repeated = analyses.runtime_target(gcx);
            let cyclic = analyses.cfg().cyclic_blocks();
            let mut changed = false;
            // Shifts by the same amount share one computed factor within a block.
            let mut factors = FxHashMap::default();
            for block in blocks {
                let target = if cyclic.contains(block) { &repeated } else { &once };
                let mut builder = FunctionBuilder::new(func);
                factors.clear();
                expand_block(&mut builder, block, |builder, inst| {
                    if let Some(kind) = lower_shift(builder, target, &mut factors, inst) {
                        builder.func_mut().inst_mut(inst).replace_kind(kind);
                        changed = true;
                    }
                    false
                });
            }
            changed
        })
    }
}

/// Emits the helpers of a shift's arithmetic form and returns the operation that replaces it.
fn lower_shift(
    builder: &mut FunctionBuilder<'_>,
    target: &Target,
    factors: &mut FxHashMap<ValueId, ValueId>,
    inst: InstId,
) -> Option<InstKind> {
    let (shift, value, arithmetic) = match builder.func().inst(inst).kind {
        InstKind::Shl(shift, value) | InstKind::Shr(shift, value) => (shift, value, false),
        InstKind::Sar(shift, value) => (shift, value, true),
        _ => return None,
    };
    let left = matches!(builder.func().inst(inst).kind, InstKind::Shl(..));
    let word = |builder: &FunctionBuilder<'_>, value| {
        builder.func().value_ty(value) == Some(MirType::I256)
    };
    if !word(builder, value)
        || !word(builder, shift)
        || !builder.func().inst_result_value(inst).is_some_and(|result| word(builder, result))
    {
        return None;
    }
    let metadata = builder.func().inst(inst).metadata.debug_context();
    builder.set_debug_context(&metadata);
    let factor = shift_factor(builder, target, factors, shift);
    if !arithmetic {
        // v = mul value, factor (shl)
        // v = div value, factor (shr)
        return Some(if left {
            InstKind::Mul(value, factor)
        } else {
            InstKind::Div(value, factor)
        });
    }
    // negative = slt value, 0
    // mask = sub 0, zext negative
    // flipped = xor value, mask
    // quotient = div flipped, factor
    // v = xor quotient, mask
    let zero = builder.imm(0);
    let negative = builder.slt(value, zero);
    let negative = builder.cast_word(negative);
    let mask = builder.sub(zero, negative);
    let flipped = builder.xor(value, mask);
    let quotient = builder.div(flipped, factor);
    Some(InstKind::Xor(quotient, mask))
}

/// Returns the value `2**shift`: an immediate when the target prefers pushing it.
fn shift_factor(
    builder: &mut FunctionBuilder<'_>,
    target: &Target,
    factors: &mut FxHashMap<ValueId, ValueId>,
    shift: ValueId,
) -> ValueId {
    if let Some(shift) = builder.func().value_u256(shift)
        && let Some(factor) = target.legacy_shift_factor(shift)
    {
        return builder.imm(factor);
    }
    *factors.entry(shift).or_insert_with(|| {
        // factor = exp 2, shift
        let two = builder.imm(2);
        builder.exp(two, shift)
    })
}

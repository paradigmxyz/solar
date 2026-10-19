//! Demanded-bits simplification of effectful operands.
//!
//! Some opcodes read only the low bits of an operand: `MSTORE8` stores the low 8 bits of its
//! value. The opcode table declares these widths as `input_bits`. A value can have other users
//! that need every bit, so the e-graph cannot rewrite the value itself. This step instead
//! replaces the operand of the effectful instruction with a cheaper value that agrees on the
//! demanded bits. It is an analysis over operand chains with use-count conditions, which ISLE
//! rules on one instruction cannot express.
//!
//! Starting from the demanded mask of an operand, it walks the operand's definition:
//!
//! - `and(x, c)` is `x` when `c` covers the demanded bits, and zero when `c` misses all of them.
//! - `byte(31, x)` and `signextend(b, x)` are `x` when the demanded bits lie within the byte or
//!   sign-extended width.
//! - `shl(k, x)` is zero when the demanded bits lie below `k`.
//! - `shl(k, x)` and `shr(k, x)` demand `mask >> k` and `mask << k` of `x`. The walk rewrites `x`
//!   inside the shift only when every value from the operand down to the shift has one user, since
//!   the rewrite changes the shift's result outside the demanded bits.
//!
//! A masked value with other users is replaced only when the unmasked value has other users too:
//! otherwise the store would keep both values live, which costs more stack traffic than the mask.
//!
//! The replaced definitions stay in place and later dead-code elimination removes them when no
//! other user remains. Only the operand changes, so instruction metadata keeps describing the
//! same access. Operands whose replacement would change the MIR type are left alone.
//!
//! Address operands (160 bits) are excluded: the rules in `isle/mir/egraph/memory.isle` already
//! bypass their casts and masks, and only when the mask has no other user, since an address mask
//! shared with other code is cheaper to reuse than to keep both it and the unmasked value live.

use crate::{
    backend::evm::op,
    mir::{EffectKind, Function, Immediate, InstId, InstKind, MirType, Value, ValueId},
};
use alloy_primitives::U256;
use solar_data_structures::index::IndexVec;

/// Operand definitions followed for one demanded mask.
const MAX_DEPTH: u32 = 4;

/// Operands that read this many bits or more are addresses or full words.
const MIN_EXCLUDED_BITS: u16 = 160;

/// Narrows the demanded operands of every effectful instruction, returning the number of changes.
pub(super) fn simplify_demanded_operands(func: &mut Function) -> usize {
    let mut changed = 0;
    let mut uses = None;
    let insts = func.instructions().collect::<Vec<_>>();
    for inst_id in insts {
        let kind = &func.inst(inst_id).kind;
        if kind.op_def().effect == EffectKind::Pure {
            continue;
        }
        let Some(definition) = kind.evm_opcode().and_then(op::definition) else { continue };
        let operands = kind.operands();
        for (index, (&operand, &bits)) in operands.iter().zip(definition.input_bits).enumerate() {
            if bits == 0 || bits >= MIN_EXCLUDED_BITS {
                continue;
            }
            let uses = uses.get_or_insert_with(|| super::use_counts(func));
            let demanded = (U256::ONE << bits) - U256::ONE;
            let simplified = simplify(func, uses, operand, demanded, true, MAX_DEPTH, &mut changed);
            // A shared operand stays live, so storing the unmasked value would keep both live
            // unless the unmasked value has other users.
            let keeps_two_live = uses[operand] > 1
                && uses[simplified] <= 1
                && func.value(simplified).as_immediate().is_none();
            if simplified != operand && !keeps_two_live {
                // inst(.., operand, ..) => inst(.., simplified, ..)
                replace_operand(func, uses, inst_id, index, simplified);
                changed += 1;
                tracing::trace!(
                    target: super::TRACE_TARGET,
                    function = %func.name,
                    action = "demanded_operand",
                    ?inst_id,
                    index,
                    "mir_egraph"
                );
            }
        }
    }
    changed
}

/// What an instruction contributes to the demanded bits of its result.
enum Step {
    /// The result agrees with this value on every demanded bit.
    Operand(ValueId),
    /// Every demanded bit is zero.
    Zero,
    /// Nothing is known.
    Stop,
}

/// Returns a value that agrees with `value` on the bits in `demanded`, following at most `depth`
/// definitions.
///
/// `exclusive` holds while every value from the demanded operand down to `value` has one user.
/// Only then may a shift on the path change in place, since no other instruction reads its
/// result through the values above it. Each in-place edit adds to `changed`.
fn simplify(
    func: &mut Function,
    uses: &mut IndexVec<ValueId, u32>,
    mut value: ValueId,
    demanded: U256,
    mut exclusive: bool,
    depth: u32,
    changed: &mut usize,
) -> ValueId {
    let ty = func.value_ty(value);
    for remaining in (0..depth).rev() {
        exclusive &= uses[value] == 1;
        let Value::Inst(def) = *func.value(value) else { break };
        let step = match func.inst(def).kind {
            InstKind::And(a, b) => match (func.value_u256(a), func.value_u256(b)) {
                (_, Some(mask)) => mask_step(mask, demanded, a),
                (Some(mask), None) => mask_step(mask, demanded, b),
                (None, None) => Step::Stop,
            },
            InstKind::Byte(index, operand) => match func.value_u256(index) {
                Some(index) if index == U256::from(31) && fits(demanded, 8) => {
                    Step::Operand(operand)
                }
                Some(index) if index >= U256::from(32) => Step::Zero,
                _ => Step::Stop,
            },
            InstKind::SignExtend(index, operand) => match func.value_u256(index) {
                Some(index) if index >= U256::from(31) => Step::Operand(operand),
                Some(index) if fits(demanded, 8 * (index.to::<usize>() + 1)) => {
                    Step::Operand(operand)
                }
                _ => Step::Stop,
            },
            InstKind::Shl(count, operand) => match func.value_u256(count) {
                Some(count) if count >= U256::from(256) => Step::Zero,
                Some(count) if fits(demanded, count.to::<usize>()) => Step::Zero,
                Some(count) => {
                    let inner = demanded >> count.to::<usize>();
                    if exclusive {
                        simplify_shifted(func, uses, def, operand, inner, remaining, changed);
                    }
                    Step::Stop
                }
                None => Step::Stop,
            },
            InstKind::Shr(count, operand) => match func.value_u256(count) {
                Some(count) if count >= U256::from(256) => Step::Zero,
                Some(count) => {
                    let inner = demanded << count.to::<usize>();
                    if exclusive {
                        simplify_shifted(func, uses, def, operand, inner, remaining, changed);
                    }
                    Step::Stop
                }
                None => Step::Stop,
            },
            _ => Step::Stop,
        };
        match step {
            Step::Operand(operand) if func.value_ty(operand) == ty => value = operand,
            Step::Zero if ty == Some(MirType::I256) => {
                let zero = func.alloc_value(Value::Immediate(Immediate::I256(U256::ZERO)));
                uses.resize(func.num_values(), 0);
                return zero;
            }
            _ => break,
        }
    }
    value
}

/// Replaces the shifted operand of the shift `def`, when only the `demanded` bits of it matter.
fn simplify_shifted(
    func: &mut Function,
    uses: &mut IndexVec<ValueId, u32>,
    def: InstId,
    operand: ValueId,
    demanded: U256,
    depth: u32,
    changed: &mut usize,
) {
    let simplified = simplify(func, uses, operand, demanded, true, depth, changed);
    if simplified != operand {
        // shift(count, operand) => shift(count, simplified)
        replace_operand(func, uses, def, 1, simplified);
        *changed += 1;
    }
}

/// Classifies `and(operand, mask)` by the demanded bits.
fn mask_step(mask: U256, demanded: U256, operand: ValueId) -> Step {
    if mask & demanded == U256::ZERO {
        Step::Zero
    } else if mask & demanded == demanded {
        Step::Operand(operand)
    } else {
        Step::Stop
    }
}

/// Returns whether `demanded` has no bit at or above `bits`.
fn fits(demanded: U256, bits: usize) -> bool {
    bits >= 256 || demanded >> bits == U256::ZERO
}

/// Replaces operand `index` of an instruction, keeping the use counts current.
fn replace_operand(
    func: &mut Function,
    uses: &mut IndexVec<ValueId, u32>,
    inst_id: InstId,
    index: usize,
    replacement: ValueId,
) {
    let mut position = 0;
    func.inst_mut(inst_id).kind.visit_operands_mut(|operand| {
        if position == index {
            uses[*operand] -= 1;
            uses[replacement] += 1;
            *operand = replacement;
        }
        position += 1;
    });
}

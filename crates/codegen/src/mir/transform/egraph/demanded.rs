//! Demanded-bits simplification of effectful operands.
//!
//! Some opcodes read only the low bits of an operand: `MSTORE8` stores the low 8 bits of its
//! value. The opcode table declares these widths as `input_bits`. A value can have other users
//! that need every bit, so the e-graph cannot rewrite the value itself. This step instead
//! replaces the operand of the effectful instruction with a cheaper value that agrees on the
//! demanded bits. It is an analysis over operand chains with use-count conditions, which ISLE
//! rules on one instruction cannot express.
//!
//! This is LLVM's demanded-bits simplification, one use at a time: InstCombine's
//! `SimplifyDemandedUseBits` changes an instruction with one user in place, while
//! `SimplifyMultipleUseDemandedBits` only substitutes an equivalent value at the current use and
//! never edits a shared definition. The mask rules follow
//! `DemandedBits::determineLiveOperandBits`. LLVM IR types a narrow store, so the demand arrives
//! as a `trunc`; `MSTORE8` takes a full word, so it comes from `input_bits`. Unlike LLVM, this
//! does not join the demand of all users.
//!
//! Starting from the demanded mask of an operand, it walks the operand's definition:
//!
//! - `and(x, c)` is `x` when `c` covers the demanded bits, and zero when `c` misses all of them.
//! - `or(x, c)` and `xor(x, c)` are `x` when `c` misses all the demanded bits.
//! - `byte(31, x)` and `signextend(b, x)` are `x` when the demanded bits lie within the byte or
//!   sign-extended width.
//! - `shl(k, x)` and `shr(k, x)` are zero when they shift out every demanded bit. Otherwise they
//!   demand `mask >> k` and `mask << k` of `x`. The walk rewrites `x` inside the shift only when
//!   every value from the operand down to the shift has one user, since the rewrite changes the
//!   shift's result outside the demanded bits.
//!
//! A masked value with other users is replaced only when the unmasked value has other users too:
//! otherwise the store would keep both values live, which costs more stack traffic than the mask.
//! Address operands (160 bits) are excluded: the rules in `isle/mir/egraph/memory.isle` already
//! bypass their casts and masks, and only when the mask has no other user.
//!
//! The replaced definitions stay in place and later dead-code elimination removes them. Their
//! uses still count, so a chain whose only other user is such a dead leftover waits for the next
//! e-graph run. Only the operand changes, so instruction metadata keeps describing the same
//! access.

use crate::{
    backend::evm::op,
    mir::{
        EffectKind, Function, Immediate, InstId, InstKind, MirType, Value, ValueId,
        analysis::integers::integer_mask,
    },
};
use alloy_primitives::U256;
use solar_data_structures::index::IndexVec;

/// Operand definitions followed for one demanded mask.
const MAX_DEPTH: u32 = 4;

/// Operands that read this many bits or more are addresses or full words.
const MIN_EXCLUDED_BITS: u16 = 160;

type Uses = IndexVec<ValueId, u32>;

/// Narrows the demanded operands of every effectful instruction, returning the number of changes.
pub(super) fn simplify_demanded_operands(func: &mut Function) -> usize {
    let mut candidates = Vec::new();
    for inst_id in func.instructions() {
        let kind = &func.inst(inst_id).kind;
        if kind.op_def().effect == EffectKind::Pure {
            continue;
        }
        let Some(definition) = kind.evm_opcode().and_then(op::definition) else { continue };
        let operands = kind.operands();
        for (index, (&operand, &bits)) in operands.iter().zip(definition.input_bits).enumerate() {
            if 0 < bits && bits < MIN_EXCLUDED_BITS {
                candidates.push((inst_id, index, operand, integer_mask(bits.into())));
            }
        }
    }
    if candidates.is_empty() {
        return 0;
    }
    let uses = super::use_counts(func);
    let mut narrower = Narrower { func, uses, changed: 0 };
    for (inst_id, index, operand, demanded) in candidates {
        narrower.narrow_operand(inst_id, index, operand, demanded, MAX_DEPTH);
    }
    narrower.changed
}

/// Rewrites operands in place, keeping the use counts current.
struct Narrower<'a> {
    func: &'a mut Function,
    uses: Uses,
    changed: usize,
}

impl Narrower<'_> {
    /// Replaces operand `index` of `inst_id` with a simpler value that agrees on the `demanded`
    /// bits.
    fn narrow_operand(
        &mut self,
        inst_id: InstId,
        index: usize,
        operand: ValueId,
        demanded: U256,
        depth: u32,
    ) {
        let (simplified, exclusive) = self.simplify(operand, demanded, depth);
        // A shared value stays for its other users, so an unmasked replacement that nothing else
        // reads would be live next to it, which costs a stack copy that the mask did not.
        // Constants are free.
        let keeps_two_live = !exclusive
            && self.uses[simplified] <= 1
            && self.func.value(simplified).as_immediate().is_none();
        if simplified == operand || keeps_two_live {
            return;
        }
        // inst(.., operand, ..) => inst(.., simplified, ..)
        let mut position = 0;
        self.func.inst_mut(inst_id).kind.visit_operands_mut(|slot| {
            if position == index {
                self.uses[*slot] -= 1;
                self.uses[simplified] += 1;
                *slot = simplified;
            }
            position += 1;
        });
        self.changed += 1;
        tracing::trace!(
            target: super::TRACE_TARGET,
            function = %self.func.name,
            action = "demanded_operand",
            ?inst_id,
            index,
            "mir_egraph"
        );
    }

    /// Returns a value that agrees with `value` on the bits in `demanded`, following at most
    /// `depth` definitions, and whether every value on the path had one user.
    ///
    /// Only while every value from the demanded operand down to a shift has one user may the
    /// shift change in place, since no other instruction reads its result through the values
    /// above it.
    fn simplify(&mut self, mut value: ValueId, demanded: U256, depth: u32) -> (ValueId, bool) {
        let func = &*self.func;
        let ty = func.value_ty(value);
        let mut exclusive = true;
        for remaining in (0..depth).rev() {
            exclusive &= self.uses[value] == 1;
            let func = &*self.func;
            let Value::Inst(def) = *func.value(value) else { break };
            let step = match func.inst(def).kind {
                InstKind::And(x, c) => match func.value_u256(c) {
                    Some(mask) if mask & demanded == U256::ZERO => Step::Zero,
                    Some(mask) if mask & demanded == demanded => Step::Operand(x),
                    _ => Step::Stop,
                },
                InstKind::Or(x, c) | InstKind::Xor(x, c) => match func.value_u256(c) {
                    Some(constant) if constant & demanded == U256::ZERO => Step::Operand(x),
                    _ => Step::Stop,
                },
                InstKind::Byte(index, x) => match func.value_u256(index) {
                    Some(index) if index == U256::from(31) && (demanded >> 8usize).is_zero() => {
                        Step::Operand(x)
                    }
                    _ => Step::Stop,
                },
                InstKind::SignExtend(index, x) => match func.value_u256(index) {
                    Some(index)
                        if index >= U256::from(31)
                            || (demanded >> (8 * (index.to::<usize>() + 1))).is_zero() =>
                    {
                        Step::Operand(x)
                    }
                    _ => Step::Stop,
                },
                InstKind::Shl(count, x) => match func.value_u256(count) {
                    Some(count) => shift_step(demanded >> count, x),
                    None => Step::Stop,
                },
                InstKind::Shr(count, x) => match func.value_u256(count) {
                    Some(count) => shift_step(demanded << count, x),
                    None => Step::Stop,
                },
                _ => Step::Stop,
            };
            match step {
                Step::Operand(x) if func.value_ty(x) == ty => value = x,
                Step::Zero if ty == Some(MirType::I256) => {
                    let zero = self.func.alloc_value(Value::Immediate(Immediate::I256(U256::ZERO)));
                    self.uses.resize(self.func.num_values(), 0);
                    return (zero, exclusive);
                }
                Step::Shifted(x, inner) => {
                    if exclusive {
                        // shift(count, x) => shift(count, x narrowed to the inner demanded bits)
                        self.narrow_operand(def, 1, x, inner, remaining);
                    }
                    break;
                }
                _ => break,
            }
        }
        (value, exclusive)
    }
}

/// What an instruction contributes to the demanded bits of its result.
enum Step {
    /// The result agrees with this value on every demanded bit.
    Operand(ValueId),
    /// Every demanded bit is zero.
    Zero,
    /// A shift of its operand: narrow that operand to the given demanded bits in place.
    Shifted(ValueId, U256),
    /// Nothing is known.
    Stop,
}

/// Classifies a shift by the bits it demands of its operand: none when it shifts out every
/// demanded bit.
fn shift_step(inner: U256, operand: ValueId) -> Step {
    if inner.is_zero() { Step::Zero } else { Step::Shifted(operand, inner) }
}

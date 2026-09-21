//! ISLE rewrite rules for the e-graph pass.
//!
//! The rules live in `isle/mir/egraph.isle` and `isle/mir/word.isle`. The instruction vocabulary
//! they match on is generated from the MIR operation schema into `isle/mir/prelude.isle`, and
//! `build.rs` compiles both into Rust. This module implements the extractors
//! and constructors the rules call. Root operations and nested definitions expose
//! constants on the right of declared commutative pairs and comparisons, using
//! the same canonicalization as e-graph insertion and materialization.

use super::{OperandViews, canonical_operands, same_value};
use crate::{
    backend::evm::op,
    mir::{
        ArgIdx, BlockId, Function, Immediate, InstKind, MemoryObjectKind, MemoryObjectLayout,
        MirType, Op, Value as MirValue, ValueId,
        memory::{EvmMemoryLayout, MemoryLayoutPolicy},
        utils::eval::eval_opcode,
    },
};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::index::IndexVec;

/// Rewrite-rule name of a MIR value.
type Value = ValueId;

/// Bound on the results one `multi` term collects, required by generated code.
const MAX_ISLE_RETURNS: usize = 8;

#[allow(
    clippy::all,
    clippy::nursery,
    clippy::pedantic,
    dead_code,
    non_camel_case_types,
    non_snake_case,
    rust_2018_idioms,
    unnameable_types,
    unreachable_code,
    unreachable_patterns,
    unreachable_pub,
    unused_imports,
    unused_mut,
    unused_variables
)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/egraph.isle.rs"));
}

/// Context the rewrite rules run against: one function plus its value table.
pub(super) struct RuleContext<'a> {
    func: &'a mut Function,
    evm_version: EvmVersion,
    /// Original block of the root, for rules that must not extend cross-block dependencies.
    block: Option<BlockId>,
    /// Pre-pass use counts for profitability guards, when available.
    uses: Option<&'a IndexVec<ValueId, u32>>,
    /// Up to two retained equivalent definitions exposed during bounded matching.
    views: OperandViews,
    integer_ty: MirType,
}

impl<'a> RuleContext<'a> {
    /// Creates a context over `func`.
    pub(super) fn new(func: &'a mut Function, evm_version: EvmVersion) -> Self {
        Self {
            func,
            evm_version,
            block: None,
            uses: None,
            views: [None; 2],
            integer_ty: MirType::I256,
        }
    }

    /// Restricts placement-sensitive matching to producers in this block.
    pub(super) fn with_block(mut self, block: BlockId) -> Self {
        self.block = Some(block);
        self
    }

    /// Supplies existing use counts without rebuilding use information per rule.
    pub(super) fn with_uses(mut self, uses: &'a IndexVec<ValueId, u32>) -> Self {
        self.uses = Some(uses);
        self
    }

    /// Exposes an existing operand class alternative without rewriting its definition.
    pub(super) fn with_views(mut self, views: OperandViews) -> Self {
        self.views = views;
        self
    }

    /// Appends every equivalent instruction the rules can build for `op`.
    pub(super) fn rewrite(&mut self, op: &Op, alternatives: &mut Vec<Op>) {
        let op = canonical_operands(self.func, *op);
        self.integer_ty = self.operation_type(&op).unwrap_or(MirType::I256);
        generated::constructor_rewrite(self, &op, alternatives);
    }

    /// Returns the value `op` is equal to, when a rule applies.
    pub(super) fn simplify(&mut self, op: &Op) -> Option<ValueId> {
        let op = canonical_operands(self.func, *op);
        self.integer_ty = self.operation_type(&op).unwrap_or(MirType::I256);
        generated::constructor_simplify(self, &op)
    }

    fn integer_mask(&self) -> U256 {
        crate::mir::analysis::integers::integer_mask(self.integer_ty.integer_bits().unwrap())
    }

    fn operation_type(&self, op: &Op) -> Option<MirType> {
        let value = match *op {
            Op::Select { true_val, .. } => true_val,
            Op::Eq { a, .. }
            | Op::Ne { a, .. }
            | Op::Lt { a, .. }
            | Op::Gt { a, .. }
            | Op::SLt { a, .. }
            | Op::SGt { a, .. } => a,
            _ if op.result_kind() == crate::mir::ResultKind::Integer => op.first_operand()?,
            _ => return None,
        };
        self.func.value_ty(value).filter(|ty| ty.integer_bits().is_some())
    }

    fn has_const(&self, value: ValueId, expected: U256) -> bool {
        self.func.value_u256(value) == Some(expected)
    }
}

fn defining_kind(func: &Function, value: ValueId) -> Option<&InstKind> {
    match func.value(value) {
        MirValue::Inst(inst_id) => Some(&func.inst(*inst_id).kind),
        _ => None,
    }
}

/// How many defining instructions [`max_bits`] follows before giving up.
const MAX_BITS_DEPTH: u32 = 8;

/// Returns an upper bound on the number of significant bits of `value`.
///
/// Immediates and a few instructions bound their result exactly: comparisons
/// are one bit, `byte` is eight, addresses produced by an opcode are 160, and
/// the arithmetic and bitwise operations bound their result from their
/// operands. Phis take the widest input. Stop traversing operands once the
/// result is determined; unknown values may hold any word.
fn max_bits(func: &Function, value: ValueId, depth: u32) -> u32 {
    max_bits_with_args(func, value, depth, &|_| 256)
}

/// Bounds a value using caller-proved argument widths instead of nominal types.
pub(in crate::mir::transform) fn max_bits_with_args(
    func: &Function,
    value: ValueId,
    depth: u32,
    argument_bits: &impl Fn(ArgIdx) -> u32,
) -> u32 {
    if let Some(bits) = func.value_ty(value).and_then(MirType::integer_bits)
        && bits < 256
    {
        return func.value_u256(value).map_or(bits, |constant| constant.bit_len() as u32);
    }
    if let Some(constant) = func.value_u256(value) {
        return constant.bit_len() as u32;
    }
    if let MirValue::Arg(index) = func.value(value) {
        return argument_bits(*index);
    }
    if depth == 0 {
        return 256;
    }
    let Some(kind) = defining_kind(func, value) else { return 256 };
    if let Some(definition) = kind.evm_opcode().and_then(op::definition)
        && definition.result_bits < 256
    {
        return u32::from(definition.result_bits);
    }
    let bits = |value| max_bits_with_args(func, value, depth - 1, argument_bits);
    let shift = |shift| func.value_u256(shift).map(|shift| shift.min(U256::from(256)).to::<u32>());
    match *kind {
        InstKind::Zext(value) | InstKind::Bitcast(value) => bits(value),
        InstKind::And(a, b) => {
            let a = bits(a);
            if a == 0 { 0 } else { a.min(bits(b)) }
        }
        InstKind::Or(a, b) | InstKind::Xor(a, b) | InstKind::Select(_, a, b) => {
            let a = bits(a);
            if a == 256 { 256 } else { a.max(bits(b)) }
        }
        InstKind::Add(a, b) => {
            let a = bits(a);
            if a >= 255 { 256 } else { (a.max(bits(b)) + 1).min(256) }
        }
        InstKind::Mul(a, b) => {
            let a = bits(a);
            let b = bits(b);
            match (a, b) {
                (0, _) | (_, 0) => 0,
                (1, _) => b,
                (_, 1) => a,
                _ => (a + b).min(256),
            }
        }
        InstKind::Shl(amount, value) => match shift(amount) {
            Some(256) => 0,
            Some(amount) => (bits(value) + amount).min(256),
            None => 256,
        },
        InstKind::Shr(amount, value) => match shift(amount) {
            Some(amount) => bits(value).saturating_sub(amount),
            None => 256,
        },
        InstKind::Div(value, divisor) => match func.value_u256(divisor) {
            Some(divisor) if divisor.is_zero() => 0,
            Some(divisor) => bits(value).saturating_sub(divisor.bit_len() as u32 - 1),
            None => bits(value),
        },
        InstKind::Mod(value, modulus) => match func.value_u256(modulus) {
            Some(modulus) => bits(value).min(modulus.bit_len() as u32),
            None => bits(value),
        },
        InstKind::Phi(ref incoming) => {
            if incoming.is_empty() {
                return 256;
            }
            let mut widest = 0;
            for &(_, value) in incoming {
                widest = widest.max(bits(value));
                if widest == 256 {
                    break;
                }
            }
            widest
        }
        _ => 256,
    }
}

/// Returns whether `value` is always below `bound`.
fn below(func: &Function, value: ValueId, bound: U256) -> bool {
    let bits = max_bits(func, value, MAX_BITS_DEPTH);
    bits < 256 && bound >= U256::ONE << bits
}

/// Returns whether `value` never exceeds `bound`.
fn at_most(func: &Function, value: ValueId, bound: U256) -> bool {
    let bits = max_bits(func, value, MAX_BITS_DEPTH);
    bits < 256 && bound >= (U256::ONE << bits) - U256::ONE
}

/// Returns whether the value carries the canonical boolean invariant.
pub(in crate::mir::transform) fn is_bool_value(func: &Function, value: ValueId) -> bool {
    func.value_ty(value) == Some(crate::mir::MirType::I1)
}

fn has_known_sign_bit(func: &Function, value: ValueId) -> bool {
    if let Some(constant) = func.value_u256(value) {
        let bits = func.value_ty(value).and_then(MirType::integer_bits).unwrap_or(256);
        return constant.bit((bits - 1) as usize);
    }
    match defining_kind(func, value) {
        Some(InstKind::Or(a, b)) => has_known_sign_bit(func, *a) || has_known_sign_bit(func, *b),
        Some(InstKind::Sar(_, value)) => has_known_sign_bit(func, *value),
        _ => false,
    }
}

const UINT160_MASK: U256 = U256::from_limbs([u64::MAX, u64::MAX, u32::MAX as u64, 0]);

impl generated::Context for RuleContext<'_> {
    fn single_use(&mut self, value: Value) -> bool {
        self.uses.and_then(|uses| uses.get(value)) == Some(&1)
    }

    fn inst_data(&mut self, value: Value) -> Option<Op> {
        self.views
            .iter()
            .flatten()
            .find(|&&(operand, _)| operand == value)
            .map(|&(_, op)| op)
            .or_else(|| defining_kind(self.func, value).map(InstKind::op))
            .filter(|op| {
                matches!(
                    op,
                    Op::Eq { .. }
                        | Op::Ne { .. }
                        | Op::Zext { .. }
                        | Op::Trunc { .. }
                        | Op::Sext { .. }
                        | Op::PtrToInt { .. }
                        | Op::IntToPtr { .. }
                        | Op::Bitcast { .. }
                ) || if op.result_kind() == crate::mir::ResultKind::Integer
                    || matches!(op, Op::Select { .. })
                {
                    self.func
                        .value_ty(value)
                        .filter(|ty| ty.integer_bits().is_some())
                        .unwrap_or(MirType::I256)
                        == self.integer_ty
                } else {
                    self.operation_type(op).unwrap_or(MirType::I256) == self.integer_ty
                }
            })
            .map(|op| canonical_operands(self.func, op))
    }

    fn iconst(&mut self, value: Value) -> Option<U256> {
        self.func.value_u256(value)
    }

    fn nonzero_const(&mut self, value: Value) -> Option<U256> {
        self.func.value_u256(value).filter(|constant| !constant.is_zero())
    }

    fn zero(&mut self, value: Value) -> Option<()> {
        self.has_const(value, U256::ZERO).then_some(())
    }

    fn one(&mut self, value: Value) -> Option<()> {
        self.has_const(value, U256::from(1)).then_some(())
    }

    fn all_ones(&mut self, value: Value) -> Option<()> {
        let value = self.func.value_u256(value)?;
        (value == self.integer_mask()).then_some(())
    }

    fn bool_value(&mut self, value: Value) -> Option<()> {
        is_bool_value(self.func, value).then_some(())
    }

    fn integer_bits(&mut self, value: Value) -> Option<u32> {
        let MirType::Int(bits) = self.func.value_ty(value)? else { return None };
        (bits.get() <= 256).then_some(bits.get())
    }

    fn u32_lt(&mut self, a: u32, b: u32) -> bool {
        a < b
    }

    fn u32_le(&mut self, a: u32, b: u32) -> bool {
        a <= b
    }

    fn current_address(&mut self, value: Value) -> Option<()> {
        let value = match defining_kind(self.func, value) {
            Some(InstKind::Zext(inner)) => *inner,
            _ => value,
        };
        matches!(defining_kind(self.func, value), Some(InstKind::Address)).then_some(())
    }

    fn is_zero_or_one(&mut self, value: Value) -> bool {
        self.has_const(value, U256::ZERO) || self.has_const(value, U256::from(1))
    }

    fn masks_clean_address(&mut self, mask: U256, value: Value) -> bool {
        mask == UINT160_MASK && max_bits(self.func, value, MAX_BITS_DEPTH) <= 160
    }

    fn below_const(&mut self, value: Value, bound: U256) -> bool {
        below(self.func, value, bound)
    }

    fn at_most_const(&mut self, value: Value, bound: U256) -> bool {
        at_most(self.func, value, bound)
    }

    fn shifted_out(&mut self, shift: U256, value: Value) -> bool {
        shift >= U256::from(max_bits(self.func, value, MAX_BITS_DEPTH))
    }

    fn sign_clear(&mut self, byte: U256, value: Value) -> bool {
        // `signextend(b, x)` reads bit `8 * (b + 1) - 1` as the sign.
        byte < U256::from(31)
            && max_bits(self.func, value, MAX_BITS_DEPTH) < 8 * (byte.to::<u32>() + 1)
    }

    fn mask_covers(&mut self, mask: U256, value: Value) -> bool {
        // A mask of the form `2^n - 1` keeps `n` low bits.
        let contiguous = mask.wrapping_add(U256::ONE) & mask == U256::ZERO;
        contiguous && max_bits(self.func, value, MAX_BITS_DEPTH) <= mask.bit_len() as u32
    }

    fn differ(&mut self, a: Value, b: Value) -> bool {
        a != b
    }

    fn has_bitwise_shifting(&mut self) -> bool {
        self.evm_version.has_bitwise_shifting()
    }

    fn has_self_balance(&mut self) -> bool {
        self.evm_version.has_self_balance()
    }

    fn imm(&mut self, value: U256) -> Value {
        self.func
            .alloc_value(MirValue::Immediate(Immediate::for_type(Some(self.integer_ty), value)))
    }

    fn imm_bool(&mut self, value: bool) -> Value {
        self.func.alloc_value(MirValue::Immediate(Immediate::I1(value)))
    }

    fn word_bits(&mut self) -> u64 {
        u64::from(self.integer_ty.integer_bits().unwrap())
    }

    fn sign_bit(&mut self) -> u64 {
        self.word_bits() - 1
    }

    fn word_type(&mut self) -> bool {
        self.integer_ty == MirType::I256
    }

    fn u256(&mut self, value: u64) -> U256 {
        U256::from(value)
    }

    fn u256_max(&mut self) -> U256 {
        self.integer_mask()
    }

    fn u256_not(&mut self, value: U256) -> U256 {
        !value & self.integer_mask()
    }

    fn u256_is_zero(&mut self, value: U256) -> bool {
        value.is_zero()
    }

    fn u256_is_one(&mut self, value: U256) -> bool {
        value == U256::from(1)
    }

    fn u256_is_all_ones(&mut self, value: U256) -> bool {
        value == self.integer_mask()
    }

    fn u256_gt(&mut self, value: U256, limit: u64) -> bool {
        value > U256::from(limit)
    }

    fn u256_ge(&mut self, value: U256, limit: u64) -> bool {
        value >= U256::from(limit)
    }

    fn u256_lt(&mut self, value: U256, limit: u64) -> bool {
        value < U256::from(limit)
    }

    fn u256_eq(&mut self, value: U256, expected: u64) -> bool {
        value == U256::from(expected)
    }

    fn u256_has_bits(&mut self, value: U256, bits: u64) -> bool {
        let bits = U256::from(bits);
        value & bits == bits
    }

    fn u256_add(&mut self, a: U256, b: U256) -> U256 {
        a.wrapping_add(b) & self.integer_mask()
    }

    fn u256_sub(&mut self, a: U256, b: U256) -> U256 {
        a.wrapping_sub(b) & self.integer_mask()
    }

    fn u256_neg(&mut self, value: U256) -> U256 {
        U256::ZERO.wrapping_sub(value) & self.integer_mask()
    }

    fn u256_and(&mut self, a: U256, b: U256) -> U256 {
        a & b
    }

    fn u256_shl(&mut self, shift: U256, value: U256) -> U256 {
        eval_opcode(op::SHL, &[shift, value]).expect("SHL has word semantics") & self.integer_mask()
    }

    fn u256_shr(&mut self, shift: U256, value: U256) -> U256 {
        eval_opcode(op::SHR, &[shift, value]).expect("SHR has word semantics")
    }

    fn u256_byte(&mut self, index: U256, value: U256) -> U256 {
        eval_opcode(op::BYTE, &[index, value]).expect("BYTE has word semantics")
    }

    fn in_current_block(&mut self, value: Value) -> bool {
        self.block.is_some_and(|block| {
            matches!(self.func.value(value), MirValue::Inst(inst)
                if self.func.blocks[block].instructions.contains(inst))
        })
    }

    fn u256_same(&mut self, a: U256, b: U256) -> bool {
        a == b
    }

    fn u256_from_limbs(&mut self, a: u64, b: u64, c: u64, d: u64) -> U256 {
        U256::from_limbs([a, b, c, d])
    }

    fn shift_sum(&mut self, a: U256, b: U256) -> U256 {
        let width = U256::from(self.integer_ty.integer_bits().unwrap());
        (a.min(width) + b.min(width)).min(width)
    }

    fn sign_byte(&mut self, shift: U256) -> Option<U256> {
        if self.integer_ty == MirType::I256
            && shift < U256::from(256)
            && shift.byte(0).is_multiple_of(8)
        {
            Some(U256::from(31 - shift.to::<u32>() / 8))
        } else {
            None
        }
    }

    fn u256_min(&mut self, a: U256, b: U256) -> U256 {
        a.min(b)
    }

    fn u256_le(&mut self, a: U256, b: U256) -> bool {
        a <= b
    }

    fn power_of_two_shift(&mut self, value: U256) -> Option<U256> {
        if value.is_zero() || (value & (value - U256::from(1))) != U256::ZERO {
            return None;
        }
        let shift = U256::from(value.trailing_zeros());
        (!shift.is_zero()).then_some(shift)
    }

    fn same_value(&mut self, a: Value, b: Value) -> bool {
        same_value(self.func, a, b)
    }

    fn has_known_sign_bit(&mut self, value: Value) -> bool {
        has_known_sign_bit(self.func, value)
    }

    fn object_data_offset(&mut self, kind: MemoryObjectKind) -> u64 {
        EvmMemoryLayout::object_data_offset(kind)
    }

    fn field_offset(&mut self, layout: MemoryObjectLayout, field: u64) -> Option<u64> {
        EvmMemoryLayout::field_offset(layout, field)
    }

    fn layout_kind(&mut self, layout: MemoryObjectLayout) -> MemoryObjectKind {
        layout.kind()
    }
}

//! ISLE rewrite rules for local instruction simplification.
//!
//! The rules live in `isle/inst_simplify.isle`. The instruction vocabulary
//! they match on is generated from the MIR operation schema into
//! `isle/prelude.isle`, and `build.rs` compiles both into Rust. This module
//! implements the extractors and constructors the rules call.

use super::InstSimplifier;
use crate::{
    memory::{EvmMemoryLayout, MemoryLayoutPolicy},
    mir::{
        Function, Immediate, InstKind, MemoryObjectKind, MemoryObjectLayout, Op, Value as MirValue,
        ValueId,
    },
};
use alloy_primitives::U256;
use solar_config::EvmVersion;

/// Rewrite-rule name of a MIR value.
type Value = ValueId;

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
    unreachable_pub,
    unused_imports,
    unused_mut,
    unused_variables
)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/inst_simplify.isle.rs"));
}

/// Context the rewrite rules run against: one function plus its value table.
pub(crate) struct RuleContext<'a> {
    func: &'a mut Function,
    evm_version: EvmVersion,
}

impl<'a> RuleContext<'a> {
    /// Creates a context over `func`.
    pub(crate) fn new(func: &'a mut Function, evm_version: EvmVersion) -> Self {
        Self { func, evm_version }
    }

    /// Returns a cheaper instruction to compute in place of `op`, when a rule applies.
    pub(crate) fn rewrite(&mut self, op: &Op) -> Option<Op> {
        generated::constructor_rewrite(self, op)
    }

    /// Returns the value `op` is equal to, when a rule applies.
    pub(crate) fn simplify(&mut self, op: &Op) -> Option<ValueId> {
        generated::constructor_simplify(self, op)
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

/// Returns whether `value` is known to be exactly zero or one.
///
/// Solidity's `bool` type does not prove that the EVM word is canonical:
/// inline assembly can assign dirty words to variables, arguments, and return
/// values. Only boolean immediates and values produced by an EVM comparison
/// are known to be exactly zero or one.
fn is_bool_value(func: &Function, value: ValueId) -> bool {
    match func.value(value) {
        MirValue::Immediate(Immediate::Bool(_)) => true,
        MirValue::Inst(inst_id) => matches!(
            func.inst(*inst_id).kind,
            InstKind::Lt(..)
                | InstKind::Gt(..)
                | InstKind::SLt(..)
                | InstKind::SGt(..)
                | InstKind::Eq(..)
                | InstKind::IsZero(..)
        ),
        MirValue::Arg(_) | MirValue::Immediate(_) | MirValue::Undef(_) | MirValue::Error(_) => {
            false
        }
    }
}

/// Returns whether `value` is an address produced by an EVM opcode.
fn is_clean_address(func: &Function, value: ValueId) -> bool {
    matches!(
        defining_kind(func, value),
        Some(
            InstKind::Address
                | InstKind::Caller
                | InstKind::Origin
                | InstKind::Coinbase
                | InstKind::Create(..)
                | InstKind::Create2(..)
        )
    )
}

fn has_known_sign_bit(func: &Function, value: ValueId) -> bool {
    if let Some(value) = func.value_u256(value) {
        return value.bit(255);
    }
    match defining_kind(func, value) {
        Some(InstKind::Or(a, b)) => has_known_sign_bit(func, *a) || has_known_sign_bit(func, *b),
        Some(InstKind::Sar(_, value)) => has_known_sign_bit(func, *value),
        _ => false,
    }
}

const UINT160_MASK: U256 = U256::from_limbs([u64::MAX, u64::MAX, u32::MAX as u64, 0]);

impl generated::Context for RuleContext<'_> {
    fn inst_data(&mut self, value: Value) -> Option<Op> {
        defining_kind(self.func, value).map(InstKind::op)
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
        self.has_const(value, U256::MAX).then_some(())
    }

    fn uint160_mask(&mut self, value: Value) -> Option<()> {
        self.has_const(value, UINT160_MASK).then_some(())
    }

    fn bool_value(&mut self, value: Value) -> Option<()> {
        is_bool_value(self.func, value).then_some(())
    }

    fn clean_address(&mut self, value: Value) -> Option<()> {
        is_clean_address(self.func, value).then_some(())
    }

    fn current_address(&mut self, value: Value) -> Option<()> {
        matches!(defining_kind(self.func, value), Some(InstKind::Address)).then_some(())
    }

    fn is_const(&mut self, value: Value) -> bool {
        self.func.value_u256(value).is_some()
    }

    fn is_zero_or_one(&mut self, value: Value) -> bool {
        self.has_const(value, U256::ZERO) || self.has_const(value, U256::from(1))
    }

    fn masks_clean_address(&mut self, mask: U256, value: Value) -> bool {
        mask == UINT160_MASK && is_clean_address(self.func, value)
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
        self.func.alloc_value(MirValue::Immediate(Immediate::uint256(value)))
    }

    fn imm_bool(&mut self, value: bool) -> Value {
        self.func.alloc_value(MirValue::Immediate(Immediate::bool(value)))
    }

    fn u256(&mut self, value: u64) -> U256 {
        U256::from(value)
    }

    fn u256_max(&mut self) -> U256 {
        U256::MAX
    }

    fn u256_not(&mut self, value: U256) -> U256 {
        !value
    }

    fn u256_is_zero(&mut self, value: U256) -> bool {
        value.is_zero()
    }

    fn u256_is_one(&mut self, value: U256) -> bool {
        value == U256::from(1)
    }

    fn u256_is_all_ones(&mut self, value: U256) -> bool {
        value == U256::MAX
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
        a.wrapping_add(b)
    }

    fn u256_sub(&mut self, a: U256, b: U256) -> U256 {
        a.wrapping_sub(b)
    }

    fn u256_neg(&mut self, value: U256) -> U256 {
        U256::ZERO.wrapping_sub(value)
    }

    fn u256_and(&mut self, a: U256, b: U256) -> U256 {
        a & b
    }

    fn power_of_two_shift(&mut self, value: U256) -> Option<U256> {
        if value.is_zero() || (value & (value - U256::from(1))) != U256::ZERO {
            return None;
        }
        let shift = U256::from(value.trailing_zeros());
        (!shift.is_zero()).then_some(shift)
    }

    fn same_value(&mut self, a: Value, b: Value) -> bool {
        InstSimplifier::same_value(self.func, a, b)
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

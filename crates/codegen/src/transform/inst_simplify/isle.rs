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
pub(super) struct RuleContext<'a> {
    func: &'a mut Function,
}

impl<'a> RuleContext<'a> {
    /// Creates a context over `func`.
    pub(super) fn new(func: &'a mut Function) -> Self {
        Self { func }
    }

    /// Returns the value `op` is equal to, when a rule applies.
    pub(super) fn simplify(&mut self, op: &Op) -> Option<ValueId> {
        generated::constructor_simplify(self, op)
    }

    fn defining_kind(&self, value: ValueId) -> Option<&InstKind> {
        match self.func.value(value) {
            MirValue::Inst(inst_id) => Some(&self.func.inst(*inst_id).kind),
            _ => None,
        }
    }

    fn is_const(&self, value: ValueId, expected: U256) -> bool {
        self.func.value_u256(value) == Some(expected)
    }
}

impl generated::Context for RuleContext<'_> {
    fn inst_data(&mut self, value: Value) -> Option<Op> {
        self.defining_kind(value).map(InstKind::op)
    }

    fn iconst(&mut self, value: Value) -> Option<U256> {
        self.func.value_u256(value)
    }

    fn zero(&mut self, value: Value) -> Option<()> {
        self.is_const(value, U256::ZERO).then_some(())
    }

    fn one(&mut self, value: Value) -> Option<()> {
        self.is_const(value, U256::from(1)).then_some(())
    }

    fn all_ones(&mut self, value: Value) -> Option<()> {
        self.is_const(value, U256::MAX).then_some(())
    }

    fn uint160_mask(&mut self, value: Value) -> Option<()> {
        let mask = (U256::from(1) << 160) - U256::from(1);
        self.is_const(value, mask).then_some(())
    }

    fn bool_value(&mut self, value: Value) -> Option<()> {
        InstSimplifier::is_bool_value(self.func, value).then_some(())
    }

    fn clean_address(&mut self, value: Value) -> Option<()> {
        InstSimplifier::is_clean_address(self.func, value).then_some(())
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

    fn u256_gt(&mut self, value: U256, limit: u64) -> bool {
        value > U256::from(limit)
    }

    fn u256_ge(&mut self, value: U256, limit: u64) -> bool {
        value >= U256::from(limit)
    }

    fn u256_lt(&mut self, value: U256, limit: u64) -> bool {
        value < U256::from(limit)
    }

    fn u256_has_bits(&mut self, value: U256, bits: u64) -> bool {
        let bits = U256::from(bits);
        value & bits == bits
    }

    fn same_value(&mut self, a: Value, b: Value) -> bool {
        InstSimplifier::same_value(self.func, a, b)
    }

    fn has_known_sign_bit(&mut self, value: Value) -> bool {
        InstSimplifier::has_known_sign_bit(self.func, value)
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

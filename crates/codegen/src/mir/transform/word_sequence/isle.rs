//! Read-only ISLE matching and recipe construction in a private value namespace.
//!
//! Temporary IDs start beyond the function's value domain and can only refer
//! to earlier temporaries. They are resolved when a winning recipe is committed;
//! rejected recipes leave the function and its metadata untouched.

use super::{Recipe, Temporary};
use crate::{
    mir::{Function, InstId, MirType, Op, Value as MirValue, ValueId},
    target::Target,
};
use alloy_primitives::U256;
use solar_data_structures::map::{FxHashMap, FxHashSet};

type Value = ValueId;
const MAX_ISLE_RETURNS: usize = 16;
const MAX_TEMPORARIES: usize = 64;

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
    include!(concat!(env!("OUT_DIR"), "/word_sequence.isle.rs"));
}

pub(super) fn alternatives(
    func: &Function,
    seen: &FxHashSet<InstId>,
    op: &Op,
    target: Target,
) -> Vec<Recipe> {
    let mut result = Vec::new();
    let scalar_ty = op
        .into_kind()
        .and_then(|kind| {
            kind.operands()
                .iter()
                .filter_map(|&value| func.value_ty(value))
                .filter(|ty| matches!(ty, MirType::Int(_)))
                .max_by_key(|ty| ty.integer_bits())
        })
        .unwrap_or(MirType::I256);
    generated::constructor_sequence_rewrite(
        &mut Context { func, seen, target, scalar_ty, temporaries: FxHashMap::default() },
        op,
        &mut result,
    );
    result
}

struct Context<'a> {
    func: &'a Function,
    seen: &'a FxHashSet<InstId>,
    target: Target,
    scalar_ty: MirType,
    temporaries: FxHashMap<ValueId, Temporary>,
}

impl Context<'_> {
    fn temporary(&mut self, value: Temporary) -> Option<Value> {
        if self.temporaries.len() >= MAX_TEMPORARIES {
            return None;
        }
        let id = ValueId::new(self.func.num_values() + self.temporaries.len());
        self.temporaries.insert(id, value);
        Some(id)
    }
}

impl generated::Context for Context<'_> {
    fn inst_data(&mut self, value: Value) -> Option<Op> {
        let MirValue::Inst(inst) = self.func.value(value) else { return None };
        let kind = &self.func.inst(*inst).kind;
        (self.seen.contains(inst)
            && kind.operands().iter().all(|&operand| {
                self.func
                    .value_ty(operand)
                    .is_none_or(|ty| ty == MirType::I1 || ty == self.scalar_ty)
            }))
        .then(|| kind.op())
    }

    fn make(&mut self, op: &Op) -> Option<Value> {
        self.temporary(Temporary::Operation(*op))
    }

    fn sequence(&mut self, root: &Op) -> Recipe {
        Recipe { root: *root, scalar_ty: self.scalar_ty, temporaries: self.temporaries.clone() }
    }

    fn imm(&mut self, value: U256) -> Option<Value> {
        self.temporary(Temporary::Constant(value))
    }

    fn iconst(&mut self, value: Value) -> Option<U256> {
        self.func.value_u256(value)
    }
    fn zero(&mut self, value: Value) -> Option<()> {
        (self.func.value_u256(value) == Some(U256::ZERO)).then_some(())
    }
    fn one(&mut self, value: Value) -> Option<()> {
        (self.func.value_u256(value) == Some(U256::ONE)).then_some(())
    }
    fn all_ones(&mut self, value: Value) -> Option<()> {
        (self.func.value_u256(value)
            == Some(U256::MAX >> (256 - self.scalar_ty.integer_bits().unwrap())))
        .then_some(())
    }
    fn u256(&mut self, value: u64) -> U256 {
        U256::from(value)
    }
    fn u256_max(&mut self) -> U256 {
        U256::MAX >> (256 - self.scalar_ty.integer_bits().unwrap())
    }
    fn u256_from_limbs(&mut self, a: u64, b: u64, c: u64, d: u64) -> U256 {
        U256::from_limbs([a, b, c, d])
    }
    fn bool_value(&mut self, value: Value) -> Option<()> {
        super::super::egraph::is_bool_value(self.func, value).then_some(())
    }

    fn optimize_for_size(&mut self) -> bool {
        self.target.optimization().is_size()
    }

    fn u256_ge(&mut self, a: U256, b: U256) -> bool {
        a >= b
    }

    fn u256_sub(&mut self, a: U256, b: U256) -> U256 {
        a.wrapping_sub(b) & (U256::MAX >> (256 - self.scalar_ty.integer_bits().unwrap()))
    }

    fn u256_same(&mut self, a: U256, b: U256) -> bool {
        a == b
    }
}

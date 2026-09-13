//! Read-only ISLE matching and recipe construction in a private value namespace.
//!
//! Temporary IDs start beyond the function's value domain and can only refer
//! to earlier temporaries. They are resolved when a winning recipe is committed;
//! rejected recipes leave the function and its metadata untouched.

use super::{Recipe, Temporary};
use crate::mir::{Function, InstId, Op, Value as MirValue, ValueId};
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

pub(super) fn alternatives(func: &Function, seen: &FxHashSet<InstId>, op: &Op) -> Vec<Recipe> {
    let mut result = Vec::new();
    generated::constructor_sequence_rewrite(
        &mut Context { func, seen, temporaries: FxHashMap::default() },
        op,
        &mut result,
    );
    result
}

struct Context<'a> {
    func: &'a Function,
    seen: &'a FxHashSet<InstId>,
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
        self.seen.contains(inst).then(|| self.func.inst(*inst).kind.op())
    }

    fn make(&mut self, op: &Op) -> Option<Value> {
        self.temporary(Temporary::Operation(*op))
    }

    fn sequence(&mut self, root: &Op) -> Recipe {
        Recipe { root: *root, temporaries: self.temporaries.clone() }
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
        (self.func.value_u256(value) == Some(U256::MAX)).then_some(())
    }
    fn u256(&mut self, value: u64) -> U256 {
        U256::from(value)
    }
    fn u256_max(&mut self) -> U256 {
        U256::MAX
    }
    fn u256_from_limbs(&mut self, a: u64, b: u64, c: u64, d: u64) -> U256 {
        U256::from_limbs([a, b, c, d])
    }
    fn u256_same(&mut self, a: U256, b: U256) -> bool {
        a == b
    }
}

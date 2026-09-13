//! Stack-local equivalent-expression rules and their availability extractors.
//!
//! Only definitions earlier in the current block are exposed. In particular,
//! a resident value carried around a loop must not be mistaken for a fresh
//! expression over the next iteration's phi values. Rules allocate no MIR
//! values and only propose alternatives; the parent module prices real plans.

use crate::{
    backend::evm::codegen::StackModel,
    mir::{BlockId, Function, Op, Value as MirValue, ValueId},
};

type Value = ValueId;
const MAX_ISLE_RETURNS: usize = 16;

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
    include!(concat!(env!("OUT_DIR"), "/stack_select.isle.rs"));
}

pub(super) fn alternatives(
    func: &Function,
    stack: &StackModel,
    block: BlockId,
    index: usize,
    op: &Op,
) -> Vec<Op> {
    let mut result = Vec::new();
    generated::constructor_stack_rewrite(
        &mut RuleContext { func, stack, block, index },
        op,
        &mut result,
    );
    result
}

struct RuleContext<'a> {
    func: &'a Function,
    stack: &'a StackModel,
    block: BlockId,
    index: usize,
}

impl generated::Context for RuleContext<'_> {
    fn inst_data(&mut self, value: Value) -> Option<Op> {
        let MirValue::Inst(inst) = self.func.value(value) else { return None };
        self.func.blocks[self.block].instructions[..self.index]
            .contains(inst)
            .then(|| self.func.inst(*inst).kind.op())
    }

    fn resident(&mut self, op: &Op) -> Option<Value> {
        let op = op.canonicalize_commutative();
        for value in self.stack.iter().flatten().take(16) {
            if let Some(definition) = self.inst_data(value)
                && definition.canonicalize_commutative() == op
            {
                return Some(value);
            }
        }
        None
    }
}

//! Carries the address of a contiguous run of full-word memory stores.
//!
//! ABI encoders and inline assembly often write adjacent words as `mstore(p,
//! a); mstore(p + 32, b); mstore(p + 64, c)`. Keeping `p` and rebuilding each
//! later address makes the stack scheduler duplicate a progressively deeper
//! base value. This pass turns a run with a constant positive stride into an
//! address recurrence, so the scheduler can retain the previous address:
//!
//! ```text
//! mstore(p, a); q = p + 32; mstore(q, b); r = p + 64; mstore(r, c)
//!     =>
//! mstore(p, a); q = p + 32; mstore(q, b); r = q + 32; mstore(r, c)
//! ```
//!
//! The pass recognizes any positive constant stride and any initial constant
//! offset, not only word-sized runs beginning at zero. It requires adjacent
//! `mstore` instructions in one block and rewrites only the third and later
//! addresses of a run. The first two establish the base and stride; changing
//! the later addresses is then addition modulo \(2^{256}\), exactly as on the
//! EVM.
//!
//! A run stays in its source order: each store remains between the definition
//! of its address and the next definition. Stores are effects, so this pass
//! neither crosses nor reorders them. It runs after expression canonicalization,
//! which otherwise folds a stepped address back to `base + offset`, and before
//! stack scheduling, which prices and realizes the longer live range.
//!
//! A candidate step's immediate must cost no more to materialize than the
//! direct offset it replaces. Descending or wraparound-looking runs therefore
//! keep their direct addresses rather than carrying a wide modular step.

use crate::{
    mir::{
        Function, Immediate, InstId, InstKind, MirType, Module, Value, ValueId,
        pass::{MirPass, ModuleAnalyses, run_function_pass},
    },
    target::Target,
};
use alloy_primitives::U256;
use solar_sema::Gcx;

pub(crate) struct MemoryAddressSteps;

impl MirPass for MemoryAddressSteps {
    fn name(&self) -> &'static str {
        "memory-address-steps"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        let target = Target::new(gcx);
        run_function_pass(module, analyses, |func, _| run(func, target))
    }
}

fn run(func: &mut Function, target: Target) -> bool {
    let mut changed = false;
    for block in func.blocks.indices().collect::<Vec<_>>() {
        let instructions = func.blocks[block].instructions.clone();
        let mut start = 0;
        while start + 4 < instructions.len() {
            let Some((base, first_offset)) = store_address(func, instructions[start]) else {
                start += 1;
                continue;
            };
            let Some((second, second_offset)) = stepped_store(func, &instructions, start + 1, base)
            else {
                start += 1;
                continue;
            };
            let step = second_offset.wrapping_sub(first_offset);
            if step.is_zero() || target.cmp(target.push(step), target.push(second_offset)).is_gt() {
                start += 1;
                continue;
            }

            let mut rewrites = Vec::new();
            let mut previous = second;
            let mut expected = second_offset.wrapping_add(step);
            let mut index = start + 3;
            while let Some((address, offset, add)) =
                stepped_store_with_def(func, &instructions, index, base)
            {
                if offset != expected {
                    break;
                }
                if target.cmp(target.push(step), target.push(offset)).is_gt() {
                    break;
                }
                rewrites.push((add, previous));
                previous = address;
                expected = expected.wrapping_add(step);
                index += 2;
            }
            if !rewrites.is_empty() {
                let step = immediate(func, step);
                for (add, previous) in rewrites {
                    func.inst_mut(add).replace_kind(InstKind::Add(previous, step));
                    changed = true;
                }
            }
            start = index;
        }
    }
    changed
}

fn store_address(func: &Function, store: InstId) -> Option<(ValueId, U256)> {
    let InstKind::MStore(address, _) = func.inst(store).kind else { return None };
    address_parts(func, address)
}

fn stepped_store(
    func: &Function,
    instructions: &[InstId],
    add_index: usize,
    base: ValueId,
) -> Option<(ValueId, U256)> {
    let (address, offset, _) = stepped_store_with_def(func, instructions, add_index, base)?;
    Some((address, offset))
}

fn stepped_store_with_def(
    func: &Function,
    instructions: &[InstId],
    add_index: usize,
    base: ValueId,
) -> Option<(ValueId, U256, InstId)> {
    let add = *instructions.get(add_index)?;
    let store = *instructions.get(add_index + 1)?;
    let address = func.inst_result_value(add)?;
    let (found_base, offset) = address_parts(func, address)?;
    (found_base == base
        && matches!(func.inst(store).kind, InstKind::MStore(found, _) if found == address))
    .then_some((address, offset, add))
}

fn address_parts(func: &Function, address: ValueId) -> Option<(ValueId, U256)> {
    let Value::Inst(add) = *func.value(address) else { return Some((address, U256::ZERO)) };
    let InstKind::Add(lhs, rhs) = func.inst(add).kind else { return Some((address, U256::ZERO)) };
    if let Some(offset) = func.value_u256(rhs) {
        Some((lhs, offset))
    } else {
        func.value_u256(lhs).map(|offset| (rhs, offset))
    }
}

fn immediate(func: &mut Function, value: U256) -> ValueId {
    func.alloc_value(Value::Immediate(Immediate::for_type(Some(MirType::I256), value)))
}

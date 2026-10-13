//! Classification of words as absolute memory addresses by what they are computed from.

use crate::mir::{Function, InstKind, Value, ValueId};
use solar_data_structures::index::{IndexVec, index_vec};

/// What an absolute address is computed from, ordered from the most to the least known.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AddressInput {
    /// Constants alone.
    Constant,
    /// Constants and calldata.
    Calldata,
    /// The heap, a parameter, a loaded word, or anything else.
    Other,
}

/// Classifies every value of `func` as an absolute address by what it is computed from.
///
/// One pass per change reaches the fixed point: a value only rises from a constant to calldata
/// to anything else, and a loop-carried value starts as a constant and rises to whatever enters
/// the loop. Shared operands are classified once, however many expressions read them.
pub(crate) fn absolute_address_inputs(func: &Function) -> IndexVec<ValueId, AddressInput> {
    let mut inputs = index_vec![AddressInput::Constant; func.num_values()];
    for (value, input) in inputs.iter_mut_enumerated() {
        if !matches!(func.value(value), Value::Immediate(_) | Value::Inst(_)) {
            *input = AddressInput::Other;
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for inst_id in func.instructions() {
            let Some(value) = func.inst_result_value(inst_id) else { continue };
            let input = match &func.inst(inst_id).kind {
                InstKind::CalldataLoad(_) | InstKind::CalldataSize => AddressInput::Calldata,
                InstKind::Add(lhs, rhs)
                | InstKind::Sub(lhs, rhs)
                | InstKind::Mul(lhs, rhs)
                | InstKind::Shl(lhs, rhs)
                | InstKind::Shr(lhs, rhs)
                | InstKind::And(lhs, rhs)
                | InstKind::Or(lhs, rhs)
                | InstKind::Select(_, lhs, rhs) => inputs[*lhs].max(inputs[*rhs]),
                InstKind::Zext(inner) | InstKind::IntToPtr(inner) => inputs[*inner],
                InstKind::Phi(incoming) => incoming
                    .iter()
                    .map(|&(_, incoming)| inputs[incoming])
                    .max()
                    .unwrap_or(AddressInput::Constant),
                _ => AddressInput::Other,
            };
            if input > inputs[value] {
                inputs[value] = input;
                changed = true;
            }
        }
    }
    inputs
}

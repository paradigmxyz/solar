//! Compact literal construction for physical EVM IR.
//!
//! Candidates use a literal PUSH, its bitwise complement, a shifted short word,
//! or a shifted complement. These bounded constructions cover word masks and
//! aligned constants without recursive search. We minimize encoded bytes, then
//! static gas, and never choose an unavailable shift instruction. Assembly still
//! encodes the chosen literal widths without performing these rewrites. Candidate
//! costs use a fixed four-form plan without allocating instructions; only the
//! winning form is materialized. Ties retain the original candidate order.

use super::{InstKind, Instruction};
use crate::backend::evm::op;
use alloy_primitives::U256;
use solar_config::EvmVersion;

/// A bounded literal form; shift operands are always in `1..=255`.
enum Form {
    Push,
    Not,
    Shl(usize),
    NotShr(usize),
}

struct Plan {
    base: U256,
    form: Form,
    cost: (usize, usize),
}

impl Plan {
    fn new(version: EvmVersion, base: U256, form: Form) -> Self {
        let (bytes, gas) = push_cost(version, base);
        let extra = match form {
            Form::Push => (0, 0),
            Form::Not => (1, 3),
            Form::Shl(_) => (3, 6),
            Form::NotShr(_) => (4, 9),
        };
        Self { base, form, cost: (bytes + extra.0, gas + extra.1) }
    }

    fn materialize(self) -> Vec<Instruction> {
        match self.form {
            // push base
            Form::Push => vec![InstKind::Push(self.base).into()],
            // push base; not
            Form::Not => vec![InstKind::Push(self.base).into(), InstKind::Op(op::NOT).into()],
            // push base; push shift; shl
            Form::Shl(shift) => vec![
                InstKind::Push(self.base).into(),
                InstKind::Push(U256::from(shift)).into(),
                InstKind::Op(op::SHL).into(),
            ],
            // push base; not; push shift; shr
            Form::NotShr(shift) => vec![
                InstKind::Push(self.base).into(),
                InstKind::Op(op::NOT).into(),
                InstKind::Push(U256::from(shift)).into(),
                InstKind::Op(op::SHR).into(),
            ],
        }
    }
}

/// Chooses by bytes, then gas, retaining the first candidate on an exact tie.
fn plan(version: EvmVersion, value: U256, max_extra: usize) -> Plan {
    let mut best = Plan::new(version, value, Form::Push);
    let mut consider = |candidate: Plan| {
        if candidate.cost < best.cost {
            best = candidate;
        }
    };
    consider(Plan::new(version, !value, Form::Not));
    if max_extra >= 2 && version.has_bitwise_shifting() && !value.is_zero() {
        let trailing = value.trailing_zeros();
        if trailing > 0 {
            consider(Plan::new(version, value >> trailing, Form::Shl(trailing)));
        }
        let leading = value.leading_zeros();
        if leading > 0 {
            let low_mask = (U256::ONE << leading) - U256::ONE;
            consider(Plan::new(version, !((value << leading) | low_mask), Form::NotShr(leading)));
        }
    }
    best
}

pub(super) fn materialize(version: EvmVersion, value: U256) -> Vec<Instruction> {
    materialize_bounded(version, value, 2)
}

/// Returns the default two-word construction cost without allocating instructions.
pub(super) fn materialization_cost(version: EvmVersion, value: U256) -> (usize, usize) {
    plan(version, value, 2).cost
}

/// Builds a literal without using more temporary words than the caller proved available.
pub(super) fn materialize_bounded(
    version: EvmVersion,
    value: U256,
    max_extra: usize,
) -> Vec<Instruction> {
    plan(version, value, max_extra).materialize()
}

fn push_cost(version: EvmVersion, value: U256) -> (usize, usize) {
    (op::push_len(version, value), if value.is_zero() && version.has_push0() { 2 } else { 3 })
}

pub(super) fn cost(version: EvmVersion, instructions: &[Instruction]) -> (usize, usize) {
    instructions.iter().fold((0, 0), |(bytes, gas), inst| {
        let (size, price) = match inst.kind {
            InstKind::Push(value) => push_cost(version, value),
            InstKind::Dup(depth) | InstKind::Swap(depth) => (if depth <= 16 { 1 } else { 2 }, 3),
            InstKind::Exchange(..) => {
                if version.has_extended_stack_ops() {
                    (2, 3)
                } else {
                    (3, 9)
                }
            }
            InstKind::Op(op::POP) => (1, 2),
            _ => (1, 3),
        };
        (bytes + size, gas + price)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planned_cost_and_winner_match_candidate_sequences() {
        let mut values = vec![U256::ZERO, U256::ONE, U256::MAX];
        for bit in 0usize..256 {
            let value = U256::ONE << bit;
            values.extend([value, !value, value - U256::ONE, !(value - U256::ONE)]);
        }
        let mut state = 1388u64;
        for shift in 0usize..256 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let value = U256::from(state) << shift;
            values.extend([value, !value]);
        }
        for version in [EvmVersion::Byzantium, EvmVersion::Constantinople, EvmVersion::Osaka] {
            for &value in &values {
                assert_eq!(
                    materialization_cost(version, value),
                    cost(version, &reference_materialization(version, value, 2)),
                );
                for budget in [0, 1, 2, 3, 1024] {
                    let expected = reference_materialization(version, value, budget);
                    let selected = plan(version, value, budget);
                    assert_eq!(selected.cost, cost(version, &expected));
                    assert_eq!(selected.materialize(), expected);
                }
            }
        }
    }

    // The previous bounded helper constructs every candidate before comparing costs.
    fn reference_materialization(
        version: EvmVersion,
        value: U256,
        max_extra: usize,
    ) -> Vec<Instruction> {
        let mut best = vec![InstKind::Push(value).into()];
        let mut consider = |candidate: Vec<Instruction>| {
            if cost(version, &candidate) < cost(version, &best) {
                best = candidate;
            }
        };
        // push ~value
        // not
        consider(vec![InstKind::Push(!value).into(), InstKind::Op(op::NOT).into()]);
        if max_extra >= 2 && version.has_bitwise_shifting() && !value.is_zero() {
            let trailing = value.trailing_zeros();
            if trailing > 0 {
                // push value >> trailing
                // push trailing
                // shl
                consider(vec![
                    InstKind::Push(value >> trailing).into(),
                    InstKind::Push(U256::from(trailing)).into(),
                    InstKind::Op(op::SHL).into(),
                ]);
            }
            let leading = value.leading_zeros();
            if leading > 0 {
                // push ~(value << leading | low_mask)
                // not
                // push leading
                // shr
                let low_mask = (U256::ONE << leading) - U256::ONE;
                consider(vec![
                    InstKind::Push(!((value << leading) | low_mask)).into(),
                    InstKind::Op(op::NOT).into(),
                    InstKind::Push(U256::from(leading)).into(),
                    InstKind::Op(op::SHR).into(),
                ]);
            }
        }
        best
    }
}

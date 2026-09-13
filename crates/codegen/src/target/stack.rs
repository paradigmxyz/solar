//! Static sequence estimates for stack scheduling and frame traffic.
//!
//! These costs compose the opcode table's fork-independent prices. Frame loads
//! use representative address widths because the final spill addresses and
//! labels are not known while planning. They exclude memory expansion, which
//! operand plans cannot change by allocating new spill slots. Actual immediate
//! materialization and physical stack operations still use the selected target.
//! Keep the estimates here so the scheduler owns layouts and actions, not prices.

use super::Cost;
use crate::backend::evm::op;

/// Shared prices for the scheduler's fixed instruction sequences.
pub(crate) struct StackCosts;

impl StackCosts {
    /// Copy a resident word before storing it to its frame slot.
    pub(crate) const DUP: Cost = Cost::fixed_opcode(op::DUP1);
    /// Discard a resident word.
    pub(crate) const POP: Cost = Cost::fixed_opcode(op::POP);
    /// Estimate for a cheap, stable context read.
    pub(crate) const NULLARY_READ: Cost = Cost::fixed_opcode(op::CALLDATASIZE);
    /// Push a representative direct address, then load its word.
    pub(crate) const DIRECT_LOAD: Cost =
        Cost::fixed_opcode(op::PUSH2).plus(Cost::fixed_opcode(op::MLOAD));
    /// Load the frame pointer, add a representative slot offset, then load its word.
    pub(crate) const DYNAMIC_FRAME_LOAD: Cost = Cost::fixed_opcode(op::PUSH1)
        .plus(Cost::fixed_opcode(op::MLOAD))
        .plus(Cost::fixed_opcode(op::PUSH1))
        .plus(Cost::fixed_opcode(op::ADD))
        .plus(Cost::fixed_opcode(op::MLOAD));
    /// Push a conservative deferred target address and jump to it.
    pub(crate) const CONTROL_FLOW_JUMP: Cost =
        Cost::fixed_opcode(op::PUSH3).plus(Cost::fixed_opcode(op::JUMP));
    /// Introduce a cleanup trampoline's destination.
    pub(crate) const JUMPDEST: Cost = Cost::fixed_opcode(op::JUMPDEST);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target::Target;
    use solar_config::{EvmVersion, OptimizationMode};

    #[test]
    fn stack_estimates_match_opcode_schedule() {
        for version in [
            EvmVersion::Homestead,
            EvmVersion::Istanbul,
            EvmVersion::Berlin,
            EvmVersion::Osaka,
            EvmVersion::Amsterdam,
        ] {
            let target = Target::with(version, OptimizationMode::Gas, 200);
            for (cost, sequence) in [
                (StackCosts::DUP, &[op::DUP1][..]),
                (StackCosts::POP, &[op::POP][..]),
                (StackCosts::NULLARY_READ, &[op::CALLDATASIZE][..]),
                (StackCosts::DIRECT_LOAD, &[op::PUSH2, op::MLOAD][..]),
                (
                    StackCosts::DYNAMIC_FRAME_LOAD,
                    &[op::PUSH1, op::MLOAD, op::PUSH1, op::ADD, op::MLOAD][..],
                ),
                (StackCosts::CONTROL_FLOW_JUMP, &[op::PUSH3, op::JUMP][..]),
                (StackCosts::JUMPDEST, &[op::JUMPDEST][..]),
            ] {
                assert_eq!(cost, sequence.iter().map(|&opcode| target.opcode(opcode)).sum());
            }
        }
    }
}

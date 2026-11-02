//! Eligibility queries and the shared interpreter API used by rewrite testing.

use crate::{
    backend::evm::op,
    mir::{Callee, InstKind, MirPhase, Semantics, Terminator, utils::eval::eval_inst},
};
use alloy_primitives::U256;
use std::convert::Infallible;

pub(crate) use solar_mir_interpreter::{
    Host, Log,
    testing::{
        Limit, Limits, Machine, Memory, Meter, Outcome, ProbeExecution as Execution, StorageAccess,
        mix64,
    },
};

pub(crate) const MEMORY_LIMIT: u64 = 1 << 24;

/// Whether the shared probe can execute the instruction without a host.
pub(crate) fn supports(kind: &InstKind) -> bool {
    if !kind.op_def().phases.contains(MirPhase::Lowered) {
        return false;
    }
    match kind.semantics() {
        Some(Semantics::Phi(_) | Semantics::Select(..)) => true,
        Some(Semantics::Opcode(name, _)) => op::from_mnemonic(name).is_some_and(|opcode| {
            matches!(opcode, op::MLOAD | op::MSTORE | op::MSTORE8 | op::MCOPY | op::KECCAK256)
                || matches!(eval_inst(kind, |_| Ok::<_, Infallible>(U256::ZERO)), Ok(Some(_)))
        }),
        Some(Semantics::Call(callee, _)) => matches!(callee, Callee::Function(_)),
        _ => matches!(eval_inst(kind, |_| Ok::<_, Infallible>(U256::ZERO)), Ok(Some(_))),
    }
}

/// Whether the instruction can run with the given context reads.
pub(crate) fn supports_with_host(kind: &InstKind, context: &[u8]) -> bool {
    supports(kind)
        || kind.op_def().phases.contains(MirPhase::Lowered)
            && matches!(kind.semantics(), Some(Semantics::Opcode(name, _))
            if op::from_mnemonic(name).is_some_and(|opcode|
                matches!(opcode, op::SLOAD | op::SSTORE | op::TLOAD | op::TSTORE | op::LOG0..=op::LOG4)
                    || context.contains(&opcode)))
}

/// Whether a probe can execute the terminator.
pub(crate) fn supports_terminator(terminator: &Terminator) -> bool {
    !matches!(terminator, Terminator::SelfDestruct { .. } | Terminator::RevertReturndata)
}

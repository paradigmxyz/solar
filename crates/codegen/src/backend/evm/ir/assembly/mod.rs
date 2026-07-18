//! Finalized, layout-linear EVM IR.
//!
//! Block EVM IR remains the right representation for CFG, scheduling, and
//! layout decisions. This compact form makes final instruction adjacency
//! explicit for whole-program code-size transforms and is consumed directly
//! by byte emission.

mod inst;
mod lower;
mod optimize;

pub(in crate::backend::evm) use inst::{AsmIndex, AsmInst, AsmInstKind, PushValueId};
pub use inst::{DeferredConst, Label};
pub(in crate::backend::evm) use lower::{lower_evm_ir, raise_evm_ir};
#[cfg(test)]
pub(in crate::backend::evm) use optimize::dedup_terminal_spans;
pub(in crate::backend::evm) use optimize::run as optimize;

use crate::backend::evm::assembler::LocalInterner;
use alloy_primitives::U256;

type LocalPushValues = LocalInterner<U256, PushValueId>;

/// A compact label-bearing opcode stream ready for relocation and encoding.
#[derive(Clone, Debug, Default)]
pub(in crate::backend::evm) struct Program {
    pub(in crate::backend::evm) instructions: Vec<AsmInst>,
}

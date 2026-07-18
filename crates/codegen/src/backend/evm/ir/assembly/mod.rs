//! Primitive layout-linear assembly form.
//!
//! All control-flow and code-size transforms run on block EVM IR. This compact
//! form only records labels, relocations, deferred pushes, and opcodes for byte
//! encoding.

mod inst;
mod lower;

pub(in crate::backend::evm) use inst::{AsmIndex, AsmInst, AsmInstKind, PushValueId};
pub(crate) use inst::{DeferredConst, Label};
pub(in crate::backend::evm) use lower::lower_evm_ir;

/// A compact label-bearing opcode stream ready for relocation and byte encoding.
#[derive(Clone, Debug, Default)]
pub(in crate::backend::evm) struct Program {
    pub(in crate::backend::evm) instructions: Vec<AsmInst>,
}

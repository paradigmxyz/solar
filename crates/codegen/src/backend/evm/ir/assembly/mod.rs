//! Primitive layout-linear assembly form.
//!
//! All control-flow and code-size transforms run on block EVM IR. This compact
//! form only records labels, relocations, deferred pushes, and opcodes for byte
//! encoding.

mod inst;
mod lower;

pub(in crate::backend::evm) use inst::{AsmIndex, AsmInst, AsmInstKind, DeferredAlloc, PushValueId};
pub(crate) use inst::{DeferredConst, Label};
pub(in crate::backend::evm) use lower::lower_evm_ir;

/// A compact label-bearing opcode stream ready for relocation and byte encoding.
#[derive(Clone, Debug, Default)]
pub(in crate::backend::evm) struct Program {
    pub(in crate::backend::evm) instructions: Vec<AsmInst>,
}

impl Program {
    pub(in crate::backend::evm) fn push(&mut self, inst: AsmInst) {
        self.instructions.push(inst);
    }

    pub(in crate::backend::evm) fn push_op(&mut self, opcode: u8) {
        self.push(AsmInst::op(opcode));
    }

    pub(in crate::backend::evm) fn push_label(&mut self, label: Label) {
        self.push(AsmInst::push_label(label));
    }

    pub(in crate::backend::evm) fn define_label(&mut self, label: Label) {
        self.push(AsmInst::label(label));
    }
}

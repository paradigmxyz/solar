//! Primitive layout-linear assembly form.
//!
//! All control-flow and code-size transforms run on block EVM IR. This compact
//! form only records labels, relocations, deferred pushes, and opcodes for byte
//! encoding.

use solar_data_structures::index::IndexVec;

mod inst;
mod lower;

pub(in crate::backend::evm) use inst::{
    AsmIndex, AsmInst, AsmInstKind, DeferredAlloc, LabelTableId, PushValueId,
};
pub(crate) use inst::{DeferredConst, Label};
pub(in crate::backend::evm) use lower::lower_evm_ir;

/// A packed table of fixed-width label offsets.
#[derive(Clone, Debug)]
pub(in crate::backend::evm) struct LabelTable {
    pub(in crate::backend::evm) labels: Box<[Label]>,
    pub(in crate::backend::evm) label_width: u8,
}

/// A compact label-bearing opcode stream ready for relocation and byte encoding.
#[derive(Clone, Debug, Default)]
pub(in crate::backend::evm) struct Program {
    pub(in crate::backend::evm) instructions: Vec<AsmInst>,
    pub(in crate::backend::evm) label_tables: IndexVec<LabelTableId, LabelTable>,
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

    pub(in crate::backend::evm) fn push_label_table(
        &mut self,
        labels: Box<[Label]>,
        label_width: u8,
    ) {
        assert!(!labels.is_empty(), "label table must not be empty");
        assert!(labels.len() * usize::from(label_width) <= 32, "label table must fit one EVM word");
        let table = self.label_tables.push(LabelTable { labels, label_width });
        self.push(AsmInst::push_label_table(table));
    }

    pub(in crate::backend::evm) fn define_label(&mut self, label: Label) {
        self.push(AsmInst::label(label));
    }
}

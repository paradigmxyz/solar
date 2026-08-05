//! Primitive layout-linear assembly form.
//!
//! All control-flow and code-size transforms run on block EVM IR. This compact
//! form only records labels, relocations, deferred pushes, and opcodes for byte
//! encoding.

use solar_data_structures::index::IndexVec;

mod indexed_jump;
mod inst;
mod lower;

pub(in crate::backend::evm) use indexed_jump::{
    estimated_indexed_jump_code_size, estimated_indexed_jump_terminator_size,
    indexed_jump_target_width_bound, packs_indexed_jump,
};
pub(in crate::backend::evm) use inst::{
    AsmIndex, AsmInst, AsmInstKind, DeferredAlloc, ImmutablePushId, PackedLabelsId, PushValueId,
};
pub(crate) use inst::{DeferredConst, Label};

/// Labels packed into one fixed-width immediate.
#[derive(Clone, Debug)]
pub(in crate::backend::evm) struct PackedLabels {
    pub(in crate::backend::evm) labels: Box<[Label]>,
    pub(in crate::backend::evm) base: Option<Label>,
    pub(in crate::backend::evm) label_width: u8,
}

/// A compact label-bearing opcode stream ready for relocation and byte encoding.
#[derive(Clone, Debug, Default)]
pub(in crate::backend::evm) struct Program {
    pub(in crate::backend::evm) instructions: Vec<AsmInst>,
    pub(in crate::backend::evm) packed_labels: IndexVec<PackedLabelsId, PackedLabels>,
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

    pub(in crate::backend::evm) fn push_packed_labels(
        &mut self,
        labels: Box<[Label]>,
        base: Option<Label>,
        label_width: u8,
    ) {
        assert!(!labels.is_empty(), "packed labels must not be empty");
        assert!(
            labels.len() * usize::from(label_width) <= 32,
            "packed labels must fit one EVM word"
        );
        let labels = self.packed_labels.push(PackedLabels { labels, base, label_width });
        self.push(AsmInst::push_packed_labels(labels));
    }

    pub(in crate::backend::evm) fn define_label(&mut self, label: Label) {
        self.push(AsmInst::label(label));
    }
}

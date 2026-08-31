//! Primitive layout-linear assembly form.
//!
//! All control-flow and code-size transforms run on block EVM IR. This compact
//! form only records labels, relocations, deferred pushes, opcodes, and opaque
//! program data for byte encoding.

use super::{Data, DataId, DebugFunction, DebugFunctionExit, DebugSpans};
use crate::backend::evm::op::WORD_BYTES;
use solar_data_structures::index::IndexVec;
use solar_interface::Span;

mod indexed_jump;
mod inst;
mod lower;

pub(in crate::backend::evm) use indexed_jump::{
    estimated_indexed_jump_code_size, estimated_indexed_jump_terminator_size,
    indexed_jump_target_width_bound, packs_indexed_jump,
};
pub(in crate::backend::evm) use inst::{
    AsmIndex, AsmInst, AsmInstKind, DataRefId, DeferredAlloc, ImmutablePushId, PackedLabelsId,
    PushValueId,
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
    pub(in crate::backend::evm) data: IndexVec<DataId, Data>,
    pub(in crate::backend::evm) data_refs: IndexVec<DataRefId, super::DataRef>,
    pub(in crate::backend::evm) source_spans: Option<Vec<DebugSpans>>,
    pub(in crate::backend::evm) function_invokes: Option<Vec<Option<DebugFunction>>>,
    pub(in crate::backend::evm) function_exits: Option<Vec<Option<DebugFunctionExit>>>,
    current_source_spans: DebugSpans,
}

impl Program {
    pub(in crate::backend::evm) fn with_debug_info(capture: bool) -> Self {
        Self {
            source_spans: capture.then(Vec::new),
            function_invokes: capture.then(Vec::new),
            function_exits: capture.then(Vec::new),
            current_source_spans: DebugSpans::new(),
            ..Self::default()
        }
    }

    pub(in crate::backend::evm) fn set_source_span(&mut self, span: Option<Span>) {
        self.current_source_spans.clear();
        self.current_source_spans.extend(span);
    }

    pub(in crate::backend::evm) fn set_source_spans(&mut self, spans: &[Span]) {
        self.current_source_spans.clear();
        self.current_source_spans.extend_from_slice(spans);
    }

    pub(in crate::backend::evm) fn push(&mut self, inst: AsmInst) {
        self.instructions.push(inst);
        if let Some(source_spans) = &mut self.source_spans {
            source_spans.push(self.current_source_spans.clone());
        }
        if let Some(function_invokes) = &mut self.function_invokes {
            function_invokes.push(None);
        }
        if let Some(function_exits) = &mut self.function_exits {
            function_exits.push(None);
        }
    }

    pub(in crate::backend::evm) fn mark_last_function_invoke(
        &mut self,
        function: Option<DebugFunction>,
    ) {
        if let Some(function_invokes) = &mut self.function_invokes
            && let Some(last) = function_invokes.last_mut()
        {
            *last = function;
        }
    }

    pub(in crate::backend::evm) fn set_function_invoke(
        &mut self,
        index: usize,
        function: Option<DebugFunction>,
    ) {
        if let Some(function_invokes) = &mut self.function_invokes
            && let Some(slot) = function_invokes.get_mut(index)
        {
            *slot = function;
        }
    }

    pub(in crate::backend::evm) fn mark_last_function_exit(
        &mut self,
        exit: Option<DebugFunctionExit>,
    ) {
        if let Some(function_exits) = &mut self.function_exits
            && let Some(last) = function_exits.last_mut()
        {
            *last = exit;
        }
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
            labels.len() * usize::from(label_width) <= WORD_BYTES,
            "packed labels must fit one EVM word"
        );
        let labels = self.packed_labels.push(PackedLabels { labels, base, label_width });
        self.push(AsmInst::push_packed_labels(labels));
    }

    pub(in crate::backend::evm) fn define_label(&mut self, label: Label) {
        self.push(AsmInst::label(label));
    }

    pub(in crate::backend::evm) fn append_data(&mut self, data: DataId) {
        self.push(AsmInst::data(data));
    }

    pub(in crate::backend::evm) fn push_data_ref(&mut self, data: super::DataRef) -> DataRefId {
        self.data_refs.push(data)
    }
}

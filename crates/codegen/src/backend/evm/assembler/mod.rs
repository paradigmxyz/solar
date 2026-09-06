//! Final relocation and EVM byte encoding.
//!
//! The assembler handles:
//! - Deferred immediate and immutable materialization.
//! - Label relocation.
//! - Exact PUSH-width relaxation to a least fixed point.
//! - Opaque program-data placement.
//! - Byte emission.

use super::op::WORD_BYTES;
use crate::{
    backend::evm::{
        DebugFunction, DebugFunctionExit, DebugInstruction, DebugSpans,
        ir::{self, assembly},
        op,
    },
    mir::{ImmutableId, TypeSize},
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::GrowableBitSet, map::FxHashMap};
use solar_interface::{Span, Symbol, sym};
use solar_sema::Gcx;

mod id_counter;
pub(in crate::backend::evm) use id_counter::IdCounter;

pub(super) use assembly::{AsmInst, AsmInstKind, DeferredAlloc, ImmutablePushId, PushValueId};
pub(crate) use assembly::{DeferredConst, Label};

mod local_interner;
pub(in crate::backend::evm) use local_interner::LocalInterner;

use assembly::Program as AssemblyProgram;

/// An immutable placeholder emitted into the assembled bytecode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ImmutableRef {
    /// The immutable identifier.
    pub id: ImmutableId,
    /// Byte offset of the `PUSH<N>` opcode in the assembled bytecode.
    /// The placeholder bytes start one byte later.
    pub code_offset: usize,
    /// Type size encoded by the placeholder.
    pub type_size: TypeSize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::backend::evm) struct ImmutablePush {
    // Unlike a deferred constant, this value is unknown until the constructor runs.
    // Assembly must therefore retain its fixed width and emit a patch relocation.
    id: ImmutableId,
    type_size: TypeSize,
}

/// Result of assembly.
#[derive(Debug)]
pub(crate) struct AssembledCode {
    /// The final bytecode.
    pub bytecode: Vec<u8>,
    /// All immutable placeholders, in emission order.
    pub immutable_refs: Vec<ImmutableRef>,
    /// Final EVM IR captured immediately before byte emission.
    pub evm_ir: Option<ir::Module>,
    /// Final instruction offsets and source spans.
    pub debug_info: Option<Vec<DebugInstruction>>,
}

/// The bytecode artifact currently being assembled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ArtifactKind {
    /// Deployed runtime bytecode.
    #[default]
    Runtime,
    /// Creation bytecode that runs during deployment.
    Constructor,
}

impl ArtifactKind {
    /// Returns this artifact's name in EVM IR output.
    pub(in crate::backend::evm) const fn name(self) -> Symbol {
        match self {
            Self::Runtime => sym::runtime,
            Self::Constructor => sym::deployment,
        }
    }
}

/// Final EVM IR lowered to reusable primitive assembly.
#[derive(Clone, Debug, Default)]
pub(in crate::backend::evm) struct PreparedAssembly {
    pub(in crate::backend::evm) program: AssemblyProgram,
    pub(in crate::backend::evm) evm_ir: Option<ir::Module>,
    pub(in crate::backend::evm) push_values: LocalInterner<U256, PushValueId>,
    pub(in crate::backend::evm) immutable_pushes: LocalInterner<ImmutablePush, ImmutablePushId>,
    pub(in crate::backend::evm) next_label: IdCounter<Label>,
    pub(in crate::backend::evm) deferred_values: FxHashMap<DeferredConst, U256>,
}

/// Relocating assembler for finalized EVM IR.
#[derive(Debug)]
pub(crate) struct Assembler<'gcx> {
    pub(in crate::backend::evm) gcx: Gcx<'gcx>,
    /// Artifact whose labels are being laid out.
    pub(in crate::backend::evm) artifact_kind: ArtifactKind,
    /// EVM IR emitted directly by MIR lowering.
    pub(in crate::backend::evm) program: ir::Module,
    /// Whether `program` already has explicit EVM IR terminators.
    pub(in crate::backend::evm) program_is_finalized: bool,
    /// Block currently receiving emitted instructions.
    pub(in crate::backend::evm) current_block: Option<ir::BlockId>,
    /// Source span attached to newly emitted EVM IR operations.
    pub(in crate::backend::evm) current_source_spans: DebugSpans,
    /// Legacy source-map modifier nesting depth attached to new EVM IR operations.
    pub(in crate::backend::evm) current_modifier_depth: u32,
    /// Original assembler label attached to each EVM IR block.
    pub(in crate::backend::evm) block_labels: Vec<Option<Label>>,
    /// Defined assembler labels and their EVM IR blocks.
    pub(in crate::backend::evm) label_blocks: FxHashMap<Label, ir::BlockId>,
    /// Labels marked cold before or after their definition.
    pub(in crate::backend::evm) cold_labels: GrowableBitSet<Label>,
    /// Labels whose blocks run once per loop iteration.
    pub(in crate::backend::evm) loop_labels: GrowableBitSet<Label>,
    /// Unresolved block references emitted as push operands.
    pub(in crate::backend::evm) label_relocations: Vec<(ir::BlockId, usize, Label)>,
    /// Unresolved deferred constants emitted as push operands.
    pub(in crate::backend::evm) deferred_relocations: Vec<(ir::BlockId, usize, DeferredConst)>,
    /// Indexed jumps whose possible targets are assembler labels.
    pub(in crate::backend::evm) indexed_jump_relocations:
        Vec<(ir::BlockId, Vec<Label>, ir::Metadata)>,
    /// Interned push immediates too large for inline storage.
    pub(in crate::backend::evm) push_values: LocalInterner<U256, PushValueId>,
    /// Interned immutable placeholders.
    pub(in crate::backend::evm) immutable_pushes: LocalInterner<ImmutablePush, ImmutablePushId>,
    /// Next label ID.
    pub(in crate::backend::evm) next_label: IdCounter<Label>,
    /// Next deferred constant ID.
    pub(in crate::backend::evm) next_deferred: IdCounter<DeferredConst>,
    /// Resolved values for deferred constants.
    pub(in crate::backend::evm) deferred_values: FxHashMap<DeferredConst, U256>,
    /// Unresolved deferred allocations emitted as push operands.
    pub(in crate::backend::evm) alloc_relocations: Vec<(ir::BlockId, usize, DeferredAlloc)>,
    /// Next deferred allocation ID.
    pub(in crate::backend::evm) next_deferred_alloc: IdCounter<DeferredAlloc>,
    /// Final placement of deferred allocations.
    pub(in crate::backend::evm) deferred_allocations:
        FxHashMap<DeferredAlloc, DeferredAllocResolution>,
}

/// Final lowering selected for a deferred allocation.
#[derive(Clone, Copy, Debug)]
pub(in crate::backend::evm) enum DeferredAllocResolution {
    Static(U256),
    Dynamic(U256),
}

impl<'gcx> Assembler<'gcx> {
    /// Creates a new assembler.
    #[must_use]
    pub(crate) fn new(gcx: Gcx<'gcx>) -> Self {
        Self {
            gcx,
            artifact_kind: ArtifactKind::Runtime,
            program: ir::Module::new(sym::asm),
            program_is_finalized: false,
            current_block: None,
            current_source_spans: DebugSpans::new(),
            current_modifier_depth: 0,
            block_labels: Vec::new(),
            label_blocks: FxHashMap::default(),
            cold_labels: GrowableBitSet::new_empty(),
            loop_labels: GrowableBitSet::new_empty(),
            label_relocations: Vec::new(),
            deferred_relocations: Vec::new(),
            indexed_jump_relocations: Vec::new(),
            push_values: LocalInterner::new(),
            immutable_pushes: LocalInterner::new(),
            next_label: IdCounter::new(),
            next_deferred: IdCounter::new(),
            deferred_values: FxHashMap::default(),
            alloc_relocations: Vec::new(),
            next_deferred_alloc: IdCounter::new(),
            deferred_allocations: FxHashMap::default(),
        }
    }

    /// Clears all emitted instructions and local identifiers.
    pub(crate) fn clear(&mut self) {
        self.artifact_kind = ArtifactKind::Runtime;
        self.program.clear();
        self.program_is_finalized = false;
        self.current_block = None;
        self.current_source_spans.clear();
        self.current_modifier_depth = 0;
        self.block_labels.clear();
        self.label_blocks.clear();
        self.cold_labels.clear();
        self.loop_labels.clear();
        self.label_relocations.clear();
        self.deferred_relocations.clear();
        self.indexed_jump_relocations.clear();
        self.push_values.clear();
        self.immutable_pushes.clear();
        self.next_label.clear();
        self.next_deferred.clear();
        self.deferred_values.clear();
        self.alloc_relocations.clear();
        self.next_deferred_alloc.clear();
        self.deferred_allocations.clear();
    }

    /// Sets the artifact context used by conservative layout estimates.
    pub(crate) fn set_artifact_kind(&mut self, kind: ArtifactKind) {
        self.artifact_kind = kind;
    }

    /// Sets the source module name carried by emitted EVM IR.
    pub(crate) fn set_evm_ir_name(&mut self, name: Symbol) {
        self.program.set_name(Symbol::intern(&format!("{name}_{}", self.artifact_kind.name())));
    }

    /// Enables size-oriented outlining for an oversized gas-mode runtime.
    pub(crate) fn set_enable_size_outlining(&mut self, enable: bool) {
        self.program.enable_size_outlining = enable;
    }

    /// Returns the conservative indexed-jump target width for this artifact.
    pub(crate) fn indexed_jump_target_width_bound(&self) -> usize {
        assembly::indexed_jump_target_width_bound(
            self.gcx.sess.opts.evm_version,
            self.artifact_kind == ArtifactKind::Constructor,
        )
    }

    pub(in crate::backend::evm) fn push_inst(&mut self, value: U256) -> AsmInst {
        if let Ok(value) = u32::try_from(value)
            && let Some(inst) = AsmInst::push_inline(value)
        {
            return inst;
        }

        AsmInst::push(self.push_values.intern(value))
    }

    pub(in crate::backend::evm) fn immutable_push_inst(
        &mut self,
        id: ImmutableId,
        type_size: TypeSize,
    ) -> AsmInst {
        AsmInst::push_immutable(self.immutable_pushes.intern(ImmutablePush { id, type_size }))
    }

    pub(super) fn push_value(&self, index: PushValueId) -> U256 {
        *self.push_values.get(index)
    }

    fn immutable_push(&self, index: ImmutablePushId) -> ImmutablePush {
        *self.immutable_pushes.get(index)
    }

    /// Resolves relocations and encodes finalized EVM IR as bytecode.
    #[must_use]
    #[cfg(test)]
    pub(crate) fn assemble(&mut self) -> AssembledCode {
        self.assemble_with_evm_ir(false)
    }

    #[must_use]
    pub(crate) fn assemble_with_evm_ir(&mut self, capture_evm_ir: bool) -> AssembledCode {
        self.assemble_with_captures(capture_evm_ir, false)
    }

    #[must_use]
    pub(crate) fn assemble_with_captures(
        &mut self,
        capture_evm_ir: bool,
        capture_debug_info: bool,
    ) -> AssembledCode {
        let prepared = self.prepare(capture_evm_ir, capture_debug_info);
        let result = self.assemble_prepared(&prepared, &[]);
        self.clear();
        result
    }

    pub(in crate::backend::evm) fn assemble_prepared(
        &mut self,
        prepared: &PreparedAssembly,
        deferred_values: &[(DeferredConst, U256)],
    ) -> AssembledCode {
        self.push_values = prepared.push_values.clone();
        self.immutable_pushes = prepared.immutable_pushes.clone();
        self.next_label = prepared.next_label.clone();
        self.deferred_values.clone_from(&prepared.deferred_values);
        self.deferred_values.extend(deferred_values.iter().copied());

        let mut program = prepared.program.clone();
        for inst in &mut program.instructions {
            if let AsmInstKind::PushDeferred(id) = inst.kind() {
                let value = self
                    .deferred_values
                    .get(&id)
                    .copied()
                    .unwrap_or_else(|| panic!("deferred constant {id:?} was never resolved"));
                *inst = self.push_inst(value);
            }
        }

        let evm_ir = prepared.evm_ir.as_ref().map(|module| {
            let mut module = module.clone();
            for block in &mut module.blocks {
                for inst in &mut block.instructions {
                    if let Some(id) = inst.deferred_push() {
                        let value = self.deferred_values.get(&id).copied().unwrap_or_else(|| {
                            panic!("deferred constant {id:?} was never resolved")
                        });
                        let metadata = inst.metadata.clone();
                        *inst = ir::Instruction::push_value(value);
                        inst.metadata = metadata;
                    }
                }
            }
            module
        });

        // Label-free constructor and deployment snippets need neither offset
        // discovery nor push-width relaxation.
        if !program.instructions.iter().any(|inst| {
            matches!(
                inst.kind(),
                AsmInstKind::Label(_)
                    | AsmInstKind::PushLabel(_)
                    | AsmInstKind::PushLabelFixed(_, _)
                    | AsmInstKind::PushPackedLabels(_)
                    | AsmInstKind::PushData(_)
            )
        }) {
            let mut result = self.emit_bytecode(
                &program,
                FxHashMap::default(),
                FxHashMap::default(),
                &FxHashMap::default(),
            );
            result.evm_ir = evm_ir;
            return result;
        }

        let (label_offsets, data_offsets, push_widths) = self.resolve_offsets(&program);
        let mut result = self.emit_bytecode(&program, label_offsets, data_offsets, &push_widths);
        result.evm_ir = evm_ir;
        result
    }

    /// Resolves all ordinary label pushes against the complete assembly
    /// program. Indexed-jump table entries are fixed-width instructions by the
    /// time this runs; their widths are refined by EVM-IR lowering first.
    pub(in crate::backend::evm) fn resolve_label_offsets(
        &self,
        program: &AssemblyProgram,
    ) -> (FxHashMap<Label, usize>, FxHashMap<usize, u8>) {
        let (label_offsets, _, push_widths) = self.resolve_offsets(program);
        (label_offsets, push_widths)
    }

    fn resolve_offsets(
        &self,
        program: &AssemblyProgram,
    ) -> (FxHashMap<Label, usize>, FxHashMap<ir::DataId, usize>, FxHashMap<usize, u8>) {
        // Start from the narrowest possible label pushes. Widening pushes can
        // only increase later label offsets, so required widths grow
        // monotonically to the least fixed point.
        let mut push_widths: FxHashMap<usize, u8> = FxHashMap::default();
        for (idx, inst) in program.instructions.iter().enumerate() {
            if matches!(inst.kind(), AsmInstKind::PushLabel(_) | AsmInstKind::PushData(_)) {
                push_widths.insert(idx, 0);
            }
        }

        loop {
            let (label_offsets, data_offsets, new_widths) =
                self.compute_offsets(program, &push_widths);
            if new_widths == push_widths {
                return (label_offsets, data_offsets, push_widths);
            }

            debug_assert!(new_widths.iter().all(|(idx, width)| {
                push_widths.get(idx).is_some_and(|previous| width >= previous)
            }));
            push_widths = new_widths;
        }
    }

    /// Computes label offsets given current PUSH widths.
    fn compute_offsets(
        &self,
        program: &AssemblyProgram,
        push_widths: &FxHashMap<usize, u8>,
    ) -> (FxHashMap<Label, usize>, FxHashMap<ir::DataId, usize>, FxHashMap<usize, u8>) {
        let mut offset = 0usize;
        let mut label_offsets = FxHashMap::default();
        let mut data_offsets = FxHashMap::default();
        let mut new_widths = FxHashMap::default();
        let out = BytecodeAssembler::new(self.gcx, false);

        for (idx, inst) in program.instructions.iter().enumerate() {
            match inst.kind() {
                AsmInstKind::Op(_) => {
                    offset += 1;
                }
                AsmInstKind::OpImmediate(_, _) => {
                    offset += 2;
                }
                AsmInstKind::PushInline(value) => {
                    offset += out.encoded_push_len(U256::from(value));
                }
                AsmInstKind::Push(index) => {
                    offset += out.encoded_push_len(self.push_value(index));
                }
                AsmInstKind::PushLabel(_) | AsmInstKind::PushData(_) => {
                    // Use current estimated width
                    let width = push_widths.get(&idx).copied().unwrap_or(2);
                    offset += out.fixed_push_len(width);
                }
                AsmInstKind::PushLabelFixed(_, width) => {
                    offset += out.fixed_push_len(width);
                }
                AsmInstKind::PushPackedLabels(labels) => {
                    let labels = &program.packed_labels[labels];
                    let width = usize::from(labels.label_width) * labels.labels.len();
                    offset += out.fixed_push_len(width as u8);
                }
                AsmInstKind::PushDeferred(id) => {
                    // Deployment offsets may not be known until the prepared
                    // program is assembled. Reserve the maximum push for
                    // unknown values, while using known values exactly.
                    offset += self
                        .deferred_values
                        .get(&id)
                        .map_or(33, |&value| out.encoded_push_len(value));
                }
                AsmInstKind::PushImmutable(id) => {
                    offset += 1 + usize::from(self.immutable_push(id).type_size.bytes());
                }
                AsmInstKind::Label(label) => {
                    label_offsets.insert(label, offset);
                    offset += 1;
                }
                AsmInstKind::Data(data) => {
                    data_offsets.insert(data, offset);
                    offset += program.data[data].bytes.len();
                }
            }
        }

        // Compute new widths based on resolved offsets
        for (idx, inst) in program.instructions.iter().enumerate() {
            if let AsmInstKind::PushLabel(label) = inst.kind()
                && let Some(&target_offset) = label_offsets.get(&label)
            {
                let width = out.push_width(U256::from(target_offset));
                new_widths.insert(idx, width);
            } else if let AsmInstKind::PushData(data) = inst.kind() {
                let target_offset = resolve_data_offset(program, &data_offsets, data);
                let width = out.push_width(U256::from(target_offset));
                new_widths.insert(idx, width);
            }
        }

        (label_offsets, data_offsets, new_widths)
    }

    /// Emits the final bytecode.
    fn emit_bytecode(
        &self,
        program: &AssemblyProgram,
        label_offsets: FxHashMap<Label, usize>,
        data_offsets: FxHashMap<ir::DataId, usize>,
        push_widths: &FxHashMap<usize, u8>,
    ) -> AssembledCode {
        let mut out = BytecodeAssembler::new(self.gcx, program.source_spans.is_some());
        for (idx, inst) in program.instructions.iter().enumerate() {
            let source_spans =
                program.source_spans.as_ref().map_or(&[][..], |spans| spans[idx].as_slice());
            let function_invoke =
                program.function_invokes.as_ref().and_then(|invokes| invokes[idx]);
            let function_exit = program.function_exits.as_ref().and_then(|exits| exits[idx]);
            let modifier_depth = program.modifier_depths.as_ref().map_or(0, |depths| depths[idx]);
            out.set_function_events(function_invoke, function_exit);
            out.set_modifier_depth(modifier_depth);
            match inst.kind() {
                AsmInstKind::Op(opcode) => {
                    out.emit_op(opcode, source_spans);
                }
                AsmInstKind::OpImmediate(opcode, immediate) => {
                    out.emit_op_immediate(opcode, immediate, source_spans);
                }
                AsmInstKind::PushInline(value) => {
                    out.emit_push_value(U256::from(value), source_spans);
                }
                AsmInstKind::Push(index) => {
                    out.emit_push_value(self.push_value(index), source_spans);
                }
                AsmInstKind::PushLabel(label) => {
                    let target_offset = label_offsets
                        .get(&label)
                        .copied()
                        .unwrap_or_else(|| panic!("label {label:?} was never defined"));
                    let width = push_widths.get(&idx).copied().unwrap_or(2);
                    out.emit_push_fixed_width(U256::from(target_offset), width, source_spans);
                }
                AsmInstKind::PushLabelFixed(label, width) => {
                    let target_offset = label_offsets
                        .get(&label)
                        .copied()
                        .unwrap_or_else(|| panic!("label {label:?} was never defined"));
                    out.emit_push_fixed_width(U256::from(target_offset), width, source_spans);
                }
                AsmInstKind::PushPackedLabels(labels) => {
                    let labels = &program.packed_labels[labels];
                    let base_offset = labels.base.map_or(0, |base| {
                        label_offsets
                            .get(&base)
                            .copied()
                            .unwrap_or_else(|| panic!("label {base:?} was never defined"))
                    });
                    let mut value = U256::ZERO;
                    for (index, &label) in labels.labels.iter().enumerate() {
                        let target_offset = label_offsets
                            .get(&label)
                            .copied()
                            .unwrap_or_else(|| panic!("label {label:?} was never defined"));
                        let target = U256::from(
                            target_offset
                                .checked_sub(base_offset)
                                .expect("packed label must not precede its base"),
                        );
                        assert!(
                            target.byte_len() <= usize::from(labels.label_width),
                            "label offset does not fit packed labels entry"
                        );
                        value |= target << (index * usize::from(labels.label_width) * 8);
                    }
                    let width = labels.labels.len() * usize::from(labels.label_width);
                    out.emit_push_fixed_width(value, width as u8, source_spans);
                }
                AsmInstKind::PushData(data) => {
                    let target_offset = resolve_data_offset(program, &data_offsets, data);
                    let width = push_widths.get(&idx).copied().unwrap_or(2);
                    out.emit_push_fixed_width(U256::from(target_offset), width, source_spans);
                }
                AsmInstKind::PushDeferred(_) => {
                    unreachable!("deferred values must be resolved before assembly");
                }
                AsmInstKind::PushImmutable(id) => {
                    out.emit_push_immutable(self.immutable_push(id), source_spans);
                }
                AsmInstKind::Label(_) => {
                    out.emit_op(op::JUMPDEST, source_spans);
                }
                AsmInstKind::Data(data) => {
                    out.bytecode.extend_from_slice(&program.data[data].bytes);
                }
            }
        }
        out.finish()
    }

    /// Returns the minimum number of non-zero bytes needed to push a value.
    #[cfg(test)]
    fn push_width(value: U256) -> u8 {
        value.byte_len() as u8
    }
}

fn resolve_data_offset(
    program: &AssemblyProgram,
    data_offsets: &FxHashMap<ir::DataId, usize>,
    data_ref: assembly::DataRefId,
) -> usize {
    let data = program.data_refs[data_ref];
    let data_size = program.data[data.id].bytes.len();
    assert!(
        data.offset as usize <= data_size,
        "program data offset {} exceeds data size {data_size}",
        data.offset
    );
    data_offsets
        .get(&data.id)
        .copied()
        .unwrap_or_else(|| panic!("program data {:?} was never emitted", data.id))
        .checked_add(data.offset as usize)
        .expect("program data offset overflow")
}

#[derive(Debug)]
struct BytecodeAssembler<'gcx> {
    gcx: Gcx<'gcx>,
    bytecode: Vec<u8>,
    immutable_refs: Vec<ImmutableRef>,
    debug_info: Option<Vec<DebugInstruction>>,
    function_invoke: Option<DebugFunction>,
    function_exit: Option<DebugFunctionExit>,
    modifier_depth: u32,
}

impl<'gcx> BytecodeAssembler<'gcx> {
    fn new(gcx: Gcx<'gcx>, capture_debug_info: bool) -> Self {
        Self {
            gcx,
            bytecode: Vec::new(),
            immutable_refs: Vec::new(),
            debug_info: capture_debug_info.then(Vec::new),
            function_invoke: None,
            function_exit: None,
            modifier_depth: 0,
        }
    }

    fn set_function_events(
        &mut self,
        invoke: Option<DebugFunction>,
        exit: Option<DebugFunctionExit>,
    ) {
        self.function_invoke = invoke;
        self.function_exit = exit;
    }

    fn set_modifier_depth(&mut self, depth: u32) {
        self.modifier_depth = depth;
    }

    fn emit_op(&mut self, opcode: u8, source_spans: &[Span]) {
        let offset = self.bytecode.len();
        self.bytecode.push(opcode);
        self.record_instruction(offset, source_spans);
    }

    fn emit_op_immediate(&mut self, opcode: u8, immediate: u8, source_spans: &[Span]) {
        let offset = self.bytecode.len();
        self.bytecode.extend([opcode, immediate]);
        self.record_instruction(offset, source_spans);
    }

    fn emit_push_immutable(&mut self, push: ImmutablePush, source_spans: &[Span]) {
        let offset = self.bytecode.len();
        self.immutable_refs.push(ImmutableRef {
            id: push.id,
            code_offset: offset,
            type_size: push.type_size,
        });
        let byte_width = push.type_size.bytes();
        self.bytecode.push(op::push(byte_width));
        self.bytecode.extend(std::iter::repeat_n(0, usize::from(byte_width)));
        self.record_instruction(offset, source_spans);
    }

    fn encoded_push_len(&self, value: U256) -> usize {
        self.fixed_push_len(self.push_width(value))
    }

    /// Emits a PUSH instruction with automatically sized width.
    fn emit_push_value(&mut self, value: U256, source_spans: &[Span]) {
        self.emit_push_fixed_width(value, self.push_width(value), source_spans);
    }

    /// Emits a PUSH instruction with a specific width.
    fn emit_push_fixed_width(&mut self, value: U256, width: u8, source_spans: &[Span]) {
        assert!(self.push_width(value) <= width, "value does not fit fixed PUSH width");
        let offset = self.bytecode.len();
        if width == 0 {
            self.emit_push_zero();
        } else {
            self.bytecode.push(op::push(width));

            let bytes = value.to_be_bytes::<WORD_BYTES>();
            let start = WORD_BYTES - width as usize;
            self.bytecode.extend_from_slice(&bytes[start..]);
        }
        self.record_instruction(offset, source_spans);
    }

    fn emit_push_zero(&mut self) {
        if self.gcx.sess.opts.evm_version.has_push0() {
            self.bytecode.push(op::PUSH0);
        } else {
            self.bytecode.push(op::PUSH1);
            self.bytecode.push(0);
        }
    }

    fn fixed_push_len(&self, width: u8) -> usize {
        if width == 0 { self.zero_push_len() } else { 1 + width as usize }
    }

    fn zero_push_len(&self) -> usize {
        if self.gcx.sess.opts.evm_version.has_push0() { 1 } else { 2 }
    }

    /// Returns the minimum immediate width needed to push a value for this EVM version.
    fn push_width(&self, value: U256) -> u8 {
        if value.is_zero() && !self.gcx.sess.opts.evm_version.has_push0() {
            1
        } else {
            value.byte_len() as u8
        }
    }

    fn finish(self) -> AssembledCode {
        AssembledCode {
            bytecode: self.bytecode,
            immutable_refs: self.immutable_refs,
            evm_ir: None,
            debug_info: self.debug_info,
        }
    }

    fn record_instruction(&mut self, offset: usize, source_spans: &[Span]) {
        let Some(debug_info) = &mut self.debug_info else { return };
        debug_info.push(DebugInstruction {
            offset: offset.try_into().expect("EVM bytecode offset exceeds u32"),
            opcode: self.bytecode[offset],
            source_spans: source_spans.iter().copied().collect(),
            function_invoke: self.function_invoke,
            function_exit: self.function_exit,
            modifier_depth: self.modifier_depth,
        });
    }
}

// DO NOT ADD CODEGEN TESTS HERE. USE UI TESTS UNDER tests/ui/codegen INSTEAD.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::evm::disassemble;
    use snapbox::{assert_data_eq, str};
    use solar_config::{CompileOpts, EvmVersion};
    use solar_interface::Session;
    use solar_sema::Compiler;

    fn with_assembler<T: Send>(opts: CompileOpts, f: impl FnOnce(Assembler<'_>) -> T + Send) -> T {
        let compiler = Compiler::new(Session::builder().opts(opts).build());
        compiler.enter(|c| f(Assembler::new(c.gcx())))
    }

    #[test]
    fn opcode_mnemonics_round_trip() {
        for opcode in 0..=u8::MAX {
            if let Some(mnemonic) = op::mnemonic(opcode) {
                assert_eq!(op::from_mnemonic(mnemonic), Some(opcode));
            }
        }
        assert_eq!(op::stack_io(op::ADD), Some((2, 1)));
        assert_eq!(op::stack_io(op::MSTORE), Some((2, 0)));
        assert_eq!(op::stack_io(op::CALLVALUE), Some((0, 1)));
        assert_eq!(op::stack_io(op::CALLF), None);
        solar_interface::enter(|| {
            assert_eq!(op::from_ir_symbol(solar_interface::kw::Add), Some(op::ADD));
        });
    }

    #[test]
    fn test_push_width() {
        assert_eq!(Assembler::push_width(U256::ZERO), 0);
        assert_eq!(Assembler::push_width(U256::from(1)), 1);
        assert_eq!(Assembler::push_width(U256::from(255)), 1);
        assert_eq!(Assembler::push_width(U256::from(256)), 2);
        assert_eq!(Assembler::push_width(U256::from(0xFFFF)), 2);
        assert_eq!(Assembler::push_width(U256::from(0x10000)), 3);
    }

    #[test]
    fn assembler_inst_is_compact() {
        assert_eq!(std::mem::size_of::<AsmInst>(), 4);
    }

    #[test]
    fn push_values_are_inline_or_interned() {
        with_assembler(CompileOpts::default(), |mut asm| {
            let inline = u32::MAX >> 1;
            let large = U256::from(1u64 << 31);

            assert!(AsmInst::push_inline(inline).is_some());
            assert!(AsmInst::push_inline(1u32 << 31).is_none());

            let inline = asm.push_inst(U256::from(inline));
            let first = asm.push_inst(large);
            let second = asm.push_inst(large);

            assert_eq!(inline.kind(), AsmInstKind::PushInline(u32::MAX >> 1));
            assert_eq!(first.kind(), AsmInstKind::Push(PushValueId::from_usize(0)));
            assert_eq!(first, second);
            assert_eq!(asm.push_values.len(), 1);
            assert_eq!(*asm.push_values.get(PushValueId::from_usize(0)), large);
        });
    }

    #[test]
    fn immutable_push_uses_declared_width() {
        with_assembler(CompileOpts::default(), |mut asm| {
            let narrow = ImmutableId::new(3);
            let address = ImmutableId::new(4);
            let narrow_size = TypeSize::new_int_bits(8);
            let address_size = TypeSize::new_int_bits(160);

            asm.emit_push_immutable(narrow, narrow_size);
            asm.emit_push_immutable(address, address_size);
            let result = asm.assemble();

            assert_data_eq!(
                disassemble(&result.bytecode, EvmVersion::Osaka),
                str![[r#"
PUSH1 0x00
PUSH20 0x0000000000000000000000000000000000000000

"#]]
            );
            assert_eq!(
                result.immutable_refs,
                [
                    ImmutableRef { id: narrow, code_offset: 0, type_size: narrow_size },
                    ImmutableRef { id: address, code_offset: 2, type_size: address_size },
                ]
            );
        });
    }

    #[test]
    fn assembler_can_be_reused_after_assembly() {
        with_assembler(CompileOpts::default(), |mut asm| {
            let large = U256::from(1u64 << 31);

            asm.emit_push(large);
            let first = asm.assemble();

            assert_data_eq!(
                disassemble(&first.bytecode, EvmVersion::Osaka),
                str![[r#"
PUSH4 0x80000000

"#]]
            );
            assert!(asm.program.blocks.is_empty());
            assert_eq!(asm.push_values.len(), 0);
            assert_eq!(asm.immutable_pushes.len(), 0);

            asm.emit_push(U256::from(2));
            let second = asm.assemble();

            assert_data_eq!(
                disassemble(&second.bytecode, EvmVersion::Osaka),
                str![[r#"
PUSH1 0x02

"#]]
            );
        });
    }

    #[test]
    fn deferred_allocations_expand_after_layout() {
        with_assembler(CompileOpts::default(), |mut static_asm| {
            let static_alloc = static_asm.emit_deferred_alloc();
            static_asm.set_deferred_alloc_static(static_alloc, U256::from(0xa0));
            assert_eq!(static_asm.assemble().bytecode, [op::PUSH1, 0xa0]);
        });

        with_assembler(CompileOpts::default(), |mut dynamic_asm| {
            let dynamic_alloc = dynamic_asm.emit_deferred_alloc();
            dynamic_asm.set_deferred_alloc_dynamic(dynamic_alloc, U256::from(0x20));
            assert_eq!(
                dynamic_asm.assemble().bytecode,
                [
                    op::PUSH1,
                    0x40,
                    op::MLOAD,
                    op::DUP1,
                    op::PUSH1,
                    0x20,
                    op::ADD,
                    op::PUSH1,
                    0x40,
                    op::MSTORE,
                ]
            );
        });
    }
}

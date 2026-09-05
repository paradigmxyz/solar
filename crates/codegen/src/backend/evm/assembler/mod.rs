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
        ir::{self, assembly},
        op,
    },
    mir::{ImmutableId, TypeSize},
};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::map::FxHashMap;
use solar_interface::{Symbol, sym};

pub(super) use assembly::{AsmInstKind, DeferredAlloc, ImmutablePushId, PushValueId};
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
    pub(in crate::backend::evm) id: ImmutableId,
    pub(in crate::backend::evm) type_size: TypeSize,
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

/// Immutable primitive program, its constant pools, and deferred bindings.
/// Encoding borrows these so constructor offset retries cannot mutate prepared state.
#[derive(Clone, Debug, Default)]
pub(in crate::backend::evm) struct PreparedAssembly {
    pub(in crate::backend::evm) program: AssemblyProgram,
    pub(in crate::backend::evm) evm_ir: Option<ir::Module>,
    pub(in crate::backend::evm) deferred_values: FxHashMap<DeferredConst, U256>,
}

impl PreparedAssembly {
    pub(in crate::backend::evm) fn assemble(
        &self,
        evm_version: EvmVersion,
        overrides: &[(DeferredConst, U256)],
    ) -> AssembledCode {
        let assembler =
            Assembler::new(evm_version, &self.program, &self.deferred_values, overrides);
        let (labels, data, widths) = assembler.resolve_offsets();
        let mut result = assembler.emit_bytecode(labels, data, &widths);
        result.evm_ir = self.evm_ir.as_ref().map(|module| {
            let mut module = module.clone();
            // push deferred(id) -> push final_value(id)
            for block in &mut module.blocks {
                for inst in &mut block.instructions {
                    if let Some(id) = inst.deferred_push() {
                        *inst = ir::Instruction::push_value(assembler.required_deferred_value(id));
                    }
                }
            }
            module
        });
        result
    }
}

/// Primitive relocation and byte encoding over a borrowed compact program.
/// Every encode computes fresh offsets; late values are never written into the program.
pub(in crate::backend::evm) struct Assembler<'a> {
    evm_version: EvmVersion,
    program: &'a AssemblyProgram,
    deferred_values: &'a FxHashMap<DeferredConst, U256>,
    overrides: &'a [(DeferredConst, U256)],
}

impl<'a> Assembler<'a> {
    pub(in crate::backend::evm) fn new(
        evm_version: EvmVersion,
        program: &'a AssemblyProgram,
        deferred_values: &'a FxHashMap<DeferredConst, U256>,
        overrides: &'a [(DeferredConst, U256)],
    ) -> Self {
        Self { evm_version, program, deferred_values, overrides }
    }

    fn deferred_value(&self, id: DeferredConst) -> Option<U256> {
        self.overrides
            .iter()
            .rev()
            .find_map(|&(key, value)| (key == id).then_some(value))
            .or_else(|| self.deferred_values.get(&id).copied())
    }

    fn required_deferred_value(&self, id: DeferredConst) -> U256 {
        self.deferred_value(id)
            .unwrap_or_else(|| panic!("deferred constant {id:?} was never resolved"))
    }

    fn push_value(&self, index: PushValueId) -> U256 {
        *self.program.push_values.get(index)
    }

    fn immutable_push(&self, index: ImmutablePushId) -> ImmutablePush {
        *self.program.immutable_pushes.get(index)
    }

    /// Resolves all ordinary label pushes against the complete assembly
    /// program. Indexed-jump table entries are fixed-width instructions by the
    /// time this runs; their widths are refined by EVM-IR lowering first.
    pub(in crate::backend::evm) fn label_offsets(&self) -> FxHashMap<Label, usize> {
        self.resolve_offsets().0
    }

    fn resolve_offsets(
        &self,
    ) -> (FxHashMap<Label, usize>, FxHashMap<ir::DataId, usize>, FxHashMap<usize, u8>) {
        // Start from the narrowest possible label pushes. Widening pushes can
        // only increase later label offsets, so required widths grow
        // monotonically to the least fixed point.
        let mut push_widths: FxHashMap<usize, u8> = FxHashMap::default();
        for (idx, inst) in self.program.instructions.iter().enumerate() {
            if matches!(inst.kind(), AsmInstKind::PushLabel(_) | AsmInstKind::PushData(_)) {
                push_widths.insert(idx, 0);
            }
        }

        loop {
            let (label_offsets, data_offsets, new_widths) = self.compute_offsets(&push_widths);
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
        push_widths: &FxHashMap<usize, u8>,
    ) -> (FxHashMap<Label, usize>, FxHashMap<ir::DataId, usize>, FxHashMap<usize, u8>) {
        let mut offset = 0usize;
        let mut label_offsets = FxHashMap::default();
        let mut data_offsets = FxHashMap::default();
        let mut new_widths = FxHashMap::default();
        let out = BytecodeAssembler::new(self.evm_version);

        for (idx, inst) in self.program.instructions.iter().enumerate() {
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
                    let labels = &self.program.packed_labels[labels];
                    let width = usize::from(labels.label_width) * labels.labels.len();
                    offset += out.fixed_push_len(width as u8);
                }
                AsmInstKind::PushDeferred(id) => {
                    // Deployment offsets may not be known until the prepared
                    // program is assembled. Reserve the maximum push for
                    // unknown values, while using known values exactly.
                    offset +=
                        self.deferred_value(id).map_or(33, |value| out.encoded_push_len(value));
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
                    offset += self.program.data[data].bytes.len();
                }
            }
        }

        // Compute new widths based on resolved offsets
        for (idx, inst) in self.program.instructions.iter().enumerate() {
            if let AsmInstKind::PushLabel(label) = inst.kind()
                && let Some(&target_offset) = label_offsets.get(&label)
            {
                let width = out.push_width(U256::from(target_offset));
                new_widths.insert(idx, width);
            } else if let AsmInstKind::PushData(data) = inst.kind() {
                let target_offset = resolve_data_offset(self.program, &data_offsets, data);
                let width = out.push_width(U256::from(target_offset));
                new_widths.insert(idx, width);
            }
        }

        (label_offsets, data_offsets, new_widths)
    }

    /// Emits the final bytecode.
    fn emit_bytecode(
        &self,
        label_offsets: FxHashMap<Label, usize>,
        data_offsets: FxHashMap<ir::DataId, usize>,
        push_widths: &FxHashMap<usize, u8>,
    ) -> AssembledCode {
        let mut out = BytecodeAssembler::new(self.evm_version);
        for (idx, inst) in self.program.instructions.iter().enumerate() {
            match inst.kind() {
                AsmInstKind::Op(opcode) => {
                    out.emit_op(opcode);
                }
                AsmInstKind::OpImmediate(opcode, immediate) => {
                    out.emit_op(opcode);
                    out.emit_op(immediate);
                }
                AsmInstKind::PushInline(value) => {
                    out.emit_push_value(U256::from(value));
                }
                AsmInstKind::Push(index) => {
                    out.emit_push_value(self.push_value(index));
                }
                AsmInstKind::PushLabel(label) => {
                    let target_offset = label_offsets
                        .get(&label)
                        .copied()
                        .unwrap_or_else(|| panic!("label {label:?} was never defined"));
                    let width = push_widths.get(&idx).copied().unwrap_or(2);
                    out.emit_push_fixed_width(U256::from(target_offset), width);
                }
                AsmInstKind::PushLabelFixed(label, width) => {
                    let target_offset = label_offsets
                        .get(&label)
                        .copied()
                        .unwrap_or_else(|| panic!("label {label:?} was never defined"));
                    out.emit_push_fixed_width(U256::from(target_offset), width);
                }
                AsmInstKind::PushPackedLabels(labels) => {
                    let labels = &self.program.packed_labels[labels];
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
                    out.emit_push_fixed_width(value, width as u8);
                }
                AsmInstKind::PushData(data) => {
                    let target_offset = resolve_data_offset(self.program, &data_offsets, data);
                    let width = push_widths.get(&idx).copied().unwrap_or(2);
                    out.emit_push_fixed_width(U256::from(target_offset), width);
                }
                AsmInstKind::PushDeferred(id) => {
                    out.emit_push_value(self.required_deferred_value(id));
                }
                AsmInstKind::PushImmutable(id) => {
                    out.emit_push_immutable(self.immutable_push(id));
                }
                AsmInstKind::Label(_) => {
                    out.emit_op(op::JUMPDEST);
                }
                AsmInstKind::Data(data) => {
                    out.bytecode.extend_from_slice(&self.program.data[data].bytes);
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
struct BytecodeAssembler {
    evm_version: EvmVersion,
    bytecode: Vec<u8>,
    immutable_refs: Vec<ImmutableRef>,
}

impl BytecodeAssembler {
    fn new(evm_version: EvmVersion) -> Self {
        Self { evm_version, bytecode: Vec::new(), immutable_refs: Vec::new() }
    }

    fn emit_op(&mut self, opcode: u8) {
        self.bytecode.push(opcode);
    }

    fn emit_push_immutable(&mut self, push: ImmutablePush) {
        self.immutable_refs.push(ImmutableRef {
            id: push.id,
            code_offset: self.bytecode.len(),
            type_size: push.type_size,
        });
        let byte_width = push.type_size.bytes();
        self.bytecode.push(op::push(byte_width));
        self.bytecode.extend(std::iter::repeat_n(0, usize::from(byte_width)));
    }

    fn encoded_push_len(&self, value: U256) -> usize {
        self.fixed_push_len(self.push_width(value))
    }

    /// Emits a PUSH instruction with automatically sized width.
    fn emit_push_value(&mut self, value: U256) {
        self.emit_push_fixed_width(value, self.push_width(value));
    }

    /// Emits a PUSH instruction with a specific width.
    fn emit_push_fixed_width(&mut self, value: U256, width: u8) {
        assert!(self.push_width(value) <= width, "value does not fit fixed PUSH width");
        if width == 0 {
            self.emit_push_zero();
            return;
        }

        self.bytecode.push(op::push(width));

        let bytes = value.to_be_bytes::<WORD_BYTES>();
        let start = WORD_BYTES - width as usize;
        self.bytecode.extend_from_slice(&bytes[start..]);
    }

    fn emit_push_zero(&mut self) {
        if self.evm_version.has_push0() {
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
        if self.evm_version.has_push0() { 1 } else { 2 }
    }

    /// Returns the minimum immediate width needed to push a value for this EVM version.
    fn push_width(&self, value: U256) -> u8 {
        if value.is_zero() && !self.evm_version.has_push0() { 1 } else { value.byte_len() as u8 }
    }

    fn finish(self) -> AssembledCode {
        AssembledCode { bytecode: self.bytecode, immutable_refs: self.immutable_refs, evm_ir: None }
    }
}

// DO NOT ADD CODEGEN TESTS HERE. USE UI TESTS UNDER tests/ui/codegen INSTEAD.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::evm::{
        disassemble,
        ir::{assembly::AsmInst, builder::Builder},
    };
    use snapbox::{assert_data_eq, str};
    use solar_config::{CompileOpts, EvmVersion};
    use solar_interface::Session;
    use solar_sema::Compiler;

    fn with_assembler<T: Send>(opts: CompileOpts, f: impl FnOnce(Builder<'_>) -> T + Send) -> T {
        let compiler = Compiler::new(Session::builder().opts(opts).build());
        compiler.enter(|c| f(Builder::new(c.gcx())))
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
        let mut asm = AssemblyProgram::default();
        {
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
        }
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
            assert_eq!(asm.block_count(), 0);

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
    fn prepared_assembly_reuses_pools_with_late_values() {
        {
            let deferred = DeferredConst::from_usize(0);
            let target = Label::from_usize(0);
            let mut program = AssemblyProgram::default();
            // push deferred; push target; jump; target: jumpdest; push immutable
            program.push(AsmInst::push_deferred(deferred));
            program.push_label(target);
            program.push_op(op::JUMP);
            program.define_label(target);
            let immutable =
                program.immutable_push_inst(ImmutableId::new(3), TypeSize::new_int_bits(8));
            program.push(immutable);
            let prepared = PreparedAssembly { program, ..Default::default() };
            let encode = |value| {
                prepared.assemble(EvmVersion::Osaka, &[(deferred, U256::MAX), (deferred, value)])
            };
            let zero = encode(U256::ZERO);
            let wide = encode(U256::from(0x100));
            let again = encode(U256::ZERO);
            assert_data_eq!(
                format!(
                    "{}{}{}",
                    disassemble(&zero.bytecode, EvmVersion::Osaka),
                    disassemble(&wide.bytecode, EvmVersion::Osaka),
                    disassemble(&again.bytecode, EvmVersion::Osaka)
                ),
                str![[r#"
PUSH0
PUSH1 0x04 ; bb0
JUMP
; bb0
JUMPDEST
PUSH1 0x00
PUSH2 0x0100
PUSH1 0x06 ; bb0
JUMP
; bb0
JUMPDEST
PUSH1 0x00
PUSH0
PUSH1 0x04 ; bb0
JUMP
; bb0
JUMPDEST
PUSH1 0x00

"#]]
            );
        }
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

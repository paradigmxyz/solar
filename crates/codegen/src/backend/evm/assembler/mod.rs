//! Primitive EVM relocation and byte encoding.
//!
//! The assembler handles:
//! - Deferred immediate and immutable materialization.
//! - Label relocation.
//! - Exact PUSH-width relaxation to a least fixed point.
//! - Byte emission.

use crate::{
    backend::evm::{
        ir::{self, assembly},
        op,
    },
    memory::EvmMemoryLayout,
    mir::IMMUTABLE_WORD_SIZE,
};
use alloy_primitives::U256;
use solar_config::OptimizationMode;
use solar_data_structures::{bit_set::GrowableBitSet, map::FxHashMap};
use solar_interface::diagnostics::DiagCtxt;
use solar_sema::Gcx;

const EVM_WORD_BYTES: usize = 32;

mod id_counter;
pub(in crate::backend::evm) use id_counter::IdCounter;

pub(super) use assembly::{AsmInst, AsmInstKind, DeferredAlloc, PushValueId};
pub(crate) use assembly::{DeferredConst, Label};

mod local_interner;
pub(in crate::backend::evm) use local_interner::LocalInterner;

use assembly::Program as AssemblyProgram;

/// A `PUSH32` immutable placeholder emitted into the assembled bytecode.
///
/// TODO: Track placeholder byte width here when smaller immutable references
/// are supported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ImmutableRef {
    /// The immutable's byte offset identifier.
    pub id: u32,
    /// Byte offset of the `PUSH32` opcode in the assembled bytecode.
    /// The 32 placeholder bytes start one byte later.
    pub code_offset: usize,
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

/// Final EVM IR lowered to reusable primitive assembly.
#[derive(Clone, Debug, Default)]
pub(in crate::backend::evm) struct PreparedAssembly {
    program: AssemblyProgram,
    evm_ir: Option<ir::Module>,
    push_values: LocalInterner<U256, PushValueId>,
    next_label: IdCounter<Label>,
    deferred_values: FxHashMap<DeferredConst, U256>,
}

/// Relocating assembler for finalized EVM IR.
#[derive(Debug)]
pub(crate) struct Assembler<'gcx> {
    gcx: Gcx<'gcx>,
    /// EVM IR emitted directly by MIR lowering.
    program: ir::Module,
    /// Block currently receiving emitted instructions.
    current_block: Option<ir::BlockId>,
    /// Original assembler label attached to each EVM IR block.
    block_labels: Vec<Option<Label>>,
    /// Defined assembler labels and their EVM IR blocks.
    label_blocks: FxHashMap<Label, ir::BlockId>,
    /// Labels marked cold before or after their definition.
    cold_labels: GrowableBitSet<Label>,
    /// Unresolved block references emitted as push operands.
    label_relocations: Vec<(ir::BlockId, usize, Label)>,
    /// Unresolved deferred constants emitted as push operands.
    deferred_relocations: Vec<(ir::BlockId, usize, DeferredConst)>,
    /// Interned push immediates too large for inline storage.
    push_values: LocalInterner<U256, PushValueId>,
    /// Next label ID.
    next_label: IdCounter<Label>,
    /// Next deferred constant ID.
    next_deferred: IdCounter<DeferredConst>,
    /// Resolved values for deferred constants.
    deferred_values: FxHashMap<DeferredConst, U256>,
    /// Unresolved deferred allocations emitted as push operands.
    alloc_relocations: Vec<(ir::BlockId, usize, DeferredAlloc)>,
    /// Next deferred allocation ID.
    next_deferred_alloc: IdCounter<DeferredAlloc>,
    /// Final placement of deferred allocations.
    deferred_allocations: FxHashMap<DeferredAlloc, DeferredAllocResolution>,
}

/// Final lowering selected for a deferred allocation.
#[derive(Clone, Copy, Debug)]
enum DeferredAllocResolution {
    Static(U256),
    Dynamic(U256),
}

impl<'gcx> Assembler<'gcx> {
    /// Creates a new assembler.
    #[must_use]
    pub(crate) fn new(gcx: Gcx<'gcx>) -> Self {
        Self {
            gcx,
            program: Self::new_ir_module(),
            current_block: None,
            block_labels: Vec::new(),
            label_blocks: FxHashMap::default(),
            cold_labels: GrowableBitSet::new_empty(),
            label_relocations: Vec::new(),
            deferred_relocations: Vec::new(),
            push_values: LocalInterner::new(),
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
        self.program = Self::new_ir_module();
        self.current_block = None;
        self.block_labels.clear();
        self.label_blocks.clear();
        self.cold_labels.clear();
        self.label_relocations.clear();
        self.deferred_relocations.clear();
        self.push_values.clear();
        self.next_label.clear();
        self.next_deferred.clear();
        self.deferred_values.clear();
        self.alloc_relocations.clear();
        self.next_deferred_alloc.clear();
        self.deferred_allocations.clear();
    }

    /// Creates a new label.
    pub(crate) fn new_label(&mut self) -> Label {
        self.next_label.next()
    }

    /// Creates a new deferred constant.
    pub(crate) fn new_deferred_const(&mut self) -> DeferredConst {
        self.next_deferred.next()
    }

    /// Emits a raw opcode.
    pub(crate) fn emit_op(&mut self, opcode: u8) {
        self.push_ir_instruction(ir::Instruction::opcode(opcode));
    }

    /// Emits a push instruction with an immediate value.
    pub(crate) fn emit_push(&mut self, value: U256) {
        self.push_ir_instruction(ir::Instruction::push_value(value));
    }

    /// Emits a push instruction that will be resolved to a label's offset.
    pub(crate) fn emit_push_label(&mut self, label: Label) {
        let (block, instruction) =
            self.push_ir_instruction(ir::Instruction::push_value(U256::ZERO));
        self.label_relocations.push((block, instruction, label));
    }

    /// Emits a push instruction for a deferred constant.
    pub(crate) fn emit_push_deferred(&mut self, id: DeferredConst) {
        let (block, instruction) =
            self.push_ir_instruction(ir::Instruction::push_value(U256::ZERO));
        self.deferred_relocations.push((block, instruction, id));
    }

    /// Sets the value of a deferred constant.
    pub(crate) fn set_deferred_const(&mut self, id: DeferredConst, value: U256) {
        self.deferred_values.insert(id, value);
    }

    /// Emits an allocation whose static or dynamic placement is chosen after
    /// exact backend frame layout is known.
    pub(in crate::backend::evm) fn emit_deferred_alloc(&mut self) -> DeferredAlloc {
        let id = self.next_deferred_alloc.next();
        let (block, instruction) =
            self.push_ir_instruction(ir::Instruction::push_value(U256::ZERO));
        self.alloc_relocations.push((block, instruction, id));
        id
    }

    /// Resolves an allocation to a compile-time address.
    pub(in crate::backend::evm) fn set_deferred_alloc_static(
        &mut self,
        id: DeferredAlloc,
        address: U256,
    ) {
        self.deferred_allocations.insert(id, DeferredAllocResolution::Static(address));
    }

    /// Resolves an allocation to the ordinary free-memory-pointer bump.
    pub(in crate::backend::evm) fn set_deferred_alloc_dynamic(
        &mut self,
        id: DeferredAlloc,
        size: U256,
    ) {
        self.deferred_allocations.insert(id, DeferredAllocResolution::Dynamic(size));
    }

    /// Emits a `PUSH32` zero placeholder for the immutable identified by `id`.
    pub(crate) fn emit_push_immutable(&mut self, id: u32) {
        self.push_ir_instruction(ir::Instruction::push_immutable(id));
    }

    /// Defines a label and emits a `JUMPDEST` at the current position.
    pub(crate) fn define_label(&mut self, label: Label) {
        let mut block = ir::Block::new(self.program.blocks.len() as u32);
        if self.cold_labels.contains(label) {
            block.metadata.hotness = ir::Hotness::Cold;
        }
        let block = self.program.add_block(block);
        self.current_block = Some(block);
        self.block_labels.push(Some(label));
        self.label_blocks.insert(label, block);
    }

    /// Marks a label-started block as cold for EVM IR layout passes.
    pub(in crate::backend::evm) fn mark_label_cold(&mut self, label: Label) {
        self.cold_labels.insert(label);
        if let Some(&block) = self.label_blocks.get(&label) {
            self.program.blocks[block].metadata.hotness = ir::Hotness::Cold;
        }
    }

    fn new_ir_module() -> ir::Module {
        ir::Module::new("asm")
    }

    fn current_block(&mut self) -> ir::BlockId {
        if let Some(block) = self.current_block {
            return block;
        }
        let block = self.program.add_block(ir::Block::new(self.program.blocks.len() as u32));
        self.current_block = Some(block);
        self.block_labels.push(None);
        block
    }

    fn push_ir_instruction(&mut self, instruction: ir::Instruction) -> (ir::BlockId, usize) {
        let block = self.current_block();
        let index = self.program.blocks[block].instructions.len();
        self.program.blocks[block].instructions.push(instruction);
        (block, index)
    }

    pub(in crate::backend::evm) fn push_inst(&mut self, value: U256) -> AsmInst {
        if let Ok(value) = u32::try_from(value)
            && let Some(inst) = AsmInst::push_inline(value)
        {
            return inst;
        }

        AsmInst::push(self.push_values.intern(value))
    }

    pub(super) fn push_value(&self, index: PushValueId) -> U256 {
        *self.push_values.get(index)
    }

    fn finish_evm_ir(&mut self) -> Option<(ir::Module, Vec<Option<Label>>)> {
        let mut module = std::mem::replace(&mut self.program, Self::new_ir_module());
        self.current_block = None;
        if module.blocks.is_empty() {
            return None;
        }

        for (block, instruction, label) in self.label_relocations.drain(..) {
            let target = self
                .label_blocks
                .get(&label)
                .copied()
                .unwrap_or_else(|| panic!("label {label:?} was never defined"));
            module.blocks[block].instructions[instruction] = ir::Instruction::push_block(target);
        }
        for (block, instruction, id) in self.deferred_relocations.drain(..) {
            module.blocks[block].instructions[instruction] = ir::Instruction::push_deferred(id);
        }
        // Allocation placeholders expand to more than one instruction, so they
        // splice after every in-place relocation patch above. Descending
        // instruction order keeps earlier indices in the same block valid.
        let mut alloc_relocations = std::mem::take(&mut self.alloc_relocations);
        alloc_relocations
            .sort_by_key(|&(block, instruction, _)| std::cmp::Reverse((block, instruction)));
        for (block, instruction, id) in alloc_relocations {
            let resolution = self
                .deferred_allocations
                .get(&id)
                .copied()
                .unwrap_or_else(|| panic!("deferred allocation {id:?} was never resolved"));
            let push = |value: U256| ir::Instruction::push_value(value);
            let replacement = match resolution {
                DeferredAllocResolution::Static(address) => vec![push(address)],
                DeferredAllocResolution::Dynamic(size) => vec![
                    push(U256::from(EvmMemoryLayout::FMP_SLOT)),
                    ir::Instruction::opcode(op::MLOAD),
                    ir::Instruction::opcode(op::DUP1),
                    push(size),
                    ir::Instruction::opcode(op::ADD),
                    push(U256::from(EvmMemoryLayout::FMP_SLOT)),
                    ir::Instruction::opcode(op::MSTORE),
                ],
            };
            module.blocks[block].instructions.splice(instruction..=instruction, replacement);
        }
        self.deferred_allocations.clear();
        self.label_blocks.clear();
        self.cold_labels.clear();

        Self::finalize_evm_ir(&mut module);
        Some((module, std::mem::take(&mut self.block_labels)))
    }

    fn finalize_evm_ir(module: &mut ir::Module) {
        for index in 0..module.blocks.len() {
            let block_id = ir::BlockId::from_usize(index);
            let next =
                (index + 1 < module.blocks.len()).then(|| ir::BlockId::from_usize(index + 1));
            let block = &mut module.blocks[block_id];
            let (kind, remove) = if let [.., push, jump] = block.instructions.as_slice()
                && !jump.is_encoded_push()
                && jump.opcode == op::JUMP
                && let Some(target) = push.pushed_block()
                && push.is_encoded_push()
            {
                (ir::TerminatorKind::Jump(target), 2)
            } else if let Some(last) = block.instructions.last()
                && !last.is_encoded_push()
                && last.opcode == op::STOP
            {
                (ir::TerminatorKind::Op(op::STOP), 1)
            } else if let Some(last) = block.instructions.last()
                && !last.is_encoded_push()
                && op::is_terminal(last.opcode)
            {
                (ir::TerminatorKind::Op(last.opcode), 1)
            } else {
                (next.map_or(ir::TerminatorKind::Op(op::STOP), ir::TerminatorKind::Jump), 0)
            };
            block.instructions.truncate(block.instructions.len() - remove);
            block.terminator = Some(ir::Terminator::new(kind));
        }
    }

    /// Resolves relocations and encodes finalized EVM IR as bytecode.
    #[must_use]
    #[cfg(test)]
    pub(crate) fn assemble(&mut self) -> AssembledCode {
        self.assemble_with_evm_ir(false)
    }

    #[must_use]
    pub(crate) fn assemble_with_evm_ir(&mut self, capture_evm_ir: bool) -> AssembledCode {
        let prepared = self.prepare(capture_evm_ir);
        let result = self.assemble_prepared(&prepared, &[]);
        self.clear();
        result
    }

    #[tracing::instrument(
        name = "evm_ir_pipeline",
        level = "debug",
        skip_all,
        fields(program = %self.program.name()),
    )]
    pub(in crate::backend::evm) fn prepare(&mut self, capture_evm_ir: bool) -> PreparedAssembly {
        let Some((mut ir_program, mut labels)) = self.finish_evm_ir() else {
            return PreparedAssembly::default();
        };

        Self::resolve_known_deferred_constants(&mut ir_program, &self.deferred_values);

        if self.gcx.sess.opts.optimization != OptimizationMode::None {
            let input_is_valid = cfg!(debug_assertions) && is_valid_evm_ir(&ir_program);
            for pass in ir::DEFAULT_PIPELINE {
                ir::run_pass(self.gcx, &mut ir_program, pass);
            }
            debug_assert!(!input_is_valid || is_valid_evm_ir(&ir_program));
        }

        let evm_ir = capture_evm_ir.then(|| ir_program.clone());
        let program = assembly::lower_evm_ir(&ir_program, &mut labels, self);
        PreparedAssembly {
            evm_ir,
            program,
            push_values: std::mem::take(&mut self.push_values),
            next_label: std::mem::take(&mut self.next_label),
            deferred_values: std::mem::take(&mut self.deferred_values),
        }
    }

    fn resolve_known_deferred_constants(
        module: &mut ir::Module,
        values: &FxHashMap<DeferredConst, U256>,
    ) {
        for block in &mut module.blocks {
            for inst in &mut block.instructions {
                let Some(id) = inst.deferred_push() else { continue };
                if let Some(&value) = values.get(&id) {
                    *inst = ir::Instruction::push_value(value);
                }
            }
        }
    }

    pub(in crate::backend::evm) fn assemble_prepared(
        &mut self,
        prepared: &PreparedAssembly,
        deferred_values: &[(DeferredConst, U256)],
    ) -> AssembledCode {
        self.push_values = prepared.push_values.clone();
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
                        *inst = ir::Instruction::push_value(value);
                    }
                }
            }
            module
        });

        // Label-free constructor and deployment snippets need neither offset
        // discovery nor push-width relaxation.
        if !program
            .instructions
            .iter()
            .any(|inst| matches!(inst.kind(), AsmInstKind::Label(_) | AsmInstKind::PushLabel(_)))
        {
            let mut result =
                self.emit_bytecode(&program, FxHashMap::default(), &FxHashMap::default());
            result.evm_ir = evm_ir;
            return result;
        }

        // Start from the narrowest possible label pushes. Widening pushes can
        // only increase later label offsets, so required widths grow
        // monotonically to the least fixed point. Starting wide and shrinking
        // can instead settle on a larger valid encoding at a byte-width
        // boundary.
        let mut push_widths: FxHashMap<usize, u8> = FxHashMap::default();
        for (idx, inst) in program.instructions.iter().enumerate() {
            if matches!(inst.kind(), AsmInstKind::PushLabel(_)) {
                push_widths.insert(idx, 0);
            }
        }

        loop {
            let (label_offsets, new_widths) = self.compute_offsets(&program, &push_widths);
            if new_widths == push_widths {
                let mut result = self.emit_bytecode(&program, label_offsets, &push_widths);
                result.evm_ir = evm_ir;
                return result;
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
    ) -> (FxHashMap<Label, usize>, FxHashMap<usize, u8>) {
        let mut offset = 0usize;
        let mut label_offsets = FxHashMap::default();
        let mut new_widths = FxHashMap::default();
        let out = BytecodeAssembler::new(self.gcx);

        for (idx, inst) in program.instructions.iter().enumerate() {
            match inst.kind() {
                AsmInstKind::Op(_) => {
                    offset += 1;
                }
                AsmInstKind::PushInline(value) => {
                    offset += out.encoded_push_len(U256::from(value));
                }
                AsmInstKind::Push(index) => {
                    offset += out.encoded_push_len(self.push_value(index));
                }
                AsmInstKind::PushLabel(_) => {
                    // Use current estimated width
                    let width = push_widths.get(&idx).copied().unwrap_or(2);
                    offset += out.fixed_push_len(width);
                }
                AsmInstKind::PushDeferred(_) => {
                    unreachable!("deferred values must be resolved before assembly");
                }
                AsmInstKind::PushImmutable(_) => {
                    // PUSH32 opcode plus 32 placeholder bytes.
                    offset += 33;
                }
                AsmInstKind::Label(label) => {
                    label_offsets.insert(label, offset);
                    offset += 1;
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
            }
        }

        (label_offsets, new_widths)
    }

    /// Emits the final bytecode.
    fn emit_bytecode(
        &self,
        program: &AssemblyProgram,
        label_offsets: FxHashMap<Label, usize>,
        push_widths: &FxHashMap<usize, u8>,
    ) -> AssembledCode {
        let mut out = BytecodeAssembler::new(self.gcx);

        for (idx, inst) in program.instructions.iter().enumerate() {
            match inst.kind() {
                AsmInstKind::Op(opcode) => {
                    out.emit_op(opcode);
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
                AsmInstKind::PushDeferred(_) => {
                    unreachable!("deferred values must be resolved before assembly");
                }
                AsmInstKind::PushImmutable(id) => {
                    out.emit_push_immutable(id);
                }
                AsmInstKind::Label(_) => {
                    out.emit_op(op::JUMPDEST);
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

fn is_valid_evm_ir(module: &ir::Module) -> bool {
    let dcx = DiagCtxt::with_silent_emitter(None);
    ir::validate(&dcx, module);
    dcx.has_errors().is_ok()
}

#[derive(Debug)]
struct BytecodeAssembler<'gcx> {
    gcx: Gcx<'gcx>,
    bytecode: Vec<u8>,
    immutable_refs: Vec<ImmutableRef>,
}

impl<'gcx> BytecodeAssembler<'gcx> {
    fn new(gcx: Gcx<'gcx>) -> Self {
        Self { gcx, bytecode: Vec::new(), immutable_refs: Vec::new() }
    }

    fn emit_op(&mut self, opcode: u8) {
        self.bytecode.push(opcode);
    }

    fn emit_push_immutable(&mut self, id: u32) {
        self.immutable_refs.push(ImmutableRef { id, code_offset: self.bytecode.len() });
        self.bytecode.push(op::PUSH32);
        self.bytecode.extend(std::iter::repeat_n(0, IMMUTABLE_WORD_SIZE));
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
        if width == 0 {
            self.emit_push_zero();
            return;
        }

        self.bytecode.push(op::push(width));

        let bytes = value.to_be_bytes::<EVM_WORD_BYTES>();
        let start = EVM_WORD_BYTES - width as usize;
        self.bytecode.extend_from_slice(&bytes[start..]);
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
        AssembledCode { bytecode: self.bytecode, immutable_refs: self.immutable_refs, evm_ir: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::evm::test_utils::disassemble;
    use snapbox::{assert_data_eq, str};
    use solar_config::{CompileOpts, EvmVersion};
    use solar_interface::Session;
    use solar_sema::Compiler;

    fn with_assembler<T: Send>(opts: CompileOpts, f: impl FnOnce(Assembler<'_>) -> T + Send) -> T {
        let compiler = Compiler::new(Session::builder().opts(opts).build());
        compiler.enter(|c| f(Assembler::new(c.gcx())))
    }

    fn size_optimized_opts() -> CompileOpts {
        opts(EvmVersion::Shanghai, OptimizationMode::Size)
    }

    fn opts(evm_version: EvmVersion, optimization: OptimizationMode) -> CompileOpts {
        CompileOpts { evm_version, optimization, ..Default::default() }
    }

    fn assemble(opts: CompileOpts, f: impl FnOnce(&mut Assembler<'_>) + Send) -> AssembledCode {
        with_assembler(opts, |mut asm| {
            f(&mut asm);
            asm.assemble()
        })
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
    fn assembler_can_be_reused_after_assembly() {
        with_assembler(CompileOpts::default(), |mut asm| {
            let large = U256::from(1u64 << 31);

            asm.emit_push(large);
            let first = asm.assemble();

            assert_data_eq!(
                disassemble(&first.bytecode),
                str![[r#"
PUSH4 0x80000000

"#]]
            );
            assert!(asm.program.blocks.is_empty());
            assert_eq!(asm.push_values.len(), 0);

            asm.emit_push(U256::from(2));
            let second = asm.assemble();

            assert_data_eq!(
                disassemble(&second.bytecode),
                str![[r#"
PUSH1 0x02

"#]]
            );
        });
    }

    #[test]
    fn deferred_allocations_expand_after_layout() {
        let mut static_asm = Assembler::new();
        let static_alloc = static_asm.emit_deferred_alloc();
        static_asm.set_deferred_alloc_static(static_alloc, U256::from(0xa0));
        assert_eq!(static_asm.assemble().bytecode, [op::PUSH1, 0xa0]);

        let mut dynamic_asm = Assembler::new();
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
    }

    #[test]
    fn push_zero_uses_push0_when_available() {
        let result = assemble(opts(EvmVersion::Shanghai, OptimizationMode::None), |asm| {
            asm.emit_push(U256::ZERO);
        });

        assert_data_eq!(
            disassemble(&result.bytecode),
            str![[r#"
PUSH0

"#]]
        );
    }

    #[test]
    fn push_zero_uses_push1_before_shanghai() {
        let result = assemble(opts(EvmVersion::Berlin, OptimizationMode::Gas), |asm| {
            asm.emit_push(U256::ZERO);
        });

        assert_data_eq!(
            disassemble(&result.bytecode),
            str![[r#"
PUSH1 0x00

"#]]
        );
    }

    #[test]
    fn compact_push_respects_optimization_mode() {
        let assemble_push = |optimization| {
            assemble(opts(EvmVersion::Shanghai, optimization), |asm| asm.emit_push(U256::MAX))
        };
        let size_optimized = assemble_push(OptimizationMode::Size);
        let gas_optimized = assemble_push(OptimizationMode::Gas);
        let unoptimized = assemble_push(OptimizationMode::None);

        assert_data_eq!(
            disassemble(&size_optimized.bytecode),
            str![[r#"
PUSH0
NOT

"#]]
        );
        assert_data_eq!(
            disassemble(&gas_optimized.bytecode),
            str![[r#"
PUSH0
NOT

"#]]
        );
        assert_data_eq!(
            disassemble(&unoptimized.bytecode),
            str![[r#"
PUSH32 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff

"#]]
        );
    }

    #[test]
    fn compact_push_uses_push1_zero_before_shanghai() {
        let result = assemble(opts(EvmVersion::Berlin, OptimizationMode::Size), |asm| {
            asm.emit_push(U256::MAX);
        });

        assert_data_eq!(
            disassemble(&result.bytecode),
            str![[r#"
PUSH1 0x00
NOT

"#]]
        );
    }

    #[test]
    fn test_simple_assembly() {
        with_assembler(CompileOpts::default(), |mut asm| {
            asm.emit_push(U256::from(42));
            asm.emit_push(U256::from(10));
            asm.emit_op(op::ADD);
            asm.emit_op(op::STOP);

            let result = asm.assemble();

            assert_data_eq!(
                disassemble(&result.bytecode),
                str![[r#"
PUSH1 0x2a
PUSH1 0x0a
ADD

"#]]
            );
        });
    }

    #[test]
    fn test_label_resolution() {
        with_assembler(CompileOpts::default(), |mut asm| {
            let loop_label = asm.new_label();
            let end_label = asm.new_label();

            asm.define_label(loop_label);
            asm.emit_push(U256::from(1));
            asm.emit_push_label(end_label);
            asm.emit_op(op::JUMPI);
            asm.emit_push_label(loop_label);
            asm.emit_op(op::JUMP);

            asm.define_label(end_label);
            asm.emit_op(op::STOP);

            let result = asm.assemble();

            assert_data_eq!(
                disassemble(&result.bytecode),
                str![[r#"
JUMPDEST
PUSH1 0x01
PUSH1 0x08
JUMPI
PUSH0
JUMP
JUMPDEST

"#]]
            );
        });
    }

    #[test]
    fn label_push_width_relaxation_cascades() {
        let result = assemble(opts(EvmVersion::Shanghai, OptimizationMode::None), |asm| {
            let first = asm.new_label();
            let second = asm.new_label();

            asm.emit_push_label(first);
            asm.define_label(first);
            asm.emit_push_label(second);
            for _ in 0..7 {
                asm.emit_push(U256::MAX);
            }
            asm.emit_push(U256::from(1) << 144);
            asm.define_label(second);
            asm.emit_op(op::STOP);
        });

        assert_data_eq!(
            disassemble(&result.bytecode),
            str![[r#"
PUSH1 0x02
JUMPDEST
PUSH2 0x0101
PUSH32 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
PUSH32 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
PUSH32 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
PUSH32 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
PUSH32 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
PUSH32 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
PUSH32 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
PUSH19 0x01000000000000000000000000000000000000
JUMPDEST

"#]]
        );
    }

    #[test]
    fn cold_terminal_block_moves_after_hot_block() {
        with_assembler(CompileOpts::default(), |mut asm| {
            let cold = asm.new_label();
            let hot = asm.new_label();

            asm.emit_push(U256::ONE);
            asm.emit_push_label(cold);
            asm.emit_op(op::JUMPI);
            asm.emit_push_label(hot);
            asm.emit_op(op::JUMP);
            asm.mark_label_cold(cold);
            asm.define_label(cold);
            asm.emit_push(U256::ZERO);
            asm.emit_push(U256::ZERO);
            asm.emit_op(op::REVERT);
            asm.define_label(hot);
            asm.emit_op(op::STOP);

            let result = asm.assemble();

            assert_data_eq!(
                disassemble(&result.bytecode),
                str![[r#"
PUSH1 0x01
PUSH1 0x06
JUMPI
STOP
JUMPDEST
PUSH0
PUSH0
REVERT

"#]]
            );
        });
    }

    #[test]
    fn block_layout_materializes_moved_implicit_stop() {
        with_assembler(CompileOpts::default(), |mut asm| {
            let cold = asm.new_label();
            let eof = asm.new_label();

            asm.emit_push(U256::ONE);
            asm.emit_push_label(cold);
            asm.emit_op(op::JUMPI);
            asm.emit_push_label(eof);
            asm.emit_op(op::JUMP);
            asm.mark_label_cold(cold);
            asm.define_label(cold);
            asm.emit_push(U256::ZERO);
            asm.emit_push(U256::ZERO);
            asm.emit_op(op::REVERT);
            asm.define_label(eof);

            let result = asm.assemble();

            assert_data_eq!(
                disassemble(&result.bytecode),
                str![[r#"
PUSH1 0x01
PUSH1 0x06
JUMPI
STOP
JUMPDEST
PUSH0
PUSH0
REVERT

"#]]
            );
        });
    }

    #[test]
    fn terminal_dedup_labels_prior_unlabeled_target() {
        with_assembler(CompileOpts::default(), |mut asm| {
            let duplicate = asm.new_label();

            for copy in 0..2 {
                if copy == 1 {
                    asm.define_label(duplicate);
                }
                asm.emit_push(U256::from(0x1234));
                asm.emit_push(U256::ZERO);
                asm.emit_op(op::MSTORE);
                asm.emit_push(U256::ZERO);
                asm.emit_push(U256::ZERO);
                asm.emit_op(op::REVERT);
            }

            let result = asm.assemble();

            assert_data_eq!(
                disassemble(&result.bytecode),
                str![[r#"
PUSH2 0x1234
PUSH0
MSTORE
PUSH0
PUSH0
REVERT

"#]]
            );
        });
    }

    #[test]
    fn block_layout_elides_jump_after_jumpi() {
        with_assembler(CompileOpts::default(), |mut asm| {
            let conditional = asm.new_label();
            let default = asm.new_label();

            asm.emit_push(U256::ONE);
            asm.emit_push_label(conditional);
            asm.emit_op(op::JUMPI);
            asm.emit_push_label(default);
            asm.emit_op(op::JUMP);
            asm.define_label(conditional);
            asm.emit_op(op::INVALID);
            asm.define_label(default);
            asm.emit_op(op::STOP);

            let result = asm.assemble();

            assert_data_eq!(
                disassemble(&result.bytecode),
                str![[r#"
PUSH1 0x01
PUSH1 0x06
JUMPI
STOP
JUMPDEST
INVALID

"#]]
            );
        });
    }

    #[test]
    fn cold_terminal_block_keeps_fallthrough_position() {
        with_assembler(CompileOpts::default(), |mut asm| {
            let cold = asm.new_label();

            asm.emit_push(U256::ONE);
            asm.mark_label_cold(cold);
            asm.define_label(cold);
            asm.emit_push(U256::ZERO);
            asm.emit_push(U256::ZERO);
            asm.emit_op(op::REVERT);

            let result = asm.assemble();

            assert_data_eq!(
                disassemble(&result.bytecode),
                str![[r#"
PUSH1 0x01
PUSH0
PUSH0
REVERT

"#]]
            );
        });
    }

    #[test]
    fn compact_full_word_all_ones_push() {
        with_assembler(size_optimized_opts(), |mut asm| {
            asm.emit_push(U256::MAX);
            asm.emit_op(op::STOP);

            let result = asm.assemble();

            assert_data_eq!(
                disassemble(&result.bytecode),
                str![[r#"
PUSH0
NOT

"#]]
            );
        });
    }

    #[test]
    fn compact_lower_all_ones_mask_push() {
        with_assembler(size_optimized_opts(), |mut asm| {
            let mask = (U256::from(1) << 160) - U256::from(1);

            asm.emit_push(mask);
            asm.emit_op(op::STOP);

            let result = asm.assemble();

            assert_data_eq!(
                disassemble(&result.bytecode),
                str![[r#"
PUSH0
NOT
PUSH1 0x60
SHR

"#]]
            );
        });
    }

    #[test]
    fn compact_not_small_push() {
        with_assembler(size_optimized_opts(), |mut asm| {
            asm.emit_push(!U256::from(31));
            asm.emit_op(op::STOP);

            let result = asm.assemble();

            assert_data_eq!(
                disassemble(&result.bytecode),
                str![[r#"
PUSH1 0x1f
NOT

"#]]
            );
        });
    }

    #[test]
    fn compact_not_byte_push() {
        with_assembler(size_optimized_opts(), |mut asm| {
            asm.emit_push(!U256::from(255));
            asm.emit_op(op::STOP);

            let result = asm.assemble();

            assert_data_eq!(
                disassemble(&result.bytecode),
                str![[r#"
PUSH1 0xff
NOT

"#]]
            );
        });
    }

    #[test]
    fn compact_left_aligned_selector_push() {
        with_assembler(size_optimized_opts(), |mut asm| {
            let selector = U256::from(0x35ea6a75u64) << 224;

            asm.emit_push(selector);
            asm.emit_op(op::STOP);

            let result = asm.assemble();

            assert_data_eq!(
                disassemble(&result.bytecode),
                str![[r#"
PUSH4 0x35ea6a75
PUSH1 0xe0
SHL

"#]]
            );
        });
    }

    #[test]
    fn compact_right_padded_text_push() {
        with_assembler(size_optimized_opts(), |mut asm| {
            let text = U256::from_be_slice(b"Machine finished:");
            let value = text << ((32 - "Machine finished:".len()) * 8);

            asm.emit_push(value);
            asm.emit_op(op::STOP);

            let result = asm.assemble();

            assert_data_eq!(
                disassemble(&result.bytecode),
                str![[r#"
PUSH17 0x4d616368696e652066696e69736865643a
PUSH1 0x78
SHL

"#]]
            );
        });
    }
}

//! Construction of scheduled EVM IR at the MIR lowering boundary.
//!
//! Labels and deferred allocations travel with their instructions, so instruction deletion
//! cannot invalidate a relocation table. Finishing resolves labels, expands allocations and
//! recognizes terminal opcodes once. Only then does block EVM IR enter optimization and assembly.
//! The builder owns emission state; prepared assembly owns its immutable stream and constant pools.

use super::{
    self as ir,
    assembly::{self, AsmIndex, DeferredAlloc},
};
use crate::{
    backend::evm::{
        assembler::{ArtifactKind, AssembledCode, DeferredConst, Label, PreparedAssembly},
        op::{self, push_len},
    },
    memory::EvmMemoryLayout,
    mir::{DataRef as MirDataRef, ImmutableId, Module as MirModule, TypeSize},
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::GrowableBitSet, index::index_vec, map::FxHashMap};
use solar_interface::{Symbol, sym};
use solar_sema::Gcx;

/// The unfinished scheduled program and its construction-only bindings.
#[derive(Debug)]
pub(in crate::backend::evm) struct Builder<'gcx> {
    gcx: Gcx<'gcx>,
    artifact_kind: ArtifactKind,
    program: ir::Module,
    current_block: Option<ir::BlockId>,
    label_blocks: FxHashMap<Label, ir::BlockId>,
    cold_labels: GrowableBitSet<Label>,
    indexed_jumps: Vec<(ir::BlockId, Vec<Label>)>,
    next_label: Label,
    next_deferred: DeferredConst,
    deferred_values: FxHashMap<DeferredConst, U256>,
    next_deferred_alloc: DeferredAlloc,
    deferred_allocations: FxHashMap<DeferredAlloc, DeferredAllocResolution>,
}

/// Reserves one typed ID, checking the compact instruction payload limit.
fn take_next<I: AsmIndex>(next: &mut I) -> I {
    let id = *next;
    id.inst_payload();
    *next = I::from_usize(id.index() + 1);
    id
}

/// Placement chosen after exact frame layout is known.
#[derive(Clone, Copy, Debug)]
enum DeferredAllocResolution {
    Static(U256),
    Dynamic(U256),
}

/// Assembles finalized EVM IR without passing through construction state.
pub(in crate::backend::evm) fn assemble_evm_ir<'gcx>(
    gcx: Gcx<'gcx>,
    module: ir::Module,
    capture_evm_ir: bool,
) -> solar_interface::Result<AssembledCode> {
    if module
        .blocks
        .iter()
        .any(|block| block.instructions.iter().any(|inst| inst.deferred_push().is_some()))
    {
        return Err(gcx.dcx().err("cannot assemble unresolved `push_deferred` instruction").emit());
    }
    if module.blocks.iter().any(|block| {
        block.instructions.iter().any(|inst| {
            matches!(inst.opcode, op::EXTCALL | op::EXTDELEGATECALL | op::EXTSTATICCALL)
        })
    }) {
        return Err(gcx
            .dcx()
            .err("cannot assemble EOF-only external calls into legacy bytecode")
            .emit());
    }

    debug_assert!(ir::verify::Verifier::is_valid(&module));

    let prepared = super::assembly::prepare(gcx, module, FxHashMap::default(), capture_evm_ir);
    gcx.dcx().has_errors()?;
    Ok(prepared.assemble(gcx.sess.opts.evm_version, &[]))
}

impl<'gcx> Builder<'gcx> {
    pub(crate) fn new(gcx: Gcx<'gcx>) -> Self {
        Self {
            gcx,
            artifact_kind: ArtifactKind::Runtime,
            program: ir::Module::new(sym::asm),
            current_block: None,
            label_blocks: FxHashMap::default(),
            cold_labels: GrowableBitSet::new_empty(),
            indexed_jumps: Vec::new(),
            next_label: Label::from_usize(0),
            next_deferred: DeferredConst::from_usize(0),
            deferred_values: FxHashMap::default(),
            next_deferred_alloc: DeferredAlloc::from_usize(0),
            deferred_allocations: FxHashMap::default(),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.artifact_kind = ArtifactKind::Runtime;
        self.program.clear();
        self.current_block = None;
        self.label_blocks.clear();
        self.cold_labels.clear();
        self.indexed_jumps.clear();
        self.next_label = Label::from_usize(0);
        self.next_deferred = DeferredConst::from_usize(0);
        self.deferred_values.clear();
        self.next_deferred_alloc = DeferredAlloc::from_usize(0);
        self.deferred_allocations.clear();
    }

    pub(crate) fn set_artifact_kind(&mut self, kind: ArtifactKind) {
        self.artifact_kind = kind;
    }

    pub(crate) fn set_evm_ir_name(&mut self, name: Symbol) {
        self.program.set_name(Symbol::intern(&format!("{name}_{}", self.artifact_kind.name())));
    }

    pub(crate) fn set_enable_size_outlining(&mut self, enable: bool) {
        self.program.enable_size_outlining = enable;
    }

    pub(crate) fn indexed_jump_target_width_bound(&self) -> usize {
        assembly::indexed_jump_target_width_bound(
            self.gcx.sess.opts.evm_version,
            self.artifact_kind == ArtifactKind::Constructor,
        )
    }

    pub(in crate::backend::evm) fn prepare(&mut self, capture_evm_ir: bool) -> PreparedAssembly {
        let module = self.finish_evm_ir();
        assembly::prepare(
            self.gcx,
            module,
            std::mem::take(&mut self.deferred_values),
            capture_evm_ir,
        )
    }

    #[cfg(test)]
    pub(crate) fn assemble(&mut self) -> AssembledCode {
        self.assemble_with_evm_ir(false)
    }

    pub(crate) fn assemble_with_evm_ir(&mut self, capture_evm_ir: bool) -> AssembledCode {
        let prepared = self.prepare(capture_evm_ir);
        let result = prepared.assemble(self.gcx.sess.opts.evm_version, &[]);
        self.clear();
        result
    }

    /// Creates a new label.
    pub(crate) fn new_label(&mut self) -> Label {
        take_next(&mut self.next_label)
    }

    /// Creates a new deferred constant.
    pub(crate) fn new_deferred_const(&mut self) -> DeferredConst {
        take_next(&mut self.next_deferred)
    }

    /// Emits a raw opcode.
    pub(crate) fn emit_op(&mut self, opcode: u8) {
        self.push_ir_instruction(ir::Instruction::opcode(opcode));
    }

    /// Emits a logical stack operation.
    pub(crate) fn emit_stack_op(&mut self, stack_op: op::StackOp) {
        self.push_ir_instruction(ir::Instruction::stack_op(stack_op));
    }

    /// Emits a push instruction with an immediate value.
    pub(crate) fn emit_push(&mut self, value: U256) {
        self.push_ir_instruction(ir::Instruction::push_value(value));
    }

    /// Loads MIR constant data into the EVM IR module with matching IDs.
    pub(crate) fn load_data(&mut self, module: &MirModule) {
        assert!(self.program.data.is_empty(), "EVM IR data must be empty before loading MIR data");
        self.program.data = module
            .iter_data()
            .map(|(id, data)| ir::Data {
                bytes: data.clone(),
                name: module.data_name(id),
                emit_in_runtime: self.artifact_kind == ArtifactKind::Runtime
                    && module.data_is_emitted_in_runtime(id),
            })
            .collect();
    }

    /// Emits a relocatable constant-data address push.
    pub(crate) fn emit_push_data(&mut self, data: MirDataRef) {
        self.push_ir_instruction(ir::Instruction::push_data(ir::DataRef::new(
            ir::DataId::from_usize(data.id.index()),
            data.offset,
        )));
    }

    /// Returns optimistic and block-layout byte sizes for the entry trace through
    /// the current block.
    pub(crate) fn current_trace_size_bounds(
        &self,
        block_target_width: usize,
        deferred_value_width: usize,
    ) -> Option<(usize, usize)> {
        let current = self.current_block?;
        let mut references = index_vec![0usize; self.program.blocks.len()];
        references[ir::BlockId::ENTRY] = 1;
        for block in &self.program.blocks {
            for inst in &block.instructions {
                if let Some(ir::PushValue::Label(label)) = inst.value
                    && let Some(&target) = self.label_blocks.get(&label)
                {
                    references[target] += 1;
                }
            }
        }
        for (_, targets) in &self.indexed_jumps {
            for &label in targets {
                if let Some(&target) = self.label_blocks.get(&label) {
                    references[target] += 1;
                }
            }
        }
        for block in self.program.blocks.indices() {
            if self.explicit_jump_target(block).is_none()
                && !self.block_has_explicit_terminator(block)
                && block.index() + 1 < self.program.blocks.len()
            {
                references[ir::BlockId::from_usize(block.index() + 1)] += 1;
            }
        }

        let mut trace = vec![current];
        while trace.last().copied() != Some(ir::BlockId::ENTRY) {
            let target = *trace.last()?;
            let mut predecessors = self
                .program
                .blocks
                .indices()
                .filter(|&block| self.trace_successor(block) == Some(target));
            let predecessor = predecessors.next()?;
            if predecessors.next().is_some() || trace.contains(&predecessor) {
                return None;
            }
            trace.push(predecessor);
        }
        trace.reverse();

        let mut bounds = (0usize, 0usize);
        for (position, &block) in trace.iter().enumerate() {
            if position != 0 && references[block] > 1 {
                bounds.0 += 1;
                bounds.1 += 1;
            }

            let next = trace.get(position + 1).copied();
            let instructions = &self.program.blocks[block].instructions;
            let end = if self.explicit_jump_target(block).is_some_and(|target| Some(target) == next)
            {
                instructions.len() - 2
            } else {
                instructions.len()
            };
            for inst in &instructions[..end] {
                let (min_size, layout_size) =
                    self.instruction_size_bounds(inst, block_target_width, deferred_value_width);
                bounds.0 += min_size;
                bounds.1 += layout_size;
            }
        }
        Some(bounds)
    }

    fn trace_successor(&self, block: ir::BlockId) -> Option<ir::BlockId> {
        if self.indexed_jumps.iter().any(|&(source, _)| source == block) {
            None
        } else if let Some(target) = self.explicit_jump_target(block) {
            Some(target)
        } else if self.block_has_explicit_terminator(block) {
            None
        } else {
            (block.index() + 1 < self.program.blocks.len())
                .then(|| ir::BlockId::from_usize(block.index() + 1))
        }
    }

    /// Number of blocks the program holds so far; the next defined label starts block `len`.
    pub(crate) fn block_count(&self) -> usize {
        self.program.blocks.len()
    }

    /// Control-flow edges among the blocks in `range` before EVM IR finalization.
    pub(crate) fn dataflow_edges(
        &self,
        range: std::ops::Range<usize>,
    ) -> Vec<(ir::BlockId, ir::BlockId)> {
        let in_range = |block: ir::BlockId| range.contains(&block.index());
        let mut edges = Vec::new();
        let mut address_taken = Vec::new();
        let mut push_edge = |edges: &mut Vec<_>, source, target| {
            if in_range(source) && in_range(target) {
                edges.push((source, target));
                if !address_taken.contains(&target) {
                    address_taken.push(target);
                }
            }
        };
        // Only sources in this function can contribute edges or address-taken targets.
        for (source, block) in
            self.program.blocks.iter_enumerated().skip(range.start).take(range.len())
        {
            for inst in &block.instructions {
                if let Some(ir::PushValue::Label(label)) = inst.value
                    && let Some(&target) = self.label_blocks.get(&label)
                {
                    push_edge(&mut edges, source, target);
                }
            }
        }
        for (source, targets) in &self.indexed_jumps {
            for label in targets {
                if let Some(&target) = self.label_blocks.get(label) {
                    push_edge(&mut edges, *source, target);
                }
            }
        }
        for index in range.clone() {
            let block = ir::BlockId::from_usize(index);
            let instructions = &self.program.blocks[block].instructions;
            let dynamic = instructions.iter().enumerate().any(|(position, inst)| {
                !inst.is_encoded_push()
                    && matches!(inst.opcode, op::JUMP | op::JUMPI)
                    && !position
                        .checked_sub(1)
                        .and_then(|previous| instructions.get(previous))
                        .is_some_and(ir::Instruction::is_encoded_push)
            });
            if dynamic {
                edges.extend(address_taken.iter().map(|&target| (block, target)));
            }
            if !self.block_has_explicit_terminator(block)
                && self.explicit_jump_target(block).is_none()
                && range.contains(&(index + 1))
            {
                edges.push((block, ir::BlockId::from_usize(index + 1)));
            }
        }
        edges
    }

    fn explicit_jump_target(&self, block: ir::BlockId) -> Option<ir::BlockId> {
        let instructions = &self.program.blocks[block].instructions;
        let [.., push, jump] = instructions.as_slice() else { return None };
        if jump.is_encoded_push() || jump.opcode != op::JUMP {
            return None;
        }
        let Some(ir::PushValue::Label(label)) = push.value else { return None };
        self.label_blocks.get(&label).copied()
    }

    fn block_has_explicit_terminator(&self, block: ir::BlockId) -> bool {
        self.indexed_jumps.iter().any(|&(source, _)| source == block)
            || self.program.blocks[block]
                .instructions
                .last()
                .is_some_and(|inst| !inst.is_encoded_push() && op::is_terminal(inst.opcode))
    }

    fn instruction_size_bounds(
        &self,
        inst: &ir::Instruction,
        block_target_width: usize,
        deferred_value_width: usize,
    ) -> (usize, usize) {
        let push_size = |value| push_len(self.gcx.sess.opts.evm_version, value);
        let size = match inst.value {
            Some(ir::PushValue::Immediate(value)) => push_size(value),
            Some(ir::PushValue::Immutable(_)) => {
                usize::from(inst.immutable_type_size().expect("immutable width").bytes()) + 1
            }
            Some(ir::PushValue::Label(_)) => return (2, block_target_width + 1),
            Some(ir::PushValue::Deferred(id)) => {
                let Some(&value) = self.deferred_values.get(&id) else {
                    return (1, deferred_value_width + 1);
                };
                push_size(value)
            }
            Some(ir::PushValue::Alloc(id)) => {
                let slot_size = push_size(U256::from(EvmMemoryLayout::FMP_SLOT));
                match self.deferred_allocations.get(&id) {
                    Some(DeferredAllocResolution::Static(address)) => push_size(*address),
                    Some(DeferredAllocResolution::Dynamic(size)) => {
                        slot_size * 2 + push_size(*size) + 4
                    }
                    None => return (1, slot_size * 2 + 33 + 4),
                }
            }
            Some(_) => return (1, 33),
            None => inst.as_stack_op().map_or(1, |stack_op| {
                stack_op
                    .assembled_len(self.gcx.sess.opts.evm_version)
                    .expect("stack operation must support the target EVM version")
            }),
        };
        (size, size)
    }

    /// Emits a push instruction that will be resolved to a label's offset.
    pub(crate) fn emit_push_label(&mut self, label: Label) {
        // push label
        self.push_ir_instruction(ir::Instruction::push_label(label));
    }

    /// Terminates the current block with an indexed jump to one of `targets`.
    pub(crate) fn emit_indexed_jump(&mut self, targets: Vec<Label>) {
        assert!(!targets.is_empty(), "indexed jump must have at least one target");
        let block = self.current_block.take().expect("indexed jump requires a current block");
        self.indexed_jumps.push((block, targets));
    }

    /// Emits a push instruction for a deferred constant.
    pub(crate) fn emit_push_deferred(&mut self, id: DeferredConst) {
        // push deferred(id)
        self.push_ir_instruction(ir::Instruction::push_deferred(id));
    }

    /// Sets the value of a deferred constant.
    pub(crate) fn set_deferred_const(&mut self, id: DeferredConst, value: U256) {
        self.deferred_values.insert(id, value);
    }

    /// Emits an allocation whose static or dynamic placement is chosen after
    /// exact backend frame layout is known.
    pub(in crate::backend::evm) fn emit_deferred_alloc(&mut self) -> DeferredAlloc {
        let id = take_next(&mut self.next_deferred_alloc);
        // alloc deferred(id)
        self.push_ir_instruction(ir::Instruction::push_alloc(id));
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

    /// Emits a `PUSH<N>` zero placeholder for the immutable identified by `id`.
    pub(crate) fn emit_push_immutable(&mut self, id: ImmutableId, type_size: TypeSize) {
        self.push_ir_instruction(ir::Instruction::push_immutable(id, type_size));
    }

    /// Defines a label and emits a `JUMPDEST` at the current position.
    pub(crate) fn define_label(&mut self, label: Label) {
        let mut block = ir::Block::new(self.program.blocks.len() as u32);
        if self.cold_labels.contains(label) {
            block.metadata.hotness = ir::Hotness::Cold;
        }
        let block = self.program.add_block(block);
        self.current_block = Some(block);
        self.label_blocks.insert(label, block);
    }

    /// Marks a label-started block as cold for EVM IR layout passes.
    pub(in crate::backend::evm) fn mark_label_cold(&mut self, label: Label) {
        self.cold_labels.insert(label);
        if let Some(&block) = self.label_blocks.get(&label) {
            self.program.blocks[block].metadata.hotness = ir::Hotness::Cold;
        }
    }

    fn current_block(&mut self) -> ir::BlockId {
        if let Some(block) = self.current_block {
            return block;
        }
        let block = self.program.add_block(ir::Block::new(self.program.blocks.len() as u32));
        self.current_block = Some(block);
        block
    }

    fn push_ir_instruction(&mut self, instruction: ir::Instruction) -> (ir::BlockId, usize) {
        let (block, index) = self.next_instruction_position();
        self.program.blocks[block].instructions.push(instruction);
        (block, index)
    }

    /// Returns the block and index the next emitted instruction will take.
    pub(crate) fn next_instruction_position(&mut self) -> (ir::BlockId, usize) {
        let block = self.current_block();
        (block, self.program.blocks[block].instructions.len())
    }

    pub(crate) fn remove_instructions(
        &mut self,
        removals: &mut [(ir::BlockId, std::ops::Range<usize>)],
    ) {
        // instructions[..start], instructions[end..]
        // Descending ranges keep every earlier instruction position valid.
        removals.sort_unstable_by_key(|(block, range)| std::cmp::Reverse((*block, range.start)));
        for (block, range) in removals {
            self.program.blocks[*block].instructions.drain(range.clone());
        }
    }

    fn finish_evm_ir(&mut self) -> ir::Module {
        let mut module = std::mem::take(&mut self.program);
        self.current_block = None;
        for block in &mut module.blocks {
            // push label -> push block
            for inst in &mut block.instructions {
                if let Some(ir::PushValue::Label(label)) = inst.value {
                    let target = self
                        .label_blocks
                        .get(&label)
                        .copied()
                        .unwrap_or_else(|| panic!("label {label:?} was never defined"));
                    *inst = ir::Instruction::push_block(target);
                }
            }
            // alloc static(address) -> push address
            // alloc dynamic(size) -> push 0x40; mload; dup 1; push size; add; push 0x40; mstore
            for index in (0..block.instructions.len()).rev() {
                if let Some(ir::PushValue::Alloc(id)) = block.instructions[index].value {
                    let resolution = self
                        .deferred_allocations
                        .get(&id)
                        .unwrap_or_else(|| panic!("deferred allocation {id:?} was never resolved"));
                    match resolution {
                        DeferredAllocResolution::Static(address) => {
                            block.instructions[index] = ir::Instruction::push_value(*address);
                        }
                        DeferredAllocResolution::Dynamic(size) => {
                            let push_slot = || {
                                ir::Instruction::push_value(U256::from(EvmMemoryLayout::FMP_SLOT))
                            };
                            block.instructions.splice(
                                index..=index,
                                [
                                    push_slot(),
                                    ir::Instruction::opcode(op::MLOAD),
                                    ir::Instruction::stack_op(op::StackOp::Dup(1)),
                                    ir::Instruction::push_value(*size),
                                    ir::Instruction::opcode(op::ADD),
                                    push_slot(),
                                    ir::Instruction::opcode(op::MSTORE),
                                ],
                            );
                        }
                    }
                }
            }
        }
        self.deferred_allocations.clear();
        self.finalize_evm_ir(&mut module);
        self.label_blocks.clear();
        self.cold_labels.clear();
        module
    }

    fn finalize_evm_ir(&mut self, module: &mut ir::Module) {
        for block_id in module.blocks.indices() {
            let next = (block_id.index() + 1 < module.blocks.len())
                .then(|| ir::BlockId::from_usize(block_id.index() + 1));
            let block = &mut module.blocks[block_id];
            let (terminator, remove) = if let [.., push, jump] = block.instructions.as_slice()
                && !jump.is_encoded_push()
                && jump.opcode == op::JUMP
                && let Some(target) = push.pushed_block()
            {
                (ir::Terminator::new(ir::TerminatorKind::Jump(target)), 2)
            } else if let Some(last) = block.instructions.last()
                && !last.is_encoded_push()
                && op::is_terminal(last.opcode)
            {
                (ir::Terminator::new(ir::TerminatorKind::Op(last.opcode)), 1)
            } else {
                (
                    next.map_or_else(ir::Terminator::implicit_stop, |target| {
                        ir::Terminator::new(ir::TerminatorKind::Jump(target))
                    }),
                    0,
                )
            };
            block.instructions.truncate(block.instructions.len() - remove);
            block.terminator = Some(terminator);
        }

        for (block, targets) in self.indexed_jumps.drain(..) {
            let targets = targets
                .into_iter()
                .map(|label| {
                    self.label_blocks
                        .get(&label)
                        .copied()
                        .unwrap_or_else(|| panic!("label {label:?} was never defined"))
                })
                .collect::<Vec<_>>()
                .into_boxed_slice();
            module.blocks[block].terminator =
                Some(ir::Terminator::new(ir::TerminatorKind::IndexedJump(targets)));
        }
    }
}

pub(in crate::backend::evm) fn resolve_known_deferred_constants(
    module: &mut ir::Module,
    values: &FxHashMap<DeferredConst, U256>,
) {
    for block in &mut module.blocks {
        for inst in &mut block.instructions {
            // push deferred(id) -> push value
            if let Some(id) = inst.deferred_push()
                && let Some(&value) = values.get(&id)
            {
                *inst = ir::Instruction::push_value(value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_interface::Session;
    use solar_sema::Compiler;

    #[test]
    fn function_edges_exclude_references_from_other_functions() {
        let compiler = Compiler::new(Session::builder().opts(Default::default()).build());
        compiler.enter(|c| {
            let mut builder = Builder::new(c.gcx());
            let outside = builder.new_label();
            let entry = builder.new_label();
            let local = builder.new_label();
            let indirect = builder.new_label();
            // outside: push local
            // entry: push entry; fall through to local
            // local: stop
            // indirect: jump
            builder.define_label(outside);
            builder.emit_push_label(local);
            builder.define_label(entry);
            builder.emit_push_label(entry);
            builder.define_label(local);
            builder.emit_op(op::STOP);
            builder.define_label(indirect);
            builder.emit_op(op::JUMP);

            let entry = ir::BlockId::from_usize(1);
            let local = ir::BlockId::from_usize(2);
            let indirect = ir::BlockId::from_usize(3);
            // An outside address reference must not add a local indirect-jump destination.
            assert_eq!(
                builder.dataflow_edges(1..4),
                [(entry, entry), (entry, local), (indirect, entry)],
            );
        });
    }

    #[test]
    fn removing_instructions_keeps_symbolic_operands() {
        let compiler = Compiler::new(Session::builder().opts(Default::default()).build());
        compiler.enter(|c| {
            let mut builder = Builder::new(c.gcx());
            let label = builder.new_label();
            let (block, start) = builder.next_instruction_position();
            builder.emit_op(op::ADD);
            builder.emit_push_label(label);
            builder.remove_instructions(&mut [(block, start..start + 1)]);
            builder.define_label(label);

            let module = builder.finish_evm_ir();
            assert_eq!(
                module.blocks[ir::BlockId::ENTRY].instructions[0].pushed_block(),
                Some(ir::BlockId::from_usize(1))
            );
        });
    }
}

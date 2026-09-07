//! EVM IR construction through the backend assembler interface.

use super::{self as ir};
use crate::{
    backend::evm::{
        DebugFunction, DebugFunctionExit,
        assembler::{ArtifactKind, Assembler, DeferredAllocResolution, DeferredConst, Label},
        ir::assembly::DeferredAlloc,
        op::{self, push_len},
    },
    memory::EvmMemoryLayout,
    mir::{DataRef as MirDataRef, ImmutableId, Module as MirModule, TypeSize},
};
use alloy_primitives::U256;
use solar_data_structures::{index::index_vec, map::FxHashMap};
use solar_sema::Gcx;

impl<'gcx> Assembler<'gcx> {
    /// Creates an assembler with finalized EVM IR loaded into the ordinary backend pipeline.
    pub(in crate::backend::evm) fn from_evm_ir(
        gcx: Gcx<'gcx>,
        mut module: ir::Module,
    ) -> solar_interface::Result<Self> {
        if module
            .blocks
            .iter()
            .any(|block| block.instructions.iter().any(|inst| inst.deferred_push().is_some()))
        {
            return Err(gcx
                .dcx()
                .err("cannot assemble unresolved `push_deferred` instruction")
                .emit());
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

        // Parsed block labels may be sparse, but assembly indexes labels with a vector.
        for (index, block) in module.blocks.iter_mut().enumerate() {
            block.label = u32::try_from(index).expect("EVM IR block index should fit in u32");
        }
        let block_labels = vec![None; module.blocks.len()];
        Ok(Self { program: module, program_is_finalized: true, block_labels, ..Self::new(gcx) })
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

    /// Requires the last emitted instruction to stay immediately before the next one.
    ///
    /// See [`ir::Metadata::keep_with_next`].
    pub(crate) fn keep_last_with_next(&mut self) {
        let block = self.current_block();
        self.program.blocks[block]
            .instructions
            .last_mut()
            .expect("`keep_last_with_next` needs an emitted instruction")
            .keep_with_next();
    }

    /// Emits a logical stack operation.
    pub(crate) fn emit_stack_op(&mut self, stack_op: op::StackOp) {
        self.push_ir_instruction(ir::Instruction::stack_op(stack_op));
    }

    /// Sets the source span attached to subsequently emitted operations.
    pub(crate) fn set_source_span(&mut self, span: Option<solar_interface::Span>) {
        self.set_source_spans(span);
    }

    /// Sets all retained source origins attached to subsequently emitted operations.
    pub(crate) fn set_source_spans(
        &mut self,
        spans: impl IntoIterator<Item = solar_interface::Span>,
    ) {
        self.program.track_debug_info();
        self.current_source_spans.clear();
        self.current_source_spans.extend(spans.into_iter().filter(|span| !span.is_dummy()));
    }

    /// Sets the legacy source-map modifier nesting depth attached to new operations.
    pub(crate) fn set_modifier_depth(&mut self, depth: u32) {
        self.program.track_debug_info();
        self.current_modifier_depth = depth;
    }

    /// Marks the current block as entering a source-language function.
    pub(crate) fn mark_function_invoke(&mut self, function: DebugFunction) {
        self.program.track_debug_info();
        let block = self.current_block();
        self.program.blocks[block].metadata.function_invoke = Some(function);
    }

    /// Marks the most recently emitted operation as closing a function activation.
    pub(crate) fn mark_function_exit(&mut self, exit: DebugFunctionExit) {
        self.program.track_debug_info();
        let block = self.current_block.expect("function exit requires an emitted operation");
        let instruction = self.program.blocks[block]
            .instructions
            .last_mut()
            .expect("function exit requires an emitted operation");
        instruction.metadata.set_function_exit(exit);
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
        for &(_, _, label) in &self.label_relocations {
            if let Some(&target) = self.label_blocks.get(&label) {
                references[target] += 1;
            }
        }
        for (_, targets, _) in &self.indexed_jump_relocations {
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
            for (index, inst) in instructions[..end].iter().enumerate() {
                let (min_size, layout_size) = self.instruction_size_bounds(
                    block,
                    index,
                    inst,
                    block_target_width,
                    deferred_value_width,
                );
                bounds.0 += min_size;
                bounds.1 += layout_size;
            }
        }
        Some(bounds)
    }

    fn trace_successor(&self, block: ir::BlockId) -> Option<ir::BlockId> {
        if self.indexed_jump_relocations.iter().any(|&(source, _, _)| source == block) {
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
        for &(source, _, label) in &self.label_relocations {
            if let Some(&target) = self.label_blocks.get(&label) {
                push_edge(&mut edges, source, target);
            }
        }
        for (source, targets, _) in &self.indexed_jump_relocations {
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
        if !push.is_encoded_push() || jump.is_encoded_push() || jump.opcode != op::JUMP {
            return None;
        }
        let instruction = instructions.len() - 2;
        let label = self.label_relocations.iter().find_map(|&(source, index, label)| {
            (source == block && index == instruction).then_some(label)
        })?;
        self.label_blocks.get(&label).copied()
    }

    fn block_has_explicit_terminator(&self, block: ir::BlockId) -> bool {
        self.indexed_jump_relocations.iter().any(|&(source, _, _)| source == block)
            || self.program.blocks[block]
                .instructions
                .last()
                .is_some_and(|inst| !inst.is_encoded_push() && op::is_terminal(inst.opcode))
    }

    fn instruction_size_bounds(
        &self,
        block: ir::BlockId,
        instruction: usize,
        inst: &ir::Instruction,
        block_target_width: usize,
        deferred_value_width: usize,
    ) -> (usize, usize) {
        if let Some(type_size) = inst.immutable_type_size() {
            let size = usize::from(type_size.bytes()) + 1;
            (size, size)
        } else if !inst.is_encoded_push() {
            let size = inst.as_stack_op().map_or(1, |stack_op| {
                stack_op
                    .assembled_len(self.gcx.sess.opts.evm_version)
                    .expect("stack operation must support the target EVM version")
            });
            (size, size)
        } else if let Some(value) = inst.pushed_value() {
            let size = push_len(self.gcx.sess.opts.evm_version, value);
            (size, size)
        } else if self
            .label_relocations
            .iter()
            .any(|&(source, index, _)| source == block && index == instruction)
        {
            (2, block_target_width + 1)
        } else if let Some(id) =
            self.deferred_relocations.iter().find_map(|&(source, index, id)| {
                (source == block && index == instruction).then_some(id)
            })
        {
            self.deferred_values.get(&id).map_or((1, deferred_value_width + 1), |&value| {
                let size = push_len(self.gcx.sess.opts.evm_version, value);
                (size, size)
            })
        } else if let Some(id) = self.alloc_relocations.iter().find_map(|&(source, index, id)| {
            (source == block && index == instruction).then_some(id)
        }) {
            let slot_size =
                push_len(self.gcx.sess.opts.evm_version, U256::from(EvmMemoryLayout::FMP_SLOT));
            self.deferred_allocations.get(&id).map_or((1, slot_size * 2 + 33 + 4), |resolution| {
                let size = match resolution {
                    DeferredAllocResolution::Static(address) => {
                        push_len(self.gcx.sess.opts.evm_version, *address)
                    }
                    DeferredAllocResolution::Dynamic(size) => {
                        slot_size * 2 + push_len(self.gcx.sess.opts.evm_version, *size) + 4
                    }
                };
                (size, size)
            })
        } else {
            (1, 33)
        }
    }

    /// Emits a push instruction that will be resolved to a label's offset.
    pub(crate) fn emit_push_label(&mut self, label: Label) {
        let (block, instruction) = self.push_ir_instruction(ir::Instruction::push_relocation());
        self.label_relocations.push((block, instruction, label));
    }

    /// Terminates the current block with an indexed jump to one of `targets`.
    pub(crate) fn emit_indexed_jump(&mut self, targets: Vec<Label>) {
        assert!(!targets.is_empty(), "indexed jump must have at least one target");
        let block = self.current_block.take().expect("indexed jump requires a current block");
        let mut metadata = ir::Metadata::default();
        metadata.set_source_spans(self.current_source_spans.iter().copied());
        metadata.set_modifier_depth(self.current_modifier_depth);
        self.indexed_jump_relocations.push((block, targets, metadata));
    }

    /// Emits a push instruction for a deferred constant.
    pub(crate) fn emit_push_deferred(&mut self, id: DeferredConst) {
        let (block, instruction) = self.push_ir_instruction(ir::Instruction::push_relocation());
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
        let (block, instruction) = self.push_ir_instruction(ir::Instruction::push_relocation());
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
        if self.loop_labels.contains(label) {
            block.metadata.in_loop = true;
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

    fn current_block(&mut self) -> ir::BlockId {
        if let Some(block) = self.current_block {
            return block;
        }
        let block = self.program.add_block(ir::Block::new(self.program.blocks.len() as u32));
        self.current_block = Some(block);
        self.block_labels.push(None);
        block
    }

    fn push_ir_instruction(&mut self, mut instruction: ir::Instruction) -> (ir::BlockId, usize) {
        instruction.metadata.set_source_spans(self.current_source_spans.iter().copied());
        instruction.metadata.set_modifier_depth(self.current_modifier_depth);
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
        removals.sort_unstable_by_key(|(block, range)| (*block, range.start));
        let mut per_block =
            FxHashMap::<ir::BlockId, Vec<(std::ops::Range<usize>, usize)>>::default();
        for (block, range) in removals.iter() {
            let ranges = per_block.entry(*block).or_default();
            let before = ranges.last().map_or(0, |(range, before)| before + range.len());
            ranges.push((range.clone(), before));
        }
        fn shift<T>(
            relocations: &mut Vec<(ir::BlockId, usize, T)>,
            ranges: &FxHashMap<ir::BlockId, Vec<(std::ops::Range<usize>, usize)>>,
        ) {
            relocations.retain_mut(|(block, index, _)| {
                let Some(ranges) = ranges.get(block) else { return true };
                let position = ranges.partition_point(|(range, _)| range.start <= *index);
                let Some((range, before)) = position.checked_sub(1).map(|index| &ranges[index])
                else {
                    return true;
                };
                if range.contains(index) {
                    return false;
                }
                *index -= before + range.len();
                true
            });
        }
        shift(&mut self.label_relocations, &per_block);
        shift(&mut self.deferred_relocations, &per_block);
        shift(&mut self.alloc_relocations, &per_block);
        for (block, ranges) in per_block {
            let instructions = &mut self.program.blocks[block].instructions;
            for (range, _) in ranges.into_iter().rev() {
                instructions.drain(range);
            }
        }
    }

    pub(in crate::backend::evm) fn finish_evm_ir(
        &mut self,
    ) -> Option<(ir::Module, Vec<Option<Label>>)> {
        let mut module = std::mem::take(&mut self.program);
        self.current_block = None;
        if module.blocks.is_empty() {
            module.clear();
            self.program = module;
            return None;
        }

        for (block, instruction, label) in self.label_relocations.drain(..) {
            let target = self
                .label_blocks
                .get(&label)
                .copied()
                .unwrap_or_else(|| panic!("label {label:?} was never defined"));
            let replacement = ir::Instruction::push_block(target);
            module.blocks[block].instructions[instruction].replace_preserving_metadata(replacement);
        }
        for (block, instruction, id) in self.deferred_relocations.drain(..) {
            let replacement = ir::Instruction::push_deferred(id);
            module.blocks[block].instructions[instruction].replace_preserving_metadata(replacement);
        }
        // Allocation placeholders expand to more than one instruction, so they
        // splice after every in-place relocation patch above. Descending
        // instruction order keeps earlier indices in the same block valid.
        let mut alloc_relocations = std::mem::take(&mut self.alloc_relocations);
        alloc_relocations.sort_unstable_by_key(|&(block, instruction, _)| {
            std::cmp::Reverse((block, instruction))
        });
        for (block, instruction, id) in alloc_relocations.drain(..) {
            let metadata = module.blocks[block].instructions[instruction].metadata.clone();
            let resolution = self
                .deferred_allocations
                .get(&id)
                .copied()
                .unwrap_or_else(|| panic!("deferred allocation {id:?} was never resolved"));
            let push = |value: U256| ir::Instruction::push_value(value);
            let mut replacement = match resolution {
                DeferredAllocResolution::Static(address) => vec![push(address)],
                DeferredAllocResolution::Dynamic(size) => vec![
                    push(U256::from(EvmMemoryLayout::FMP_SLOT)),
                    ir::Instruction::opcode(op::MLOAD),
                    ir::Instruction::stack_op(op::StackOp::Dup(1)),
                    push(size),
                    ir::Instruction::opcode(op::ADD),
                    push(U256::from(EvmMemoryLayout::FMP_SLOT)),
                    ir::Instruction::opcode(op::MSTORE),
                ],
            };
            for inst in &mut replacement {
                inst.metadata.copy_source_debug_from(&metadata);
            }
            if let Some(function) = metadata.function_invoke()
                && let Some(inst) = replacement.last_mut()
            {
                inst.metadata.set_function_invoke(function);
            }
            if let Some(exit) = metadata.function_exit()
                && let Some(inst) = replacement.last_mut()
            {
                inst.metadata.set_function_exit(exit);
            }
            module.blocks[block].instructions.splice(instruction..=instruction, replacement);
        }
        self.deferred_allocations.clear();
        self.alloc_relocations = alloc_relocations;

        if self.program_is_finalized {
            self.program_is_finalized = false;
            debug_assert!(self.indexed_jump_relocations.is_empty());
        } else {
            self.finalize_evm_ir(&mut module);
        }

        self.label_blocks.clear();
        self.cold_labels.clear();
        self.loop_labels.clear();

        Some((module, std::mem::take(&mut self.block_labels)))
    }

    fn finalize_evm_ir(&mut self, module: &mut ir::Module) {
        for block_id in module.blocks.indices() {
            let next = (block_id.index() + 1 < module.blocks.len())
                .then(|| ir::BlockId::from_usize(block_id.index() + 1));
            let block = &mut module.blocks[block_id];
            let (mut terminator, remove, metadata) = if let [.., push, jump] =
                block.instructions.as_slice()
                && !jump.is_encoded_push()
                && jump.opcode == op::JUMP
                && let Some(target) = push.pushed_block()
                && push.is_encoded_push()
            {
                (ir::Terminator::new(ir::TerminatorKind::Jump(target)), 2, jump.metadata.clone())
            } else if let Some(last) = block.instructions.last()
                && !last.is_encoded_push()
                && last.opcode == op::STOP
            {
                (ir::Terminator::new(ir::TerminatorKind::Op(op::STOP)), 1, last.metadata.clone())
            } else if let Some(last) = block.instructions.last()
                && !last.is_encoded_push()
                && op::is_terminal(last.opcode)
            {
                (ir::Terminator::new(ir::TerminatorKind::Op(last.opcode)), 1, last.metadata.clone())
            } else {
                (
                    next.map_or_else(ir::Terminator::implicit_stop, |target| {
                        ir::Terminator::new(ir::TerminatorKind::Jump(target))
                    }),
                    0,
                    ir::Metadata::default(),
                )
            };
            block.instructions.truncate(block.instructions.len() - remove);
            terminator.metadata.copy_source_debug_from(&metadata);
            if let Some(function) = metadata.function_invoke() {
                terminator.metadata.set_function_invoke(function);
            }
            if let Some(exit) = metadata.function_exit() {
                terminator.metadata.set_function_exit(exit);
            }
            block.terminator = Some(terminator);
        }

        for (block, targets, metadata) in self.indexed_jump_relocations.drain(..) {
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
            let mut terminator = ir::Terminator::new(ir::TerminatorKind::IndexedJump(targets));
            terminator.metadata = metadata;
            module.blocks[block].terminator = Some(terminator);
        }
    }
}

pub(in crate::backend::evm) fn resolve_known_deferred_constants(
    module: &mut ir::Module,
    values: &FxHashMap<DeferredConst, U256>,
) {
    for block in &mut module.blocks {
        for inst in &mut block.instructions {
            let Some(id) = inst.deferred_push() else { continue };
            if let Some(&value) = values.get(&id) {
                inst.replace_preserving_metadata(ir::Instruction::push_value(value));
            }
        }
    }
}

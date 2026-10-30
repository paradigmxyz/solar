//! MIR functions.

use super::{
    AbiLayoutRef, AbiParamLayout, AbiParamLocation, ArgIdx, BasicBlock, BlockId, Immediate, InstId,
    InstKind, Instruction, MangledSymbol, MirType, PtrLocation, StorageAlias, Value, ValueId,
    memory::EvmMemoryLayout, utils,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_ast::{StateMutability, Visibility};
use solar_config::OptimizationMode;
use solar_data_structures::{
    bit_set::DenseBitSet,
    fmt::FmtIteratorExt,
    index::IndexVec,
    map::{FxHashMap, StdEntry},
};
use solar_interface::{Ident, Span, Symbol};
use solar_ir::Hotness;
use std::fmt;

/// A function in the MIR.
#[derive(Clone, Debug)]
pub struct Function {
    /// Function name used by codegen.
    pub name: MangledSymbol,
    /// Source span of the function name.
    pub name_span: Span,
    /// Source span of the complete function declaration.
    pub declaration_span: Span,
    /// Source-language identifier retained independently of the MIR symbol.
    pub debug_identifier: Option<Symbol>,
    /// Function selector (4 bytes, for external functions).
    pub selector: Option<[u8; 4]>,
    /// Function attributes.
    pub attributes: FunctionAttributes,
    /// Parameter types.
    pub params: IndexVec<ArgIdx, MirType>,
    /// The function's single logical result, or `void`.
    return_type: MirType,
    /// Components selected by internal ABI lowering. The logical result stays intact.
    return_abi: Option<Box<[MirType]>>,
    /// ABI layout of values returned by an external entry before `lower-abi`
    /// materializes returndata encoding.
    pub abi_returns: Option<AbiLayoutRef>,
    /// ABI return shape with scalar type information retained until `lower-abi`.
    pub abi_return_params: Option<AbiParamLayout>,
    /// ABI input layout retained until `lower-abi` materializes aggregate parameters.
    pub abi_params: Option<AbiParamLayout>,
    /// Source locations for the ABI parameters retained until `lower-abi`.
    pub abi_param_locations: Option<Box<[AbiParamLocation]>>,
    /// Bytes reserved for lowered local memory slots.
    ///
    /// Internal-call functions place these in the internal frame; external entries
    /// reserve the same space in their low-memory scratch layout.
    pub internal_frame_size: u64,
    /// Bytes reserved for the low-memory external ABI return buffer.
    pub external_static_return_size: u64,
    /// All values allocated in this function.
    ///
    /// Values remain allocated after their uses or defining instruction are removed, so this is
    /// not an active value list. Use [`Self::value`] or [`Self::value_mut`] for ID-based access
    /// and [`Self::num_values`] for the allocated ID-domain size.
    values: IndexVec<ValueId, Value>,
    /// Types of all argument slots.
    ///
    /// This remains populated when progressive lowering clears the callable parameter list.
    arg_types: IndexVec<ArgIdx, MirType>,
    /// All instructions allocated in this function.
    ///
    /// Instructions remain allocated after removal from their block, so this is not the active
    /// instruction list. Use [`Self::instructions`] to iterate active instructions and
    /// [`Self::inst`] or [`Self::inst_mut`] for ID-based access.
    instructions: IndexVec<InstId, Instruction>,
    /// All basic blocks in this function. This is never empty; block zero is the entry.
    pub blocks: IndexVec<BlockId, BasicBlock>,
}

impl Function {
    /// Creates a new function.
    #[must_use]
    pub fn new(name: Ident) -> Self {
        let mut blocks = IndexVec::new();
        let entry = blocks.push(BasicBlock::new());
        debug_assert_eq!(entry, BlockId::ENTRY);

        Self {
            name: MangledSymbol::new(name.name),
            name_span: name.span,
            declaration_span: name.span,
            debug_identifier: None,
            selector: None,
            attributes: FunctionAttributes::default(),
            params: IndexVec::new(),
            return_type: MirType::Void,
            return_abi: None,
            abi_returns: None,
            abi_return_params: None,
            abi_params: None,
            abi_param_locations: None,
            internal_frame_size: 0,
            external_static_return_size: 0,
            values: IndexVec::new(),
            arg_types: IndexVec::new(),
            instructions: IndexVec::new(),
            blocks,
        }
    }

    /// Returns the function's single logical result type.
    pub fn return_type(&self) -> MirType {
        self.return_type
    }

    /// Sets the single logical result before selecting its internal ABI.
    pub fn set_return_type(&mut self, ty: MirType) {
        self.return_type = ty;
        self.return_abi = None;
    }

    /// Selects the components delivered by the internal calling convention.
    pub fn set_return_abi(&mut self, components: impl Into<Box<[MirType]>>) {
        let components = components.into();
        self.return_abi = if (self.return_type == MirType::Void && components.is_empty())
            || (self.return_type != MirType::Void && components.as_ref() == [self.return_type])
        {
            None
        } else {
            Some(components)
        };
    }

    /// Returns the selected internal ABI, if lowering has materialized it.
    pub fn return_abi(&self) -> Option<&[MirType]> {
        self.return_abi.as_deref()
    }

    /// Returns the logical value before ABI lowering, then its physical components.
    pub fn return_components(&self) -> &[MirType] {
        self.return_abi.as_deref().unwrap_or_else(|| {
            if self.return_type == MirType::Void {
                &[]
            } else {
                std::slice::from_ref(&self.return_type)
            }
        })
    }

    /// Returns whether this function is an external ABI entry.
    pub fn is_external_entry(&self) -> bool {
        self.selector.is_some()
            || self.attributes.is_constructor
            || self.attributes.is_receive
            || self.attributes.is_fallback
    }

    /// Returns whether the runtime dispatches to this function: an external entry other than the
    /// constructor.
    pub fn is_runtime_entry(&self) -> bool {
        self.is_external_entry() && !self.attributes.is_constructor
    }

    /// Returns the value for the given ID.
    #[must_use]
    pub fn value(&self, id: ValueId) -> &Value {
        &self.values[id]
    }

    /// Returns a mutable reference to the value for the given ID.
    #[must_use]
    pub fn value_mut(&mut self, id: ValueId) -> &mut Value {
        &mut self.values[id]
    }

    /// Returns the size of the allocated value ID domain.
    #[must_use]
    pub fn num_values(&self) -> usize {
        self.values.len()
    }

    /// Returns the type of a value, if it has one.
    #[must_use]
    pub fn value_ty(&self, id: ValueId) -> Option<MirType> {
        match self.value(id) {
            Value::Inst(inst) => self.inst(*inst).result_ty,
            Value::Arg(index) => Some(self.arg_ty(*index)),
            Value::Immediate(imm) => Some(imm.ty()),
            Value::Undef(ty) => Some(*ty),
            Value::Error(_) => None,
        }
    }

    /// Returns the kind of the instruction that defines a value, if an instruction does.
    #[must_use]
    pub fn value_inst_kind(&self, id: ValueId) -> Option<&InstKind> {
        match self.value(id) {
            Value::Inst(inst) => Some(&self.inst(*inst).kind),
            _ => None,
        }
    }

    /// Returns the slice location of a value, if it is slice-typed.
    #[must_use]
    pub fn value_slice_location(&self, id: ValueId) -> Option<PtrLocation> {
        match self.value_ty(id) {
            Some(MirType::Slice(location)) => Some(location),
            _ => None,
        }
    }

    /// Whether `value` is a word loaded from a memory object, which may hold a memory reference.
    #[must_use]
    pub fn memory_object_load(&self, value: ValueId) -> bool {
        matches!(self.value(value), Value::Inst(inst) if matches!(
            self.inst(*inst).kind,
            InstKind::Load(address, PtrLocation::Memory)
                if self.object_address(address).is_some_and(|(object, _)| {
                    self.value_ty(object) == Some(MirType::MEMPTR)
                })
        ))
    }

    /// Returns the element index of a word offset `shl index, 5`.
    #[must_use]
    pub fn word_index(&self, offset: ValueId) -> Option<ValueId> {
        let Value::Inst(inst) = self.value(offset) else { return None };
        match self.inst(*inst).kind {
            InstKind::Shl(shift, index)
                if self.value_u64(shift)
                    == Some(u64::from(
                        super::memory::EvmMemoryLayout::WORD_SIZE.trailing_zeros(),
                    )) =>
            {
                Some(index)
            }
            _ => None,
        }
    }

    /// Returns the object and length of a store to an object's length word:
    /// `mstore object, len`, also through a pointer cast.
    #[must_use]
    pub fn memory_length_store(&self, inst: InstId) -> Option<(ValueId, ValueId)> {
        let InstKind::Store(address, len, PtrLocation::Memory) = self.inst(inst).kind else {
            return None;
        };
        match self.object_address(address) {
            Some((object, 0)) if self.value_ty(object) == Some(MirType::MEMPTR) => {
                Some((object, len))
            }
            _ => None,
        }
    }

    /// Resolves an address to the object it points into and its byte offset from the object
    /// start, through pointer casts, constant additions, and memory slices of the object.
    #[must_use]
    pub fn object_address(&self, mut value: ValueId) -> Option<(ValueId, u64)> {
        let mut offset = 0_u64;
        for _ in 0..self.num_values() {
            let Value::Inst(inst) = self.value(value) else { break };
            value = match self.inst(*inst).kind {
                InstKind::PtrToInt(base, 256)
                | InstKind::IntToPtr(base)
                | InstKind::ExtractValue {
                    ty: MirType::Slice(_),
                    aggregate: base,
                    index: MirType::SLICE_PTR_FIELD,
                }
                | InstKind::MakeSlice { ptr: base, location: PtrLocation::Memory, .. } => base,
                InstKind::Add(base, delta) | InstKind::PtrAdd(base, delta)
                    if let Some(delta) = self.value_u64(delta) =>
                {
                    offset = offset.checked_add(delta)?;
                    base
                }
                _ => break,
            };
        }
        Some((value, offset))
    }

    /// Returns the constant byte size of the allocation that defines `value`.
    #[must_use]
    pub fn alloc_size(&self, value: ValueId) -> Option<u64> {
        let Value::Inst(inst) = self.value(value) else { return None };
        let InstKind::Alloc { size, .. } = self.inst(*inst).kind else { return None };
        self.value_u64(size)
    }

    /// Resolves the address of a runtime-indexed word, `ptradd base, shl index, 5` with constant
    /// offsets around it, to the object it points into, the byte offset of word zero from the
    /// object start, and the index.
    #[must_use]
    pub fn element_address(&self, address: ValueId) -> Option<(ValueId, u64, ValueId)> {
        let (address, outer) = self.object_address(address)?;
        let Value::Inst(inst) = self.value(address) else { return None };
        let InstKind::PtrAdd(base, offset) = self.inst(*inst).kind else { return None };
        let index = self.word_index(offset)?;
        let (object, offset) = self.object_address(base)?;
        Some((object, offset.checked_add(outer)?, index))
    }

    /// Resolves the address of a word access to the object it reaches, the word-aligned byte offset
    /// of the word (of word zero for a runtime index), and the index, if any.
    #[must_use]
    pub fn word_access(&self, address: ValueId) -> Option<(ValueId, u64, Option<ValueId>)> {
        let (object, offset, index) = match self.element_address(address) {
            Some((object, offset, index)) => (object, offset, Some(index)),
            None => {
                let (object, offset) = self.object_address(address)?;
                (object, offset, None)
            }
        };
        super::memory::EvmMemoryLayout::word_slot(offset).map(|_| (object, offset, index))
    }

    /// Returns whether an instruction only derives an address from another one, as
    /// [`Self::object_address`] follows.
    #[must_use]
    pub fn is_object_derivation(&self, inst: InstId) -> bool {
        match self.inst(inst).kind {
            InstKind::PtrToInt(_, 256)
            | InstKind::IntToPtr(_)
            | InstKind::ExtractValue {
                ty: MirType::Slice(_),
                index: MirType::SLICE_PTR_FIELD,
                ..
            }
            | InstKind::MakeSlice { location: PtrLocation::Memory, .. } => true,
            InstKind::Add(_, delta) | InstKind::PtrAdd(_, delta) => self.value_u64(delta).is_some(),
            _ => false,
        }
    }

    /// Returns the type of an argument.
    #[must_use]
    pub fn arg_ty(&self, index: ArgIdx) -> MirType {
        self.arg_types[index]
    }

    /// Updates an argument's retained type and its callable parameter type, if present.
    pub fn set_arg_ty(&mut self, index: ArgIdx, ty: MirType) {
        self.arg_types[index] = ty;
        if let Some(param) = self.params.get_mut(index) {
            *param = ty;
        }
    }

    /// Returns all argument indexes.
    pub fn arg_indices(&self) -> impl Iterator<Item = ArgIdx> + use<> {
        self.arg_types.indices()
    }

    /// Returns active argument uses grouped by argument index.
    ///
    /// Each operand occurrence is retained, including operands in terminators.
    #[must_use]
    pub fn arg_uses(&self) -> IndexVec<ArgIdx, Vec<ValueId>> {
        let mut uses = IndexVec::with_capacity(self.arg_types.len());
        for _ in self.arg_types.indices() {
            uses.push(Vec::new());
        }
        for value in self.live_values() {
            if let Value::Arg(index) = self.value(value) {
                uses[*index].push(value);
            }
        }
        uses
    }

    /// Returns an immediate value as U256.
    #[must_use]
    pub fn value_u256(&self, id: ValueId) -> Option<U256> {
        self.value(id).as_immediate()?.as_u256()
    }

    /// Returns an immediate value as u64 when lossless.
    #[must_use]
    pub fn value_u64(&self, id: ValueId) -> Option<u64> {
        self.value_u256(id).and_then(super::utils::u256_to_u64)
    }

    /// Returns whether the instruction stores a full word to the free-memory-pointer slot.
    #[must_use]
    pub fn is_fmp_slot_store(&self, id: InstId) -> bool {
        matches!(
            self.inst(id).kind,
            InstKind::Store(ptr, _, PtrLocation::Memory)
                if self.value_u64(ptr) == Some(EvmMemoryLayout::FMP_SLOT)
        )
    }

    /// Returns whether the block stores a full word to the free-memory-pointer slot.
    #[must_use]
    pub fn block_stores_fmp_slot(&self, block: &BasicBlock) -> bool {
        block.instructions.iter().any(|&id| self.is_fmp_slot_store(id))
    }

    /// Returns a possibly replaced immediate value as U256.
    #[must_use]
    pub fn value_u256_after_replacements(
        &self,
        id: ValueId,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> Option<U256> {
        self.value_u256(super::utils::resolve_replacement(id, replacements))
    }

    /// Returns the instruction for the given ID.
    #[must_use]
    pub fn inst(&self, id: InstId) -> &Instruction {
        &self.instructions[id]
    }

    /// Returns a mutable reference to the instruction for the given ID.
    #[must_use]
    pub fn inst_mut(&mut self, id: InstId) -> &mut Instruction {
        &mut self.instructions[id]
    }

    /// Returns the size of the allocated instruction ID domain.
    #[must_use]
    pub fn num_insts(&self) -> usize {
        self.instructions.len()
    }

    /// Returns the IDs of all active instructions in block order.
    pub fn instructions(&self) -> impl Iterator<Item = InstId> + '_ {
        self.blocks.iter().flat_map(|block| block.instructions.iter().copied())
    }

    /// Returns all values in active MIR block order.
    ///
    /// Each instruction yields its operands followed by its result, if any. Terminator operands
    /// follow the instructions in their block. A value is yielded once for every occurrence.
    pub fn live_values(&self) -> impl Iterator<Item = ValueId> + '_ {
        LiveValues { func: self, block: BlockId::ENTRY, inst: 0, values: SmallVec::new(), next: 0 }
    }

    /// Reuses one value identity for active uses of each exactly equal immediate.
    ///
    /// Codegen calls this after the canonical pass pipeline and final phase check. Keep this
    /// limited to replacing immediate uses with an equal immediate unless the caller adds
    /// another validation boundary.
    pub fn canonicalize_immediate_uses(&mut self) -> usize {
        let mut canonical = FxHashMap::<Immediate, ValueId>::default();
        let mut replacements = FxHashMap::default();
        for value in self.live_values() {
            let Value::Immediate(immediate) = self.value(value) else { continue };
            match canonical.entry(immediate.clone()) {
                StdEntry::Occupied(entry) => {
                    let existing = *entry.get();
                    if existing != value {
                        replacements.insert(value, existing);
                    }
                }
                StdEntry::Vacant(entry) => {
                    entry.insert(value);
                }
            }
        }
        if replacements.is_empty() {
            return 0;
        }

        let mut replaced = 0;
        self.for_each_instruction_mut(|_, inst| {
            replaced += utils::replace_inst_uses(inst, &replacements);
        });
        for block in &mut self.blocks {
            if let Some(term) = &mut block.terminator {
                replaced += utils::replace_terminator_uses(term, &replacements);
            }
        }
        replaced
    }

    /// Reuses one value identity for active uses of each function argument.
    ///
    /// Arguments are immutable for the duration of a MIR function. Keeping a
    /// canonical identity lets the backend recognize that independently
    /// lowered operand occurrences denote the same physical word, which is
    /// required for carrying an argument through stack layouts without first
    /// materializing a memory home.
    pub fn canonicalize_argument_uses(&mut self) -> usize {
        if self.arg_types.is_empty() {
            return 0;
        }

        let mut canonical =
            IndexVec::<ArgIdx, Option<ValueId>>::with_capacity(self.arg_types.len());
        for _ in self.arg_types.indices() {
            canonical.push(None);
        }

        let values = &self.values;
        let instructions = &mut self.instructions;
        let mut replaced = 0;
        let mut canonicalize = |value: &mut ValueId| {
            let Value::Arg(index) = values[*value] else { return };
            if let Some(existing) = canonical[index] {
                if existing != *value {
                    *value = existing;
                    replaced += 1;
                }
            } else {
                canonical[index] = Some(*value);
            }
        };
        for block in &mut self.blocks {
            for &inst_id in &block.instructions {
                instructions[inst_id].rewrite_operands(&mut canonicalize);
            }
            if let Some(term) = &mut block.terminator {
                term.visit_operands_mut(&mut canonicalize);
            }
        }
        replaced
    }

    /// Calls `f` for every active instruction in block order.
    pub fn for_each_instruction_mut(&mut self, mut f: impl FnMut(InstId, &mut Instruction)) {
        let blocks = &self.blocks;
        let instructions = &mut self.instructions;
        for block in blocks {
            for &inst_id in &block.instructions {
                f(inst_id, &mut instructions[inst_id]);
            }
        }
    }

    /// Returns an instruction's position among allocated value-producing instructions.
    #[must_use]
    pub(crate) fn inst_result_index(&self, id: InstId) -> Option<usize> {
        self.instructions
            .iter_enumerated()
            .filter(|(_, inst)| inst.result_ty.is_some())
            .position(|(inst_id, _)| inst_id == id)
    }

    /// Returns the value produced by the given instruction, if it has one.
    #[must_use]
    pub fn inst_result_value(&self, id: InstId) -> Option<ValueId> {
        self.instructions[id].result()
    }

    /// Removes an unused instruction result while keeping the instruction and its effects.
    ///
    /// The allocated value remains in the arena as an undefined value because IDs are stable for
    /// the lifetime of a function. Callers must prove that the result has no remaining active uses.
    pub fn remove_inst_result(&mut self, id: InstId) -> Option<ValueId> {
        let ty = self.instructions[id].result_ty.take()?;
        let result = self.instructions[id]
            .set_result(None)
            .expect("value-producing instruction must have an allocated result");
        self.values[result] = Value::Undef(ty);
        Some(result)
    }

    /// Returns the block containing each placed instruction, indexed by instruction.
    #[must_use]
    pub fn inst_block_table(&self) -> IndexVec<InstId, Option<BlockId>> {
        let mut inst_blocks = IndexVec::from_vec(vec![None; self.instructions.len()]);
        for (block_id, block) in self.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                inst_blocks[inst_id] = Some(block_id);
            }
        }
        inst_blocks
    }

    /// Returns true if the block contains any phi instruction.
    #[must_use]
    pub fn block_has_phi(&self, block: BlockId) -> bool {
        self.blocks[block]
            .instructions
            .iter()
            .any(|&inst_id| matches!(self.instructions[inst_id].kind, InstKind::Phi(_)))
    }

    /// Returns true if every instruction in the block is a phi instruction.
    #[must_use]
    pub fn block_has_only_phis(&self, block: BlockId) -> bool {
        self.blocks[block]
            .instructions
            .iter()
            .all(|&inst_id| matches!(self.instructions[inst_id].kind, InstKind::Phi(_)))
    }

    /// Returns the result values produced by phi instructions in the block.
    #[must_use]
    pub fn block_phi_results(&self, block: BlockId) -> DenseBitSet<ValueId> {
        let mut results = DenseBitSet::new_empty(self.num_values());
        for &inst_id in &self.blocks[block].instructions {
            if matches!(self.instructions[inst_id].kind, InstKind::Phi(_))
                && let Some(result) = self.inst_result_value(inst_id)
            {
                results.insert(result);
            }
        }
        results
    }

    /// Returns the basic block for the given ID.
    #[must_use]
    pub fn block(&self, id: BlockId) -> &BasicBlock {
        &self.blocks[id]
    }

    /// Returns a mutable reference to the basic block.
    pub fn block_mut(&mut self, id: BlockId) -> &mut BasicBlock {
        &mut self.blocks[id]
    }

    /// Allocates a new value.
    pub fn alloc_value(&mut self, value: Value) -> ValueId {
        assert!(
            !matches!(value, Value::Inst(_)),
            "instruction results must be allocated with their instruction"
        );
        self.values.push(value)
    }

    /// Adds a parameter and allocates its argument value.
    pub(crate) fn alloc_param(&mut self, ty: MirType) -> ValueId {
        let index = self.params.push(ty);
        let arg_index = self.arg_types.push(ty);
        assert_eq!(arg_index, index, "parameter and argument type indexes must match");
        self.alloc_arg(index)
    }

    /// Adds a retained argument type and allocates its value without changing the signature.
    pub(crate) fn alloc_implicit_arg(&mut self, ty: MirType) -> ValueId {
        let index = self.arg_types.push(ty);
        self.alloc_arg(index)
    }

    /// Allocates a value referring to an existing argument.
    pub fn alloc_arg(&mut self, index: ArgIdx) -> ValueId {
        let _ = self.arg_ty(index);
        self.alloc_value(Value::Arg(index))
    }

    /// Replaces both the callable parameters and their retained type table.
    pub fn set_params(&mut self, params: IndexVec<ArgIdx, MirType>) {
        self.arg_types = params.clone();
        self.params = params;
    }

    /// Allocates a value-producing instruction and its result value.
    pub fn alloc_value_inst(&mut self, mut inst: Instruction) -> (InstId, ValueId) {
        assert!(inst.result_ty.is_some(), "value-producing instruction must have a result type");

        let inst_id = self.instructions.next_idx();
        let value_id = self.values.next_idx();
        assert!(inst.set_result(Some(value_id)).is_none(), "new instruction already has a result");
        let allocated_inst = self.instructions.push(inst);
        let allocated_value = self.values.push(Value::Inst(inst_id));
        debug_assert_eq!(allocated_inst, inst_id);
        debug_assert_eq!(allocated_value, value_id);
        (inst_id, value_id)
    }

    /// Allocates a value-producing instruction for a preallocated undefined result value.
    pub(crate) fn alloc_inst_with_result(
        &mut self,
        mut inst: Instruction,
        result: ValueId,
    ) -> InstId {
        assert!(inst.result_ty.is_some(), "value-producing instruction must have a result type");
        assert!(
            matches!(self.values[result], Value::Undef(_)),
            "preallocated instruction result must be undefined"
        );

        let inst_id = self.instructions.next_idx();
        assert!(inst.set_result(Some(result)).is_none(), "new instruction already has a result");
        self.values[result] = Value::Inst(inst_id);
        let allocated_inst = self.instructions.push(inst);
        debug_assert_eq!(allocated_inst, inst_id);
        inst_id
    }

    /// Allocates an instruction that produces no value.
    pub fn alloc_inst(&mut self, mut inst: Instruction) -> InstId {
        assert!(inst.result_ty.is_none(), "value-producing instruction must allocate its result");
        inst.set_result(None);
        self.instructions.push(inst)
    }

    /// Allocates a new basic block.
    pub fn alloc_block(&mut self) -> BlockId {
        self.blocks.push(BasicBlock::new())
    }

    /// Replaces all value uses according to a one-step replacement map.
    pub fn replace_uses(&mut self, replacements: &FxHashMap<ValueId, ValueId>) {
        if replacements.is_empty() {
            return;
        }

        self.for_each_instruction_mut(|_, inst| {
            super::utils::replace_inst_uses(inst, replacements);
        });
        for block in self.blocks.iter_mut() {
            if let Some(term) = &mut block.terminator {
                super::utils::replace_terminator_uses(term, replacements);
            }
        }
    }

    /// Replaces all value uses according to a canonicalized replacement map.
    pub fn replace_uses_canonicalized(&mut self, replacements: &FxHashMap<ValueId, ValueId>) {
        if replacements.is_empty() {
            return;
        }

        self.for_each_instruction_mut(|_, inst| {
            super::utils::replace_inst_uses_canonicalized(inst, replacements);
        });
        for block in self.blocks.iter_mut() {
            if let Some(term) = &mut block.terminator {
                super::utils::replace_terminator_uses_canonicalized(term, replacements);
            }
        }
    }

    /// Annotates storage-alias metadata for state-access instructions and returns whether any
    /// metadata changed.
    pub fn annotate_storage_aliases(&mut self, scope: super::utils::StorageAliasScope) -> bool {
        let inst_ids: Vec<_> = self.instructions().collect();
        let mut changed = false;
        for inst_id in inst_ids {
            let slot = match self.inst(inst_id).kind {
                InstKind::Load(slot, PtrLocation::Storage)
                | InstKind::Store(slot, _, PtrLocation::Storage) => Some(slot),
                InstKind::Load(slot, PtrLocation::Transient)
                | InstKind::Store(slot, _, PtrLocation::Transient)
                    if scope == super::utils::StorageAliasScope::StorageAndTransient =>
                {
                    Some(slot)
                }
                _ => None,
            };
            let alias = slot.map(|slot| StorageAlias::for_value(self, slot));
            let metadata = &mut self.inst_mut(inst_id).metadata;
            changed |= metadata.storage_alias() != alias;
            metadata.set_storage_alias(alias);
        }
        changed
    }

    /// Returns stored storage-alias metadata, or computes a conservative alias key.
    #[must_use]
    pub fn storage_alias(&self, inst_id: InstId, slot: ValueId) -> StorageAlias {
        self.inst(inst_id)
            .metadata
            .storage_alias()
            .unwrap_or_else(|| StorageAlias::for_value(self, slot))
    }

    /// Returns storage-alias metadata after applying value replacements.
    #[must_use]
    pub fn storage_alias_after_replacements(
        &self,
        inst_id: InstId,
        slot: ValueId,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> StorageAlias {
        let original_slot = slot;
        let slot = super::utils::resolve_replacement(slot, replacements);
        if slot == original_slot {
            self.storage_alias(inst_id, slot)
        } else {
            StorageAlias::for_value(self, slot)
        }
    }

    /// Drops storage aliases whose base slot no instruction defines any more.
    ///
    /// A key naming a deleted value is consistent only until values are renumbered, when it could
    /// come to name another value. Readers recompute a dropped key from the slot operand.
    pub fn drop_dangling_storage_aliases(&mut self) {
        let mut placed = DenseBitSet::new_empty(self.num_insts());
        for inst in self.instructions() {
            placed.insert(inst);
        }
        let values = &self.values;
        for inst in self.instructions.iter_mut() {
            let base = inst.metadata.storage_alias().and_then(|alias| alias.symbolic_base());
            if let Some(base) = base
                && matches!(values[base], Value::Inst(defining) if !placed.contains(defining))
            {
                // sstore slot, value !metadata(storage=offset(deleted, n)) -> sstore slot, value
                inst.metadata.set_storage_alias(None);
            }
        }
    }

    /// Returns true if this function is public or external.
    #[must_use]
    pub fn is_public(&self) -> bool {
        matches!(self.attributes.visibility, Visibility::Public | Visibility::External)
    }
}

/// Iterator for [`Function::live_values`], refilling one buffer per instruction or terminator.
struct LiveValues<'a> {
    func: &'a Function,
    block: BlockId,
    inst: usize,
    values: SmallVec<[ValueId; 8]>,
    next: usize,
}

impl Iterator for LiveValues<'_> {
    type Item = ValueId;

    fn next(&mut self) -> Option<ValueId> {
        loop {
            if let Some(&value) = self.values.get(self.next) {
                self.next += 1;
                return Some(value);
            }
            let block = self.func.blocks.get(self.block)?;
            self.values.clear();
            self.next = 0;
            if let Some(&inst_id) = block.instructions.get(self.inst) {
                let inst = self.func.inst(inst_id);
                inst.kind.collect_operands(&mut self.values);
                self.values.extend(inst.result());
                self.inst += 1;
            } else {
                if let Some(term) = &block.terminator {
                    term.visit_operands(|value| self.values.push(value));
                }
                self.block += 1;
                self.inst = 0;
            }
        }
    }
}

/// Function attributes.
#[derive(Clone, Debug)]
pub struct FunctionAttributes {
    /// Visibility modifier.
    pub visibility: Visibility,
    /// State mutability.
    pub state_mutability: StateMutability,
    /// Whether the external entry decodes its own ABI inputs and encodes its outputs.
    pub is_abi_wrapper: bool,
    /// Whether this is a constructor.
    pub is_constructor: bool,
    /// Whether this is a fallback function.
    pub is_fallback: bool,
    /// Whether this is a receive function.
    pub is_receive: bool,
    /// Whether the original signature may reference caller-visible memory.
    ///
    /// Dead-result elimination and memory lowering can erase reference types, but must preserve
    /// the original signature's frame-lifetime constraint. The backend uses this sticky bit to
    /// avoid reclaiming memory that may have escaped through inline assembly.
    pub may_return_memory: bool,
    /// Whether this function dispatches an internal function-pointer shape.
    pub is_function_pointer_dispatcher: bool,
    /// Never clone this function into multiple callers.
    pub no_inline: bool,
    /// Clone this function into every caller where inlining is legal, whatever it costs, as its
    /// `@custom:solar-inline always` tag requires.
    pub always_inline: bool,
    /// Favor cloning this function into its callers, as its bare `@custom:solar-inline` tag
    /// asks, within the inliner's legality and stack budgets.
    pub inline_hint: bool,
    /// The objective its `@custom:solar-optimize` tag selects for this function's code, instead
    /// of its module's.
    pub optimization: Option<OptimizationMode>,
    /// Whether every reachable exit aborts, so calls to this function are rare. Computed by the
    /// hotness analysis, which every pass keeps fresh.
    pub hotness: Hotness,
    /// Proved upper bound, in bits, on the words each array parameter can
    /// hold while this function reads it, recorded by element cleanup for
    /// the ABI return proofs that run after the element masks are gone.
    pub array_element_bits: FxHashMap<ArgIdx, u32>,
    /// The widest word the single array this function returns can hold, when element
    /// cleanup proved one. Its caller can re-encode the array without cleaning it.
    pub array_return_element_bits: Option<u32>,
}

impl Default for FunctionAttributes {
    fn default() -> Self {
        Self {
            visibility: Visibility::Internal,
            state_mutability: StateMutability::NonPayable,
            is_abi_wrapper: false,
            is_constructor: false,
            is_fallback: false,
            is_receive: false,
            may_return_memory: false,
            is_function_pointer_dispatcher: false,
            no_inline: false,
            always_inline: false,
            inline_hint: false,
            optimization: None,
            hotness: Hotness::Hot,
            array_element_bits: FxHashMap::default(),
            array_return_element_bits: None,
        }
    }
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fn {}({})", self.name, self.params.iter().format(", "))?;

        if self.return_type != MirType::Void {
            write!(f, " -> {}", self.return_type)?;
        }

        Ok(())
    }
}

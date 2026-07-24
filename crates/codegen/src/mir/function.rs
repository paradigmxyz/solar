//! MIR functions.

use super::{
    BasicBlock, BlockId, InstId, InstKind, Instruction, MirType, StorageAlias, Value, ValueId,
};
use alloy_primitives::U256;
use solar_data_structures::{
    bit_set::DenseBitSet,
    fmt::{self, FmtIteratorExt},
    index::IndexVec,
    map::FxHashMap,
};
use solar_interface::Ident;
use solar_sema::hir::{StateMutability, Visibility};

/// A function in the MIR.
#[derive(Clone, Debug)]
pub(crate) struct Function {
    /// Function name.
    pub(crate) name: Ident,
    /// Function selector (4 bytes, for external functions).
    pub(crate) selector: Option<[u8; 4]>,
    /// Function attributes.
    pub(crate) attributes: FunctionAttributes,
    /// Parameter types.
    pub(crate) params: Vec<MirType>,
    /// Return types.
    pub(crate) returns: Vec<MirType>,
    /// Bytes reserved for lowered local memory slots.
    ///
    /// Internal-call functions place these in the internal frame; external entries
    /// reserve the same space in their low-memory scratch layout.
    pub(crate) internal_frame_size: u64,
    /// Bytes reserved for the low-memory external ABI return buffer.
    pub(crate) external_static_return_size: u64,
    /// All values in this function.
    pub(crate) values: IndexVec<ValueId, Value>,
    /// All instructions allocated in this function.
    instructions: IndexVec<InstId, Instruction>,
    /// All basic blocks in this function. This is never empty; block zero is the entry.
    pub(crate) blocks: IndexVec<BlockId, BasicBlock>,
}

impl Function {
    /// Creates a new function.
    #[must_use]
    pub(crate) fn new(name: Ident) -> Self {
        let mut blocks = IndexVec::new();
        let entry = blocks.push(BasicBlock::new());
        debug_assert_eq!(entry, BlockId::ENTRY);

        Self {
            name,
            selector: None,
            attributes: FunctionAttributes::default(),
            params: Vec::new(),
            returns: Vec::new(),
            internal_frame_size: 0,
            external_static_return_size: 0,
            values: IndexVec::new(),
            instructions: IndexVec::new(),
            blocks,
        }
    }

    /// Returns the value for the given ID.
    #[must_use]
    pub(crate) fn value(&self, id: ValueId) -> &Value {
        &self.values[id]
    }

    /// Returns an immediate value as U256.
    #[must_use]
    pub(crate) fn value_u256(&self, id: ValueId) -> Option<U256> {
        self.value(id).as_immediate()?.as_u256()
    }

    /// Returns an immediate value as u64 when lossless.
    #[must_use]
    pub(crate) fn value_u64(&self, id: ValueId) -> Option<u64> {
        self.value_u256(id).and_then(super::utils::u256_to_u64)
    }

    /// Returns a possibly replaced immediate value as U256.
    #[must_use]
    pub(crate) fn value_u256_after_replacements(
        &self,
        id: ValueId,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> Option<U256> {
        self.value_u256(super::utils::resolve_replacement(id, replacements))
    }

    /// Returns the instruction for the given ID.
    #[must_use]
    pub(crate) fn inst(&self, id: InstId) -> &Instruction {
        &self.instructions[id]
    }

    /// Returns a mutable reference to the instruction for the given ID.
    pub(crate) fn inst_mut(&mut self, id: InstId) -> &mut Instruction {
        &mut self.instructions[id]
    }

    /// Returns the size of the allocated instruction ID domain.
    #[must_use]
    pub(crate) fn num_insts(&self) -> usize {
        self.instructions.len()
    }

    /// Returns the IDs of all active instructions in block order.
    pub(crate) fn instructions(&self) -> impl Iterator<Item = InstId> + '_ {
        self.blocks.iter().flat_map(|block| block.instructions.iter().copied())
    }

    /// Calls `f` for every active instruction in block order.
    pub(crate) fn for_each_instruction_mut(&mut self, mut f: impl FnMut(InstId, &mut Instruction)) {
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
    pub(crate) fn inst_result_value(&self, id: InstId) -> Option<ValueId> {
        self.values
            .iter_enumerated()
            .find(|(_, value)| matches!(value, Value::Inst(inst) if *inst == id))
            .map(|(value_id, _)| value_id)
    }

    /// Returns a map from each instruction to its result value.
    #[must_use]
    pub(crate) fn inst_results(&self) -> FxHashMap<InstId, ValueId> {
        let mut results =
            FxHashMap::with_capacity_and_hasher(self.instructions.len(), Default::default());
        for (value_id, value) in self.values.iter_enumerated() {
            if let Value::Inst(inst_id) = value {
                results.insert(*inst_id, value_id);
            }
        }
        results
    }

    /// Returns a map from each instruction to the block containing it.
    #[must_use]
    pub(crate) fn inst_blocks(&self) -> FxHashMap<InstId, BlockId> {
        let mut inst_blocks =
            FxHashMap::with_capacity_and_hasher(self.instructions.len(), Default::default());
        for (block_id, block) in self.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                inst_blocks.insert(inst_id, block_id);
            }
        }
        inst_blocks
    }

    /// Returns predecessors with duplicate CFG edges collapsed.
    #[must_use]
    pub(crate) fn unique_predecessors(&self, block: BlockId) -> Vec<BlockId> {
        let mut predecessors = Vec::new();
        for &pred in &self.blocks[block].predecessors {
            if !predecessors.contains(&pred) {
                predecessors.push(pred);
            }
        }
        predecessors
    }

    /// Returns true if the block contains any phi instruction.
    #[must_use]
    pub(crate) fn block_has_phi(&self, block: BlockId) -> bool {
        self.blocks[block]
            .instructions
            .iter()
            .any(|&inst_id| matches!(self.instructions[inst_id].kind, InstKind::Phi(_)))
    }

    /// Returns true if every instruction in the block is a phi instruction.
    #[must_use]
    pub(crate) fn block_has_only_phis(&self, block: BlockId) -> bool {
        self.blocks[block]
            .instructions
            .iter()
            .all(|&inst_id| matches!(self.instructions[inst_id].kind, InstKind::Phi(_)))
    }

    /// Returns the result values produced by phi instructions in the block.
    #[must_use]
    pub(crate) fn block_phi_results(&self, block: BlockId) -> DenseBitSet<ValueId> {
        let mut results = DenseBitSet::new_empty(self.values.len());
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
    pub(crate) fn block(&self, id: BlockId) -> &BasicBlock {
        &self.blocks[id]
    }

    /// Returns a mutable reference to the basic block.
    pub(crate) fn block_mut(&mut self, id: BlockId) -> &mut BasicBlock {
        &mut self.blocks[id]
    }

    /// Allocates a new value.
    pub(crate) fn alloc_value(&mut self, value: Value) -> ValueId {
        self.values.push(value)
    }

    /// Allocates a new instruction.
    pub(crate) fn alloc_inst(&mut self, inst: Instruction) -> InstId {
        self.instructions.push(inst)
    }

    /// Allocates a new basic block.
    pub(crate) fn alloc_block(&mut self) -> BlockId {
        self.blocks.push(BasicBlock::new())
    }

    /// Replaces all value uses according to a one-step replacement map.
    pub(crate) fn replace_uses(&mut self, replacements: &FxHashMap<ValueId, ValueId>) {
        if replacements.is_empty() {
            return;
        }

        self.for_each_instruction_mut(|_, inst| {
            super::utils::replace_inst_uses(&mut inst.kind, replacements);
        });
        for block in self.blocks.iter_mut() {
            if let Some(term) = &mut block.terminator {
                super::utils::replace_terminator_uses(term, replacements);
            }
        }
    }

    /// Replaces all value uses according to a canonicalized replacement map.
    pub(crate) fn replace_uses_canonicalized(
        &mut self,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) {
        if replacements.is_empty() {
            return;
        }

        self.for_each_instruction_mut(|_, inst| {
            super::utils::replace_inst_uses_canonicalized(&mut inst.kind, replacements);
        });
        for block in self.blocks.iter_mut() {
            if let Some(term) = &mut block.terminator {
                super::utils::replace_terminator_uses_canonicalized(term, replacements);
            }
        }
    }

    /// Annotates storage-alias metadata for state-access instructions.
    pub(crate) fn annotate_storage_aliases(&mut self, scope: super::utils::StorageAliasScope) {
        let inst_ids: Vec<_> = self.instructions().collect();
        for inst_id in inst_ids {
            let slot = match self.inst(inst_id).kind {
                InstKind::SLoad(slot) | InstKind::SStore(slot, _) => Some(slot),
                InstKind::TLoad(slot) | InstKind::TStore(slot, _)
                    if scope == super::utils::StorageAliasScope::StorageAndTransient =>
                {
                    Some(slot)
                }
                _ => None,
            };
            let alias = slot.map(|slot| StorageAlias::for_value(self, slot));
            self.inst_mut(inst_id).metadata.set_storage_alias(alias);
        }
    }

    /// Returns stored storage-alias metadata, or computes a conservative alias key.
    #[must_use]
    pub(crate) fn storage_alias(&self, inst_id: InstId, slot: ValueId) -> StorageAlias {
        self.inst(inst_id)
            .metadata
            .storage_alias()
            .unwrap_or_else(|| StorageAlias::for_value(self, slot))
    }

    /// Returns storage-alias metadata after applying value replacements.
    #[must_use]
    pub(crate) fn storage_alias_after_replacements(
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

    /// Returns true if this function is public or external.
    #[must_use]
    pub(crate) fn is_public(&self) -> bool {
        matches!(self.attributes.visibility, Visibility::Public | Visibility::External)
    }
}

/// Function attributes.
#[derive(Clone, Debug)]
pub(crate) struct FunctionAttributes {
    /// Visibility modifier.
    pub(crate) visibility: Visibility,
    /// State mutability.
    pub(crate) state_mutability: StateMutability,
    /// Whether this is a constructor.
    pub(crate) is_constructor: bool,
    /// Whether this is a fallback function.
    pub(crate) is_fallback: bool,
    /// Whether this is a receive function.
    pub(crate) is_receive: bool,
    /// Never clone this function into multiple callers (synthesized shared
    /// helpers whose whole point is existing once per module). A sole call
    /// site may still absorb it: with one caller there is nothing to share.
    pub(crate) no_inline: bool,
}

impl Default for FunctionAttributes {
    fn default() -> Self {
        Self {
            visibility: Visibility::Internal,
            state_mutability: StateMutability::NonPayable,
            is_constructor: false,
            is_fallback: false,
            is_receive: false,
            no_inline: false,
        }
    }
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fn {}({})", self.name, self.params.iter().format(", "))?;

        if !self.returns.is_empty() {
            write!(f, " -> ({})", self.returns.iter().format(", "))?;
        }

        Ok(())
    }
}

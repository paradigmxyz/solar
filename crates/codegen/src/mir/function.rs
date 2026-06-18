//! MIR functions.

use super::{BasicBlock, BlockId, InstId, InstKind, Instruction, MirType, Value, ValueId};
use solar_data_structures::{
    fmt::{self, FmtIteratorExt},
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use solar_interface::Ident;
use solar_sema::hir::{StateMutability, Visibility};

/// A function in the MIR.
#[derive(Clone, Debug)]
pub struct Function {
    /// Function name.
    pub name: Ident,
    /// Function selector (4 bytes, for external functions).
    pub selector: Option<[u8; 4]>,
    /// Function attributes.
    pub attributes: FunctionAttributes,
    /// Parameter types.
    pub params: Vec<MirType>,
    /// Return types.
    pub returns: Vec<MirType>,
    /// Bytes reserved for lowered local memory slots.
    ///
    /// Internal-call functions place these in the internal frame; external entries
    /// reserve the same space in their low-memory scratch layout.
    pub internal_frame_size: u64,
    /// Bytes reserved for the low-memory external ABI return buffer.
    pub external_static_return_size: u64,
    /// All values in this function.
    pub values: IndexVec<ValueId, Value>,
    /// All instructions in this function.
    pub instructions: IndexVec<InstId, Instruction>,
    /// All basic blocks in this function.
    pub blocks: IndexVec<BlockId, BasicBlock>,
    /// The entry block.
    pub entry_block: BlockId,
}

impl Function {
    /// Creates a new function.
    #[must_use]
    pub fn new(name: Ident) -> Self {
        let mut blocks = IndexVec::new();
        let entry_block = blocks.push(BasicBlock::new());

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
            entry_block,
        }
    }

    /// Returns the value for the given ID.
    #[must_use]
    pub fn value(&self, id: ValueId) -> &Value {
        &self.values[id]
    }

    /// Returns the instruction for the given ID.
    #[must_use]
    pub fn instruction(&self, id: InstId) -> &Instruction {
        &self.instructions[id]
    }

    /// Returns the value produced by the given instruction, if it has one.
    #[must_use]
    pub fn inst_result_value(&self, id: InstId) -> Option<ValueId> {
        self.values
            .iter_enumerated()
            .find(|(_, value)| matches!(value, Value::Inst(inst) if *inst == id))
            .map(|(value_id, _)| value_id)
    }

    /// Returns a map from each instruction to its result value.
    #[must_use]
    pub fn inst_results(&self) -> FxHashMap<InstId, ValueId> {
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
    pub fn inst_blocks(&self) -> FxHashMap<InstId, BlockId> {
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
    pub fn unique_predecessors(&self, block: BlockId) -> Vec<BlockId> {
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
    pub fn block_phi_results(&self, block: BlockId) -> FxHashSet<ValueId> {
        self.blocks[block]
            .instructions
            .iter()
            .filter_map(|&inst_id| {
                matches!(self.instructions[inst_id].kind, InstKind::Phi(_))
                    .then(|| self.inst_result_value(inst_id))
                    .flatten()
            })
            .collect()
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

    /// Returns the entry block.
    #[must_use]
    pub fn entry(&self) -> &BasicBlock {
        &self.blocks[self.entry_block]
    }

    /// Allocates a new value.
    pub fn alloc_value(&mut self, value: Value) -> ValueId {
        self.values.push(value)
    }

    /// Allocates a new instruction.
    pub fn alloc_inst(&mut self, inst: Instruction) -> InstId {
        self.instructions.push(inst)
    }

    /// Allocates a new basic block.
    pub fn alloc_block(&mut self) -> BlockId {
        self.blocks.push(BasicBlock::new())
    }

    /// Returns true if this function is public or external.
    #[must_use]
    pub fn is_public(&self) -> bool {
        matches!(self.attributes.visibility, Visibility::Public | Visibility::External)
    }

    /// Returns the function selector as a hex string.
    #[must_use]
    pub fn selector_hex(&self) -> Option<String> {
        self.selector.map(alloy_primitives::hex::encode)
    }

    /// Returns the human-readable textual MIR representation of this function.
    pub fn to_text(&self) -> impl fmt::Display + '_ {
        super::display::display_function_text(self)
    }

    /// Returns this function's DOT-format CFG.
    pub fn to_dot(&self) -> impl fmt::Display + '_ {
        super::display::display_function_dot(self)
    }
}

/// Function attributes.
#[derive(Clone, Debug)]
pub struct FunctionAttributes {
    /// Visibility modifier.
    pub visibility: Visibility,
    /// State mutability.
    pub state_mutability: StateMutability,
    /// Whether this is a constructor.
    pub is_constructor: bool,
    /// Whether this is a fallback function.
    pub is_fallback: bool,
    /// Whether this is a receive function.
    pub is_receive: bool,
}

impl Default for FunctionAttributes {
    fn default() -> Self {
        Self {
            visibility: Visibility::Internal,
            state_mutability: StateMutability::NonPayable,
            is_constructor: false,
            is_fallback: false,
            is_receive: false,
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

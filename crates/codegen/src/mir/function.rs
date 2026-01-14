//! MIR functions.

use super::{BasicBlock, BlockId, InstId, Instruction, MirType, Value, ValueId};
use solar_data_structures::index::IndexVec;
use solar_interface::Ident;
use solar_sema::hir::{StateMutability, Visibility};
use std::fmt;

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
        write!(f, "fn {}(", self.name)?;
        for (i, param) in self.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{param}")?;
        }
        write!(f, ")")?;

        if !self.returns.is_empty() {
            write!(f, " -> (")?;
            for (i, ret) in self.returns.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{ret}")?;
            }
            write!(f, ")")?;
        }

        Ok(())
    }
}

//! MIR basic blocks.

use super::{BlockId, InstId, ValueId};
use smallvec::SmallVec;
use std::fmt;

/// A basic block in the MIR.
#[derive(Clone, Debug)]
pub struct BasicBlock {
    /// The instructions in this block (excluding the terminator).
    pub instructions: Vec<InstId>,
    /// The terminator instruction.
    pub terminator: Option<Terminator>,
    /// Predecessor blocks.
    pub predecessors: SmallVec<[BlockId; 4]>,
    /// Successor blocks.
    pub successors: SmallVec<[BlockId; 2]>,
}

impl BasicBlock {
    /// Creates a new empty basic block.
    #[must_use]
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            terminator: None,
            predecessors: SmallVec::new(),
            successors: SmallVec::new(),
        }
    }

    /// Returns true if this block has a terminator.
    #[must_use]
    pub const fn is_terminated(&self) -> bool {
        self.terminator.is_some()
    }

    /// Returns the terminator, if present.
    #[must_use]
    pub const fn terminator(&self) -> Option<&Terminator> {
        self.terminator.as_ref()
    }
}

impl Default for BasicBlock {
    fn default() -> Self {
        Self::new()
    }
}

/// A block terminator instruction.
#[derive(Clone, Debug)]
pub enum Terminator {
    /// Unconditional jump to another block.
    Jump(BlockId),
    /// Conditional branch.
    Branch {
        /// The condition value (must be boolean).
        condition: ValueId,
        /// The block to jump to if true.
        then_block: BlockId,
        /// The block to jump to if false.
        else_block: BlockId,
    },
    /// Multi-way switch.
    Switch {
        /// The value to switch on.
        value: ValueId,
        /// The default block.
        default: BlockId,
        /// The cases: (value, block).
        cases: Vec<(ValueId, BlockId)>,
    },
    /// Return from function.
    Return {
        /// The return values.
        values: SmallVec<[ValueId; 2]>,
    },
    /// Revert execution.
    Revert {
        /// Memory offset of revert data.
        offset: ValueId,
        /// Size of revert data.
        size: ValueId,
    },
    /// Stop execution.
    Stop,
    /// Self-destruct the contract.
    SelfDestruct {
        /// The address to send remaining funds to.
        recipient: ValueId,
    },
    /// Invalid operation (unreachable code).
    Invalid,
}

impl Terminator {
    /// Returns the successor blocks of this terminator.
    #[must_use]
    pub fn successors(&self) -> SmallVec<[BlockId; 2]> {
        match self {
            Self::Jump(target) => smallvec::smallvec![*target],
            Self::Branch { then_block, else_block, .. } => {
                smallvec::smallvec![*then_block, *else_block]
            }
            Self::Switch { default, cases, .. } => {
                let mut succs = SmallVec::with_capacity(cases.len() + 1);
                succs.push(*default);
                for (_, block) in cases {
                    succs.push(*block);
                }
                succs
            }
            Self::Return { .. }
            | Self::Revert { .. }
            | Self::Stop
            | Self::SelfDestruct { .. }
            | Self::Invalid => SmallVec::new(),
        }
    }

    /// Returns the mnemonic for this terminator.
    #[must_use]
    pub const fn mnemonic(&self) -> &'static str {
        match self {
            Self::Jump(_) => "jump",
            Self::Branch { .. } => "branch",
            Self::Switch { .. } => "switch",
            Self::Return { .. } => "return",
            Self::Revert { .. } => "revert",
            Self::Stop => "stop",
            Self::SelfDestruct { .. } => "selfdestruct",
            Self::Invalid => "invalid",
        }
    }
}

impl fmt::Display for Terminator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Jump(target) => write!(f, "jump bb{}", target.index()),
            Self::Branch { condition, then_block, else_block } => {
                write!(
                    f,
                    "branch v{}, bb{}, bb{}",
                    condition.index(),
                    then_block.index(),
                    else_block.index()
                )
            }
            Self::Switch { value, default, cases } => {
                write!(f, "switch v{}, default bb{}", value.index(), default.index())?;
                for (val, block) in cases {
                    write!(f, ", v{} => bb{}", val.index(), block.index())?;
                }
                Ok(())
            }
            Self::Return { values } => {
                write!(f, "return")?;
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, " v{}", v.index())?;
                }
                Ok(())
            }
            Self::Revert { offset, size } => {
                write!(f, "revert v{}, v{}", offset.index(), size.index())
            }
            Self::Stop => write!(f, "stop"),
            Self::SelfDestruct { recipient } => {
                write!(f, "selfdestruct v{}", recipient.index())
            }
            Self::Invalid => write!(f, "invalid"),
        }
    }
}

//! MIR basic blocks.

use super::{BlockId, InstId, InstructionMetadata, ValueId};
use smallvec::SmallVec;
use std::fmt;

/// A basic block in the MIR.
#[derive(Clone, Debug)]
pub(crate) struct BasicBlock {
    /// The instructions in this block (excluding the terminator).
    pub(crate) instructions: Vec<InstId>,
    /// The terminator instruction.
    pub(crate) terminator: Option<Terminator>,
    /// Source context of the control transfer, independent of the last value instruction.
    pub(crate) terminator_metadata: InstructionMetadata,
    /// Predecessor blocks.
    pub(crate) predecessors: SmallVec<[BlockId; 4]>,
}

impl BasicBlock {
    /// Replaces a control transfer together with its source context.
    ///
    /// The caller remains responsible for updating CFG edges and phi inputs.
    pub(crate) fn set_terminator(&mut self, terminator: Terminator, metadata: InstructionMetadata) {
        self.terminator = Some(terminator);
        self.terminator_metadata = metadata;
    }

    /// Replaces a transfer with compiler-generated control flow, dropping stale context.
    pub(crate) fn set_generated_terminator(&mut self, terminator: Terminator) {
        let mut metadata = InstructionMetadata::EMPTY;
        metadata.mark_debug_info_dropped();
        self.set_terminator(terminator, metadata);
    }

    /// Moves a control transfer out, leaving fresh generated-code context behind.
    pub(crate) fn take_terminator(&mut self) -> (Option<Terminator>, InstructionMetadata) {
        let mut generated = InstructionMetadata::EMPTY;
        generated.mark_debug_info_dropped();
        let metadata = std::mem::replace(&mut self.terminator_metadata, generated);
        (self.terminator.take(), metadata)
    }

    /// Creates a new empty basic block.
    #[must_use]
    pub(crate) fn new() -> Self {
        let mut terminator_metadata = InstructionMetadata::EMPTY;
        terminator_metadata.mark_debug_info_dropped();
        Self {
            instructions: Vec::new(),
            terminator: None,
            terminator_metadata,
            predecessors: SmallVec::new(),
        }
    }
}

impl Default for BasicBlock {
    fn default() -> Self {
        Self::new()
    }
}

/// A block terminator instruction.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Terminator {
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
    /// Revert with the returndata produced by the preceding external call.
    ///
    /// This keeps returndata bubbling semantic until ABI lowering selects the
    /// target EVM version and materializes the copy into memory.
    RevertReturndata,
    /// Return raw, already-encoded data: `RETURN(offset, size)`. Used for
    /// ABI-encoded external returns whose size is computed at runtime.
    ReturnData {
        /// Memory offset of the return data.
        offset: ValueId,
        /// Size of the return data in bytes.
        size: ValueId,
    },
    /// Stop execution.
    Stop,
    /// Self-destruct the contract.
    SelfDestruct {
        /// The address to send remaining funds to.
        recipient: ValueId,
    },
    /// Transfer control to another function without returning.
    ///
    /// Unlike an internal call, control never comes back to the current
    /// function: the callee's own terminators end the execution (or transfer
    /// it further). The dispatch phase uses this to route `entry` switch cases
    /// to the ABI wrappers, which terminate externally with `RETURN`/`REVERT`.
    TailCall {
        /// The function to transfer control to.
        function: super::FunctionId,
        /// The arguments, matching the callee's parameters.
        args: SmallVec<[ValueId; 2]>,
    },
    /// Invalid operation (unreachable code).
    Invalid,
}

impl Terminator {
    /// Visits the successor blocks of this terminator without allocating.
    pub(crate) fn for_each_successor(&self, mut visit: impl FnMut(BlockId)) {
        match self {
            Self::Jump(target) => visit(*target),
            Self::Branch { then_block, else_block, .. } => {
                visit(*then_block);
                visit(*else_block);
            }
            Self::Switch { default, cases, .. } => {
                visit(*default);
                for &(_, block) in cases {
                    visit(block);
                }
            }
            Self::Return { .. }
            | Self::Revert { .. }
            | Self::RevertReturndata
            | Self::ReturnData { .. }
            | Self::Stop
            | Self::SelfDestruct { .. }
            | Self::TailCall { .. }
            | Self::Invalid => {}
        }
    }

    /// Returns whether this terminator branches to `target`.
    #[must_use]
    pub(crate) fn has_successor(&self, target: BlockId) -> bool {
        match self {
            Self::Jump(block) => *block == target,
            Self::Branch { then_block, else_block, .. } => {
                *then_block == target || *else_block == target
            }
            Self::Switch { default, cases, .. } => {
                *default == target || cases.iter().any(|&(_, block)| block == target)
            }
            Self::Return { .. }
            | Self::Revert { .. }
            | Self::RevertReturndata
            | Self::ReturnData { .. }
            | Self::Stop
            | Self::SelfDestruct { .. }
            | Self::TailCall { .. }
            | Self::Invalid => false,
        }
    }

    /// Visits the value operands of this terminator without allocating.
    pub(crate) fn for_each_operand(&self, mut visit: impl FnMut(ValueId)) {
        match self {
            Self::Jump(_) => {}
            Self::Branch { condition, .. } => visit(*condition),
            Self::Switch { value, cases, .. } => {
                visit(*value);
                for &(case_value, _) in cases {
                    visit(case_value);
                }
            }
            Self::Return { values } => {
                for &value in values {
                    visit(value);
                }
            }
            Self::Revert { offset, size } | Self::ReturnData { offset, size } => {
                visit(*offset);
                visit(*size);
            }
            Self::RevertReturndata | Self::Stop | Self::Invalid => {}
            Self::SelfDestruct { recipient } => visit(*recipient),
            Self::TailCall { args, .. } => {
                for &arg in args {
                    visit(arg);
                }
            }
        }
    }

    /// Returns the successor blocks of this terminator.
    #[must_use]
    pub(crate) fn successors(&self) -> SmallVec<[BlockId; 2]> {
        let mut successors = match self {
            Self::Switch { cases, .. } => SmallVec::with_capacity(cases.len() + 1),
            _ => SmallVec::new(),
        };
        self.for_each_successor(|block| successors.push(block));
        successors
    }

    /// Returns the mnemonic for this terminator.
    #[must_use]
    pub(crate) const fn mnemonic(&self) -> &'static str {
        match self {
            Self::Jump(_) => "jump",
            Self::Branch { .. } => "jumpi",
            Self::Switch { .. } => "switch",
            Self::Return { .. } => "return",
            Self::Revert { .. } => "revert",
            Self::RevertReturndata => "revert_returndata",
            Self::ReturnData { .. } => "returndata",
            Self::Stop => "stop",
            Self::SelfDestruct { .. } => "selfdestruct",
            Self::TailCall { .. } => "tail_call",
            Self::Invalid => "invalid",
        }
    }

    /// Returns the [`ValueId`] operands of this terminator (the values it reads).
    /// Block targets are NOT included; use [`Self::successors`] for those.
    #[must_use]
    pub(crate) fn operands(&self) -> SmallVec<[ValueId; 4]> {
        let mut out = match self {
            Self::Return { values } => SmallVec::with_capacity(values.len()),
            Self::TailCall { args, .. } => SmallVec::with_capacity(args.len()),
            _ => SmallVec::new(),
        };
        self.for_each_operand(|value| out.push(value));
        out
    }

    /// Visits every value operand mutably.
    pub(crate) fn visit_operands_mut(&mut self, mut f: impl FnMut(&mut ValueId)) {
        match self {
            Self::Jump(_) | Self::RevertReturndata | Self::Stop | Self::Invalid => {}
            Self::Branch { condition, .. } => f(condition),
            Self::Switch { value, cases, .. } => {
                f(value);
                for (case_value, _) in cases {
                    f(case_value);
                }
            }
            Self::Return { values } => {
                for value in values {
                    f(value);
                }
            }
            Self::Revert { offset, size } | Self::ReturnData { offset, size } => {
                f(offset);
                f(size);
            }
            Self::SelfDestruct { recipient } => f(recipient),
            Self::TailCall { args, .. } => {
                for arg in args {
                    f(arg);
                }
            }
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
                    "jumpi v{}, bb{}, bb{}",
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
            Self::RevertReturndata => write!(f, "revert_returndata"),
            Self::ReturnData { offset, size } => {
                write!(f, "returndata v{}, v{}", offset.index(), size.index())
            }
            Self::Stop => write!(f, "stop"),
            Self::SelfDestruct { recipient } => {
                write!(f, "selfdestruct v{}", recipient.index())
            }
            Self::TailCall { function, args } => {
                write!(f, "tail_call fn{}", function.index())?;
                for arg in args {
                    write!(f, ", v{}", arg.index())?;
                }
                Ok(())
            }
            Self::Invalid => write!(f, "invalid"),
        }
    }
}

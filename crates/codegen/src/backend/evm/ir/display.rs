//! EVM IR text formatting.

use super::*;
use crate::backend::evm::opcode as op;
use solar_data_structures::fmt::FmtIteratorExt;

impl Module {
    /// Returns the canonical EVM IR text-format representation.
    pub fn to_text(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| {
            writeln!(f, "@module {}", self.name)?;
            write!(
                f,
                "{}",
                self.blocks.iter_enumerated().format_with("", |f, (block_id, block)| {
                    write!(f, "{}", display_block(self, block_id, block))
                })
            )
        })
    }
}

fn display_block<'a>(
    module: &'a Module,
    block_id: BlockId,
    block: &'a Block,
) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| {
        let entry = if module.entry_block == Some(block_id) { " (entry)" } else { "" };
        let cold = if block.metadata.hotness.is_cold() { " [cold]" } else { "" };
        writeln!(f, "bb{}{}{}:", block.label, entry, cold)?;
        for inst in &block.instructions {
            writeln!(f, "  {}", display_instruction(module, inst))?;
        }
        if let Some(term) = &block.terminator {
            writeln!(f, "  {}", display_terminator(module, term))?;
        }
        Ok(())
    })
}

fn display_instruction<'a>(module: &'a Module, inst: &'a Instruction) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| {
        write!(f, "{}", inst.mnemonic())?;
        if let Some(value) = &inst.value {
            write!(f, " {}", display_push_value(module, value))?;
        }
        write!(
            f,
            "{}",
            display_metadata(&inst.metadata, Some(default_instruction_stack_effect(inst)))
        )
    })
}

fn display_terminator<'a>(module: &'a Module, term: &'a Terminator) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| {
        match &term.kind {
            TerminatorKind::Jump(target) => {
                write!(f, "jump {}", display_block_id(module, *target))?;
            }
            TerminatorKind::JumpI { then_block, else_block } => {
                write!(
                    f,
                    "jumpi {}, {}",
                    display_block_id(module, *then_block),
                    display_block_id(module, *else_block)
                )?;
            }
            TerminatorKind::Op(opcode) => {
                if let Some(mnemonic) = op::mnemonic(*opcode) {
                    write!(f, "{mnemonic}")?;
                } else {
                    write!(f, "raw 0x{opcode:02x}")?;
                }
            }
        }
        write!(
            f,
            "{}",
            display_metadata(&term.metadata, Some(default_terminator_stack_effect(&term.kind)))
        )
    })
}

fn display_metadata(
    metadata: &Metadata,
    default_stack: Option<StackEffect>,
) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| {
        if let Some(stack) = metadata.stack
            && Some(stack) != default_stack
        {
            write!(f, " !meta(stack={}->{})", stack.inputs, stack.outputs)?;
        }
        Ok(())
    })
}

fn display_push_value<'a>(module: &'a Module, value: &'a PushValue) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| match value {
        PushValue::Immediate(value) => write!(f, "{}", display_u256(*value)),
        PushValue::Block(block) => write!(f, "{}", display_block_id(module, *block)),
    })
}

fn display_block_id(module: &Module, block: BlockId) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| write!(f, "bb{}", module.blocks[block].label))
}

fn display_u256(value: U256) -> impl fmt::Display {
    fmt::from_fn(move |f| {
        if let Ok(value) = u64::try_from(value)
            && value < 1000
        {
            write!(f, "{value}")
        } else {
            write!(f, "{value:#x}")
        }
    })
}

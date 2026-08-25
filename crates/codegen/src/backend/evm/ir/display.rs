//! EVM IR text formatting.

use super::*;
use crate::backend::evm::op;
use solar_data_structures::fmt::FmtIteratorExt;

impl Module {
    /// Returns the canonical EVM IR text-format representation.
    pub fn to_text(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| {
            writeln!(f, "@module {}", self.name)?;
            write!(
                f,
                "{}",
                self.blocks
                    .iter()
                    .format_with("", |f, block| { write!(f, "{}", display_block(self, block)) })
            )?;
            if !self.data.is_empty() {
                writeln!(f)?;
            }
            for (id, data) in self.data.iter_enumerated() {
                let name = data.named.then(|| crate::data_literal_name(id.index()));
                let name = name.map_or_else(|| id.index().to_string(), |name| name.to_string());
                write!(f, "@data {name} hex\"")?;
                for byte in &data.bytes {
                    write!(f, "{byte:02x}")?;
                }
                writeln!(f, "\"")?;
            }
            Ok(())
        })
    }
}

fn display_block<'a>(module: &'a Module, block: &'a Block) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| {
        let cold = if block.metadata.hotness.is_cold() { " [cold]" } else { "" };
        writeln!(f, "bb{}{}:", block.label, cold)?;
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
        if let Some(size) = inst.immutable_type_size() {
            write!(f, ", {}", size.bytes())?;
        }
        write!(f, "{}", display_metadata(&inst.metadata, default_instruction_stack_effect(inst)))
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
            TerminatorKind::IndexedJump(targets) => {
                write!(f, "indexed_jump ")?;
                write!(
                    f,
                    "{}",
                    targets.iter().format_with(", ", |f, target| write!(
                        f,
                        "{}",
                        display_block_id(module, *target)
                    ))
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
            display_metadata(&term.metadata, default_terminator_stack_effect(&term.kind))
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
        PushValue::Data(data) => {
            if module.data[data.id].named {
                write!(f, "{}", crate::data_literal_name(data.id.index()))?;
            } else {
                write!(f, "{}", data.id.index())?;
            }
            if data.offset != 0 {
                write!(f, "+{}", data.offset)?;
            }
            Ok(())
        }
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

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
                if let Some(name) = data.name {
                    write!(f, "@data {} hex\"", crate::utils::display_data_name(name, id.index()))?;
                } else {
                    write!(f, "@data {} hex\"", id.index())?;
                }
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
        let attributes = match (block.metadata.hotness.is_cold(), block.metadata.in_loop) {
            (false, false) => "",
            (true, false) => " [cold]",
            (false, true) => " [loop]",
            (true, true) => " [cold, loop]",
        };
        writeln!(f, "bb{}{}:", block.label, attributes)?;
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
        match inst.as_stack_op() {
            Some(op::StackOp::Dup(n)) => write!(f, "dup {n}")?,
            Some(op::StackOp::Swap(n)) => write!(f, "swap {n}")?,
            Some(op::StackOp::Exchange(n, m)) => write!(f, "exchange {n}, {m}")?,
            Some(op::StackOp::Pop) => f.write_str("pop")?,
            None => write!(f, "{}", inst.mnemonic())?,
        }
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
        let stack = metadata.stack.filter(|&stack| Some(stack) != default_stack);
        if stack.is_none() && !metadata.keep_with_next {
            return Ok(());
        }
        f.write_str(" !meta(")?;
        if let Some(stack) = stack {
            write!(f, "stack={}->{}", stack.inputs, stack.outputs)?;
            if metadata.keep_with_next {
                f.write_str(", ")?;
            }
        }
        if metadata.keep_with_next {
            f.write_str("keep_with_next")?;
        }
        f.write_str(")")
    })
}

fn display_push_value<'a>(module: &'a Module, value: &'a PushValue) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| match value {
        PushValue::Immediate(value) => write!(f, "{}", display_u256(*value)),
        PushValue::Block(block) => write!(f, "{}", display_block_id(module, *block)),
        PushValue::Data(data) => write!(
            f,
            "{}",
            crate::utils::display_data_ref(module.data[data.id].name, data.id.index(), data.offset,)
        ),
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

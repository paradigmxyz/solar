//! EVM IR text formatting.

use super::*;
use solar_data_structures::fmt::FmtIteratorExt;

impl EvmIrModule {
    /// Returns the canonical EVM IR text-format representation.
    pub fn to_text(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| {
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
    module: &'a EvmIrModule,
    block_id: EvmIrBlockId,
    block: &'a EvmIrBlock,
) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| {
        let entry = if module.entry_block == Some(block_id) { " (entry)" } else { "" };
        let cold = if block.metadata.hotness == EvmIrBlockHotness::Cold { " [cold]" } else { "" };
        write!(f, "{}{}{}", block.label, entry, cold)?;
        if !block.entry_stack.is_empty() {
            write!(f, " (in ")?;
            for (i, &value) in block.entry_stack.iter().enumerate() {
                if i != 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", display_value(module, value))?;
            }
            write!(f, ")")?;
        }
        writeln!(f, ":")?;
        for inst in &block.instructions {
            writeln!(f, "  {}", display_instruction(module, inst))?;
        }
        if let Some(term) = &block.terminator {
            writeln!(f, "  {}", display_terminator(module, term))?;
        }
        Ok(())
    })
}

fn display_instruction<'a>(
    module: &'a EvmIrModule,
    inst: &'a EvmIrInstruction,
) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| {
        if let Some(result) = inst.result {
            write!(f, "{} = ", display_value(module, result))?;
        }
        write!(f, "{}", inst.mnemonic())?;
        if !inst.operands.is_empty() {
            write!(
                f,
                " {}",
                inst.operands.iter().map(|operand| display_operand(module, operand)).format(", ")
            )?;
        }
        write!(
            f,
            "{}",
            display_metadata(&inst.metadata, Some(default_instruction_stack_effect(inst)))
        )
    })
}

fn display_terminator<'a>(
    module: &'a EvmIrModule,
    term: &'a EvmIrTerminator,
) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| {
        match &term.kind {
            EvmIrTerminatorKind::Fallthrough(target) => {
                write!(f, "fallthrough {}", display_block_id(module, *target))?;
            }
            EvmIrTerminatorKind::Jump(target) => {
                write!(f, "jump {}", display_block_id(module, *target))?;
            }
            EvmIrTerminatorKind::Branch { condition, then_block, else_block } => {
                write!(
                    f,
                    "br {}, {}, {}",
                    display_operand(module, condition),
                    display_block_id(module, *then_block),
                    display_block_id(module, *else_block)
                )?;
            }
            EvmIrTerminatorKind::Switch { value, default, cases } => {
                write!(
                    f,
                    "switch {}, default {}, [",
                    display_operand(module, value),
                    display_block_id(module, *default)
                )?;
                write!(
                    f,
                    "{}",
                    cases.iter().format_with(", ", |f, (case, target)| {
                        write!(
                            f,
                            "{} => {}",
                            display_operand(module, case),
                            display_block_id(module, *target)
                        )
                    })
                )?;
                write!(f, "]")?;
            }
            EvmIrTerminatorKind::Return { offset, size } => {
                write!(
                    f,
                    "return {}, {}",
                    display_operand(module, offset),
                    display_operand(module, size)
                )?;
            }
            EvmIrTerminatorKind::Revert { offset, size } => {
                write!(
                    f,
                    "revert {}, {}",
                    display_operand(module, offset),
                    display_operand(module, size)
                )?;
            }
            EvmIrTerminatorKind::Stop => write!(f, "stop")?,
            EvmIrTerminatorKind::Invalid => write!(f, "invalid")?,
            EvmIrTerminatorKind::SelfDestruct { recipient } => {
                write!(f, "selfdestruct {}", display_operand(module, recipient))?;
            }
            EvmIrTerminatorKind::RawOpcode(opcode) => {
                write!(f, "terminal 0x{opcode:02x}")?;
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
    metadata: &EvmIrMetadata,
    default_stack: Option<EvmIrStackEffect>,
) -> impl fmt::Display + '_ {
    enum Field<'a> {
        Stack(EvmIrStackEffect),
        Attr(&'a EvmIrMetadataItem),
    }

    fn display_field(field: Field<'_>) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| match field {
            Field::Stack(effect) => write!(f, "stack={}->{}", effect.inputs, effect.outputs),
            Field::Attr(item) => {
                write!(f, "{}", item.key)?;
                if let Some(value) = &item.value {
                    write!(f, "={value}")?;
                }
                Ok(())
            }
        })
    }

    fmt::from_fn(move |f| {
        if metadata.is_empty() {
            return Ok(());
        }
        let mut fields =
            Vec::with_capacity(metadata.attrs.len() + usize::from(metadata.stack.is_some()));
        if let Some(stack) = metadata.stack
            && Some(stack) != default_stack
        {
            fields.push(Field::Stack(stack));
        }
        fields.extend(metadata.attrs.iter().map(Field::Attr));
        if fields.is_empty() {
            return Ok(());
        }
        write!(f, " !meta({})", fields.into_iter().map(display_field).format(", "))
    })
}

fn display_operand<'a>(
    module: &'a EvmIrModule,
    operand: &'a EvmIrOperand,
) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| match operand {
        EvmIrOperand::Value(value) => write!(f, "{}", display_value(module, *value)),
        EvmIrOperand::Immediate(value) => write!(f, "{}", display_u256(*value)),
        EvmIrOperand::Block(block) => write!(f, "{}", display_block_id(module, *block)),
        EvmIrOperand::Symbol(symbol) => write!(f, "{symbol}"),
    })
}

fn display_value(module: &EvmIrModule, value: EvmIrValueId) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| write!(f, "%{}", module.values[value].name))
}

fn display_block_id(module: &EvmIrModule, block: EvmIrBlockId) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| write!(f, "{}", module.blocks[block].label))
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

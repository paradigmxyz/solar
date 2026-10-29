//! EVM IR text formatting.

use super::*;
use crate::{
    link::{display_data_ref, display_data_size, display_declarations},
    op,
};
use solar_data_structures::fmt::FmtIteratorExt;

impl Module {
    /// Returns the canonical EVM IR text-format representation.
    pub fn to_text(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| {
            writeln!(f, "@module {}", self.name)?;
            write!(f, "{}", display_declarations(&self.libraries, &self.data))?;
            write!(
                f,
                "{}",
                self.blocks
                    .iter()
                    .format_with("", |f, block| { write!(f, "{}", display_block(self, block)) })
            )
        })
    }
}

fn display_block<'a>(module: &'a Module, block: &'a Block) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| {
        let metadata = &block.metadata;
        let mut attributes = Vec::new();
        if metadata.hotness.is_cold() {
            attributes.push("cold".to_string());
        }
        if metadata.in_loop {
            attributes.push("loop".to_string());
        }
        if metadata.is_continuation {
            attributes.push("continuation".to_string());
        }
        if let Some(function) = metadata.function_invoke {
            attributes.push(format!(
                "invoke={}@{}..{}",
                function.identifier,
                function.declaration.lo().0,
                function.declaration.hi().0
            ));
        }
        if let Some(effect) = metadata.call_effect {
            attributes.push(format!(
                "ret={}, inputs={}, outputs={}",
                effect.ret, effect.effect.inputs, effect.effect.outputs
            ));
        }
        if let Some(depth) = metadata.entry_depth {
            attributes.push(format!("stack={depth}"));
        }
        write!(f, "bb{}", block.label)?;
        if !attributes.is_empty() {
            write!(f, " [{}]", attributes.iter().format(", "))?;
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
        write!(f, "{}", display_metadata(&inst.metadata))
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
        write!(f, "{}", display_metadata(&term.metadata))
    })
}

fn display_metadata(metadata: &Metadata) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| {
        let spans = metadata.source_spans();
        let function_invoke = metadata.function_invoke();
        let function_exit = metadata.function_exit();
        if spans.is_empty()
            && function_invoke.is_none()
            && function_exit.is_none()
            && !metadata.keep_with_next
        {
            return Ok(());
        }

        write!(f, " !metadata(")?;
        let mut separator = "";
        if let [span] = spans {
            write!(f, "span={}..{}", span.lo().0, span.hi().0)?;
            separator = ", ";
        } else if !spans.is_empty() {
            write!(f, "spans=[")?;
            for (index, span) in spans.iter().enumerate() {
                if index != 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}..{}", span.lo().0, span.hi().0)?;
            }
            write!(f, "]")?;
            separator = ", ";
        }
        if let Some(function) = function_invoke {
            write!(
                f,
                "{separator}invoke={}@{}..{}",
                function.identifier,
                function.declaration.lo().0,
                function.declaration.hi().0
            )?;
            separator = ", ";
        }
        if let Some(function_exit) = function_exit {
            let exit = match function_exit {
                DebugFunctionExit::Return => "return",
                DebugFunctionExit::Revert => "revert",
            };
            write!(f, "{separator}exit={exit}")?;
            separator = ", ";
        }
        if metadata.keep_with_next {
            write!(f, "{separator}keep_with_next")?;
        }
        write!(f, ")")?;
        Ok(())
    })
}

fn display_push_value<'a>(module: &'a Module, value: &'a PushValue) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| match value {
        PushValue::Immediate(value) => {
            write!(f, "{}", display_u256(*value))
        }
        PushValue::Library(library) => {
            write!(f, "{}", module.libraries.display_ref(*library))
        }
        PushValue::Block(block) => write!(f, "{}", display_block_id(module, *block)),
        PushValue::Data(data) => write!(
            f,
            "{}",
            display_data_ref(module.data[data.id].name, data.id.index(), data.offset,)
        ),
        PushValue::DataSize(size) => {
            let name = module.data[size.data].name;
            write!(f, "{}", display_data_size(name, *size))
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

//! Display implementations for MIR.
//!
//! Includes DOT format CFG generation for visualization.

use super::{
    BasicBlock, BlockId, EffectKind, Function, InstId, InstKind, Instruction, MemoryRegion,
    StorageAlias, Terminator, Value, ValueId,
};
use smallvec::SmallVec;
use solar_data_structures::fmt::{self, FmtIteratorExt};
use solar_sema::hir;

/// Displays a DOT format CFG for a function.
pub(crate) fn display_function_dot(func: &Function) -> impl fmt::Display + '_ {
    fn display_dot_node(func: &Function, block_id: BlockId) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| {
            let block_idx = block_id.index();
            let is_entry = block_id == func.entry_block;
            let color = if is_entry { ", fillcolor=\"#e0ffe0\", style=filled" } else { "" };

            write!(f, "    bb{block_idx} [label=\"")?;
            write_dot_block_label(f, func, block_id)?;
            writeln!(f, "\"{color}];")
        })
    }

    fn write_dot_block_label(
        f: &mut fmt::Formatter<'_>,
        func: &Function,
        block_id: BlockId,
    ) -> fmt::Result {
        let block = &func.blocks[block_id];
        let block_idx = block_id.index();
        let entry_marker = if block_id == func.entry_block { " (entry)" } else { "" };
        write!(f, "bb{block_idx}{entry_marker}:\\l")?;

        write!(
            f,
            "{}",
            block.instructions.iter().format_with("", |f, inst_id| write!(
                f,
                "{}",
                display_dot_instruction(func, *inst_id)
            ))
        )?;

        if let Some(term) = &block.terminator {
            write!(f, "  {}\\l", display_terminator(term, func))?;
        }

        Ok(())
    }

    fn display_dot_instruction(func: &Function, inst_id: InstId) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| {
            let inst = &func.instructions[inst_id];

            write!(f, "  ")?;
            if inst.result_ty.is_some()
                && let Some(vid) = inst_def_value(func, inst_id)
            {
                write!(f, "v{} = ", vid.index())?;
            }
            write!(f, "{}\\l", display_inst_kind(&inst.kind(), func))
        })
    }

    fn display_dot_edges(block_id: BlockId, block: &BasicBlock) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| {
            let block_idx = block_id.index();
            let Some(term) = &block.terminator else { return Ok(()) };

            match term {
                Terminator::Jump(target) => {
                    writeln!(f, "    bb{} -> bb{};", block_idx, target.index())
                }
                Terminator::Branch { condition, then_block, else_block } => {
                    writeln!(
                        f,
                        "    bb{} -> bb{} [label=\"v{} == true\", color=\"green\"];",
                        block_idx,
                        then_block.index(),
                        condition.index()
                    )?;
                    writeln!(
                        f,
                        "    bb{} -> bb{} [label=\"false\", color=\"red\"];",
                        block_idx,
                        else_block.index()
                    )
                }
                Terminator::Switch { value: _, default, cases } => {
                    writeln!(
                        f,
                        "    bb{} -> bb{} [label=\"default\"];",
                        block_idx,
                        default.index()
                    )?;
                    write!(
                        f,
                        "{}",
                        cases.iter().format_with("", |f, (case_val, target)| {
                            writeln!(
                                f,
                                "    bb{} -> bb{} [label=\"v{}\"];",
                                block_idx,
                                target.index(),
                                case_val.index()
                            )
                        })
                    )
                }
                Terminator::Return { .. }
                | Terminator::Revert { .. }
                | Terminator::ReturnData { .. }
                | Terminator::Stop
                | Terminator::SelfDestruct { .. }
                | Terminator::Invalid => Ok(()),
            }
        })
    }

    fmt::from_fn(move |f| {
        writeln!(f, "digraph \"{}\" {{", func.name)?;
        writeln!(f, "    node [shape=box, fontname=\"Courier\", fontsize=10];")?;
        writeln!(f, "    edge [fontname=\"Courier\", fontsize=9];")?;
        writeln!(f)?;

        write!(
            f,
            "{}",
            func.blocks.iter_enumerated().format_with("", |f, (block_id, _)| write!(
                f,
                "{}",
                display_dot_node(func, block_id)
            ))
        )?;

        writeln!(f)?;

        write!(
            f,
            "{}",
            func.blocks.iter_enumerated().format_with("", |f, (block_id, block)| write!(
                f,
                "{}",
                display_dot_edges(block_id, block)
            ))
        )?;

        writeln!(f, "}}")
    })
}

/// Displays a human-readable textual MIR representation of a function.
///
/// The format is designed for diffing and FileCheck-style pattern matching:
/// ```text
/// fn @name(arg0: uint256, arg1: bool) -> uint256 {
///   bb0:
///     v2 = add arg0, 1
///     br arg1, bb1, bb2
///   bb1:
///     ret v2
///   bb2:
///     ret arg0
/// }
/// ```
pub(crate) fn display_function_text(func: &Function) -> impl fmt::Display + '_ {
    fn display_text_block<'a>(
        func: &'a Function,
        block_id: BlockId,
        block: &'a BasicBlock,
    ) -> impl fmt::Display + 'a {
        fmt::from_fn(move |f| {
            let entry_marker = if block_id == func.entry_block { " (entry)" } else { "" };
            writeln!(f, "  bb{}{}:", block_id.index(), entry_marker)?;

            write!(
                f,
                "{}",
                block.instructions.iter().format_with("", |f, inst_id| write!(
                    f,
                    "{}",
                    display_text_instruction(func, *inst_id)
                ))
            )?;

            if let Some(term) = &block.terminator {
                writeln!(f, "    {}", display_terminator(term, func))?;
            }
            Ok(())
        })
    }

    fn display_text_instruction(func: &Function, inst_id: InstId) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| {
            let inst = &func.instructions[inst_id];

            write!(f, "    ")?;
            if inst.result_ty.is_some()
                && let Some(vid) = inst_def_value(func, inst_id)
            {
                write!(f, "v{} = ", vid.index())?;
            }
            writeln!(f, "{}{}", display_inst_kind(&inst.kind(), func), display_metadata(inst, func))
        })
    }

    fmt::from_fn(move |f| {
        // Header: fn @name(params) -> returns
        write!(f, "fn @{}(", func.name)?;
        write!(
            f,
            "{}",
            func.params
                .iter()
                .enumerate()
                .format_with(", ", |f, (i, ty)| write!(f, "arg{i}: {ty}"))
        )?;
        write!(f, ")")?;
        if function_prints_return_values(func) && !func.returns.is_empty() {
            write!(f, " -> ")?;
            if func.returns.len() == 1 {
                write!(f, "{}", func.returns[0])?;
            } else {
                write!(f, "({})", func.returns.iter().format(", "))?;
            }
        }
        writeln!(f, " {{")?;

        write!(
            f,
            "{}",
            func.blocks.iter_enumerated().format_with("", |f, (block_id, block)| {
                write!(f, "{}", display_text_block(func, block_id, block))
            })
        )?;

        writeln!(f, "}}")
    })
}

fn inst_def_value(func: &Function, inst_id: InstId) -> Option<ValueId> {
    func.values
        .iter_enumerated()
        .find(|(_, v)| matches!(v, Value::Inst(id) if *id == inst_id))
        .map(|(vid, _)| vid)
}

fn function_prints_return_values(func: &Function) -> bool {
    func.blocks.iter().any(|block| matches!(block.terminator, Some(Terminator::Return { .. })))
}

/// Formats an instruction kind for display.
fn display_inst_kind<'a>(kind: &'a InstKind, func: &'a Function) -> impl fmt::Display + 'a {
    fn display_inst_operands(
        f: &mut fmt::Formatter<'_>,
        kind: &InstKind,
        func: &Function,
    ) -> fmt::Result {
        write!(f, "{}", kind.mnemonic())?;
        let operands = kind.operands();
        if !operands.is_empty() {
            write!(
                f,
                " {}",
                operands.into_iter().map(|operand| display_val(operand, func)).format(", ")
            )?;
        }
        Ok(())
    }

    fmt::from_fn(move |f| match kind {
        InstKind::LoadImmutable(offset) => write!(f, "loadimmutable {offset}"),
        InstKind::InternalCall { function, args, returns } => {
            write!(f, "internal_call fn{}, {returns}", function.index())?;
            if !args.is_empty() {
                write!(f, ", {}", args.iter().map(|arg| display_val(*arg, func)).format(", "))?;
            }
            Ok(())
        }
        InstKind::InternalFrameAddr(offset) => write!(f, "internal_frame_addr {offset}"),
        InstKind::Phi(args) => {
            write!(f, "phi")?;
            if !args.is_empty() {
                write!(
                    f,
                    " {}",
                    args.iter().format_with(", ", |f, (block, val)| {
                        write!(f, "[bb{}: {}]", block.index(), display_val(*val, func))
                    })
                )?;
            }
            Ok(())
        }
        _ => display_inst_operands(f, kind, func),
    })
}

/// Format a value reference.
fn display_val(vid: ValueId, func: &Function) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| match &func.values[vid] {
        Value::Immediate(imm) if let Some(u256) = imm.as_u256() => {
            write!(f, "{}", display_u256(u256))
        }
        Value::Arg { index, .. } => write!(f, "arg{index}"),
        _ => write!(f, "v{}", vid.index()),
    })
}

fn display_u256(value: alloy_primitives::U256) -> impl fmt::Display {
    fmt::from_fn(move |f| {
        if let Ok(x) = u64::try_from(value)
            && x < 1000
        {
            write!(f, "{x}")
        } else {
            write!(f, "{value:#x}")
        }
    })
}

fn display_metadata<'a>(inst: &'a Instruction, func: &'a Function) -> impl fmt::Display + 'a {
    enum MetadataField<'a> {
        Storage(StorageAlias, &'a Function),
        Memory(MemoryRegion),
        Hir(hir::ExprId),
        Span { lo: u32, hi: u32 },
        Unchecked,
        LoopDepth(u16),
        Effect(EffectKind),
    }

    fn display_metadata_field(field: MetadataField<'_>) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| match field {
            MetadataField::Storage(storage, func) => {
                write!(f, "storage={}", display_storage_alias(storage, func))
            }
            MetadataField::Memory(memory) => write!(f, "memory={}", memory.name()),
            MetadataField::Hir(hir_expr) => write!(f, "hir={}", hir_expr.index()),
            MetadataField::Span { lo, hi } => write!(f, "span={lo}..{hi}"),
            MetadataField::Unchecked => write!(f, "unchecked"),
            MetadataField::LoopDepth(loop_depth) => write!(f, "loop_depth={loop_depth}"),
            MetadataField::Effect(effect) => write!(f, "effect={}", effect.name()),
        })
    }

    fn display_storage_alias(alias: StorageAlias, func: &Function) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| match alias {
            StorageAlias::Slot(slot) => write!(f, "slot({})", display_u256(slot)),
            StorageAlias::Symbolic(value) => write!(f, "symbolic({})", display_val(value, func)),
            StorageAlias::Offset { base, offset } => {
                write!(f, "offset({}, {})", display_val(base, func), display_u256(offset))
            }
        })
    }

    fmt::from_fn(move |f| {
        let metadata = &inst.metadata;
        let mut fields = SmallVec::<[MetadataField<'_>; 8]>::new();

        if let Some(storage) = metadata.storage_alias() {
            fields.push(MetadataField::Storage(storage, func));
        }
        if let Some(memory) = metadata.memory_region()
            && memory != MemoryRegion::Unknown
        {
            fields.push(MetadataField::Memory(memory));
        }
        if let Some(hir_expr) = metadata.hir_expr() {
            fields.push(MetadataField::Hir(hir_expr));
        }
        if let Some(span) = metadata.source_span() {
            fields.push(MetadataField::Span { lo: span.lo().0, hi: span.hi().0 });
        }
        if metadata.unchecked() {
            fields.push(MetadataField::Unchecked);
        }
        if metadata.loop_depth != 0 {
            fields.push(MetadataField::LoopDepth(metadata.loop_depth));
        }
        if let Some(effect) = metadata.effect()
            && effect != inst.kind.effect_kind()
        {
            fields.push(MetadataField::Effect(effect));
        }

        if fields.is_empty() {
            Ok(())
        } else {
            write!(f, " !metadata({})", fields.into_iter().map(display_metadata_field).format(", "))
        }
    })
}

/// Format a terminator for display, rendering operands via [`display_val`].
fn display_terminator<'a>(term: &'a Terminator, func: &'a Function) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| match term {
        Terminator::Jump(target) => write!(f, "jump bb{}", target.index()),
        Terminator::Branch { condition, then_block, else_block } => write!(
            f,
            "br {}, bb{}, bb{}",
            display_val(*condition, func),
            then_block.index(),
            else_block.index()
        ),
        Terminator::Switch { value, default, cases } => {
            write!(f, "switch {}, default bb{}, [", display_val(*value, func), default.index())?;
            write!(
                f,
                "{}",
                cases.iter().format_with(", ", |f, (val, block)| {
                    write!(f, "{} => bb{}", display_val(*val, func), block.index())
                })
            )?;
            write!(f, "]")
        }
        Terminator::Return { values } => {
            write!(f, "ret")?;
            if !values.is_empty() {
                write!(
                    f,
                    " {}",
                    values.iter().map(|value| display_val(*value, func)).format(", ")
                )?;
            }
            Ok(())
        }
        Terminator::Revert { offset, size } => {
            write!(f, "revert {}, {}", display_val(*offset, func), display_val(*size, func))
        }
        Terminator::ReturnData { offset, size } => {
            write!(f, "returndata {}, {}", display_val(*offset, func), display_val(*size, func))
        }
        Terminator::Stop => write!(f, "stop"),
        Terminator::SelfDestruct { recipient } => {
            write!(f, "selfdestruct {}", display_val(*recipient, func))
        }
        Terminator::Invalid => write!(f, "invalid"),
    })
}

#[cfg(test)]
mod tests {
    use crate::mir::{Function, FunctionBuilder, MirType};
    use snapbox::{IntoData as _, assert_data_eq, str};
    use solar_interface::{ColorChoice, Ident, Session, sym};

    fn make_func() -> Function {
        Function::new(Ident::with_dummy_span(sym::display_test))
    }

    /// Runs `f` inside a fresh test session so the symbol interner is available.
    fn with_session<F: FnOnce() + Send>(f: F) {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.enter(f);
    }

    #[test]
    fn text_linear_function() {
        with_session(|| {
            let mut func = make_func();
            {
                let mut b = FunctionBuilder::new(&mut func);
                let x = b.add_param(MirType::uint256());
                b.add_return(MirType::uint256());
                let one = b.imm_u64(1);
                let sum = b.add(x, one);
                b.ret([sum]);
            }
            let text = func.to_text().to_string();
            assert_data_eq!(
                text,
                str![[r#"
fn @display_test(arg0: u256) -> u256 {
  bb0 (entry):
    v2 = add arg0, 1
    ret v2
}

"#]]
            );
            let dot = func.to_dot().to_string();
            assert_data_eq!(
                dot,
                str![[r##"
digraph "display_test" {
    node [shape=box, fontname="Courier", fontsize=10];
    edge [fontname="Courier", fontsize=9];

    bb0 [label="bb0 (entry):\l  v2 = add arg0, 1\l  ret v2\l", fillcolor="#e0ffe0", style=filled];

}

"##]]
                .raw()
            );
        });
    }

    #[test]
    fn text_diamond_cfg() {
        with_session(|| {
            let mut func = make_func();
            {
                let mut b = FunctionBuilder::new(&mut func);
                let x = b.add_param(MirType::uint256());
                let cond = b.add_param(MirType::Bool);
                let then_bb = b.create_block();
                let else_bb = b.create_block();
                b.branch(cond, then_bb, else_bb);
                b.switch_to_block(then_bb);
                b.ret([x]);
                b.switch_to_block(else_bb);
                b.ret([x]);
            }
            let text = func.to_text().to_string();
            assert_data_eq!(
                text,
                str![[r#"
fn @display_test(arg0: u256, arg1: bool) {
  bb0 (entry):
    br arg1, bb1, bb2
  bb1:
    ret arg0
  bb2:
    ret arg0
}

"#]]
            );
            let dot = func.to_dot().to_string();
            assert_data_eq!(
                dot,
                str![[r##"
digraph "display_test" {
    node [shape=box, fontname="Courier", fontsize=10];
    edge [fontname="Courier", fontsize=9];

    bb0 [label="bb0 (entry):\l  br arg1, bb1, bb2\l", fillcolor="#e0ffe0", style=filled];
    bb1 [label="bb1:\l  ret arg0\l"];
    bb2 [label="bb2:\l  ret arg0\l"];

    bb0 -> bb1 [label="v1 == true", color="green"];
    bb0 -> bb2 [label="false", color="red"];
}

"##]]
                .raw()
            );
        });
    }

    #[test]
    fn text_storage_ops() {
        with_session(|| {
            let mut func = make_func();
            {
                let mut b = FunctionBuilder::new(&mut func);
                let slot = b.add_param(MirType::uint256());
                let val = b.add_param(MirType::uint256());
                b.sstore(slot, val);
                let loaded = b.sload(slot);
                b.ret([loaded]);
            }
            let text = func.to_text().to_string();
            assert_data_eq!(
                text,
                str![[r#"
fn @display_test(arg0: u256, arg1: u256) {
  bb0 (entry):
    sstore arg0, arg1 !metadata(storage=symbolic(arg0))
    v3 = sload arg0 !metadata(storage=symbolic(arg0))
    ret v3
}

"#]]
            );
            let dot = func.to_dot().to_string();
            assert_data_eq!(
                dot,
                str![[r##"
digraph "display_test" {
    node [shape=box, fontname="Courier", fontsize=10];
    edge [fontname="Courier", fontsize=9];

    bb0 [label="bb0 (entry):\l  sstore arg0, arg1\l  v3 = sload arg0\l  ret v3\l", fillcolor="#e0ffe0", style=filled];

}

"##]]
                .raw()
            );
        });
    }
}

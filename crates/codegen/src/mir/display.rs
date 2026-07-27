//! Display implementations for MIR.
//!
//! Includes DOT format CFG generation for visualization.

use super::{
    BasicBlock, BlockId, EffectKind, Function, FunctionId, InstId, InstKind, Instruction,
    MemoryRegion, StorageAlias, Terminator, Value, ValueId,
};
use arrayvec::ArrayVec;
use solar_data_structures::fmt::{self, FmtIteratorExt};
use solar_sema::hir;

/// Displays a DOT format CFG for a function.
pub(crate) fn display_function_dot<'a>(
    func: &'a Function,
    funcs: Option<&'a solar_data_structures::index::IndexVec<FunctionId, Function>>,
) -> impl fmt::Display + 'a {
    fn display_dot_node<'a>(
        func: &'a Function,
        funcs: Option<&'a solar_data_structures::index::IndexVec<FunctionId, Function>>,
        block_id: BlockId,
    ) -> impl fmt::Display + 'a {
        fmt::from_fn(move |f| {
            let block_idx = block_id.index();

            write!(f, "    bb{block_idx} [label=\"")?;
            write_dot_block_label(f, func, funcs, block_id)?;
            writeln!(f, "\"];")
        })
    }

    fn write_dot_block_label(
        f: &mut fmt::Formatter<'_>,
        func: &Function,
        funcs: Option<&solar_data_structures::index::IndexVec<FunctionId, Function>>,
        block_id: BlockId,
    ) -> fmt::Result {
        let block = &func.blocks[block_id];
        let block_idx = block_id.index();
        write!(f, "bb{block_idx}:\\l")?;

        write!(
            f,
            "{}",
            block.instructions.iter().format_with("", |f, inst_id| write!(
                f,
                "{}",
                display_dot_instruction(func, funcs, *inst_id)
            ))
        )?;

        if let Some(term) = &block.terminator {
            write!(f, "  {}\\l", display_terminator(term, func, funcs))?;
        }

        Ok(())
    }

    fn display_dot_instruction<'a>(
        func: &'a Function,
        funcs: Option<&'a solar_data_structures::index::IndexVec<FunctionId, Function>>,
        inst_id: InstId,
    ) -> impl fmt::Display + 'a {
        fmt::from_fn(move |f| {
            let inst = func.inst(inst_id);

            write!(f, "  ")?;
            if inst.result_ty.is_some() {
                write!(f, "v{} = ", inst_result_index(func, inst_id))?;
            }
            write!(f, "{}\\l", display_inst_kind(&inst.kind, func, funcs))
        })
    }

    fn display_dot_edges<'a>(
        func: &'a Function,
        funcs: Option<&'a solar_data_structures::index::IndexVec<FunctionId, Function>>,
        block_id: BlockId,
        block: &'a BasicBlock,
    ) -> impl fmt::Display + 'a {
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
                        "    bb{} -> bb{} [label=\"{} == true\", color=\"green\"];",
                        block_idx,
                        then_block.index(),
                        display_val(*condition, func)
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
                                "    bb{} -> bb{} [label=\"{}\"];",
                                block_idx,
                                target.index(),
                                display_val(*case_val, func)
                            )
                        })
                    )
                }
                Terminator::TailCall { function, .. } => {
                    writeln!(
                        f,
                        "    bb{} -> fn{} [style=dashed, label=\"tail_call {}\"];",
                        block_idx,
                        function.index(),
                        display_function_ref(*function, funcs)
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
                display_dot_node(func, funcs, block_id)
            ))
        )?;

        writeln!(f)?;

        write!(
            f,
            "{}",
            func.blocks.iter_enumerated().format_with("", |f, (block_id, block)| {
                write!(f, "{}", display_dot_edges(func, funcs, block_id, block))
            })
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
///     v0 = add arg0, 1
///     jumpi arg1, bb1, bb2
///   bb1:
///     ret v0
///   bb2:
///     ret arg0
/// }
/// ```
pub(crate) fn display_function_text<'a>(
    func: &'a Function,
    funcs: Option<&'a solar_data_structures::index::IndexVec<FunctionId, Function>>,
) -> impl fmt::Display + 'a {
    fn display_text_block<'a>(
        func: &'a Function,
        funcs: Option<&'a solar_data_structures::index::IndexVec<FunctionId, Function>>,
        block_id: BlockId,
        block: &'a BasicBlock,
    ) -> impl fmt::Display + 'a {
        fmt::from_fn(move |f| {
            writeln!(f, "  bb{}:", block_id.index())?;

            write!(
                f,
                "{}",
                block.instructions.iter().format_with("", |f, inst_id| write!(
                    f,
                    "{}",
                    display_text_instruction(func, funcs, *inst_id)
                ))
            )?;

            if let Some(term) = &block.terminator {
                writeln!(f, "    {}", display_terminator(term, func, funcs))?;
            }
            Ok(())
        })
    }

    fn display_text_instruction<'a>(
        func: &'a Function,
        funcs: Option<&'a solar_data_structures::index::IndexVec<FunctionId, Function>>,
        inst_id: InstId,
    ) -> impl fmt::Display + 'a {
        fmt::from_fn(move |f| {
            let inst = func.inst(inst_id);

            write!(f, "    ")?;
            if inst.result_ty.is_some() {
                write!(f, "v{} = ", inst_result_index(func, inst_id))?;
            }
            writeln!(
                f,
                "{}{}",
                display_inst_kind(&inst.kind, func, funcs),
                display_metadata(inst, func)
            )
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
                write!(f, "{}", display_text_block(func, funcs, block_id, block))
            })
        )?;

        writeln!(f, "}}")
    })
}

fn function_prints_return_values(func: &Function) -> bool {
    func.blocks.iter().any(|block| matches!(block.terminator, Some(Terminator::Return { .. })))
}

fn inst_result_index(func: &Function, inst_id: InstId) -> usize {
    func.inst_result_index(inst_id)
        .expect("Value::Inst should point to a value-producing instruction")
}

/// Formats an instruction kind for display.
fn display_inst_kind<'a>(
    kind: &'a InstKind,
    func: &'a Function,
    funcs: Option<&'a solar_data_structures::index::IndexVec<FunctionId, Function>>,
) -> impl fmt::Display + 'a {
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
        InstKind::Alloc { size, kind, semantics } => {
            let kind = match kind {
                crate::mir::AllocationKind::Raw => "raw".to_string(),
                crate::mir::AllocationKind::Object(layout) => layout.to_string(),
            };
            let alignment = match semantics.alignment {
                crate::mir::AllocationAlignment::Exact => "exact",
                crate::mir::AllocationAlignment::Word => "word",
            };
            let initialization = match semantics.initialization {
                crate::mir::AllocationInitialization::Uninitialized => "uninitialized",
                crate::mir::AllocationInitialization::Zeroed => "zeroed",
            };
            let failure = match semantics.failure {
                crate::mir::AllocationFailure::Infallible => "infallible",
                crate::mir::AllocationFailure::Panic => "panic",
            };
            write!(
                f,
                "alloc {kind}, {alignment}, {initialization}, {failure}, {}",
                display_val(*size, func)
            )
        }
        InstKind::MemoryObjectFieldAddr { object, layout, field } => {
            write!(f, "memory_object_field_addr {layout}, {}, {field}", display_val(*object, func))
        }
        InstKind::MemoryObjectElementAddr { object, layout, index } => write!(
            f,
            "memory_object_element_addr {layout}, {}, {}",
            display_val(*object, func),
            display_val(*index, func)
        ),
        InstKind::MemoryObjectLen(object, kind) => {
            write!(f, "memory_object_len {kind}, {}", display_val(*object, func))
        }
        InstKind::SetMemoryObjectLen(object, len, kind) => write!(
            f,
            "set_memory_object_len {kind}, {}, {}",
            display_val(*object, func),
            display_val(*len, func)
        ),
        InstKind::MemoryObjectData(object, kind) => {
            write!(f, "memory_object_data {kind}, {}", display_val(*object, func))
        }
        InstKind::AbiEncode { selector, args, layout } => {
            write!(f, "abi_encode {layout}")?;
            if let Some(selector) = selector {
                write!(f, ", selector {}", display_val(*selector, func))?;
            }
            if !args.is_empty() {
                write!(f, ", args ")?;
                write!(f, "{}", args.iter().map(|arg| display_val(*arg, func)).format(", "))?;
            }
            Ok(())
        }
        InstKind::StorageToMemory { storage, memory, layout } => write!(
            f,
            "storage_to_memory {layout}, {}, {}",
            display_val(*storage, func),
            display_val(*memory, func)
        ),
        InstKind::MemoryToStorage { memory, storage, layout } => write!(
            f,
            "memory_to_storage {layout}, {}, {}",
            display_val(*memory, func),
            display_val(*storage, func)
        ),
        InstKind::ClearStorage { storage, layout } => {
            write!(f, "clear_storage {layout}, {}", display_val(*storage, func))
        }
        InstKind::InternalCall { function, args, returns } => {
            write!(f, "internal_call {}, {returns}", display_function_ref(*function, funcs))?;
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
/// Formats a function reference as `@name` when the module's functions are
/// available (module-level printing), falling back to the positional `fnN`
/// form when they are not (single-function display has no name table) or when
/// the name is shared by an overload and would not round-trip unambiguously.
fn display_function_ref(
    function: FunctionId,
    funcs: Option<&solar_data_structures::index::IndexVec<FunctionId, Function>>,
) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| {
        if let Some(funcs) = funcs
            && let Some(callee) = funcs.get(function)
            && funcs.iter().filter(|other| other.name == callee.name).count() == 1
        {
            write!(f, "@{}", callee.name)
        } else {
            write!(f, "fn{}", function.index())
        }
    })
}

fn display_val(vid: ValueId, func: &Function) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| match &func.values[vid] {
        Value::Immediate(imm) if let Some(u256) = imm.as_u256() => {
            write!(f, "{}", display_u256(u256))
        }
        Value::Arg { index, .. } => write!(f, "arg{index}"),
        Value::Inst(inst_id) => write!(f, "v{}", inst_result_index(func, *inst_id)),
        Value::Error(_) => write!(f, "err"),
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
        DeferredAlloc,
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
            MetadataField::DeferredAlloc => write!(f, "deferred_alloc"),
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
        let mut fields = ArrayVec::<MetadataField<'_>, 8>::new();

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
        if metadata.deferred_alloc() {
            fields.push(MetadataField::DeferredAlloc);
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
fn display_terminator<'a>(
    term: &'a Terminator,
    func: &'a Function,
    funcs: Option<&'a solar_data_structures::index::IndexVec<FunctionId, Function>>,
) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| match term {
        Terminator::Jump(target) => write!(f, "jump bb{}", target.index()),
        Terminator::Branch { condition, then_block, else_block } => write!(
            f,
            "jumpi {}, bb{}, bb{}",
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
        Terminator::TailCall { function, args } => {
            write!(f, "tail_call {}", display_function_ref(*function, funcs))?;
            for arg in args {
                write!(f, ", {}", display_val(*arg, func))?;
            }
            Ok(())
        }
        Terminator::SelfDestruct { recipient } => {
            write!(f, "selfdestruct {}", display_val(*recipient, func))
        }
        Terminator::Invalid => write!(f, "invalid"),
    })
}

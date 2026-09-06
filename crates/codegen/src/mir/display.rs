//! Display implementations for MIR.
//!
//! Includes DOT format CFG generation for visualization.

use super::{
    BasicBlock, BlockId, EffectKind, FrameMode, FrameSlotKind, Function, FunctionId, InstId,
    InstKind, InstructionMetadata, MemoryRegion, Module, StorageAlias, Terminator, Value, ValueId,
};
use crate::analysis::CfgInfo;
use arrayvec::ArrayVec;
use solar_data_structures::{
    fmt::{self, FmtIteratorExt},
    map::{FxHashMap, FxHashSet},
};
use solar_sema::hir;

/// Displays a DOT format CFG for a function.
pub(crate) fn display_function_dot<'a>(
    func: &'a Function,
    module: Option<&'a Module>,
) -> impl fmt::Display + 'a {
    fn display_dot_node<'a>(
        func: &'a Function,
        module: Option<&'a Module>,
        block_id: BlockId,
    ) -> impl fmt::Display + 'a {
        fmt::from_fn(move |f| {
            let block_idx = block_id.index();

            write!(f, "    bb{block_idx} [label=\"")?;
            write_dot_block_label(f, func, module, block_id)?;
            writeln!(f, "\"];")
        })
    }

    fn write_dot_block_label(
        f: &mut fmt::Formatter<'_>,
        func: &Function,
        module: Option<&Module>,
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
                display_dot_instruction(func, module, *inst_id)
            ))
        )?;

        if let Some(term) = &block.terminator {
            write!(f, "  {}\\l", display_terminator(term, func, module))?;
        }

        Ok(())
    }

    fn display_dot_instruction<'a>(
        func: &'a Function,
        module: Option<&'a Module>,
        inst_id: InstId,
    ) -> impl fmt::Display + 'a {
        fmt::from_fn(move |f| {
            let inst = func.inst(inst_id);

            write!(f, "  ")?;
            if inst.result_ty.is_some() {
                write!(f, "v{} = ", inst_result_index(func, inst_id))?;
            }
            write!(f, "{}\\l", display_inst_kind(&inst.kind, func, module))
        })
    }

    fn display_dot_edges<'a>(
        func: &'a Function,
        module: Option<&'a Module>,
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
                        display_function_ref(*function, module)
                    )
                }
                Terminator::Return { .. }
                | Terminator::Revert { .. }
                | Terminator::RevertReturndata
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
                display_dot_node(func, module, block_id)
            ))
        )?;

        writeln!(f)?;

        write!(
            f,
            "{}",
            func.blocks.iter_enumerated().format_with("", |f, (block_id, block)| {
                write!(f, "{}", display_dot_edges(func, module, block_id, block))
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
    module: Option<&'a Module>,
    is_dispatch_entry: bool,
) -> impl fmt::Display + 'a {
    fn display_text_block<'a>(
        func: &'a Function,
        module: Option<&'a Module>,
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
                    display_text_instruction(func, module, *inst_id)
                ))
            )?;

            if let Some(term) = &block.terminator {
                writeln!(
                    f,
                    "    {}{}",
                    display_terminator(term, func, module),
                    display_metadata(&block.terminator_metadata, None, func)
                )?;
            }
            Ok(())
        })
    }

    fn display_text_instruction<'a>(
        func: &'a Function,
        module: Option<&'a Module>,
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
                display_inst_kind(&inst.kind, func, module),
                display_metadata(&inst.metadata, Some(inst.kind.effect_kind()), func)
            )
        })
    }

    fmt::from_fn(move |f| {
        // Header: fn @name(params) -> returns
        write!(f, "fn @{}(", func.name)?;
        write!(
            f,
            "{}",
            func.params.iter_enumerated().format_with(", ", |f, (i, ty)| write!(
                f,
                "arg{}: {ty}",
                i.index()
            ))
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
        write!(f, "{}", display_function_attributes(func, is_dispatch_entry))?;
        writeln!(f, " {{")?;

        let cfg = CfgInfo::new(func);
        for &block_id in cfg.rpo() {
            write!(f, "{}", display_text_block(func, module, block_id, &func.blocks[block_id]))?;
        }
        for (block_id, block) in func.blocks.iter_enumerated() {
            if !cfg.is_reachable(block_id) {
                write!(f, "{}", display_text_block(func, module, block_id, block))?;
            }
        }

        writeln!(f, "}}")
    })
}

fn display_function_attributes(func: &Function, is_dispatch_entry: bool) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| {
        let mut first = true;
        if let Some(selector) = func.selector {
            write_function_attribute(
                f,
                &mut first,
                format_args!("selector=0x{:08x}", u32::from_be_bytes(selector)),
            )?;
        }
        if func.attributes.is_constructor {
            write_function_attribute(f, &mut first, "constructor")?;
        }
        if is_dispatch_entry {
            write_function_attribute(f, &mut first, "entry")?;
        }
        if func.attributes.may_return_memory {
            write_function_attribute(f, &mut first, "may_return_memory")?;
        }
        if func.attributes.is_function_pointer_dispatcher {
            write_function_attribute(f, &mut first, "function_pointer_dispatcher")?;
        }
        if func.attributes.is_receive {
            write_function_attribute(f, &mut first, "receive")?;
        }
        if func.attributes.is_fallback {
            write_function_attribute(f, &mut first, "fallback")?;
        }
        match func.attributes.state_mutability {
            hir::StateMutability::Pure => write_function_attribute(f, &mut first, "pure")?,
            hir::StateMutability::View => write_function_attribute(f, &mut first, "view")?,
            hir::StateMutability::Payable => write_function_attribute(f, &mut first, "payable")?,
            hir::StateMutability::NonPayable => {}
        }
        if let Some(layout) = &func.abi_params {
            write_function_attribute(f, &mut first, format_args!("abi_params={layout}"))?;
        }
        if let Some(layout) = &func.abi_returns {
            write_function_attribute(f, &mut first, format_args!("abi_returns={layout}"))?;
        }
        if let Some(layout) = &func.abi_return_params {
            write_function_attribute(f, &mut first, format_args!("abi_return_params={layout}"))?;
        }
        if !first {
            f.write_str("]")?;
        }
        Ok(())
    })
}

fn write_function_attribute(
    f: &mut fmt::Formatter<'_>,
    first: &mut bool,
    attribute: impl fmt::Display,
) -> fmt::Result {
    if *first {
        f.write_str(" [")?;
        *first = false;
    } else {
        f.write_str(", ")?;
    }
    attribute.fmt(f)
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
    module: Option<&'a Module>,
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
        InstKind::StoreImmutable(id, value) => {
            write!(f, "storeimmutable {}", display_immutable_ref(*id, module))?;
            write!(f, ", {}", display_val(*value, func))
        }
        InstKind::LoadImmutable(id) => {
            write!(f, "loadimmutable {}", display_immutable_ref(*id, module))
        }
        InstKind::DataCopy(id, dest, size) => {
            let name = module.and_then(|module| module.data_name(id.id));
            write!(
                f,
                "data_copy {}",
                crate::utils::display_data_ref(name, id.id.index(), id.offset)
            )?;
            write!(f, ", {}, {}", display_val(*dest, func), display_val(*size, func))
        }
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
        InstKind::MemoryObjectLoadField { object, layout, field } => {
            write!(f, "memory_object_load_field {layout}, {}, {field}", display_val(*object, func))
        }
        InstKind::MemoryObjectStoreField { object, layout, field, value } => write!(
            f,
            "memory_object_store_field {layout}, {}, {field}, {}",
            display_val(*object, func),
            display_val(*value, func)
        ),
        InstKind::MemoryObjectLoadElement { object, layout, index } => write!(
            f,
            "memory_object_load_element {layout}, {}, {}",
            display_val(*object, func),
            display_val(*index, func)
        ),
        InstKind::MemoryObjectLoadByte { object, index } => write!(
            f,
            "memory_object_load_byte memorybytes, {}, {}",
            display_val(*object, func),
            display_val(*index, func)
        ),
        InstKind::MemoryObjectStoreElement { object, layout, index, value } => write!(
            f,
            "memory_object_store_element {layout}, {}, {}, {}",
            display_val(*object, func),
            display_val(*index, func),
            display_val(*value, func)
        ),
        InstKind::MemoryObjectStoreByte { object, index, value } => write!(
            f,
            "memory_object_store_byte memorybytes, {}, {}, {}",
            display_val(*object, func),
            display_val(*index, func),
            display_val(*value, func)
        ),
        InstKind::MemoryObjectStoreWord { object, offset, value } => write!(
            f,
            "memory_object_store_word memorybytes, {}, {}, {}",
            display_val(*object, func),
            display_val(*offset, func),
            display_val(*value, func)
        ),
        InstKind::MemorySliceLoadWord { slice, offset } => write!(
            f,
            "memory_slice_load_word memory, {}, {}",
            display_val(*slice, func),
            display_val(*offset, func)
        ),
        InstKind::CalldataSliceLoadWord { slice, offset } => write!(
            f,
            "calldata_slice_load_word calldata, {}, {}",
            display_val(*slice, func),
            display_val(*offset, func)
        ),
        InstKind::MemoryObjectCopyFromSlice { object, kind, source } => write!(
            f,
            "memory_object_copy_from_slice {kind}, {}, {}",
            display_val(*object, func),
            display_val(*source, func)
        ),
        InstKind::MemoryObjectCopyFromSliceAt { object, kind, offset, source } => write!(
            f,
            "memory_object_copy_from_slice_at {kind}, {}, {}, {}",
            display_val(*object, func),
            display_val(*offset, func),
            display_val(*source, func)
        ),
        InstKind::MemoryObjectCopy {
            destination,
            destination_kind,
            source,
            source_kind,
            length,
        } => write!(
            f,
            "memory_object_copy {destination_kind}, {}, {source_kind}, {}, {}",
            display_val(*destination, func),
            display_val(*source, func),
            display_val(*length, func)
        ),
        InstKind::StorageArrayElementSlot { slot, index, element_slots } => write!(
            f,
            "storage_array_element_slot {}, {}, {element_slots}",
            display_val(*slot, func),
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
        InstKind::AbiEncode { mode, selector, args, layout } => {
            write!(f, "abi_encode {layout}")?;
            match mode {
                super::AbiEncodeMode::Slice => {}
                super::AbiEncodeMode::Bytes => write!(f, ", object")?,
                super::AbiEncodeMode::Scratch => write!(f, ", scratch")?,
            }
            if let Some(selector) = selector {
                write!(f, ", selector {}", display_val(*selector, func))?;
            }
            if !args.is_empty() {
                write!(f, ", args ")?;
                write!(f, "{}", args.iter().map(|arg| display_val(*arg, func)).format(", "))?;
            }
            Ok(())
        }
        InstKind::AbiDecode { data, layout } => {
            write!(f, "abi_decode {layout}, {}", display_val(*data, func))
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
        InstKind::ICall { function, args, returns } => {
            write!(f, "icall {}, {returns}", display_function_ref(*function, module))?;
            if !args.is_empty() {
                write!(f, ", {}", args.iter().map(|arg| display_val(*arg, func)).format(", "))?;
            }
            Ok(())
        }
        InstKind::InternalFrameAddr(offset) => write!(f, "internal_frame_addr {offset}"),
        InstKind::FrameLoad { offset, mode, kind } => {
            write!(
                f,
                "frame_load {}, {}, {offset}",
                display_frame_mode(*mode),
                display_frame_kind(*kind)
            )
        }
        InstKind::FrameStore { offset, mode, kind, value } => write!(
            f,
            "frame_store {}, {}, {offset}, {}",
            display_frame_mode(*mode),
            display_frame_kind(*kind),
            display_val(*value, func)
        ),
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

fn display_frame_mode(mode: FrameMode) -> &'static str {
    match mode {
        FrameMode::External => "scratch",
        FrameMode::Internal => "internal_frame",
        FrameMode::MultiReturn => "multi_return",
    }
}

fn display_frame_kind(kind: FrameSlotKind) -> impl fmt::Display {
    fmt::from_fn(move |f| match kind {
        FrameSlotKind::Word => f.write_str("word"),
        FrameSlotKind::Slice(location) => write!(f, "{location}"),
    })
}

pub(super) fn display_immutable_ref(
    id: super::ImmutableId,
    module: Option<&Module>,
) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| {
        if let Some(name) = module.and_then(|module| immutable_display_name(module, id)) {
            write!(f, "{name}")
        } else {
            write!(f, "{}", id.index())
        }
    })
}

fn immutable_display_name(module: &Module, id: super::ImmutableId) -> Option<String> {
    let immutable = module.get_immutable(id)?;
    let mut counts = FxHashMap::default();
    let mut reserved = FxHashSet::default();
    for (_, immutable) in module.iter_immutables() {
        *counts.entry(immutable.name.name).or_insert(0usize) += 1;
        reserved.insert(immutable.name.to_string());
    }
    if counts[&immutable.name.name] == 1 {
        return Some(immutable.name.to_string());
    }

    let mut allocated = FxHashSet::default();
    for (other_id, other) in module.iter_immutables() {
        if counts[&other.name.name] == 1 {
            continue;
        }

        let mut name = format!("{}{}", other.name, other_id.index());
        while reserved.contains(&name) || allocated.contains(&name) {
            name.push('_');
        }
        if other_id == id {
            return Some(name);
        }
        allocated.insert(name);
    }
    None
}

/// Formats a function reference using its exact textual declaration name.
/// Falls back to `fnN` when a single function is printed without its module.
fn display_function_ref(function: FunctionId, module: Option<&Module>) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| {
        let funcs = module.map(|module| &module.functions);
        if let Some(funcs) = funcs
            && let Some(callee) = funcs.get(function)
        {
            write!(f, "@{}", callee.name)
        } else {
            write!(f, "fn{}", function.index())
        }
    })
}

fn display_val(vid: ValueId, func: &Function) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| match func.value(vid) {
        Value::Immediate(imm) if let Some(u256) = imm.as_u256() => {
            write!(f, "{}", display_u256(u256))
        }
        Value::Arg(index) => write!(f, "arg{}", index.index()),
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

fn display_metadata<'a>(
    metadata: &'a InstructionMetadata,
    default_effect: Option<EffectKind>,
    func: &'a Function,
) -> impl fmt::Display + 'a {
    enum MetadataField<'a> {
        Storage(StorageAlias, &'a Function),
        Memory(MemoryRegion),
        Hir(hir::ExprId),
        Span { lo: u32, hi: u32 },
        Spans(&'a InstructionMetadata),
        ModifierDepth(u32),
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
            MetadataField::Spans(metadata) => write!(
                f,
                "spans=[{}]",
                metadata.source_spans().format_with(", ", |f, span| write!(
                    f,
                    "{}..{}",
                    span.lo().0,
                    span.hi().0
                )),
            ),
            MetadataField::ModifierDepth(depth) => write!(f, "modifier_depth={depth}"),
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
        let mut fields = ArrayVec::<MetadataField<'_>, 9>::new();

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
        if metadata.displays_source_span()
            && let Some(span) = metadata.source_span()
        {
            if metadata.source_spans().count() == 1 {
                fields.push(MetadataField::Span { lo: span.lo().0, hi: span.hi().0 });
            } else {
                fields.push(MetadataField::Spans(metadata));
            }
            if metadata.modifier_depth() != 0 {
                fields.push(MetadataField::ModifierDepth(metadata.modifier_depth()));
            }
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
            && Some(effect) != default_effect
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
    module: Option<&'a Module>,
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
        Terminator::RevertReturndata => write!(f, "revert_returndata"),
        Terminator::ReturnData { offset, size } => {
            write!(f, "returndata {}, {}", display_val(*offset, func), display_val(*size, func))
        }
        Terminator::Stop => write!(f, "stop"),
        Terminator::TailCall { function, args } => {
            write!(f, "tail_call {}", display_function_ref(*function, module))?;
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

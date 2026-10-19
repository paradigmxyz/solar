//! Lower semantic frame-slot loads and stores to physical memory operations.
//!
//! `frame_load` and `frame_store` name a word or slice slot by its offset within one of three
//! frames. External entries keep their mutable locals at a fixed address from the heap start.
//! Internal functions address theirs through `internal_frame_addr`, past the frame header and the
//! function's argument and return words, so the backend's frame layout decides the final address.
//! A multi-return result reads the buffer pointer from its reserved slot. A slice slot holds two
//! words, its pointer and then its length, and becomes two loads or stores around a
//! `make_slice` or slice projections for `lower-slices` to fold; multi-return buffers hold only
//! words.
//!
//! The pass runs after ABI, dispatch, and struct lowering, once signatures and the argument count
//! that internal frame offsets depend on are final, and before mapping-slot and memory-object
//! lowering.

use crate::mir::{
    FrameMode, FrameSlotKind, Function, FunctionBuilder, InstKind, Module, PtrLocation,
    memory::EvmMemoryLayout, pass::MirPass,
};
use solar_data_structures::map::FxHashMap;
use solar_sema::Gcx;

/// Lowers logical mutable-local slots after ABI and dispatch lowering.
pub(crate) struct LowerFrameSlots;

impl MirPass for LowerFrameSlots {
    fn name(&self) -> &'static str {
        "lower-frame-slots"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        _gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let mut changed = false;
        for func in &mut module.functions {
            changed |= lower_function(func);
        }
        changed
    }
}

fn lower_function(func: &mut Function) -> bool {
    if !func.instructions().any(|inst| {
        matches!(func.inst(inst).kind, InstKind::FrameLoad { .. } | InstKind::FrameStore { .. })
    }) {
        return false;
    }

    let mut replacements = FxHashMap::default();
    let blocks = func.blocks.indices();

    for block in blocks {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);

        for inst in instructions {
            match builder.func().inst(inst).kind.clone() {
                InstKind::FrameLoad { offset, mode, kind } => {
                    let address = frame_address(&mut builder, offset, mode);
                    let value = match kind {
                        FrameSlotKind::Word => builder.load(address, PtrLocation::Memory),
                        FrameSlotKind::Slice(location) if mode != FrameMode::MultiReturn => {
                            let ptr = builder.load(address, PtrLocation::Memory);
                            let len_address =
                                builder.add_u64_offset(address, EvmMemoryLayout::WORD_SIZE);
                            let len = builder.load(len_address, PtrLocation::Memory);
                            builder.make_slice(ptr, len, location)
                        }
                        FrameSlotKind::Slice(_) => {
                            unreachable!("multi-return buffers contain words")
                        }
                    };
                    let old = builder
                        .func()
                        .inst_result_value(inst)
                        .expect("frame load must produce a value");
                    replacements.insert(old, value);
                }
                InstKind::FrameStore { offset, mode, kind, value } => {
                    let address = frame_address(&mut builder, offset, mode);
                    match kind {
                        FrameSlotKind::Word => {
                            if mode == FrameMode::MultiReturn {
                                debug_assert_eq!(offset, 0);
                            }
                            builder.store(address, value);
                        }
                        FrameSlotKind::Slice(_) if mode != FrameMode::MultiReturn => {
                            let ptr = builder.slice_ptr(value);
                            let len = builder.slice_len(value);
                            builder.store(address, ptr);
                            let len_address =
                                builder.add_u64_offset(address, EvmMemoryLayout::WORD_SIZE);
                            builder.store(len_address, len);
                        }
                        FrameSlotKind::Slice(_) => {
                            unreachable!("multi-return buffers contain words")
                        }
                    }
                }
                _ => builder.func_mut().blocks[block].instructions.push(inst),
            }
        }
    }

    func.replace_uses_canonicalized(&replacements);
    true
}

fn frame_address(
    builder: &mut FunctionBuilder<'_>,
    offset: u64,
    mode: FrameMode,
) -> crate::mir::ValueId {
    match mode {
        FrameMode::External => builder.imm(EvmMemoryLayout::HEAP_START + offset),
        FrameMode::MultiReturn => builder.imm(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT),
        FrameMode::Internal => {
            let header = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE;
            let args = (builder.func().params.len() as u64) * EvmMemoryLayout::WORD_SIZE;
            let returns =
                (builder.func().return_components().len() as u64) * EvmMemoryLayout::WORD_SIZE;
            builder.internal_frame_addr(header + args + returns + offset)
        }
    }
}

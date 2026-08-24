//! Lower semantic mutable-local frame slots to physical memory operations.

use crate::{
    memory::EvmMemoryLayout,
    mir::{FrameMode, FrameSlotKind, Function, FunctionBuilder, InstKind, MirPhase, Module},
    pass::MirPass,
};
use solar_data_structures::map::FxHashMap;
use solar_sema::Gcx;

enum FrameOp {
    Load { offset: u64, mode: FrameMode, kind: FrameSlotKind },
    Store { offset: u64, mode: FrameMode, kind: FrameSlotKind, value: crate::mir::ValueId },
}

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
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        if module.phase < MirPhase::Dispatch || module.phase >= MirPhase::MemoryLowered {
            return false;
        }
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
    let blocks: Vec<_> = func.blocks.indices().collect();

    for block in blocks {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);

        for inst in instructions {
            let op = match &builder.func().inst(inst).kind {
                InstKind::FrameLoad { offset, mode, kind } => {
                    Some(FrameOp::Load { offset: *offset, mode: *mode, kind: *kind })
                }
                InstKind::FrameStore { offset, mode, kind, value } => Some(FrameOp::Store {
                    offset: *offset,
                    mode: *mode,
                    kind: *kind,
                    value: *value,
                }),
                _ => None,
            };
            match op {
                Some(FrameOp::Load { offset, mode, kind }) => {
                    let address = frame_address(&mut builder, offset, mode);
                    let value = match kind {
                        FrameSlotKind::Word => builder.mload(address),
                        FrameSlotKind::Slice(location) if mode != FrameMode::MultiReturn => {
                            let ptr = builder.mload(address);
                            let len_address =
                                builder.add_u64_offset(address, EvmMemoryLayout::WORD_SIZE);
                            let len = builder.mload(len_address);
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
                Some(FrameOp::Store { offset, mode, kind, value }) => {
                    let address = frame_address(&mut builder, offset, mode);
                    match kind {
                        FrameSlotKind::Word => {
                            if mode == FrameMode::MultiReturn {
                                debug_assert_eq!(offset, 0);
                            }
                            builder.mstore(address, value);
                        }
                        FrameSlotKind::Slice(_) if mode != FrameMode::MultiReturn => {
                            let ptr = builder.slice_ptr(value);
                            let len = builder.slice_len(value);
                            builder.mstore(address, ptr);
                            let len_address =
                                builder.add_u64_offset(address, EvmMemoryLayout::WORD_SIZE);
                            builder.mstore(len_address, len);
                        }
                        FrameSlotKind::Slice(_) => {
                            unreachable!("multi-return buffers contain words")
                        }
                    }
                }
                None => builder.func_mut().blocks[block].instructions.push(inst),
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
        FrameMode::External => builder.imm_u64(EvmMemoryLayout::HEAP_START + offset),
        FrameMode::MultiReturn => builder.imm_u64(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT),
        FrameMode::Internal => {
            let header = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE;
            let args = (builder.func().params.len() as u64) * EvmMemoryLayout::WORD_SIZE;
            let returns = (builder.func().returns.len() as u64) * EvmMemoryLayout::WORD_SIZE;
            builder.internal_frame_addr(header + args + returns + offset)
        }
    }
}

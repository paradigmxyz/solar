//! Lower abstract free-memory-pointer operations to ordinary memory operations.
//!
//! Allocation stays atomic through the optimization pipeline so placement
//! passes can reason about it without reconstructing a load/add/store idiom.
//! This pass expands the abstraction before the EVM-shaped boundary. Deferred
//! static-allocation placeholders remain for final backend layout.

use crate::{
    memory::EvmMemoryLayout,
    mir::{
        AllocationAlignment, AllocationFailure, AllocationInitialization, AllocationSemantics,
        BlockId, Function, FunctionBuilder, InstId, InstKind, MemoryRegion, Module, PanicCode,
        ValueId,
    },
    pass::MirPass,
    transform::utils::redirect_successor_predecessors,
};
use alloy_primitives::U256;
use solar_sema::Gcx;

/// Lowers `fmp`, `set_fmp`, and `alloc` instructions.
pub(crate) struct LowerAlloc;

impl MirPass for LowerAlloc {
    fn name(&self) -> &'static str {
        "lower-alloc"
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
        lower_alloc(module)
    }
}

fn lower_alloc(module: &mut Module) -> bool {
    let mut changed = false;
    for func in module.functions.iter_mut() {
        changed |= lower_function(func);
    }
    changed
}

fn lower_function(func: &mut Function) -> bool {
    let has_abstract_memory = func.instructions().any(|inst| {
        matches!(func.inst(inst).kind, InstKind::Fmp | InstKind::SetFmp(_) | InstKind::Alloc { .. })
    });
    if !has_abstract_memory {
        return false;
    }

    let mut changed = false;
    let mut block_index = 0;
    while block_index < func.blocks.len() {
        let block = BlockId::from_usize(block_index);
        let checked =
            func.blocks[block].instructions.iter().copied().enumerate().find(|(_, inst)| {
                matches!(
                    func.inst(*inst).kind,
                    InstKind::Alloc {
                        semantics: AllocationSemantics { failure: AllocationFailure::Panic, .. },
                        ..
                    }
                ) && !func.inst(*inst).metadata.deferred_alloc()
            });
        if let Some((position, inst)) = checked {
            let result = func.inst_result_value(inst).expect("allocation must produce a value");
            lower_checked_alloc(func, block, position, inst, result);
            changed = true;
        }
        block_index += 1;
    }

    let blocks = func.blocks.indices();
    for block in blocks {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        for inst in instructions {
            match builder.func().inst(inst).kind {
                InstKind::Fmp => {
                    changed = true;
                    rewrite_as_fmp_load(&mut builder, inst);
                    builder.func_mut().blocks[block].instructions.push(inst);
                }
                InstKind::SetFmp(ptr) => {
                    changed = true;
                    rewrite_as_fmp_store(&mut builder, inst, ptr);
                    builder.func_mut().blocks[block].instructions.push(inst);
                }
                InstKind::Alloc { size, semantics, .. } => {
                    if builder.func().inst(inst).metadata.deferred_alloc() {
                        builder.func_mut().blocks[block].instructions.push(inst);
                        continue;
                    }
                    debug_assert_eq!(semantics.failure, AllocationFailure::Infallible);
                    changed = true;
                    let ptr = builder
                        .func()
                        .inst_result_value(inst)
                        .expect("allocation must produce a value");
                    rewrite_as_fmp_load(&mut builder, inst);
                    builder.func_mut().blocks[block].instructions.push(inst);
                    let size = aligned_size(&mut builder, size, semantics.alignment).0;
                    let next = builder.add(ptr, size);
                    store_fmp(&mut builder, next);
                    initialize(&mut builder, ptr, size, semantics.initialization);
                }
                _ => {
                    builder.func_mut().blocks[block].instructions.push(inst);
                }
            }
        }
    }
    changed
}

fn lower_checked_alloc(
    func: &mut Function,
    block: BlockId,
    position: usize,
    inst: InstId,
    ptr: ValueId,
) {
    let InstKind::Alloc { size, semantics, .. } = func.inst(inst).kind else { unreachable!() };
    debug_assert_eq!(semantics.failure, AllocationFailure::Panic);

    let mut instructions = std::mem::take(&mut func.blocks[block].instructions);
    let tail = instructions.split_off(position + 1);
    func.blocks[block].instructions = instructions;
    let (old_terminator, metadata) = func.blocks[block].take_terminator();

    // continuation: tail; old_terminator !metadata(original block)
    let continuation = func.alloc_block();
    func.blocks[continuation].instructions = tail;
    if let Some(terminator) = old_terminator {
        func.blocks[continuation].set_terminator(terminator, metadata);
    }
    redirect_successor_predecessors(func, block, continuation);

    let panic = func.alloc_block();
    let mut builder = FunctionBuilder::new(func);
    builder.switch_to_block(block);
    rewrite_as_fmp_load(&mut builder, inst);
    let (size, align_overflow) = aligned_size(&mut builder, size, semantics.alignment);
    let next = builder.add(ptr, size);
    let bump_overflow = builder.lt(next, ptr);
    let limit = builder.imm(EvmMemoryLayout::MAX_ALLOCATION_END);
    let over_limit = builder.gt(next, limit);
    let mut invalid = builder.or(bump_overflow, over_limit);
    if let Some(align_overflow) = align_overflow {
        invalid = builder.or(invalid, align_overflow);
    }
    builder.branch(invalid, panic, continuation);

    let tail = std::mem::take(&mut builder.func_mut().blocks[continuation].instructions);
    builder.switch_to_block(continuation);
    store_fmp(&mut builder, next);
    initialize(&mut builder, ptr, size, semantics.initialization);
    builder.func_mut().blocks[continuation].instructions.extend(tail);

    builder.switch_to_block(panic);
    builder.panic(PanicCode::MemoryAllocationOverflow);
}

fn aligned_size(
    builder: &mut FunctionBuilder<'_>,
    size: ValueId,
    alignment: AllocationAlignment,
) -> (ValueId, Option<ValueId>) {
    if alignment == AllocationAlignment::Exact {
        return (size, None);
    }
    if let Some(size) = builder.func().value_u64(size)
        && let Some(aligned) = EvmMemoryLayout::align_word(size)
    {
        return (builder.imm(aligned), None);
    }
    let mask = builder.imm(U256::MAX - U256::from(EvmMemoryLayout::WORD_SIZE - 1));
    let rounded = builder.add_u64_offset(size, EvmMemoryLayout::WORD_SIZE - 1);
    let overflow = builder.lt(rounded, size);
    (builder.and(rounded, mask), Some(overflow))
}

fn initialize(
    builder: &mut FunctionBuilder<'_>,
    ptr: ValueId,
    size: ValueId,
    initialization: AllocationInitialization,
) {
    if initialization == AllocationInitialization::Zeroed {
        builder.memory_zero(ptr, size);
    }
}

fn rewrite_as_fmp_load(builder: &mut FunctionBuilder<'_>, inst: crate::mir::InstId) {
    let slot = builder.imm(EvmMemoryLayout::FMP_SLOT);
    let instruction = builder.func_mut().inst_mut(inst);
    instruction.kind = InstKind::MLoad(slot);
    instruction.metadata.set_memory_region(Some(MemoryRegion::Scratch));
}

fn rewrite_as_fmp_store(
    builder: &mut FunctionBuilder<'_>,
    inst: crate::mir::InstId,
    ptr: crate::mir::ValueId,
) {
    let slot = builder.imm(EvmMemoryLayout::FMP_SLOT);
    let instruction = builder.func_mut().inst_mut(inst);
    instruction.kind = InstKind::MStore(slot, ptr);
    instruction.metadata.set_memory_region(Some(MemoryRegion::Scratch));
}

fn store_fmp(builder: &mut FunctionBuilder<'_>, ptr: crate::mir::ValueId) {
    let slot = builder.imm(EvmMemoryLayout::FMP_SLOT);
    builder.mstore(slot, ptr);
}

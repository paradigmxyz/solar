//! Lower abstract free-memory-pointer operations to ordinary memory operations.
//!
//! Allocation stays atomic through the optimization pipeline so placement
//! passes can reason about it without reconstructing a load/add/store idiom.
//! This pass expands the abstraction only at the EVM-shaped boundary.

use crate::{
    memory::EvmMemoryLayout,
    mir::{
        AllocationAlignment, AllocationFailure, AllocationInitialization, AllocationSemantics,
        BlockId, Function, FunctionBuilder, InstId, InstKind, MemoryRegion, Module, Terminator,
        ValueId,
    },
    pass::MirPass,
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
    for func in module.functions_mut() {
        if !func.has_no_blocks() {
            changed |= lower_function(func);
        }
    }
    changed
}

fn lower_function(func: &mut Function) -> bool {
    let has_abstract_memory = func.blocks().any(|block| {
        block.instructions.iter().any(|&inst| {
            matches!(
                func.instruction(inst).kind,
                InstKind::Fmp | InstKind::SetFmp(_) | InstKind::Alloc { .. }
            )
        })
    });
    if !has_abstract_memory {
        return false;
    }

    let inst_results = func.inst_results();
    let mut changed = false;
    let mut block_index = 0;
    while block_index < func.block_count() {
        let block = BlockId::from_usize(block_index);
        let checked =
            func.block(block).instructions.iter().copied().enumerate().find(|(_, inst)| {
                matches!(
                    func.instruction(*inst).kind,
                    InstKind::Alloc {
                        semantics: AllocationSemantics { failure: AllocationFailure::Panic, .. },
                        ..
                    }
                ) && !func.instruction(*inst).metadata.deferred_alloc()
            });
        if let Some((position, inst)) = checked {
            lower_checked_alloc(func, block, position, inst, inst_results[&inst]);
            changed = true;
        }
        block_index += 1;
    }

    let blocks: Vec<BlockId> = func.block_ids().collect();
    for block in blocks {
        let instructions = std::mem::take(&mut func.block_mut(block).instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        for inst in instructions {
            match builder.func().instruction(inst).kind {
                InstKind::Fmp => {
                    changed = true;
                    rewrite_as_fmp_load(&mut builder, inst);
                    builder.func_mut().block_mut(block).instructions.push(inst);
                }
                InstKind::SetFmp(ptr) => {
                    changed = true;
                    rewrite_as_fmp_store(&mut builder, inst, ptr);
                    builder.func_mut().block_mut(block).instructions.push(inst);
                }
                InstKind::Alloc { size, semantics, .. } => {
                    if builder.func().instruction(inst).metadata.deferred_alloc() {
                        builder.func_mut().block_mut(block).instructions.push(inst);
                        continue;
                    }
                    debug_assert_eq!(semantics.failure, AllocationFailure::Infallible);
                    changed = true;
                    let ptr = inst_results[&inst];
                    rewrite_as_fmp_load(&mut builder, inst);
                    builder.func_mut().block_mut(block).instructions.push(inst);
                    let size = aligned_size(&mut builder, size, semantics.alignment).0;
                    let next = builder.add(ptr, size);
                    store_fmp(&mut builder, next);
                    initialize(&mut builder, ptr, size, semantics.initialization);
                }
                _ => {
                    builder.func_mut().block_mut(block).instructions.push(inst);
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
    let InstKind::Alloc { size, semantics, .. } = func.instruction(inst).kind else {
        unreachable!()
    };
    debug_assert_eq!(semantics.failure, AllocationFailure::Panic);

    let mut instructions = std::mem::take(&mut func.block_mut(block).instructions);
    let tail = instructions.split_off(position + 1);
    func.block_mut(block).instructions = instructions;
    let old_terminator = func.block_mut(block).terminator.take();

    let continuation = func.alloc_block();
    func.block_mut(continuation).instructions = tail;
    func.block_mut(continuation).terminator = old_terminator;
    redirect_successor_predecessors(func, block, continuation);

    let panic = func.alloc_block();
    let mut builder = FunctionBuilder::new(func);
    builder.switch_to_block(block);
    rewrite_as_fmp_load(&mut builder, inst);
    let (size, align_overflow) = aligned_size(&mut builder, size, semantics.alignment);
    let next = builder.add(ptr, size);
    let bump_overflow = builder.lt(next, ptr);
    let limit = builder.imm_u64(EvmMemoryLayout::MAX_ALLOCATION_END);
    let over_limit = builder.gt(next, limit);
    let mut invalid = builder.or(bump_overflow, over_limit);
    if let Some(align_overflow) = align_overflow {
        invalid = builder.or(invalid, align_overflow);
    }
    builder.branch(invalid, panic, continuation);

    let tail = std::mem::take(&mut builder.func_mut().block_mut(continuation).instructions);
    builder.switch_to_block(continuation);
    store_fmp(&mut builder, next);
    initialize(&mut builder, ptr, size, semantics.initialization);
    builder.func_mut().block_mut(continuation).instructions.extend(tail);

    builder.switch_to_block(panic);
    let zero = builder.imm_u64(0);
    let four = builder.imm_u64(4);
    let selector = builder.imm_u256(U256::from(0x4e48_7b71_u64) << 224);
    let code = builder.imm_u64(0x41);
    builder.mstore(zero, selector);
    builder.mstore(four, code);
    let size = builder.imm_u64(36);
    builder.revert(zero, size);
}

fn redirect_successor_predecessors(func: &mut Function, from: BlockId, to: BlockId) {
    let successors =
        func.block(to).terminator.as_ref().map(Terminator::successors).unwrap_or_default();
    for successor in successors {
        for predecessor in &mut func.block_mut(successor).predecessors {
            if *predecessor == from {
                *predecessor = to;
            }
        }
        let phi_insts: Vec<_> = func
            .block(successor)
            .instructions
            .iter()
            .copied()
            .take_while(|inst| matches!(func.instruction(*inst).kind, InstKind::Phi(_)))
            .collect();
        for phi in phi_insts {
            let InstKind::Phi(incoming) = &mut func.instruction_mut(phi).kind else {
                unreachable!()
            };
            for (predecessor, _) in incoming {
                if *predecessor == from {
                    *predecessor = to;
                }
            }
        }
    }
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
        return (builder.imm_u64(aligned), None);
    }
    let mask = builder.imm_u256(U256::MAX - U256::from(EvmMemoryLayout::WORD_SIZE - 1));
    let padding = builder.imm_u64(EvmMemoryLayout::WORD_SIZE - 1);
    let rounded = builder.add(size, padding);
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
        let calldata_end = builder.calldatasize();
        builder.calldatacopy(ptr, calldata_end, size);
    }
}

fn rewrite_as_fmp_load(builder: &mut FunctionBuilder<'_>, inst: crate::mir::InstId) {
    let slot = builder.imm_u64(EvmMemoryLayout::FMP_SLOT);
    let instruction = builder.func_mut().instruction_mut(inst);
    instruction.kind = InstKind::MLoad(slot);
    instruction.metadata.set_memory_region(Some(MemoryRegion::Scratch));
}

fn rewrite_as_fmp_store(
    builder: &mut FunctionBuilder<'_>,
    inst: crate::mir::InstId,
    ptr: crate::mir::ValueId,
) {
    let slot = builder.imm_u64(EvmMemoryLayout::FMP_SLOT);
    let instruction = builder.func_mut().instruction_mut(inst);
    instruction.kind = InstKind::MStore(slot, ptr);
    instruction.metadata.set_memory_region(Some(MemoryRegion::Scratch));
}

fn store_fmp(builder: &mut FunctionBuilder<'_>, ptr: crate::mir::ValueId) {
    let slot = builder.imm_u64(EvmMemoryLayout::FMP_SLOT);
    builder.mstore(slot, ptr);
}

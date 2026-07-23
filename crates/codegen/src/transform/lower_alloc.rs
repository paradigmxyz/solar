//! Lower abstract free-memory-pointer operations to ordinary memory operations.
//!
//! Allocation stays atomic through the optimization pipeline so placement
//! passes can reason about it without reconstructing a load/add/store idiom.
//! This pass expands the abstraction only at the EVM-shaped boundary.

use crate::{
    memory::EvmMemoryLayout,
    mir::{
        AllocationAlignment, AllocationFailure, AllocationInitialization, AllocationSemantics,
        BlockId, Function, FunctionBuilder, FunctionId, InstId, InstKind, MemoryRegion, Module,
        Terminator, ValueId,
    },
    pass::ModulePass,
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashSet;
use solar_sema::Gcx;

/// Lowers `fmp`, `set_fmp`, and `alloc` instructions.
pub(crate) struct LowerAllocPass;

impl ModulePass for LowerAllocPass {
    fn run(&mut self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        lower_alloc_except(module, &FxHashSet::default())
    }
}

/// Lowers abstract allocation operations except for backend-owned allocations
/// whose final placement depends on exact emitted frame layout.
pub(crate) fn lower_alloc_except(
    module: &mut Module,
    deferred: &FxHashSet<(FunctionId, InstId)>,
) -> bool {
    let mut changed = false;
    let function_ids: Vec<_> = module.functions.indices().collect();
    for func_id in function_ids {
        let func = &mut module.functions[func_id];
        if !func.blocks.is_empty() {
            changed |= lower_function(func_id, func, deferred);
        }
    }
    changed
}

fn lower_function(
    func_id: FunctionId,
    func: &mut Function,
    deferred: &FxHashSet<(FunctionId, InstId)>,
) -> bool {
    let has_abstract_memory = func.blocks.iter().any(|block| {
        block.instructions.iter().any(|&inst| {
            matches!(
                func.instructions[inst].kind,
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
    while block_index < func.blocks.len() {
        let block = BlockId::from_usize(block_index);
        let checked =
            func.blocks[block].instructions.iter().copied().enumerate().find(|(_, inst)| {
                matches!(
                    func.instructions[*inst].kind,
                    InstKind::Alloc {
                        semantics: AllocationSemantics { failure: AllocationFailure::Panic, .. },
                        ..
                    }
                ) && !deferred.contains(&(func_id, *inst))
            });
        if let Some((position, inst)) = checked {
            lower_checked_alloc(func, block, position, inst, inst_results[&inst]);
            changed = true;
        }
        block_index += 1;
    }

    let blocks: Vec<BlockId> = func.blocks.indices().collect();
    for block in blocks {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        for inst in instructions {
            match builder.func().instructions[inst].kind {
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
                    if deferred.contains(&(func_id, inst)) {
                        builder.func_mut().blocks[block].instructions.push(inst);
                        continue;
                    }
                    debug_assert_eq!(semantics.failure, AllocationFailure::Infallible);
                    changed = true;
                    let ptr = inst_results[&inst];
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
    let InstKind::Alloc { size, semantics, .. } = func.instructions[inst].kind else {
        unreachable!()
    };
    debug_assert_eq!(semantics.failure, AllocationFailure::Panic);

    let mut instructions = std::mem::take(&mut func.blocks[block].instructions);
    let tail = instructions.split_off(position + 1);
    func.blocks[block].instructions = instructions;
    let old_terminator = func.blocks[block].terminator.take();

    let continuation = func.alloc_block();
    func.blocks[continuation].instructions = tail;
    func.blocks[continuation].terminator = old_terminator;
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

    let tail = std::mem::take(&mut builder.func_mut().blocks[continuation].instructions);
    builder.switch_to_block(continuation);
    store_fmp(&mut builder, next);
    initialize(&mut builder, ptr, size, semantics.initialization);
    builder.func_mut().blocks[continuation].instructions.extend(tail);

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
        func.blocks[to].terminator.as_ref().map(Terminator::successors).unwrap_or_default();
    for successor in successors {
        for predecessor in &mut func.blocks[successor].predecessors {
            if *predecessor == from {
                *predecessor = to;
            }
        }
        let phi_insts: Vec<_> = func.blocks[successor]
            .instructions
            .iter()
            .copied()
            .take_while(|inst| matches!(func.instructions[*inst].kind, InstKind::Phi(_)))
            .collect();
        for phi in phi_insts {
            let InstKind::Phi(incoming) = &mut func.instructions[phi].kind else { unreachable!() };
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
    let instruction = &mut builder.func_mut().instructions[inst];
    instruction.kind = InstKind::MLoad(slot);
    instruction.metadata.set_memory_region(Some(MemoryRegion::Scratch));
}

fn rewrite_as_fmp_store(
    builder: &mut FunctionBuilder<'_>,
    inst: crate::mir::InstId,
    ptr: crate::mir::ValueId,
) {
    let slot = builder.imm_u64(EvmMemoryLayout::FMP_SLOT);
    let instruction = &mut builder.func_mut().instructions[inst];
    instruction.kind = InstKind::MStore(slot, ptr);
    instruction.metadata.set_memory_region(Some(MemoryRegion::Scratch));
}

fn store_fmp(builder: &mut FunctionBuilder<'_>, ptr: crate::mir::ValueId) {
    let slot = builder.imm_u64(EvmMemoryLayout::FMP_SLOT);
    builder.mstore(slot, ptr);
}

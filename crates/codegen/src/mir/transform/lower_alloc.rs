//! Lower abstract free-memory-pointer operations to ordinary memory operations.
//!
//! Allocation stays atomic through the optimization pipeline so placement
//! passes can reason about it without reconstructing a load/add/store idiom.
//! This pass expands the abstraction before the EVM-shaped boundary. Deferred
//! static-allocation placeholders remain for final backend layout. Raw loads and stores discard
//! allocation-only effect and placement metadata: the emitted stores, initialization, and panic
//! branches carry those effects. Source metadata stays on the original instruction.

use crate::mir::{
    AllocationFailure, AllocationInitialization, AllocationSemantics, BlockId, Function,
    FunctionBuilder, InstId, InstKind, MemoryRegion, MirType, Module, PanicCode, PtrLocation,
    ValueId, memory::EvmMemoryLayout, pass::MirPass,
    transform::utils::redirect_successor_predecessors,
};
use solar_data_structures::map::FxHashMap;
use solar_interface::{Ident, sym};
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
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        lower_alloc(module)
    }
}

fn lower_alloc(module: &mut Module) -> bool {
    let mut changed = share_checked_allocations(module);
    for func in module.functions.iter_mut() {
        changed |= lower_function(func);
    }
    changed
}

/// Share repeated checked free-memory-pointer bumps before lowering erases the
/// allocation operation.
///
/// The helper owns only the pointer load, checked bump, panic, and update.
/// Callers retain their initialization, so allocation ordering stays intact.
fn share_checked_allocations(module: &mut Module) -> bool {
    let count = module
        .functions
        .iter()
        .flat_map(|func| func.instructions().map(|inst| func.inst(inst)))
        .filter(|inst| {
            matches!(
                inst.kind,
                InstKind::Alloc {
                    semantics: AllocationSemantics { failure: AllocationFailure::Checked, .. },
                    ..
                }
            ) && !inst.metadata.deferred_alloc()
        })
        .count();
    if count < 2 {
        return false;
    }

    let helper = create_checked_alloc(module);
    for func in &mut module.functions {
        let mut replacements = FxHashMap::default();
        let blocks = func.blocks.indices();
        for block in blocks {
            let instructions = std::mem::take(&mut func.blocks[block].instructions);
            let mut builder = FunctionBuilder::new(func);
            builder.switch_to_block(block);
            for inst in instructions {
                let InstKind::Alloc { size, semantics } = builder.func().inst(inst).kind else {
                    builder.func_mut().blocks[block].instructions.push(inst);
                    continue;
                };
                if semantics.failure != AllocationFailure::Checked
                    || builder.func().inst(inst).metadata.deferred_alloc()
                {
                    builder.func_mut().blocks[block].instructions.push(inst);
                    continue;
                }
                let result =
                    builder.func().inst_result_value(inst).expect("allocation has a result");
                // object = icall checked_alloc size
                let object = builder.icall(helper, vec![size], MirType::MEMPTR);
                initialize(&mut builder, object, size, semantics.initialization);
                replacements.insert(result, object);
            }
        }
        func.replace_uses_canonicalized(&replacements);
    }
    true
}

fn create_checked_alloc(module: &mut Module) -> crate::mir::FunctionId {
    let mut function = Function::new(Ident::with_dummy_span(sym::alloc));
    {
        let mut builder = FunctionBuilder::new(&mut function);
        let size = builder.add_param(MirType::I256);
        builder.set_return_type(MirType::MEMPTR);
        let slot = builder.imm(EvmMemoryLayout::FMP_SLOT);
        // ptr = inttoptr (load memory 64) to ptr<memory>
        let free = builder.load(slot, PtrLocation::Memory);
        let ptr = builder.cast(free, MirType::MEMPTR);
        let next = builder.add(ptr, size);
        let bump_overflow = builder.lt(next, ptr);
        let limit = builder.imm(EvmMemoryLayout::MAX_ALLOCATION_END);
        let over_limit = builder.gt(next, limit);
        let invalid = builder.or(bump_overflow, over_limit);
        let panic = builder.create_block();
        let continuation = builder.create_block();
        builder.branch(invalid, panic, continuation);

        builder.switch_to_block(continuation);
        store_fmp(&mut builder, next);
        builder.ret([ptr]);

        builder.switch_to_block(panic);
        builder.panic(PanicCode::MemoryAllocationOverflow);
    }
    module.add_function(function)
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
                        semantics: AllocationSemantics { failure: AllocationFailure::Checked, .. },
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
                InstKind::Alloc { size, semantics } => {
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
    let InstKind::Alloc { size, semantics } = func.inst(inst).kind else { unreachable!() };
    debug_assert_eq!(semantics.failure, AllocationFailure::Checked);

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
    let next = builder.add(ptr, size);
    let bump_overflow = builder.lt(next, ptr);
    let limit = builder.imm(EvmMemoryLayout::MAX_ALLOCATION_END);
    let over_limit = builder.gt(next, limit);
    let invalid = builder.or(bump_overflow, over_limit);
    builder.branch(invalid, panic, continuation);

    let tail = std::mem::take(&mut builder.func_mut().blocks[continuation].instructions);
    builder.switch_to_block(continuation);
    store_fmp(&mut builder, next);
    initialize(&mut builder, ptr, size, semantics.initialization);
    builder.func_mut().blocks[continuation].instructions.extend(tail);

    builder.switch_to_block(panic);
    builder.panic(PanicCode::MemoryAllocationOverflow);
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
    // ptr = mload 64
    // object = inttoptr ptr
    let kind = if builder.func().inst(inst).result_ty.is_some_and(crate::mir::MirType::is_pointer) {
        let block = builder.current_block();
        let attached = builder.func().blocks[block].instructions.last() == Some(&inst);
        if attached {
            builder.func_mut().blocks[block].instructions.pop();
        }
        let ptr = builder.load(slot, PtrLocation::Memory);
        if attached {
            builder.func_mut().blocks[block].instructions.push(inst);
        }
        InstKind::IntToPtr(ptr)
    } else {
        InstKind::Load(slot, PtrLocation::Memory)
    };
    let instruction = builder.func_mut().inst_mut(inst);
    instruction.kind = kind;
    instruction.metadata.set_effect(None);
    instruction.metadata.set_preserves_fmp(false);
    instruction.metadata.set_memory_region(Some(MemoryRegion::Scratch));
}

fn rewrite_as_fmp_store(
    builder: &mut FunctionBuilder<'_>,
    inst: crate::mir::InstId,
    ptr: crate::mir::ValueId,
) {
    // word = ptrtoint ptr to i256
    // set_fmp ptr -> mstore 64, word
    let ptr = builder.cast(ptr, MirType::I256);
    let slot = builder.imm(EvmMemoryLayout::FMP_SLOT);
    let instruction = builder.func_mut().inst_mut(inst);
    instruction.kind = InstKind::Store(slot, ptr, PtrLocation::Memory);
    instruction.metadata.set_effect(None);
    instruction.metadata.set_preserves_fmp(false);
    instruction.metadata.set_memory_region(Some(MemoryRegion::Scratch));
}

fn store_fmp(builder: &mut FunctionBuilder<'_>, ptr: crate::mir::ValueId) {
    let slot = builder.imm(EvmMemoryLayout::FMP_SLOT);
    builder.store(slot, ptr);
    let store = *builder.func().blocks[builder.current_block()].instructions.last().unwrap();
    builder.func_mut().inst_mut(store).metadata.set_preserves_valid_fmp();
}

//! Lower abstract free-memory-pointer operations to ordinary memory operations.
//!
//! Allocation stays atomic through the optimization pipeline so placement
//! passes can reason about it without reconstructing a load/add/store idiom.
//! This pass expands the abstraction only at the EVM-shaped boundary.

use crate::{
    mir::{BlockId, Function, FunctionBuilder, FunctionId, InstId, InstKind, MemoryRegion, Module},
    pass::ModulePass,
};
use solar_data_structures::map::FxHashSet;
use solar_sema::Gcx;

/// Lowers `fmp`, `set_fmp`, and `alloc` instructions.
pub struct LowerAllocPass;

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
                InstKind::Fmp | InstKind::SetFmp(_) | InstKind::Alloc(_)
            )
        })
    });
    if !has_abstract_memory {
        return false;
    }

    let inst_results = func.inst_results();
    let mut changed = false;
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
                InstKind::Alloc(size) => {
                    if deferred.contains(&(func_id, inst)) {
                        builder.func_mut().blocks[block].instructions.push(inst);
                        continue;
                    }
                    changed = true;
                    let ptr = inst_results[&inst];
                    rewrite_as_fmp_load(&mut builder, inst);
                    builder.func_mut().blocks[block].instructions.push(inst);
                    let next = builder.add(ptr, size);
                    store_fmp(&mut builder, next);
                }
                _ => {
                    builder.func_mut().blocks[block].instructions.push(inst);
                }
            }
        }
    }
    changed
}

fn rewrite_as_fmp_load(builder: &mut FunctionBuilder<'_>, inst: crate::mir::InstId) {
    let slot = builder.imm_u64(0x40);
    let instruction = &mut builder.func_mut().instructions[inst];
    instruction.kind = InstKind::MLoad(slot);
    instruction.metadata.set_memory_region(Some(MemoryRegion::Scratch));
}

fn rewrite_as_fmp_store(
    builder: &mut FunctionBuilder<'_>,
    inst: crate::mir::InstId,
    ptr: crate::mir::ValueId,
) {
    let slot = builder.imm_u64(0x40);
    let instruction = &mut builder.func_mut().instructions[inst];
    instruction.kind = InstKind::MStore(slot, ptr);
    instruction.metadata.set_memory_region(Some(MemoryRegion::Scratch));
}

fn store_fmp(builder: &mut FunctionBuilder<'_>, ptr: crate::mir::ValueId) {
    let slot = builder.imm_u64(0x40);
    builder.mstore(slot, ptr);
}

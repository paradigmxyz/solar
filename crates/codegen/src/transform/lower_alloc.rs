//! Lower abstract free-memory-pointer operations to ordinary memory operations.
//!
//! Allocation stays atomic through the optimization pipeline so placement
//! passes can reason about it without reconstructing a load/add/store idiom.
//! This pass expands the abstraction only at the EVM-shaped boundary.

use crate::{
    mir::{BlockId, Function, FunctionBuilder, InstKind, MemoryRegion, Module},
    pass::ModulePass,
};

/// Lowers `fmp`, `set_fmp`, and `alloc` instructions.
pub struct LowerAllocPass;

impl ModulePass for LowerAllocPass {
    fn run(&mut self, module: &mut Module) -> bool {
        let mut changed = false;
        for func in module.functions.iter_mut().filter(|func| !func.blocks.is_empty()) {
            changed |= lower_function(func);
        }
        changed
    }
}

fn lower_function(func: &mut Function) -> bool {
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
    let blocks: Vec<BlockId> = func.blocks.indices().collect();
    for block in blocks {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        for inst in instructions {
            match builder.func().instructions[inst].kind {
                InstKind::Fmp => {
                    rewrite_as_fmp_load(&mut builder, inst);
                    builder.func_mut().blocks[block].instructions.push(inst);
                }
                InstKind::SetFmp(ptr) => {
                    rewrite_as_fmp_store(&mut builder, inst, ptr);
                    builder.func_mut().blocks[block].instructions.push(inst);
                }
                InstKind::Alloc(size) => {
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
    true
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

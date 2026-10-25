//! Lower abstract free-memory-pointer operations to ordinary memory operations.
//!
//! Allocation stays atomic through the optimization pipeline so placement
//! passes can reason about it without reconstructing a load/add/store idiom.
//! This pass expands the abstraction before the EVM-shaped boundary. Deferred
//! static-allocation placeholders remain for final backend layout. Raw loads and stores discard
//! allocation-only effect and placement metadata: the emitted stores, initialization, and panic
//! branches carry those effects. Source metadata stays on the original instruction.
//!
//! A checked allocation bumps the pointer only after testing the new end against overflow and
//! the allocation limit, panicking otherwise. When a module has two or more, one `alloc(size)`
//! helper owns the pointer load, check, panic, and update, and each site calls it and keeps its
//! own zeroing; a single checked allocation splits its block around the inline check instead.
//!
//! A preserving allocation that reserves bytes an encoder already wrote through an
//! earlier pointer read in the same block reuses that read instead of loading the
//! pointer again, as long as only writes addressed from the read word come between.

use crate::mir::{
    AllocationFailure, AllocationInitialization, AllocationSemantics, BlockId, EffectKind,
    Function, FunctionBuilder, InstId, InstKind, MemoryRegion, MirType, Module, PanicCode,
    PtrLocation, Value, ValueId, memory::EvmMemoryLayout, pass::MirPass,
    transform::utils::redirect_successor_predecessors,
};
use solar_data_structures::map::{FxHashMap, FxHashSet};
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
        let next = bump(&mut builder, ptr, size);
        let bump_overflow = builder.lt(next, ptr);
        let limit = builder.imm(EvmMemoryLayout::MAX_ALLOCATION_END);
        let over_limit = builder.gt(next, limit);
        let invalid = builder.or(bump_overflow, over_limit);
        let panic = builder.create_block();
        let continuation = builder.create_block();
        builder.branch(invalid, panic, continuation);

        builder.switch_to_block(continuation);
        builder.store(slot, next);
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
        let mut peek = FmpPeek::default();
        for inst in instructions {
            // Allocations and pointer updates move the pointer past any earlier read.
            let moves_fmp = matches!(
                builder.func().inst(inst).kind,
                InstKind::Alloc { .. } | InstKind::SetFmp(_)
            );
            match builder.func().inst(inst).kind {
                InstKind::Fmp => {
                    changed = true;
                    rewrite_as_fmp_load(&mut builder, inst);
                    builder.func_mut().blocks[block].instructions.push(inst);
                    peek.start(builder.func(), inst);
                }
                InstKind::Alloc { size, semantics }
                    if let Some(word) = peek.reserved_word(builder.func(), inst) =>
                {
                    debug_assert_eq!(semantics.failure, AllocationFailure::Infallible);
                    changed = true;
                    let ptr = builder
                        .func()
                        .inst_result_value(inst)
                        .expect("allocation must produce a value");
                    // ptr = inttoptr word
                    // mstore 64, word + size
                    let instruction = builder.func_mut().inst_mut(inst);
                    instruction.kind = InstKind::IntToPtr(word);
                    instruction.metadata.set_effect(None);
                    instruction.metadata.set_preserves_fmp(false);
                    instruction.metadata.set_memory_region(Some(MemoryRegion::Scratch));
                    builder.func_mut().blocks[block].instructions.push(inst);
                    let next = bump(&mut builder, word, size);
                    store_fmp(&mut builder, next);
                    initialize(&mut builder, ptr, size, semantics.initialization);
                }
                InstKind::SetFmp(ptr) => {
                    changed = true;
                    rewrite_as_fmp_store(&mut builder, inst, ptr);
                    builder.func_mut().blocks[block].instructions.push(inst);
                }
                // The backend may still place a deferred allocation with a runtime bump.
                InstKind::Alloc { .. } if builder.func().inst(inst).metadata.deferred_alloc() => {
                    builder.func_mut().blocks[block].instructions.push(inst);
                }
                InstKind::Alloc { size, semantics } => {
                    debug_assert_eq!(semantics.failure, AllocationFailure::Infallible);
                    changed = true;
                    let ptr = builder
                        .func()
                        .inst_result_value(inst)
                        .expect("allocation must produce a value");
                    rewrite_as_fmp_load(&mut builder, inst);
                    builder.func_mut().blocks[block].instructions.push(inst);
                    let next = bump(&mut builder, ptr, size);
                    store_fmp(&mut builder, next);
                    initialize(&mut builder, ptr, size, semantics.initialization);
                }
                _ => {
                    peek.observe(builder.func(), inst);
                    builder.func_mut().blocks[block].instructions.push(inst);
                }
            }
            if moves_fmp {
                peek.clear();
            }
        }
    }
    changed
}

/// The latest free-memory-pointer read in a block, while it is still the pointer.
///
/// Encoders read the pointer, write the encoding at and above it, and then reserve
/// those bytes with an `alloc !preserves_fmp` that must return the same address.
/// Lowering that reservation reuses the read word instead of loading slot 64 again,
/// as solc's `allocate_unbounded` does. Any call, allocation, pointer update, or
/// memory write not addressed from the read word ends the peek.
#[derive(Default)]
struct FmpPeek {
    word: Option<ValueId>,
    derived: FxHashSet<ValueId>,
}

impl FmpPeek {
    fn start(&mut self, func: &Function, inst: InstId) {
        let result = func.inst_result_value(inst).expect("`fmp` must produce a value");
        let word = match func.inst(inst).kind {
            InstKind::IntToPtr(word) => word,
            _ => result,
        };
        self.word = Some(word);
        self.derived.clear();
        self.derived.extend([word, result]);
    }

    fn clear(&mut self) {
        self.word = None;
        self.derived.clear();
    }

    /// The read word, if `inst` reserves the bytes written since the read.
    fn reserved_word(&self, func: &Function, inst: InstId) -> Option<ValueId> {
        let instruction = func.inst(inst);
        (instruction.metadata.preserves_fmp()
            && !instruction.metadata.deferred_alloc()
            && instruction.result_ty.is_some_and(MirType::is_pointer))
        .then_some(self.word?)
    }

    fn observe(&mut self, func: &Function, inst: InstId) {
        if self.word.is_none() {
            return;
        }
        let kind = &func.inst(inst).kind;
        let keeps = match kind.effect_kind() {
            EffectKind::Pure
            | EffectKind::MemoryRead
            | EffectKind::StorageRead
            | EffectKind::TransientRead
            | EffectKind::EnvironmentRead => true,
            EffectKind::MemoryWrite => match *kind {
                InstKind::Store(ptr, _, PtrLocation::Memory)
                | InstKind::Store8(ptr, _)
                | InstKind::MemoryZero(ptr, _)
                | InstKind::Copy(ptr, ..) => self.derived.contains(&ptr),
                _ => false,
            },
            _ => false,
        };
        if !keeps {
            self.clear();
            return;
        }
        let from_peek = match *kind {
            InstKind::Add(a, b) => self.derived.contains(&a) || self.derived.contains(&b),
            InstKind::PtrAdd(ptr, _) | InstKind::PtrToInt(ptr, _) | InstKind::IntToPtr(ptr) => {
                self.derived.contains(&ptr)
            }
            _ => false,
        };
        if from_peek && let Some(result) = func.inst_result_value(inst) {
            self.derived.insert(result);
        }
    }
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
    let next = bump(&mut builder, ptr, size);
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

/// Emits `ptr + size`, which allocation proved or checked not to wrap, marked so the sum keeps the
/// bound of `ptr`.
fn bump(builder: &mut FunctionBuilder<'_>, ptr: ValueId, size: ValueId) -> ValueId {
    let next = builder.add(ptr, size);
    if let Value::Inst(inst) = builder.func().value(next) {
        let inst = *inst;
        builder.func_mut().inst_mut(inst).metadata.set_fmp_bump(true);
    }
    next
}

fn store_fmp(builder: &mut FunctionBuilder<'_>, ptr: crate::mir::ValueId) {
    let slot = builder.imm(EvmMemoryLayout::FMP_SLOT);
    builder.store(slot, ptr);
}

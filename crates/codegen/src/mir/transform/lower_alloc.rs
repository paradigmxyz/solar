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
//! own zeroing unless that is dead (see below); a single checked allocation splits its block
//! around the inline check instead.
//!
//! A preserving allocation that reserves bytes an encoder already wrote through an
//! earlier pointer read in the same block reuses that read instead of loading the
//! pointer again, as long as only writes addressed from the read word come between.
//!
//! A `zero_slot_if_empty` allocation, the capture of a call's returndata, is an empty `bytes`
//! object when its size is one header word. Like solc, such an object is the zero slot and
//! reserves nothing, so the pointer does not move: the allocation becomes `select(size == 32,
//! ZERO_SLOT, ptr)` with a bump of `size == 32 ? 0 : size`. When nothing reachable after the
//! allocation allocates, calls, or writes memory except through the object's own pointer, and the
//! pointer does not escape, the empty object is only read and stays at the free-memory pointer
//! without the `select`, which keeps forwarding code such as proxies as small as before.
//!
//! A read that returns the entry's initial pointer, because nothing before it may write the
//! slot (see [`FreshHeap`]), takes the heap floor, which codegen knows, instead of loading
//! the slot. Its allocation still stores the bumped pointer. An entry whose only reads of the
//! slot are such reads then needs no initialization of it. Under the gas objective, a fresh
//! checked allocation keeps its own inline check rather than calling the shared helper, as the
//! helper loads the slot; the size objective keeps the call.
//!
//! Memory above the pointer that nothing wrote since the entry still holds zeros, so the
//! initialization of a zeroed allocation made there, and a `memory_zero` or zero word store into
//! it that nothing wrote before, are dead and dropped (see [`FreshHeap`]).

use crate::mir::{
    AllocationFailure, AllocationInitialization, AllocationSemantics, BlockId, EffectKind,
    Function, FunctionBuilder, FunctionId, InstId, InstKind, MemoryRegion, MirType, Module,
    PanicCode, PtrLocation, Terminator, Value, ValueId,
    analysis::FreshHeap,
    memory::EvmMemoryLayout,
    pass::{MirPass, ModuleAnalyses, run_lowering_function_pass},
    transform::utils::expand_block,
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

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        let fresh = FreshHeap::new(module, || analyses.call_summaries(module));
        lower_alloc(module, analyses, &fresh)
    }
}

fn lower_alloc(module: &mut Module, analyses: &mut ModuleAnalyses, fresh: &FreshHeap) -> bool {
    let mut changed = share_checked_allocations(module, analyses, fresh);
    changed |= run_lowering_function_pass(module, analyses, |func, function_analyses| {
        let id = function_analyses.function();
        lower_function(func, fresh, id)
    });
    changed
}

/// Share repeated checked free-memory-pointer bumps before lowering erases the
/// allocation operation.
///
/// The helper owns only the pointer load, checked bump, panic, and update.
/// Callers retain their initialization, so allocation ordering stays intact.
fn share_checked_allocations(
    module: &mut Module,
    analyses: &mut ModuleAnalyses,
    fresh: &FreshHeap,
) -> bool {
    let count = module
        .functions
        .iter_enumerated()
        .flat_map(|(id, func)| {
            func.instructions().filter(move |&inst| {
                checked_alloc(func, inst).is_some() && !fresh.contains(id, inst)
            })
        })
        .count();
    if count < 2 {
        return false;
    }

    let helper = create_checked_alloc(module);
    run_lowering_function_pass(module, analyses, |func, function_analyses| {
        let func_id = function_analyses.function();
        let mut replacements = FxHashMap::default();
        for block in func.blocks.indices() {
            let mut builder = FunctionBuilder::new(func);
            expand_block(&mut builder, block, |builder, inst| {
                let Some((size, semantics)) = checked_alloc(builder.func(), inst) else {
                    return false;
                };
                if fresh.contains(func_id, inst) {
                    return false;
                }
                let result =
                    builder.func().inst_result_value(inst).expect("allocation has a result");
                // object = icall checked_alloc size
                let object = builder.icall(helper, vec![size], MirType::MEMPTR);
                initialize(builder, object, size, fresh.initialization(func_id, inst, semantics));
                replacements.insert(result, object);
                true
            });
        }
        func.replace_uses_canonicalized(&replacements);
        true
    })
}

/// The size and semantics of `inst` if it is a checked allocation that lowering expands.
fn checked_alloc(func: &Function, inst: InstId) -> Option<(ValueId, AllocationSemantics)> {
    let instruction = func.inst(inst);
    let InstKind::Alloc { size, semantics } = instruction.kind else { return None };
    (semantics.failure == AllocationFailure::Checked && !instruction.metadata.deferred_alloc())
        .then_some((size, semantics))
}

fn create_checked_alloc(module: &mut Module) -> FunctionId {
    let mut function = Function::new(Ident::with_dummy_span(sym::alloc));
    {
        let mut builder = FunctionBuilder::new(&mut function);
        let size = builder.add_param(MirType::I256);
        builder.set_return_type(MirType::MEMPTR);
        let slot = builder.imm(EvmMemoryLayout::FMP_SLOT);
        // ptr = inttoptr (load memory 64) to ptr<memory>
        let free = builder.load(slot, PtrLocation::Memory);
        mark_alloc_ptr(&mut builder, free);
        let ptr = builder.cast(free, MirType::MEMPTR);
        let (next, invalid) = bump_checked(&mut builder, ptr, size);
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

fn lower_function(func: &mut Function, fresh: &FreshHeap, id: FunctionId) -> bool {
    let is_fresh = |inst| fresh.contains(id, inst);
    let has_abstract_memory = func.instructions().any(|inst| {
        matches!(func.inst(inst).kind, InstKind::Fmp | InstKind::SetFmp(_) | InstKind::Alloc { .. })
    });
    if !has_abstract_memory {
        return false;
    }

    let mut changed = false;
    for block in func.blocks.indices() {
        let mut builder = FunctionBuilder::new(func);
        expand_block(&mut builder, block, |builder, inst| {
            let Some((size, semantics)) = checked_alloc(builder.func(), inst) else {
                return false;
            };
            changed = true;
            let ptr =
                builder.func().inst_result_value(inst).expect("allocation must produce a value");
            let continuation = builder.create_block();
            let panic = builder.create_block();
            // ptr = mload 64 | heap_floor
            rewrite_as_fmp_read(builder, inst, is_fresh(inst));
            // next = add ptr, size
            // invalid = next < ptr || next > limit
            let (next, invalid) = bump_checked(builder, ptr, size);
            // branch invalid, panic, continuation
            builder.branch(invalid, panic, continuation);

            // panic: revert Panic(MemoryAllocationOverflow)
            builder.switch_to_block(panic);
            builder.panic(PanicCode::MemoryAllocationOverflow);

            // continuation: mstore 64, next
            builder.switch_to_block(continuation);
            store_fmp(builder, next);
            initialize(builder, ptr, size, fresh.initialization(id, inst, semantics));
            true
        });
    }

    let stable_empty = stable_empty_allocs(func);
    let mut peek = FmpPeek::default();
    for block in func.blocks.indices() {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        peek.clear();
        for inst in instructions {
            let context = builder.func().inst(inst).metadata.debug_context();
            builder.set_debug_context(&context);
            // Allocations and pointer updates move the pointer past any earlier read.
            let moves_fmp = matches!(
                builder.func().inst(inst).kind,
                InstKind::Alloc { .. } | InstKind::SetFmp(_)
            );
            match builder.func().inst(inst).kind {
                InstKind::Fmp => {
                    changed = true;
                    rewrite_as_fmp_read(&mut builder, inst, is_fresh(inst));
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
                    builder.func_mut().set_inst_kind(inst, InstKind::IntToPtr(word));
                    let instruction = builder.func_mut().inst_mut(inst);
                    instruction.metadata.set_effect(None);
                    instruction.metadata.set_preserves_fmp(false);
                    instruction.metadata.set_memory_region(Some(MemoryRegion::Scratch));
                    builder.func_mut().blocks[block].instructions.push(inst);
                    let next = bump(&mut builder, word, size);
                    store_fmp(&mut builder, next);
                    initialize(&mut builder, ptr, size, fresh.initialization(id, inst, semantics));
                }
                // memory_zero ptr, size | mstore ptr, 0 -> dropped
                InstKind::MemoryZero(..) | InstKind::Store(..)
                    if fresh.skips_zero_fill(id, inst) =>
                {
                    changed = true;
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
                InstKind::Alloc { size, semantics }
                    if builder.func().inst(inst).metadata.zero_slot_if_empty() =>
                {
                    debug_assert_eq!(semantics, AllocationSemantics::INTERNAL);
                    changed = true;
                    if stable_empty.contains(&inst) {
                        // ptr = mload 64 | heap_floor
                        // empty = size == 32
                        // mstore 64, ptr + (empty ? 0 : size)
                        let ptr = builder
                            .func()
                            .inst_result_value(inst)
                            .expect("allocation must produce a value");
                        rewrite_as_fmp_read(&mut builder, inst, is_fresh(inst));
                        let (_, reserved) = reserved_unless_empty(&mut builder, size);
                        let next = bump(&mut builder, ptr, reserved);
                        store_fmp(&mut builder, next);
                    } else {
                        rewrite_as_zero_slot_alloc(&mut builder, inst, size, is_fresh(inst));
                    }
                }
                InstKind::Alloc { size, semantics } => {
                    debug_assert_eq!(semantics.failure, AllocationFailure::Infallible);
                    changed = true;
                    let ptr = builder
                        .func()
                        .inst_result_value(inst)
                        .expect("allocation must produce a value");
                    rewrite_as_fmp_read(&mut builder, inst, is_fresh(inst));
                    let next = bump(&mut builder, ptr, size);
                    store_fmp(&mut builder, next);
                    initialize(&mut builder, ptr, size, fresh.initialization(id, inst, semantics));
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

/// The `zero_slot_if_empty` allocations whose empty object can stay unreserved.
///
/// An empty object that reserves nothing sits at the free-memory pointer, where the next
/// allocation or staged write would overwrite its length. It is safe when nothing reachable from
/// the allocation allocates, calls, or writes memory except through the object's own pointer,
/// and the pointer neither leaves through a return or a phi nor is stored. Then the object is
/// only read, as when a call's returndata is forwarded, and needs no pointer to the zero slot.
fn stable_empty_allocs(func: &Function) -> FxHashSet<InstId> {
    let mut stable = FxHashSet::default();
    for block in func.blocks.indices() {
        for (index, &inst) in func.blocks[block].instructions.iter().enumerate() {
            if matches!(func.inst(inst).kind, InstKind::Alloc { .. })
                && func.inst(inst).metadata.zero_slot_if_empty()
                && empty_object_is_stable(func, block, index)
            {
                stable.insert(inst);
            }
        }
    }
    stable
}

/// Whether the object of the allocation at `index` in `block` is only read until the function
/// ends or returns, as [`stable_empty_allocs`] defines.
fn empty_object_is_stable(func: &Function, block: BlockId, index: usize) -> bool {
    let alloc = func.blocks[block].instructions[index];
    let Some(result) = func.inst_result_value(alloc) else { return false };
    let mut derived = FxHashSet::from_iter([result]);
    let mut visited = FxHashSet::default();
    let mut pending = vec![(block, index + 1)];
    while let Some((block, start)) = pending.pop() {
        if start == 0 && !visited.insert(block) {
            continue;
        }
        let data = &func.blocks[block];
        for &inst in &data.instructions[start..] {
            let kind = &func.inst(inst).kind;
            let uses_object = kind.operands().iter().any(|operand| derived.contains(operand));
            let effect = kind.effect_kind();
            let keeps = match effect {
                EffectKind::Pure => !matches!(kind, InstKind::Phi(_)) || !uses_object,
                EffectKind::MemoryRead
                | EffectKind::StorageRead
                | EffectKind::StorageWrite
                | EffectKind::TransientRead
                | EffectKind::TransientWrite
                | EffectKind::EnvironmentRead
                | EffectKind::Log => true,
                EffectKind::MemoryWrite => match *kind {
                    InstKind::Store(ptr, value, PtrLocation::Memory) => {
                        derived.contains(&ptr) && !derived.contains(&value)
                    }
                    InstKind::Store8(ptr, _)
                    | InstKind::MemoryZero(ptr, _)
                    | InstKind::Copy(ptr, ..) => derived.contains(&ptr),
                    _ => false,
                },
                _ => false,
            };
            if !keeps {
                return false;
            }
            if uses_object
                && effect == EffectKind::Pure
                && let Some(value) = func.inst_result_value(inst)
            {
                derived.insert(value);
            }
        }
        let Some(terminator) = &data.terminator else { return false };
        let escapes = match terminator {
            Terminator::Return { .. } => {
                terminator.operands().iter().any(|operand| derived.contains(operand))
            }
            Terminator::TailCall { .. } => true,
            _ => false,
        };
        if escapes {
            return false;
        }
        pending.extend(terminator.successors().into_iter().map(|successor| (successor, 0)));
    }
    true
}

/// Rewrites the detached `inst` to read the pointer from its slot, or the heap floor when nothing
/// wrote the slot before it, and appends it to the current block after any pointer read it needs.
fn rewrite_as_fmp_read(builder: &mut FunctionBuilder<'_>, inst: InstId, fresh: bool) {
    // ptr = mload 64 | heap_floor
    // object = inttoptr ptr
    let kind = if builder.func().inst(inst).result_ty.is_some_and(MirType::is_pointer) {
        let ptr = if fresh {
            builder.heap_floor()
        } else {
            let slot = builder.imm(EvmMemoryLayout::FMP_SLOT);
            builder.load(slot, PtrLocation::Memory)
        };
        mark_alloc_ptr(builder, ptr);
        InstKind::IntToPtr(ptr)
    } else if fresh {
        InstKind::HeapFloor
    } else {
        InstKind::Load(builder.imm(EvmMemoryLayout::FMP_SLOT), PtrLocation::Memory)
    };
    let is_load = matches!(kind, InstKind::Load(..));
    builder.func_mut().set_inst_kind(inst, kind);
    let instruction = builder.func_mut().inst_mut(inst);
    instruction.metadata.set_effect(None);
    instruction.metadata.set_preserves_fmp(false);
    instruction.metadata.set_memory_region(Some(MemoryRegion::Scratch));
    instruction.metadata.set_alloc_ptr(is_load);
    let block = builder.current_block();
    builder.func_mut().blocks[block].instructions.push(inst);
}

/// Emits whether an allocation of `size` is an empty object, of one header word, and the size it
/// reserves, which is nothing for an empty object.
fn reserved_unless_empty(builder: &mut FunctionBuilder<'_>, size: ValueId) -> (ValueId, ValueId) {
    // empty = size == 32
    // reserved = empty ? 0 : size
    let header = builder.imm(EvmMemoryLayout::DYNAMIC_HEADER_SIZE);
    let empty = builder.eq(size, header);
    let zero = builder.imm(0);
    (empty, builder.select(empty, zero, size))
}

/// Rewrites the detached `inst`, an allocation that is the zero slot when `size` is one header
/// word, to reserve nothing and return the zero slot in that case, and appends it to the current
/// block after the pointer arithmetic it needs.
fn rewrite_as_zero_slot_alloc(
    builder: &mut FunctionBuilder<'_>,
    inst: InstId,
    size: ValueId,
    fresh: bool,
) {
    // ptr = inttoptr (mload 64 | heap_floor)
    // empty = size == 32
    // mstore 64, ptr + (empty ? 0 : size)
    // object = empty ? ZERO_SLOT : ptr
    let word = if fresh {
        builder.heap_floor()
    } else {
        let slot = builder.imm(EvmMemoryLayout::FMP_SLOT);
        builder.load(slot, PtrLocation::Memory)
    };
    mark_alloc_ptr(builder, word);
    let ptr = builder.cast(word, MirType::MEMPTR);
    let (empty, reserved) = reserved_unless_empty(builder, size);
    let next = bump(builder, ptr, reserved);
    store_fmp(builder, next);
    let zero_slot = builder.zero_slot();
    builder.func_mut().set_inst_kind(inst, InstKind::Select(empty, zero_slot, ptr));
    let instruction = builder.func_mut().inst_mut(inst);
    instruction.metadata.set_effect(None);
    let block = builder.current_block();
    builder.func_mut().blocks[block].instructions.push(inst);
}

fn rewrite_as_fmp_store(builder: &mut FunctionBuilder<'_>, inst: InstId, ptr: ValueId) {
    // word = ptrtoint ptr to i256
    // set_fmp ptr -> mstore 64, word
    let ptr = builder.cast(ptr, MirType::I256);
    let slot = builder.imm(EvmMemoryLayout::FMP_SLOT);
    builder.func_mut().set_inst_kind(inst, InstKind::Store(slot, ptr, PtrLocation::Memory));
    let instruction = builder.func_mut().inst_mut(inst);
    instruction.metadata.set_effect(None);
    instruction.metadata.set_preserves_fmp(false);
    instruction.metadata.set_memory_region(Some(MemoryRegion::Scratch));
}

/// Emits `ptr + size`, marked so the sum keeps the bound of `ptr`. A wrapped sum is unreachable:
/// a checked allocation tests it, and an unchecked one is followed by the write that fills the
/// range, which runs out of gas first.
fn bump(builder: &mut FunctionBuilder<'_>, ptr: ValueId, size: ValueId) -> ValueId {
    // next = add ptr, size !metadata(alloc_ptr)
    let next = builder.add(ptr, size);
    mark_alloc_ptr(builder, next);
    next
}

/// Emits `ptr + size` and whether it wrapped or passes the allocation limit.
fn bump_checked(
    builder: &mut FunctionBuilder<'_>,
    ptr: ValueId,
    size: ValueId,
) -> (ValueId, ValueId) {
    let next = bump(builder, ptr, size);
    // invalid = next < ptr || next > limit
    let bump_overflow = builder.lt(next, ptr);
    let limit = builder.imm(EvmMemoryLayout::MAX_ALLOCATION_END);
    let over_limit = builder.gt(next, limit);
    (next, builder.or(bump_overflow, over_limit))
}

/// Marks the instruction that defines `value` as a word the allocator produced, a heap pointer
/// whatever assembly stored in the free-memory-pointer slot.
fn mark_alloc_ptr(builder: &mut FunctionBuilder<'_>, value: ValueId) {
    if let Value::Inst(inst) = builder.func().value(value) {
        let inst = *inst;
        builder.func_mut().inst_mut(inst).metadata.set_alloc_ptr(true);
    }
}

fn store_fmp(builder: &mut FunctionBuilder<'_>, ptr: ValueId) {
    let slot = builder.imm(EvmMemoryLayout::FMP_SLOT);
    builder.store(slot, ptr);
}

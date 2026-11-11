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
//!
//! With `-Zcheck-analyses`, both answers of [`FreshHeap`] are recorded where they are used. A
//! fresh read also loads the slot and records the two equal, `assume eq lower_alloc, floor, word`;
//! the load makes codegen initialize the slot at the entry, so the check compares the floor with
//! what the slot holds unless something wrote it. A dropped fill reads back words of the range it
//! would have cleared and records each zero: every word when the range is a constant of at most
//! [`ZERO_FILL_SAMPLE_WORDS`] words, its first and last words otherwise, and the bytes of a range
//! shorter than a word through a shift of the word that starts it. A wrong answer shows at the
//! ends of a range: a buffer laid out earlier, by this function or a callee, starts at the old
//! pointer, which is where the range starts, and a write the analysis missed covers whole words.
//! Each sampled word is a load and a comparison, so the sample stays small and fixed rather than
//! growing with the range, which may be up to [`EvmMemoryLayout::MAX_DEAD_FILL`] bytes. Fills are
//! dropped only in modules that do not read `msize`, so the memory a read past the range of a
//! short fill expands changes the gas of a check build only.

use crate::mir::{
    AllocationFailure, AllocationInitialization, AllocationSemantics, AssumePredicate, EffectKind,
    FactSource, Function, FunctionBuilder, FunctionId, InstId, InstKind, MemoryRegion, MirType,
    Module, PanicCode, PtrLocation, Value, ValueId,
    analysis::FreshHeap,
    memory::EvmMemoryLayout,
    pass::{MirPass, ModuleAnalyses, run_lowering_function_pass},
    transform::{
        lower_assumes::{record_fact, should_check_analyses},
        utils::expand_block,
    },
};
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_interface::{Ident, sym};
use solar_sema::Gcx;

/// Lowers `fmp` and `alloc` instructions.
pub(crate) struct LowerAlloc;

impl MirPass for LowerAlloc {
    fn name(&self) -> &'static str {
        "lower-alloc"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        let fresh = FreshHeap::new(module, || analyses.call_summaries(module));
        lower_alloc(module, analyses, &fresh, should_check_analyses(gcx.sess))
    }
}

/// Lowers allocation, recording the answers of `fresh` it used with `record`.
fn lower_alloc(
    module: &mut Module,
    analyses: &mut ModuleAnalyses,
    fresh: &FreshHeap,
    record: bool,
) -> bool {
    let mut changed = share_checked_allocations(module, analyses, fresh, record);
    changed |= run_lowering_function_pass(module, analyses, |func, function_analyses| {
        let id = function_analyses.function();
        lower_function(func, fresh, id, record)
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
    record: bool,
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
                let initialization = fresh.initialization(func_id, inst, semantics);
                initialize(builder, object, size, semantics, initialization, record);
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
    function.mark_unlocated_compiler_generated();
    module.add_function(function)
}

fn lower_function(func: &mut Function, fresh: &FreshHeap, id: FunctionId, record: bool) -> bool {
    let is_fresh = |inst| fresh.contains(id, inst);
    let has_abstract_memory = func
        .instructions()
        .any(|inst| matches!(func.inst(inst).kind, InstKind::Fmp | InstKind::Alloc { .. }));
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
            rewrite_as_fmp_read(builder, inst, is_fresh(inst), record);
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
            let initialization = fresh.initialization(id, inst, semantics);
            initialize(builder, ptr, size, semantics, initialization, record);
            true
        });
    }

    let mut peek = FmpPeek::default();
    for block in func.blocks.indices() {
        let instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        peek.clear();
        for inst in instructions {
            let context = builder.func().inst(inst).metadata.debug_context();
            builder.set_debug_context(&context);
            // Allocations move the pointer past any earlier read.
            let moves_fmp = matches!(builder.func().inst(inst).kind, InstKind::Alloc { .. });
            match builder.func().inst(inst).kind {
                InstKind::Fmp => {
                    changed = true;
                    rewrite_as_fmp_read(&mut builder, inst, is_fresh(inst), record);
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
                    let initialization = fresh.initialization(id, inst, semantics);
                    initialize(&mut builder, ptr, size, semantics, initialization, record);
                }
                // memory_zero ptr, size | mstore ptr, 0 -> dropped
                InstKind::MemoryZero(..) | InstKind::Store(..)
                    if fresh.skips_zero_fill(id, inst) =>
                {
                    changed = true;
                    if record {
                        let (ptr, size) = match builder.func().inst(inst).kind {
                            InstKind::MemoryZero(ptr, size) => (ptr, size),
                            InstKind::Store(ptr, ..) => (ptr, builder.imm(32)),
                            _ => unreachable!(),
                        };
                        record_zero_range(&mut builder, ptr, size);
                    }
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
                    rewrite_as_fmp_read(&mut builder, inst, is_fresh(inst), record);
                    let next = bump(&mut builder, ptr, size);
                    store_fmp(&mut builder, next);
                    let initialization = fresh.initialization(id, inst, semantics);
                    initialize(&mut builder, ptr, size, semantics, initialization, record);
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
/// as solc's `allocate_unbounded` does. Any call, allocation, or memory write
/// not addressed from the read word ends the peek.
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

/// Initializes an allocation of `semantics` as `initialization` says, which is uninitialized when
/// its zero fill is dead; with `record`, a dropped fill records the memory zero instead.
fn initialize(
    builder: &mut FunctionBuilder<'_>,
    ptr: ValueId,
    size: ValueId,
    semantics: AllocationSemantics,
    initialization: AllocationInitialization,
    record: bool,
) {
    if initialization == AllocationInitialization::Zeroed {
        builder.memory_zero(ptr, size);
    } else if record && semantics.initialization == AllocationInitialization::Zeroed {
        record_zero_range(builder, ptr, size);
    }
}

/// The most words of a dropped zero fill of constant size that a check build reads back one by
/// one; a longer or dynamic range reads its first and last words.
const ZERO_FILL_SAMPLE_WORDS: u64 = 8;

/// Records that the `size` bytes at `ptr`, a zero fill [`FreshHeap`] found dead, are zero, by
/// reading back a sample of their words; see the module docs.
fn record_zero_range(builder: &mut FunctionBuilder<'_>, ptr: ValueId, size: ValueId) {
    // Memory objects are lowered by now, so addresses are words.
    // ptr = ptrtoint ptr<memory> ptr to i256
    let ptr = if builder.func().value_ty(ptr).is_some_and(MirType::is_pointer) {
        builder.cast(ptr, MirType::I256)
    } else {
        ptr
    };
    let zero = builder.imm(0);
    let record_zero = |builder: &mut FunctionBuilder<'_>, word| {
        let block = builder.current_block();
        // assume eq lower_alloc, word, 0
        let func = builder.func_mut();
        record_fact(func, block, FactSource::LowerAlloc, AssumePredicate::Eq, word, zero);
    };
    let Some(size) = builder.func().value_u64(size) else {
        // short = lt size, 32; covered = select short, size, 32
        // shift = shl (sub 32, covered), 3
        // first = shr (mload ptr), shift; last = shr (mload (add ptr, size - covered)), shift
        let word = builder.imm(32);
        let short = builder.lt(size, word);
        let covered = builder.select(short, size, word);
        let uncovered = builder.sub(word, covered);
        let three = builder.imm(3);
        let shift = builder.shl(uncovered, three);
        let start = builder.load(ptr, PtrLocation::Memory);
        let first = builder.shr(start, shift);
        record_zero(builder, first);
        let offset = builder.sub(size, covered);
        let address = builder.add(ptr, offset);
        let end = builder.load(address, PtrLocation::Memory);
        let last = builder.shr(end, shift);
        record_zero(builder, last);
        return;
    };
    if size == 0 {
        return;
    }
    if size < 32 {
        // high = shr (mload ptr), 8 * (32 - size)
        let word = builder.load(ptr, PtrLocation::Memory);
        let shift = builder.imm(8 * (32 - size));
        let high = builder.shr(word, shift);
        record_zero(builder, high);
        return;
    }
    let last = size - 32;
    let mut offsets = if size <= 32 * ZERO_FILL_SAMPLE_WORDS {
        (0..size / 32).map(|word| word * 32).collect::<Vec<_>>()
    } else {
        vec![0]
    };
    offsets.push(last);
    offsets.dedup();
    for offset in offsets {
        // word = mload (add ptr, offset)
        let address = if offset == 0 {
            ptr
        } else {
            let offset = builder.imm(offset);
            builder.add(ptr, offset)
        };
        let word = builder.load(address, PtrLocation::Memory);
        record_zero(builder, word);
    }
}

/// Rewrites the detached `inst` to read the pointer from its slot, or the heap floor when nothing
/// wrote the slot before it, and appends it to the current block after any pointer read it needs.
/// With `record`, a heap floor read records that the slot holds the floor.
fn rewrite_as_fmp_read(builder: &mut FunctionBuilder<'_>, inst: InstId, fresh: bool, record: bool) {
    // ptr = mload 64 | heap_floor
    // object = inttoptr ptr
    let mut floor = None;
    let kind = if builder.func().inst(inst).result_ty.is_some_and(MirType::is_pointer) {
        let ptr = if fresh {
            let ptr = builder.heap_floor();
            floor = Some(ptr);
            ptr
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
    if record && fresh {
        let floor = floor.unwrap_or_else(|| {
            builder.func().inst_result_value(inst).expect("a pointer read has a result")
        });
        // word = mload 64
        // assume eq lower_alloc, floor, word
        let slot = builder.imm(EvmMemoryLayout::FMP_SLOT);
        let word = builder.load(slot, PtrLocation::Memory);
        let func = builder.func_mut();
        record_fact(func, block, FactSource::LowerAlloc, AssumePredicate::Eq, floor, word);
    }
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

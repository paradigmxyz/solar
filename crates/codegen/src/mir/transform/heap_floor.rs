//! Keeps the free memory pointer at or above the heap floor where inline assembly lowers it.
//!
//! Codegen places spill slots and internal-call frames below each entry's initial free memory
//! pointer (CODEGEN-009), where `solc` keeps those words on the stack. Memory-unsafe assembly can
//! hand that memory back to the heap by storing an absolute address into the pointer's slot:
//! Seaport moves the pointer to just past the event data it lays out at an address that calldata
//! sizes, and resets it to `0x80` after batch transfers. The next allocation then overwrites live
//! compiler words. This pass raises each word that such a store, `mstore 64, v` with `v` computed
//! from constants and calldata alone, leaves in the slot to
//!
//! ```text
//! floor = heap_floor
//! below = lt v, floor
//! clamped = select below, floor, v
//! ```
//!
//! where `heap_floor` is the initial pointer of every entry that can run the function, which
//! codegen fixes once it has placed the frames. A pointer derived from the heap, or loaded from
//! memory or a parameter, already lies above the floor in a valid program and stays as stored.
//!
//! The slot can also hold plain data: an error argument before a revert, or a hash input before the
//! pointer is restored. A forward search tells the reads apart. It follows the stored word through
//! blocks, into internal calls, and from returns back to every call site, until stores, copies, and
//! zeroings at constant addresses have replaced all of its bytes, as Seaport's token transfers do
//! when they lay call data out over the slot, or execution ends. An `mload` of the slot reads the
//! word as the pointer when the loaded word, or one computed from it, addresses or sizes memory or
//! becomes the pointer again, in the function, in a callee it is passed to, or in a caller it is
//! returned to, as every allocation in assembly does; a loaded word only compared, stored, or
//! returned from an external function as integers is data. A load that uses its word both ways
//! reads it both as the pointer and as data. The compiler's own free-memory-pointer reads and
//! allocations read the word as the pointer too, and so do the builtins and semantic operations
//! whose lowering may allocate or encode at the pointer, unlike pure computations, checks that
//! revert with scratch data, and accesses to frames, storage slots, and memory that already exists.
//! An access that may cover a byte the writes have not replaced reads the word as data, such as a
//! scratch hash or revert data, unless its address is computed from heap pointers: the free memory
//! pointer, an allocation, a memory object, or a parameter every call fills with one. Any other
//! address may be low, as a word loaded from memory is zero where nothing has written it and an
//! external function's integer arguments come from calldata. A return from the constructor, or one
//! from an external function whose results all have static ABI types, ends execution without
//! reading the slot: codegen returns runtime code from a fixed address and encodes static results
//! in a static buffer. An external function returns to the dispatcher even when internal calls
//! reach it too.
//!
//! A word read only as the pointer is clamped where it is stored, `mstore 64, clamped`. A word that
//! is also read as data keeps its value in the slot, and the clamp moves to each pointer read that
//! the search reaches. The pointer uses of an assembly load read the clamped word in place of the
//! one it loads; when the load's word is also read as data, the words computed from it on the way
//! to its pointer uses get clamped copies, and its data uses keep the loaded word. A word that an
//! external function returns as data stays unclamped at the return, and an internal caller that
//! reads the word as the pointer clamps the result of its call instead, or the fields of a returned
//! struct that carry it. A function's loads are clamped together, so a word computed from several
//! of them, such as a phi of two branches' loads, is clamped on every path. A copy of checked
//! arithmetic wraps, and the original keeps its check on the loaded words. A word subtracted from
//! another yields a distance rather than a pointer, so it stays as computed. The slot itself is
//! clamped right before a compiler read, before a call that reads the slot only as the pointer,
//! and before a return to the dispatcher:
//!
//! ```text
//! word = mload 64
//! floor = heap_floor
//! below = lt word, floor
//! clamped = select below, floor, word
//! mstore 64, clamped
//! ```
//!
//! A callee that also reads the slot as data is entered and clamped inside. A helper that stores
//! a scratch word thus keeps it for a caller that hashes it, while a caller that allocates from it
//! clamps it on its own path.
//!
//! The pass runs first, on semantic MIR, so the passes that forward the slot's stored value to
//! its loads or delete its dead stores see the clamped value. Clamping during codegen instead
//! left a load forwarded from the store with the raw address.
//!
//! NOTE: Only a full-word store to the constant slot whose value the storing function computes from
//! constants and calldata is clamped. An absolute address passed through a parameter, memory, or a
//! call, and copies or byte stores that overwrite the pointer, keep their bytes. An access at a
//! heap pointer offset by any word is assumed not to cover the slot, and so is one at a memory
//! object that a call returns, while a loaded word stored elsewhere stays data even if a later load
//! uses it as a pointer. A slot clamped before a compiler read stays clamped for a later read of it
//! as data. Writes replace the word only when one block covers all of its bytes. A loaded word
//! passed to a callee or returned to callers that read it as a pointer is clamped as a whole there,
//! even if they also read it as data, and a word an external function returns as data is clamped
//! as a whole in each internal caller that reads it as a pointer. See CODEGEN-010.

use crate::mir::{
    AbiType, ArgIdx, BlockId, Builtin, Callee, CheckedOp, EffectKind, Function, FunctionId,
    Immediate, InstId, InstKind, Instruction, InstructionMetadata, MemoryRegion, MirType, Module,
    Terminator, Value, ValueId,
    analysis::{AddressInput, absolute_address_inputs},
    memory::EvmMemoryLayout,
    pass::{MirPass, ModuleAnalyses},
    utils::{IndexLists, replace_inst_uses},
};
use alloy_primitives::U256;
use smallvec::{SmallVec, smallvec};
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::IndexVec,
    map::{FxHashMap, FxHashSet, FxIndexMap, FxIndexSet},
};
use std::ops::BitOrAssign;

pub(crate) struct HeapFloor;

impl MirPass for HeapFloor {
    fn name(&self) -> &'static str {
        "heap-floor"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut ModuleAnalyses,
    ) -> bool {
        let (clamps, loads) = floor_clamps(module);
        let changed = !clamps.is_empty();
        for clamp in clamps {
            match clamp {
                Clamp::Store(func, block, inst) => {
                    clamp_store(&mut module.functions[func], block, inst);
                }
                // A function's loads are clamped together below.
                Clamp::Load(..) => {}
                Clamp::Slot(func, block, before) => {
                    clamp_slot(&mut module.functions[func], block, before);
                }
            }
        }
        for (func, plan) in &loads {
            clamp_loads(&mut module.functions[*func], plan);
        }
        changed
    }
}

/// Where the pass raises a word in the free memory pointer's slot to the heap floor.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Clamp {
    /// The word a store leaves in the slot.
    Store(FunctionId, BlockId, InstId),
    /// The word a load reads from the slot as the pointer, keeping the slot as it is.
    Load(FunctionId, BlockId, InstId),
    /// The slot, before an instruction or, without one, before the block's terminator.
    Slot(FunctionId, BlockId, Option<InstId>),
}

/// Finds where to clamp the words that stores of constants and calldata leave in the free memory
/// pointer's slot: at the store when every read of the word reads the pointer, and at each
/// pointer read when some read takes the word as data. The loads to clamp come with a plan for
/// each function, which covers all of the function's loads at once.
fn floor_clamps(module: &Module) -> (FxIndexSet<Clamp>, Vec<(FunctionId, LoadClamps)>) {
    let mut uses = PointerUses::new(module);
    let mut clamps = FxIndexSet::default();
    for (func_id, func) in module.functions.iter_enumerated() {
        for (block, data) in func.blocks.iter_enumerated() {
            for (index, &inst_id) in data.instructions.iter().enumerate() {
                if let InstKind::MStore(address, value) = func.inst(inst_id).kind
                    && is_slot(func, address)
                    && func.value_ty(value) == Some(MirType::I256)
                    && uses.address_inputs(func_id)[value] != AddressInput::Other
                {
                    let reads = uses.reads_after(func_id, block, index + 1);
                    match (reads.pointer, reads.data) {
                        (true, false) => {
                            clamps.insert(Clamp::Store(func_id, block, inst_id));
                        }
                        (true, true) => {
                            uses.clamp_pointer_reads(func_id, block, index + 1, &mut clamps);
                        }
                        (false, _) => {}
                    }
                }
            }
        }
    }
    let mut roots = FxIndexMap::<_, Vec<_>>::default();
    for &clamp in &clamps {
        if let Clamp::Load(func_id, block, load) = clamp {
            roots.entry(func_id).or_default().push((block, load));
        }
    }
    // Planning a function can hand a returned word's clamp to the callers that receive it, whose
    // plans then take those calls as roots too.
    let mut plans = FxIndexMap::default();
    let mut pending = roots.keys().copied().collect::<Vec<_>>();
    while let Some(func_id) = pending.pop() {
        let plan = uses.load_clamps(func_id, roots[&func_id].clone());
        plans.insert(func_id, plan);
        for (caller, block, root) in std::mem::take(&mut uses.caller_roots) {
            let caller_roots = roots.entry(caller).or_default();
            if !caller_roots.contains(&(block, root)) {
                caller_roots.push((block, root));
                pending.push(caller);
            }
        }
    }
    (clamps, plans.into_iter().collect())
}

/// Rewrites `mstore 64, v` into `mstore 64, max(v, heap_floor)`.
fn clamp_store(func: &mut Function, block: BlockId, inst: InstId) {
    let InstKind::MStore(address, value) = func.inst(inst).kind else {
        unreachable!("a floor store is an `mstore`")
    };
    let metadata = func.inst(inst).metadata.clone();
    // floor = heap_floor
    // below = lt v, floor
    // clamped = select below, floor, v
    let (clamp, clamped) = floor_max(func, value, &metadata);
    let position = index_in_block(func, block, inst);
    func.blocks[block].instructions.splice(position..position, clamp);
    // mstore 64, clamped
    func.inst_mut(inst).kind = InstKind::MStore(address, clamped);
}

/// Raises the words a function's loads read from the slot, and the returned words its calls
/// receive, to the heap floor where they are read as the pointer. A word that no other use reads
/// is rewritten in place; one that another use still reads unclamped, or that a checked operation
/// computes, gets a clamped copy for its pointer uses, computed with wrapping arithmetic so that
/// the copy checks nothing.
fn clamp_loads(func: &mut Function, plan: &LoadClamps) {
    let blocks = func.inst_block_table();
    let mut copies = FxHashMap::default();
    for &(block, load) in &plan.loads {
        let word = func.inst_result_value(load).expect("a root produces a word");
        let metadata = func.inst(load).metadata.clone();
        let (clamp, clamped) = floor_max(func, word, &metadata);
        if !plan.copied.contains(&word) {
            // The clamp is not placed yet, so it keeps reading the loaded word.
            func.replace_uses(&FxHashMap::from_iter([(word, clamped)]));
        }
        copies.insert(word, clamped);
        // word = mload 64 | icall @f | extract_value result, i
        // floor = heap_floor
        // below = lt word, floor
        // clamped = select below, floor, word
        let position = index_in_block(func, block, load) + 1;
        func.blocks[block].instructions.splice(position..position, clamp);
    }
    let mut rewritten = Vec::new();
    let mut placed = Vec::new();
    for &value in &plan.words {
        let Value::Inst(original) = *func.value(value) else {
            unreachable!("a word computed from a load has an instruction")
        };
        if !plan.copied.contains(&value) {
            rewritten.push(original);
            continue;
        }
        let instruction = func.inst(original);
        // checked_add | checked_sub | checked_mul a, b -> add | sub | mul a, b
        let kind = match instruction.kind {
            InstKind::CheckedBinary { op: CheckedOp::Add, lhs, rhs, .. } => InstKind::Add(lhs, rhs),
            InstKind::CheckedBinary { op: CheckedOp::Sub, lhs, rhs, .. } => InstKind::Sub(lhs, rhs),
            InstKind::CheckedBinary { op: CheckedOp::Mul, lhs, rhs, .. } => InstKind::Mul(lhs, rhs),
            _ => instruction.kind.clone(),
        };
        let mut copy = Instruction::new(kind, instruction.result_ty);
        copy.metadata.copy_debug_context(&instruction.metadata);
        let (copy, copy_value) = func.alloc_value_inst(copy);
        copies.insert(value, copy_value);
        rewritten.push(copy);
        placed.push((original, copy));
    }
    // op(words) -> op(clamped words)
    for inst in rewritten {
        replace_inst_uses(func.inst_mut(inst), &copies);
    }
    for (original, copy) in placed {
        let original_block = blocks[original].expect("a derived word's instruction is placed");
        let position = index_in_block(func, original_block, original) + 1;
        func.blocks[original_block].instructions.insert(position, copy);
    }
    // pointer use(word) -> pointer use(clamped copy)
    for (site, value) in &plan.sites {
        let Some(&copy) = copies.get(value) else { continue };
        let swap = |operand: &mut ValueId| {
            if *operand == *value {
                *operand = copy;
            }
        };
        match site {
            UseSite::Inst(inst) => {
                let slot_store = matches!(
                    func.inst(*inst).kind,
                    InstKind::MStore(address, _) if is_slot(func, address)
                );
                let mut kind = func.inst(*inst).kind.clone();
                if let Some(operands) = pointer_operands_mut(&mut kind, slot_store) {
                    operands.into_iter().for_each(swap);
                } else {
                    kind.visit_operands_mut(swap);
                }
                func.inst_mut(*inst).kind = kind;
            }
            UseSite::Args(inst, positions) => {
                if let InstKind::ICall { args, .. } = &mut func.inst_mut(*inst).kind {
                    for &index in positions {
                        args[index] = copy;
                    }
                }
            }
            UseSite::Terminator(block, positions) => {
                let terminator =
                    func.blocks[*block].terminator.as_mut().expect("a terminator uses the word");
                match (terminator, positions) {
                    (Terminator::TailCall { args, .. }, Some(positions)) => {
                        for &index in positions {
                            args[index] = copy;
                        }
                    }
                    (terminator, _) => terminator.visit_operands_mut(swap),
                }
            }
        }
    }
}

/// Raises the slot's word to the heap floor before `before`, or before the block's terminator.
fn clamp_slot(func: &mut Function, block: BlockId, before: Option<InstId>) {
    let metadata = before.map(|inst| func.inst(inst).metadata.clone()).unwrap_or_default();
    let emit = |kind, ty| {
        let mut instruction = Instruction::new(kind, ty);
        instruction.metadata.copy_debug_context(&metadata);
        instruction.metadata.set_memory_region(Some(MemoryRegion::Scratch));
        instruction
    };
    let slot =
        func.alloc_value(Value::Immediate(Immediate::I256(U256::from(EvmMemoryLayout::FMP_SLOT))));
    let (load, word) = func.alloc_value_inst(emit(InstKind::MLoad(slot), Some(MirType::I256)));
    let (clamp, clamped) = floor_max(func, word, &metadata);
    let store = func.alloc_inst(emit(InstKind::MStore(slot, clamped), None));
    let position = before
        .map_or(func.blocks[block].instructions.len(), |inst| index_in_block(func, block, inst));
    // word = mload 64
    // floor = heap_floor
    // below = lt word, floor
    // clamped = select below, floor, word
    // mstore 64, clamped
    let sequence = std::iter::once(load).chain(clamp).chain(std::iter::once(store));
    func.blocks[block].instructions.splice(position..position, sequence);
}

/// Allocates `max(word, heap_floor)`, returning its instructions in order and its value.
fn floor_max(
    func: &mut Function,
    word: ValueId,
    metadata: &InstructionMetadata,
) -> ([InstId; 3], ValueId) {
    let emit = |func: &mut Function, kind, ty| {
        let mut instruction = Instruction::new(kind, Some(ty));
        instruction.metadata.copy_debug_context(metadata);
        func.alloc_value_inst(instruction)
    };
    // floor = heap_floor
    // below = lt word, floor
    // clamped = select below, floor, word
    let (floor_inst, floor) = emit(func, InstKind::HeapFloor, MirType::I256);
    let (below_inst, below) = emit(func, InstKind::Lt(word, floor), MirType::I1);
    let (clamped_inst, clamped) = emit(func, InstKind::Select(below, floor, word), MirType::I256);
    ([floor_inst, below_inst, clamped_inst], clamped)
}

/// Returns the position of `inst` in `block`.
fn index_in_block(func: &Function, block: BlockId, inst: InstId) -> usize {
    func.blocks[block]
        .instructions
        .iter()
        .position(|&id| id == inst)
        .expect("the instruction is in its block")
}

/// Whether a value is the address of the free memory pointer's slot.
fn is_slot(func: &Function, address: ValueId) -> bool {
    func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT)
}

/// Whether a function returns to code that ends execution without reading the free memory
/// pointer: the constructor, whose runtime code codegen copies to a fixed address, or an
/// external function whose results all have static ABI types, which codegen encodes in a static
/// buffer.
fn returns_to_the_end(func: &Function) -> bool {
    func.attributes.is_constructor
        || func.is_external_entry()
            && func
                .abi_returns
                .as_ref()
                .is_none_or(|returns| !returns.types.iter().any(AbiType::is_dynamic))
}

/// Whether a return from `func`, which `sites` call, can reach code the module does not show: the
/// dispatcher, which returns from an external function even when internal calls reach it too, or
/// the caller of any other function no call reaches.
fn returns_out_of_module(func: &Function, sites: &[CallSite]) -> bool {
    func.is_external_entry() || sites.is_empty()
}

/// Whether a returned value holds integers alone, which an external function encodes as data.
fn is_scalar(module: &Module, ty: Option<MirType>) -> bool {
    match ty {
        Some(MirType::Int(_)) => true,
        Some(MirType::Struct(id)) => {
            module.struct_types[id].fields.iter().all(|&field| is_scalar(module, Some(field)))
        }
        _ => false,
    }
}

/// Returns the fields of the struct `value` that hold words in `derived`, when insertions into an
/// aggregate outside `derived` build it, or None when another instruction does.
fn word_fields(
    func: &Function,
    mut value: ValueId,
    derived: &FxIndexSet<ValueId>,
) -> Option<SmallVec<[u32; 2]>> {
    let mut fields = SmallVec::new();
    let mut inserted = SmallVec::<[u32; 4]>::new();
    loop {
        let Value::Inst(inst) = *func.value(value) else { return None };
        let InstKind::InsertValue { aggregate, index, value: field, .. } = func.inst(inst).kind
        else {
            return None;
        };
        // A later insertion at the same index replaces this field.
        if !inserted.contains(&index) {
            inserted.push(index);
            if derived.contains(&field) {
                fields.push(index);
            }
        }
        if !derived.contains(&aggregate) {
            return Some(fields);
        }
        value = aggregate;
    }
}

/// The bytes of the free memory pointer's slot that writes have replaced since a store, one bit
/// per byte.
type SlotBytes = u32;

/// Every byte of the slot.
const WHOLE_SLOT: SlotBytes = SlotBytes::MAX;

/// Returns the bytes of the slot that `size` bytes at `start` cover.
fn slot_bytes(start: u64, size: u64) -> SlotBytes {
    let slot = EvmMemoryLayout::FMP_SLOT;
    let first = start.max(slot);
    let end = start.saturating_add(size).min(slot + EvmMemoryLayout::WORD_SIZE);
    if first >= end {
        return 0;
    }
    let width = end - first;
    let bytes = if width == EvmMemoryLayout::WORD_SIZE { WHOLE_SLOT } else { (1 << width) - 1 };
    bytes << (first - slot)
}

/// The memory a store, copy, or zeroing writes at a constant address with a constant size.
fn constant_write_range(func: &Function, kind: &InstKind) -> Option<(u64, u64)> {
    let (dest, size) = match *kind {
        InstKind::MStore(dest, _) => {
            return func.value_u64(dest).map(|dest| (dest, EvmMemoryLayout::WORD_SIZE));
        }
        InstKind::MStore8(dest, _) => return func.value_u64(dest).map(|dest| (dest, 1)),
        InstKind::CalldataCopy(dest, _, size)
        | InstKind::CodeCopy(dest, _, size)
        | InstKind::ReturnDataCopy(dest, _, size)
        | InstKind::ExtCodeCopy(_, dest, _, size)
        | InstKind::DataCopy(_, dest, size)
        | InstKind::MCopy(dest, _, size)
        | InstKind::MemoryZero(dest, size) => (dest, size),
        _ => return None,
    };
    func.value_u64(dest).zip(func.value_u64(size))
}

/// The memory an instruction reads as data: an offset and a size, unknown when not constant.
fn read_range(func: &Function, kind: &InstKind) -> Option<(ValueId, Option<u64>)> {
    let (offset, size) = match *kind {
        InstKind::MLoad(offset) => return Some((offset, Some(EvmMemoryLayout::WORD_SIZE))),
        InstKind::MCopy(_, src, size) => (src, size),
        InstKind::Keccak256(offset, size)
        | InstKind::Log0(offset, size)
        | InstKind::Log1(offset, size, _)
        | InstKind::Log2(offset, size, ..)
        | InstKind::Log3(offset, size, ..)
        | InstKind::Log4(offset, size, ..)
        | InstKind::Create(_, offset, size)
        | InstKind::Create2(_, offset, size, _) => (offset, size),
        InstKind::Call { args_offset, args_size, .. }
        | InstKind::CallCode { args_offset, args_size, .. }
        | InstKind::StaticCall { args_offset, args_size, .. }
        | InstKind::DelegateCall { args_offset, args_size, .. } => (args_offset, args_size),
        _ => return None,
    };
    Some((offset, func.value_u64(size)))
}

/// Whether lowering a semantic operation may read the free memory pointer. Pure computations,
/// checks that revert with scratch data, and accesses to frames, storage slots, and memory that
/// already exists never do; allocations, encodings, and the other builtins may.
fn lowered_reads_pointer(kind: &InstKind) -> bool {
    kind.op_def().effect != EffectKind::Pure
        && !matches!(
            kind,
            InstKind::FrameLoad { .. }
                | InstKind::FrameStore { .. }
                | InstKind::MemoryZero(..)
                | InstKind::MemoryObjectLen(..)
                | InstKind::SetMemoryObjectLen(..)
                | InstKind::MemoryObjectLoadField { .. }
                | InstKind::MemoryObjectStoreField { .. }
                | InstKind::MemoryObjectLoadElement { .. }
                | InstKind::MemoryObjectLoadByte { .. }
                | InstKind::MemoryObjectStoreElement { .. }
                | InstKind::MemoryObjectStoreByte { .. }
                | InstKind::MemoryObjectStoreWord { .. }
                | InstKind::MemorySliceLoadWord { .. }
                | InstKind::CalldataSliceLoadWord { .. }
                | InstKind::Keccak256Bytes(..)
                | InstKind::MappingSlot(..)
                | InstKind::StorageArrayDataSlot(..)
                | InstKind::StorageArrayElementSlot { .. }
                | InstKind::StoreImmutable(..)
                | InstKind::ICall {
                    function: Callee::Builtin(
                        Builtin::Check { .. } | Builtin::CheckedAddMod | Builtin::CheckedMulMod
                    ),
                    ..
                }
        )
}

/// The instructions and terminators that read each value of a function.
fn value_users(func: &Function) -> IndexLists<ValueId, User> {
    let mut pairs = Vec::new();
    for (block, data) in func.blocks.iter_enumerated() {
        for &inst in &data.instructions {
            func.inst(inst).visit_operands(|operand| pairs.push((operand, User::Inst(inst))));
        }
        if let Some(terminator) = &data.terminator {
            terminator.visit_operands(|operand| pairs.push((operand, User::Terminator(block))));
        }
    }
    IndexLists::new(func.num_values(), pairs.iter().copied())
}

/// Returns the words every function computes from heap pointers. A parameter holds one when every
/// call passes one in and, in an external function, when it is a memory object, which the ABI
/// decodes into the heap; an external function's integers come from calldata, and a function no
/// call reaches may be given anything.
fn module_heap_addresses(module: &Module) -> IndexVec<FunctionId, DenseBitSet<ValueId>> {
    let mut called = DenseBitSet::new_empty(module.functions.len());
    for func in &module.functions {
        for inst in func.instructions() {
            if let InstKind::ICall { function: Callee::Function(callee), .. } = func.inst(inst).kind
            {
                called.insert(callee);
            }
        }
        for block in &func.blocks {
            if let Some(&Terminator::TailCall { function, .. }) = block.terminator.as_ref() {
                called.insert(function);
            }
        }
    }
    let mut params = module
        .functions
        .iter_enumerated()
        .map(|(func_id, func)| {
            func.arg_indices()
                .map(|index| {
                    if func.is_external_entry() {
                        func.arg_ty(index) == MirType::MemPtr
                    } else {
                        called.contains(func_id)
                    }
                })
                .collect::<IndexVec<ArgIdx, _>>()
        })
        .collect::<IndexVec<FunctionId, _>>();
    let mut heap = module
        .functions
        .iter_enumerated()
        .map(|(func_id, func)| function_heap_addresses(func, &params[func_id]))
        .collect::<IndexVec<FunctionId, _>>();
    let mut pending = module.functions.indices().collect::<Vec<_>>();
    while let Some(caller) = pending.pop() {
        let func = &module.functions[caller];
        let mut lowered = Vec::new();
        let mut pass = |callee: FunctionId, args: &[ValueId]| {
            for (index, &arg) in args.iter().enumerate() {
                if !heap[caller].contains(arg)
                    && let Some(param) = params[callee].get_mut(ArgIdx::new(index))
                    && *param
                {
                    *param = false;
                    lowered.push(callee);
                }
            }
        };
        for inst in func.instructions() {
            if let InstKind::ICall { function: Callee::Function(callee), args } =
                &func.inst(inst).kind
            {
                pass(*callee, args);
            }
        }
        for block in &func.blocks {
            if let Some(Terminator::TailCall { function, args }) = &block.terminator {
                pass(*function, args);
            }
        }
        for callee in lowered {
            heap[callee] = function_heap_addresses(&module.functions[callee], &params[callee]);
            pending.push(callee);
        }
    }
    heap
}

/// Returns the words `func` computes from heap pointers, with the parameters `params` marks as
/// holding one: the free memory pointer, allocations, and memory objects, a word that adds any
/// offset to one or subtracts one from it, and a choice between such words. Every candidate starts
/// as a heap pointer, and one whose inputs are not drops out until none does.
fn function_heap_addresses(
    func: &Function,
    params: &IndexVec<ArgIdx, bool>,
) -> DenseBitSet<ValueId> {
    let mut heap = DenseBitSet::new_empty(func.num_values());
    for index in 0..func.num_values() {
        let value = ValueId::new(index);
        let candidate = match *func.value(value) {
            Value::Arg(index) => params.get(index).copied().unwrap_or(false),
            Value::Inst(inst) => may_compute_heap_address(func, inst),
            _ => false,
        };
        if candidate {
            heap.insert(value);
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for inst in func.instructions() {
            if let Some(value) = func.inst_result_value(inst)
                && heap.contains(value)
                && !computes_heap_address(&func.inst(inst).kind, &heap)
            {
                heap.remove(value);
                changed = true;
            }
        }
    }
    heap
}

/// Whether an instruction may compute a heap pointer: the free memory pointer, an allocation, or
/// another memory object, or a word computed from such pointers.
fn may_compute_heap_address(func: &Function, inst: InstId) -> bool {
    let instruction = func.inst(inst);
    match instruction.kind {
        InstKind::Fmp
        | InstKind::Alloc { .. }
        | InstKind::Add(..)
        | InstKind::Sub(..)
        | InstKind::Select(..)
        | InstKind::Phi(_)
        | InstKind::Zext(_)
        | InstKind::Trunc(..)
        | InstKind::IntToPtr(_)
        | InstKind::PtrToInt(..) => true,
        InstKind::MLoad(address) => is_slot(func, address),
        _ => instruction.result_ty == Some(MirType::MemPtr),
    }
}

/// Whether an instruction computes a heap pointer from operands that `heap` holds: an offset added
/// to one, an offset subtracted from one, or a choice between them. The free memory pointer,
/// allocations, and memory objects are heap pointers themselves.
fn computes_heap_address(kind: &InstKind, heap: &DenseBitSet<ValueId>) -> bool {
    match kind {
        InstKind::Add(a, b) => heap.contains(*a) || heap.contains(*b),
        InstKind::Sub(a, b) => heap.contains(*a) && !heap.contains(*b),
        InstKind::Select(_, a, b) => heap.contains(*a) && heap.contains(*b),
        InstKind::Phi(incoming) => incoming.iter().all(|&(_, value)| heap.contains(value)),
        InstKind::Zext(operand)
        | InstKind::Trunc(operand, _)
        | InstKind::IntToPtr(operand)
        | InstKind::PtrToInt(operand, _) => heap.contains(*operand),
        _ => true,
    }
}

/// Whether an instruction computes a word from its operands, so that a word computed from a
/// pointer may be a pointer too.
fn is_derivation(kind: &InstKind) -> bool {
    matches!(
        kind,
        InstKind::Add(..)
            | InstKind::Sub(..)
            | InstKind::Mul(..)
            | InstKind::Div(..)
            | InstKind::SDiv(..)
            | InstKind::Mod(..)
            | InstKind::SMod(..)
            | InstKind::Exp(..)
            | InstKind::AddMod(..)
            | InstKind::MulMod(..)
            | InstKind::CheckedBinary { op: CheckedOp::Add | CheckedOp::Sub | CheckedOp::Mul, .. }
            | InstKind::And(..)
            | InstKind::Or(..)
            | InstKind::Xor(..)
            | InstKind::Not(..)
            | InstKind::Clz(..)
            | InstKind::Shl(..)
            | InstKind::Shr(..)
            | InstKind::Sar(..)
            | InstKind::Byte(..)
            | InstKind::SignExtend(..)
            | InstKind::Select(..)
            | InstKind::Phi(..)
            | InstKind::InsertValue { .. }
            | InstKind::ExtractValue { .. }
            | InstKind::Zext(..)
            | InstKind::Trunc(..)
            | InstKind::Sext(..)
            | InstKind::IntToPtr(..)
            | InstKind::PtrToInt(..)
    )
}

/// Whether an instruction computes from `word` a word that may still be a pointer: any derivation
/// but a subtraction of `word` from another word, which yields a distance.
fn derives_from(kind: &InstKind, word: ValueId) -> bool {
    match *kind {
        InstKind::Sub(lhs, _) | InstKind::CheckedBinary { op: CheckedOp::Sub, lhs, .. } => {
            lhs == word
        }
        _ => is_derivation(kind),
    }
}

/// Returns the operands an instruction uses as memory addresses, memory sizes, or, for a store to
/// the slot (`slot_store`), the free memory pointer. The other operands of the operations listed
/// are data, and an operation not listed may use any operand as a pointer.
fn pointer_operands_mut(
    kind: &mut InstKind,
    slot_store: bool,
) -> Option<SmallVec<[&mut ValueId; 4]>> {
    Some(match kind {
        InstKind::Lt(..)
        | InstKind::Gt(..)
        | InstKind::SLt(..)
        | InstKind::SGt(..)
        | InstKind::Eq(..)
        | InstKind::Ne(..)
        | InstKind::CheckedBinary { .. }
        | InstKind::SLoad(_)
        | InstKind::SStore(..)
        | InstKind::TLoad(_)
        | InstKind::TStore(..)
        | InstKind::CalldataLoad(_)
        | InstKind::BlockHash(_)
        | InstKind::Balance(_)
        | InstKind::ExtCodeSize(_)
        | InstKind::ExtCodeHash(_) => SmallVec::new(),
        InstKind::MLoad(offset) | InstKind::MStore8(offset, _) => smallvec![offset],
        InstKind::MStore(offset, value) => {
            if slot_store {
                smallvec![offset, value]
            } else {
                smallvec![offset]
            }
        }
        InstKind::MCopy(dest, src, size) => smallvec![dest, src, size],
        InstKind::CalldataCopy(offset, _, size)
        | InstKind::CodeCopy(offset, _, size)
        | InstKind::ReturnDataCopy(offset, _, size)
        | InstKind::ExtCodeCopy(_, offset, _, size)
        | InstKind::DataCopy(_, offset, size)
        | InstKind::MemoryZero(offset, size)
        | InstKind::Keccak256(offset, size)
        | InstKind::Log0(offset, size)
        | InstKind::Log1(offset, size, _)
        | InstKind::Log2(offset, size, ..)
        | InstKind::Log3(offset, size, ..)
        | InstKind::Log4(offset, size, ..)
        | InstKind::Create(_, offset, size)
        | InstKind::Create2(_, offset, size, _) => smallvec![offset, size],
        InstKind::Call { args_offset, args_size, ret_offset, ret_size, .. }
        | InstKind::CallCode { args_offset, args_size, ret_offset, ret_size, .. }
        | InstKind::StaticCall { args_offset, args_size, ret_offset, ret_size, .. }
        | InstKind::DelegateCall { args_offset, args_size, ret_offset, ret_size, .. } => {
            smallvec![args_offset, args_size, ret_offset, ret_size]
        }
        _ => return None,
    })
}

/// How an instruction other than a derivation or an internal call uses `word`: as a pointer in
/// the operands [`pointer_operands_mut`] lists, and as data in its other operands.
fn operand_reads(func: &Function, kind: &InstKind, word: ValueId) -> Reads {
    let slot_store = matches!(*kind, InstKind::MStore(address, _) if is_slot(func, address));
    let mut roles = kind.clone();
    let Some(pointers) = pointer_operands_mut(&mut roles, slot_store) else {
        return Reads { pointer: true, data: false };
    };
    let pointer_uses = pointers.into_iter().filter(|operand| **operand == word).count();
    let mut uses = 0;
    kind.visit_operands(|operand| uses += usize::from(operand == word));
    Reads { pointer: pointer_uses != 0, data: uses > pointer_uses }
}

/// How the pass clamps the words a function's loads read from the slot.
struct LoadClamps {
    /// The loads to clamp, and the calls and extractions that receive a returned word to clamp,
    /// with their blocks.
    loads: Vec<(BlockId, InstId)>,
    /// The words computed from the loaded ones on the way to a pointer use.
    words: Vec<ValueId>,
    /// The loaded and computed words whose unclamped value another use still reads, or that a
    /// checked operation computes, so that their pointer uses read a clamped copy.
    copied: FxHashSet<ValueId>,
    /// The uses that read a loaded or computed word as the pointer.
    sites: Vec<(UseSite, ValueId)>,
}

/// Where a pointer use reads a word.
enum UseSite {
    /// The pointer operands of an instruction, or every operand of one whose roles are unknown.
    Inst(InstId),
    /// The arguments at these positions of an internal call.
    Args(InstId, SmallVec<[usize; 2]>),
    /// The arguments at these positions of a tail call, or every operand of another terminator.
    Terminator(BlockId, Option<SmallVec<[usize; 2]>>),
}

/// A reader of a value: an instruction, or a block's terminator.
#[derive(Clone, Copy)]
enum User {
    Inst(InstId),
    Terminator(BlockId),
}

/// What an instruction does with the word in the free memory pointer's slot.
#[derive(Clone, Copy)]
enum Effect {
    /// Leaves it alone.
    None,
    /// Replaces it.
    Replaces,
    /// Reads its bytes as data.
    ReadsData,
    /// Reads its bytes as data, then writes the last of them.
    ReadsDataAndReplaces,
    /// Loads it as the pointer, in assembly.
    LoadsPointer,
    /// Loads it as the pointer and as data, in assembly.
    LoadsMixed,
    /// Reads it as the pointer, in code the compiler generates.
    ReadsPointer,
    /// Passes it to an internal call.
    Calls(FunctionId),
}

/// How paths read a word left in the free memory pointer's slot.
#[derive(Clone, Copy, Default)]
struct Reads {
    /// A path reads the word as the pointer.
    pointer: bool,
    /// A path reads the word's bytes as data.
    data: bool,
}

impl BitOrAssign for Reads {
    fn bitor_assign(&mut self, other: Self) {
        self.pointer |= other.pointer;
        self.data |= other.data;
    }
}

/// What happens to a word left in the slot from some point to the end of a function.
#[derive(Clone, Copy, Default)]
struct Flow {
    /// How paths read the word before replacing it or ending execution.
    reads: Reads,
    /// Whether a path returns to the caller with the word still in the slot.
    returned: bool,
}

/// A call to a function: an internal call at an instruction, or a tail call ending a function.
#[derive(Clone, Copy)]
enum CallSite {
    Call(FunctionId, BlockId, usize),
    Tail(FunctionId),
}

/// A stretch of a function that the clamp placement walks, from instruction `start` of `block`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Walk {
    func: FunctionId,
    block: BlockId,
    start: usize,
    returns: Returns,
}

/// Where the clamp placement goes on after a return.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Returns {
    /// At every call site of the function.
    ToCallers,
    /// After the call that entered the function, where the caller's walk goes on. The flag tells
    /// whether the slot's bytes may be read as data from there.
    ToCall { data_after: bool },
}

/// Follows a word stored into the free memory pointer's slot through blocks, internal calls, and
/// returns until it is replaced or execution ends.
struct PointerUses<'a> {
    module: &'a Module,
    /// The flow from the entry of each function, once known.
    entry_flows: FxHashMap<FunctionId, Flow>,
    /// The functions whose entry flow is being followed, by how deep the search entered them.
    following: FxHashMap<FunctionId, usize>,
    /// The shallowest function still being followed that the current flow passed a call into.
    shallowest: usize,
    /// The call sites of each function, built on first use.
    call_sites: Option<FxHashMap<FunctionId, Vec<CallSite>>>,
    /// Which values of each function come from constants and calldata alone, built on first use.
    address_inputs: FxHashMap<FunctionId, IndexVec<ValueId, AddressInput>>,
    /// The words each function computes from heap pointers, built on first use.
    heap_addresses: Option<IndexVec<FunctionId, DenseBitSet<ValueId>>>,
    /// The values of each function's parameters, built on first use.
    arg_values: FxHashMap<FunctionId, IndexVec<ArgIdx, Vec<ValueId>>>,
    /// The readers of each function's values, built on first use.
    users: FxHashMap<FunctionId, IndexLists<ValueId, User>>,
    /// How each classified load of the slot uses its word.
    load_reads: FxHashMap<(FunctionId, InstId), Reads>,
    /// The calls and extractions in callers that clamp a word planned functions return, with their
    /// functions and blocks.
    caller_roots: Vec<(FunctionId, BlockId, InstId)>,
}

impl<'a> PointerUses<'a> {
    fn new(module: &'a Module) -> Self {
        Self {
            module,
            entry_flows: FxHashMap::default(),
            following: FxHashMap::default(),
            shallowest: usize::MAX,
            call_sites: None,
            address_inputs: FxHashMap::default(),
            heap_addresses: None,
            arg_values: FxHashMap::default(),
            users: FxHashMap::default(),
            load_reads: FxHashMap::default(),
            caller_roots: Vec::new(),
        }
    }

    /// Returns which values of `func_id` come from constants and calldata alone.
    fn address_inputs(&mut self, func_id: FunctionId) -> &IndexVec<ValueId, AddressInput> {
        let module = self.module;
        self.address_inputs
            .entry(func_id)
            .or_insert_with(|| absolute_address_inputs(&module.functions[func_id]))
    }

    /// Returns how the word that the instruction before `start` leaves in the slot can be read,
    /// in its function or after it returns.
    fn reads_after(&mut self, func_id: FunctionId, block: BlockId, start: usize) -> Reads {
        let flow = self.flow_from(func_id, block, start);
        let mut reads = flow.reads;
        if flow.returned {
            reads |= self.reads_after_return(func_id, &mut FxHashSet::default());
        }
        reads
    }

    /// Returns how a word that `func_id` returns in the slot can be read after any of its call
    /// sites.
    fn reads_after_return(
        &mut self,
        func_id: FunctionId,
        returned: &mut FxHashSet<FunctionId>,
    ) -> Reads {
        let mut reads = Reads::default();
        if !returned.insert(func_id) {
            return reads;
        }
        let sites = self.call_sites(func_id).to_vec();
        let func = &self.module.functions[func_id];
        if returns_out_of_module(func, &sites) {
            reads.pointer = !returns_to_the_end(func);
        }
        for site in sites {
            match site {
                CallSite::Call(caller, block, index) => {
                    let flow = self.flow_from(caller, block, index + 1);
                    reads |= flow.reads;
                    if flow.returned {
                        reads |= self.reads_after_return(caller, returned);
                    }
                }
                CallSite::Tail(caller) => reads |= self.reads_after_return(caller, returned),
            }
        }
        reads
    }

    /// Returns what a call to `func_id` does with the word its caller left in the slot.
    ///
    /// A call into a function still being followed passes the word through as far as the
    /// search knows: following that function's own paths finds any read the call could make,
    /// and the search goes on after the call. A flow that relied on that is remembered only once
    /// the outermost function it passed into is done.
    fn entry_flow(&mut self, func_id: FunctionId) -> Flow {
        if let Some(&flow) = self.entry_flows.get(&func_id) {
            return flow;
        }
        if let Some(&depth) = self.following.get(&func_id) {
            self.shallowest = self.shallowest.min(depth);
            return Flow { reads: Reads::default(), returned: true };
        }
        let depth = self.following.len();
        self.following.insert(func_id, depth);
        let outer = std::mem::replace(&mut self.shallowest, usize::MAX);
        let flow = self.flow_from(func_id, BlockId::ENTRY, 0);
        self.following.remove(&func_id);
        if self.shallowest >= depth {
            self.entry_flows.insert(func_id, flow);
            self.shallowest = outer;
        } else {
            self.shallowest = self.shallowest.min(outer);
        }
        flow
    }

    /// Follows the word from instruction `start` of `block` to the end of `func_id`.
    fn flow_from(&mut self, func_id: FunctionId, block: BlockId, start: usize) -> Flow {
        let module = self.module;
        let func = &module.functions[func_id];
        let mut visited = DenseBitSet::new_empty(func.blocks.len());
        let mut pending = vec![(block, start)];
        let mut flow = Flow::default();
        while let Some((block, start)) = pending.pop() {
            let data = &func.blocks[block];
            let mut replaced = false;
            let mut written = 0;
            for &inst_id in data.instructions.iter().skip(start) {
                match self.effect(func_id, inst_id, &mut written) {
                    Effect::None => {}
                    Effect::Replaces => {
                        replaced = true;
                        break;
                    }
                    Effect::ReadsData => flow.reads.data = true,
                    Effect::ReadsDataAndReplaces => {
                        flow.reads.data = true;
                        replaced = true;
                        break;
                    }
                    Effect::LoadsPointer | Effect::ReadsPointer => flow.reads.pointer = true,
                    Effect::LoadsMixed => {
                        flow.reads.pointer = true;
                        flow.reads.data = true;
                    }
                    Effect::Calls(callee) => {
                        let callee_flow = self.entry_flow(callee);
                        flow.reads |= callee_flow.reads;
                        if !callee_flow.returned {
                            replaced = true;
                            break;
                        }
                    }
                }
            }
            if replaced {
                continue;
            }
            match data.terminator.as_ref() {
                Some(
                    &(Terminator::Revert { offset, size }
                    | Terminator::ReturnData { offset, size }),
                ) => {
                    if self.may_read_slot(func_id, offset, func.value_u64(size), written) {
                        flow.reads.data = true;
                    }
                }
                Some(
                    terminator @ (Terminator::Jump(_)
                    | Terminator::Branch { .. }
                    | Terminator::Switch { .. }),
                ) => terminator.for_each_successor(|successor| {
                    if visited.insert(successor) {
                        pending.push((successor, 0));
                    }
                }),
                Some(Terminator::Return { .. }) => flow.returned = true,
                // The tail callee returns to this function's callers.
                Some(&Terminator::TailCall { function, .. }) => {
                    let callee_flow = self.entry_flow(function);
                    flow.reads |= callee_flow.reads;
                    flow.returned |= callee_flow.returned;
                }
                None => flow.reads.pointer = true,
                // Execution ends.
                Some(
                    Terminator::RevertReturndata
                    | Terminator::Stop
                    | Terminator::Invalid
                    | Terminator::SelfDestruct { .. },
                ) => {}
            }
        }
        flow
    }

    /// Clamps the word a store leaves in the slot at each pointer read of it from instruction
    /// `start` of `block` on, keeping the slot's word for the reads of its bytes as data.
    fn clamp_pointer_reads(
        &mut self,
        func_id: FunctionId,
        block: BlockId,
        start: usize,
        clamps: &mut FxIndexSet<Clamp>,
    ) {
        let mut pending = vec![Walk { func: func_id, block, start, returns: Returns::ToCallers }];
        let mut walked = FxHashSet::default();
        while let Some(walk) = pending.pop() {
            if walked.insert(walk) {
                self.clamp_in_block(walk, &mut pending, clamps);
            }
        }
    }

    /// Clamps the pointer reads of one stretch of a block, queueing the stretches it reaches.
    fn clamp_in_block(
        &mut self,
        walk: Walk,
        pending: &mut Vec<Walk>,
        clamps: &mut FxIndexSet<Clamp>,
    ) {
        let module = self.module;
        let Walk { func: func_id, block, start, returns } = walk;
        let data = &module.functions[func_id].blocks[block];
        let mut written = 0;
        for (index, &inst_id) in data.instructions.iter().enumerate().skip(start) {
            match self.effect(func_id, inst_id, &mut written) {
                Effect::None | Effect::ReadsData => {}
                Effect::Replaces | Effect::ReadsDataAndReplaces => return,
                // The slot keeps its word for later reads.
                Effect::LoadsPointer | Effect::LoadsMixed => {
                    clamps.insert(Clamp::Load(func_id, block, inst_id));
                }
                // The compiler takes the slot's word as the pointer from here on.
                Effect::ReadsPointer => {
                    clamps.insert(Clamp::Slot(func_id, block, Some(inst_id)));
                    return;
                }
                Effect::Calls(callee) => {
                    let flow = self.entry_flow(callee);
                    if flow.reads.pointer {
                        let data_after =
                            flow.returned && self.data_after(func_id, block, index + 1, returns);
                        if flow.reads.data || data_after {
                            pending.push(Walk {
                                func: callee,
                                block: BlockId::ENTRY,
                                start: 0,
                                returns: Returns::ToCall { data_after },
                            });
                        } else {
                            clamps.insert(Clamp::Slot(func_id, block, Some(inst_id)));
                            return;
                        }
                    }
                    if !flow.returned {
                        return;
                    }
                }
            }
        }
        match data.terminator.as_ref() {
            Some(
                terminator @ (Terminator::Jump(_)
                | Terminator::Branch { .. }
                | Terminator::Switch { .. }),
            ) => terminator.for_each_successor(|successor| {
                pending.push(Walk { func: func_id, block: successor, start: 0, returns });
            }),
            Some(Terminator::Return { .. }) => {
                self.clamp_after_return(func_id, block, returns, pending, clamps);
            }
            Some(&Terminator::TailCall { function, .. }) => {
                let flow = self.entry_flow(function);
                if flow.reads.pointer {
                    let data_after = flow.returned && self.data_after_return(func_id, returns);
                    if flow.reads.data || data_after {
                        pending.push(Walk {
                            func: function,
                            block: BlockId::ENTRY,
                            start: 0,
                            returns: Returns::ToCall { data_after },
                        });
                    } else {
                        clamps.insert(Clamp::Slot(func_id, block, None));
                        return;
                    }
                }
                if flow.returned {
                    self.clamp_after_return(func_id, block, returns, pending, clamps);
                }
            }
            None => {
                clamps.insert(Clamp::Slot(func_id, block, None));
            }
            // Execution ends.
            Some(_) => {}
        }
    }

    /// Goes on after a return from `func_id` at the end of `block`.
    fn clamp_after_return(
        &mut self,
        func_id: FunctionId,
        block: BlockId,
        returns: Returns,
        pending: &mut Vec<Walk>,
        clamps: &mut FxIndexSet<Clamp>,
    ) {
        // The walk that entered the function goes on after its call.
        if let Returns::ToCall { .. } = returns {
            return;
        }
        let mut returning = vec![func_id];
        let mut seen = FxHashSet::default();
        while let Some(callee) = returning.pop() {
            if !seen.insert(callee) {
                continue;
            }
            let sites = self.call_sites(callee).to_vec();
            let func = &self.module.functions[callee];
            if returns_out_of_module(func, &sites) && !returns_to_the_end(func) {
                // The code the function returns to reads the slot as the pointer.
                clamps.insert(Clamp::Slot(func_id, block, None));
            }
            for site in sites {
                match site {
                    CallSite::Call(caller, call_block, index) => pending.push(Walk {
                        func: caller,
                        block: call_block,
                        start: index + 1,
                        returns: Returns::ToCallers,
                    }),
                    CallSite::Tail(caller) => returning.push(caller),
                }
            }
        }
    }

    /// Returns whether the slot's bytes may be read as data from instruction `start` of `block`
    /// on, including after `func_id` returns.
    fn data_after(
        &mut self,
        func_id: FunctionId,
        block: BlockId,
        start: usize,
        returns: Returns,
    ) -> bool {
        let flow = self.flow_from(func_id, block, start);
        flow.reads.data || flow.returned && self.data_after_return(func_id, returns)
    }

    /// Returns whether the slot's bytes may be read as data after `func_id` returns.
    fn data_after_return(&mut self, func_id: FunctionId, returns: Returns) -> bool {
        match returns {
            Returns::ToCallers => self.reads_after_return(func_id, &mut FxHashSet::default()).data,
            Returns::ToCall { data_after } => data_after,
        }
    }

    /// Returns what an instruction does with the word in the slot, given the slot bytes that
    /// earlier writes in the block have `written`, and adds the bytes it writes.
    fn effect(&mut self, func_id: FunctionId, inst_id: InstId, written: &mut SlotBytes) -> Effect {
        let module = self.module;
        let func = &module.functions[func_id];
        let inst = func.inst(inst_id);
        match inst.kind {
            InstKind::MLoad(address) if is_slot(func, address) => {
                let reads = self.load_reads(func_id, inst_id);
                match (reads.pointer, reads.data) {
                    (true, true) => Effect::LoadsMixed,
                    (true, false) => Effect::LoadsPointer,
                    (false, true) => Effect::ReadsData,
                    (false, false) => Effect::None,
                }
            }
            InstKind::MStore(address, _) if is_slot(func, address) => Effect::Replaces,
            InstKind::SetFmp(_) => Effect::Replaces,
            InstKind::ICall { function: Callee::Function(callee), .. } => Effect::Calls(callee),
            // The compiler's own pointer reads and allocations, and the builtins and semantic
            // operations whose lowering may allocate or encode at the pointer.
            InstKind::Fmp | InstKind::Alloc { .. } => Effect::ReadsPointer,
            _ if inst.unlowered_reason(func).is_some() && lowered_reads_pointer(&inst.kind) => {
                Effect::ReadsPointer
            }
            _ => {
                // A copy reads its source before it writes its destination.
                let reads = read_range(func, &inst.kind).is_some_and(|(offset, size)| {
                    self.may_read_slot(func_id, offset, size, *written)
                });
                if let Some((start, size)) = constant_write_range(func, &inst.kind) {
                    *written |= slot_bytes(start, size);
                }
                match (reads, *written == WHOLE_SLOT) {
                    (true, true) => Effect::ReadsDataAndReplaces,
                    (true, false) => Effect::ReadsData,
                    (false, true) => Effect::Replaces,
                    (false, false) => Effect::None,
                }
            }
        }
    }

    /// Returns whether a read of `size` bytes at `offset`, or of an unknown size, may cover a
    /// byte of the slot that still holds the stored word, given the bytes that later writes have
    /// `written`. A computed offset may unless the function computes it from heap pointers.
    fn may_read_slot(
        &mut self,
        func_id: FunctionId,
        offset: ValueId,
        size: Option<u64>,
        written: SlotBytes,
    ) -> bool {
        if size == Some(0) {
            return false;
        }
        let Some(start) = self.module.functions[func_id].value_u64(offset) else {
            return !self.heap_addresses(func_id).contains(offset);
        };
        slot_bytes(start, size.unwrap_or(u64::MAX)) & !written != 0
    }

    /// Returns the words `func_id` computes from heap pointers.
    fn heap_addresses(&mut self, func_id: FunctionId) -> &DenseBitSet<ValueId> {
        let module = self.module;
        &self.heap_addresses.get_or_insert_with(|| module_heap_addresses(module))[func_id]
    }

    /// Returns the values of parameter `index` of `func_id`.
    fn arg_values(&mut self, func_id: FunctionId, index: usize) -> Vec<ValueId> {
        let module = self.module;
        let values =
            self.arg_values.entry(func_id).or_insert_with(|| module.functions[func_id].arg_uses());
        values.get(ArgIdx::new(index)).cloned().unwrap_or_default()
    }

    /// Queues the call results that receive a word `func_id` returns, and returns how the code
    /// outside the module reads it, as [`Self::returned_calls`] finds them.
    fn queue_returned(
        &mut self,
        func_id: FunctionId,
        scalar: bool,
        pending: &mut Vec<(FunctionId, ValueId)>,
    ) -> Reads {
        let module = self.module;
        let (calls, outside) = self.returned_calls(func_id, scalar);
        pending.extend(calls.into_iter().filter_map(|(caller, _, call)| {
            module.functions[caller].inst_result_value(call).map(|result| (caller, result))
        }));
        outside
    }

    /// Follows the returns of `func_id`, through tail calls, to the internal calls that receive
    /// its result, and returns them with their functions and blocks, along with how the code
    /// outside the module reads a returned word: an external function encodes a `scalar` as
    /// data, and anything else may be a pointer.
    fn returned_calls(
        &mut self,
        func_id: FunctionId,
        scalar: bool,
    ) -> (Vec<(FunctionId, BlockId, InstId)>, Reads) {
        let module = self.module;
        let mut calls = Vec::new();
        let mut outside = Reads::default();
        let mut returning = vec![func_id];
        let mut seen = FxHashSet::default();
        while let Some(callee) = returning.pop() {
            if !seen.insert(callee) {
                continue;
            }
            let sites = self.call_sites(callee).to_vec();
            let func = &module.functions[callee];
            if returns_out_of_module(func, &sites) {
                if scalar && func.is_external_entry() {
                    outside.data = true;
                } else {
                    outside.pointer = true;
                }
            }
            for site in sites {
                match site {
                    CallSite::Call(caller, block, index) => {
                        let call = module.functions[caller].blocks[block].instructions[index];
                        calls.push((caller, block, call));
                    }
                    CallSite::Tail(caller) => returning.push(caller),
                }
            }
        }
        (calls, outside)
    }

    /// Returns how the word a load reads from the slot is used.
    fn load_reads(&mut self, func_id: FunctionId, load: InstId) -> Reads {
        if let Some(&reads) = self.load_reads.get(&(func_id, load)) {
            return reads;
        }
        let words = self.module.functions[func_id].inst_result_value(load).map(|w| (func_id, w));
        let reads = self.word_reads(words.into_iter().collect());
        self.load_reads.insert((func_id, load), reads);
        reads
    }

    /// Returns how the words `pending` holds, and the words computed from them, are used: as a
    /// pointer where they address or size memory or become the pointer again, and as data
    /// elsewhere, in their functions, in the callees they are passed to, and in the callers they
    /// are returned to.
    fn word_reads(&mut self, mut pending: Vec<(FunctionId, ValueId)>) -> Reads {
        let module = self.module;
        let mut reads = Reads::default();
        let mut seen = FxHashSet::default();
        while let Some((func_id, word)) = pending.pop() {
            if reads.pointer && reads.data {
                break;
            }
            if !seen.insert((func_id, word)) {
                continue;
            }
            let func = &module.functions[func_id];
            let users =
                self.users.entry(func_id).or_insert_with(|| value_users(func)).get(word).to_vec();
            for user in users {
                match user {
                    User::Inst(inst) => {
                        let kind = &func.inst(inst).kind;
                        if derives_from(kind, word) {
                            pending.extend(func.inst_result_value(inst).map(|v| (func_id, v)));
                        } else if is_derivation(kind) {
                            reads.data = true;
                        } else if let InstKind::ICall { function: Callee::Function(callee), args } =
                            kind
                        {
                            // A callee uses the word as it uses the parameter.
                            let positions =
                                args.iter().enumerate().filter(|&(_, &arg)| arg == word);
                            for (index, _) in positions {
                                let values = self.arg_values(*callee, index);
                                pending.extend(values.into_iter().map(|v| (*callee, v)));
                            }
                        } else {
                            reads |= operand_reads(func, kind, word);
                        }
                    }
                    User::Terminator(block) => match func.blocks[block].terminator.as_ref() {
                        Some(
                            Terminator::Branch { .. }
                            | Terminator::Switch { .. }
                            | Terminator::SelfDestruct { .. },
                        ) => reads.data = true,
                        // The callers use the word as they use the call's result.
                        Some(Terminator::Return { .. }) => {
                            let scalar = is_scalar(module, func.value_ty(word));
                            reads |= self.queue_returned(func_id, scalar, &mut pending);
                        }
                        Some(Terminator::TailCall { function, args }) => {
                            let positions =
                                args.iter().enumerate().filter(|&(_, &arg)| arg == word);
                            for (index, _) in positions {
                                let values = self.arg_values(*function, index);
                                pending.extend(values.into_iter().map(|v| (*function, v)));
                            }
                        }
                        _ => reads.pointer = true,
                    },
                }
            }
        }
        reads
    }

    /// Plans the clamps of the words `loads` read from the slot in `func_id`. All of a function's
    /// loads are planned at once, so that a word computed from several of them, such as a phi of
    /// two branches' loads, is clamped on every path.
    fn load_clamps(&mut self, func_id: FunctionId, loads: Vec<(BlockId, InstId)>) -> LoadClamps {
        let module = self.module;
        let func = &module.functions[func_id];
        let roots = loads
            .iter()
            .filter_map(|&(_, load)| func.inst_result_value(load))
            .collect::<FxHashSet<_>>();
        let mut derived = roots.iter().copied().collect::<FxIndexSet<_>>();
        let mut sites = Vec::new();
        // Words whose unclamped value a use reads, and the derivations of each word.
        let mut raw = FxHashSet::default();
        let mut derivations = Vec::new();
        let mut next = 0;
        while let Some(&value) = derived.get_index(next) {
            next += 1;
            let users =
                self.users.entry(func_id).or_insert_with(|| value_users(func)).get(value).to_vec();
            for user in users {
                match user {
                    User::Inst(inst) => {
                        let kind = &func.inst(inst).kind;
                        if derives_from(kind, value) {
                            if let Some(result) = func.inst_result_value(inst) {
                                derived.insert(result);
                                derivations.push((value, result));
                            }
                        } else if is_derivation(kind) {
                            raw.insert(value);
                        } else if let InstKind::ICall { function: Callee::Function(callee), args } =
                            kind
                        {
                            let (positions, data) = self.arg_roles(*callee, args, value);
                            if !positions.is_empty() {
                                sites.push((UseSite::Args(inst, positions), value));
                            }
                            if data {
                                raw.insert(value);
                            }
                        } else {
                            let reads = operand_reads(func, kind, value);
                            if reads.pointer {
                                sites.push((UseSite::Inst(inst), value));
                            }
                            if reads.data {
                                raw.insert(value);
                            }
                        }
                    }
                    User::Terminator(block) => match func.blocks[block].terminator.as_ref() {
                        Some(
                            Terminator::Branch { .. }
                            | Terminator::Switch { .. }
                            | Terminator::SelfDestruct { .. },
                        ) => {
                            raw.insert(value);
                        }
                        Some(Terminator::Return { .. }) => {
                            let scalar = is_scalar(module, func.value_ty(value));
                            let (calls, outside) = self.returned_calls(func_id, scalar);
                            let results = calls
                                .iter()
                                .filter_map(|&(caller, _, call)| {
                                    let result = module.functions[caller].inst_result_value(call);
                                    result.map(|result| (caller, result))
                                })
                                .collect();
                            let callers = self.word_reads(results);
                            // An external return encodes the word as data, so it returns
                            // unclamped, and the callers that read it as the pointer clamp what
                            // their calls receive.
                            if outside.data
                                && !outside.pointer
                                && callers.pointer
                                && let Some(roots) =
                                    self.result_roots(func, value, &derived, &calls)
                            {
                                self.caller_roots.extend(roots);
                                raw.insert(value);
                                continue;
                            }
                            let mut reads = outside;
                            reads |= callers;
                            if reads.pointer {
                                sites.push((UseSite::Terminator(block, None), value));
                            }
                            if reads.data {
                                raw.insert(value);
                            }
                        }
                        Some(Terminator::TailCall { function, args }) => {
                            let (positions, data) = self.arg_roles(*function, args, value);
                            if !positions.is_empty() {
                                sites.push((UseSite::Terminator(block, Some(positions)), value));
                            }
                            if data {
                                raw.insert(value);
                            }
                        }
                        _ => sites.push((UseSite::Terminator(block, None), value)),
                    },
                }
            }
        }
        // The words a pointer use reads, and those they are computed from, get clamped.
        let mut needed = DenseBitSet::new_empty(func.num_values());
        let mut pending = sites.iter().map(|&(_, value)| value).collect::<Vec<_>>();
        while let Some(value) = pending.pop() {
            if !needed.insert(value) || roots.contains(&value) {
                continue;
            }
            if let Value::Inst(definition) = *func.value(value) {
                func.inst(definition).visit_operands(|operand| {
                    if derived.contains(&operand) {
                        pending.push(operand);
                    }
                });
            }
        }
        // A word feeding a derivation that no pointer use needs keeps its value for it.
        for &(operand, result) in &derivations {
            if !needed.contains(result) {
                raw.insert(operand);
            }
        }
        // A copied word keeps its original, which reads its operands unclamped, and so does a
        // checked operation, whose check stays on the unclamped words.
        let mut copied = FxHashSet::default();
        let mut pending = derived
            .iter()
            .copied()
            .filter(|&value| {
                needed.contains(value)
                    && (raw.contains(&value)
                        || matches!(*func.value(value), Value::Inst(inst)
                            if matches!(func.inst(inst).kind, InstKind::CheckedBinary { .. })))
            })
            .collect::<Vec<_>>();
        while let Some(value) = pending.pop() {
            if !copied.insert(value) || roots.contains(&value) {
                continue;
            }
            if let Value::Inst(definition) = *func.value(value) {
                func.inst(definition).visit_operands(|operand| {
                    if needed.contains(operand) {
                        pending.push(operand);
                    }
                });
            }
        }
        let words = derived
            .into_iter()
            .filter(|value| needed.contains(*value) && !roots.contains(value))
            .collect();
        // A root whose pointer uses all moved to the callers stays as it is.
        let loads = loads
            .into_iter()
            .filter(|&(_, root)| {
                func.inst_result_value(root).is_some_and(|word| needed.contains(word))
            })
            .collect();
        LoadClamps { loads, words, copied, sites }
    }

    /// Returns the roots that clamp, in each of `calls` whose caller reads the result as the
    /// pointer, the word that `func` returns as `value`: the call itself for a returned word, or
    /// the extractions of the fields that carry the word from a returned struct. None when a
    /// caller receives it in another form, such as a whole struct it passes on.
    fn result_roots(
        &mut self,
        func: &Function,
        value: ValueId,
        derived: &FxIndexSet<ValueId>,
        calls: &[(FunctionId, BlockId, InstId)],
    ) -> Option<Vec<(FunctionId, BlockId, InstId)>> {
        let module = self.module;
        let fields = match func.value_ty(value) {
            Some(MirType::I256) => None,
            Some(MirType::Struct(_)) => Some(word_fields(func, value, derived)?),
            _ => return None,
        };
        let mut roots = Vec::new();
        for &(caller, block, call) in calls {
            let caller_func = &module.functions[caller];
            let Some(result) = caller_func.inst_result_value(call) else { continue };
            if !self.word_reads(vec![(caller, result)]).pointer {
                continue;
            }
            let Some(fields) = &fields else {
                roots.push((caller, block, call));
                continue;
            };
            let blocks = caller_func.inst_block_table();
            let users = self
                .users
                .entry(caller)
                .or_insert_with(|| value_users(caller_func))
                .get(result)
                .to_vec();
            for user in users {
                let User::Inst(inst) = user else { return None };
                let InstKind::ExtractValue { index, .. } = caller_func.inst(inst).kind else {
                    return None;
                };
                if fields.contains(&index) {
                    if caller_func.inst(inst).result_ty != Some(MirType::I256) {
                        return None;
                    }
                    roots.push((caller, blocks[inst]?, inst));
                }
            }
        }
        Some(roots)
    }

    /// Returns the positions at which `args` passes `word` to a parameter `callee` reads as a
    /// pointer, and whether one of the parameters it passes `word` to is read as data.
    fn arg_roles(
        &mut self,
        callee: FunctionId,
        args: &[ValueId],
        word: ValueId,
    ) -> (SmallVec<[usize; 2]>, bool) {
        let mut positions = SmallVec::new();
        let mut data = false;
        for (index, _) in args.iter().enumerate().filter(|&(_, &arg)| arg == word) {
            let params = self.arg_values(callee, index).into_iter().map(|v| (callee, v)).collect();
            let reads = self.word_reads(params);
            if reads.pointer {
                positions.push(index);
            }
            data |= reads.data;
        }
        (positions, data)
    }

    /// Returns the internal calls and tail calls to `func_id`.
    fn call_sites(&mut self, func_id: FunctionId) -> &[CallSite] {
        let module = self.module;
        let call_sites = self.call_sites.get_or_insert_with(|| {
            let mut sites = FxHashMap::<_, Vec<_>>::default();
            for (caller, func) in module.functions.iter_enumerated() {
                for (block, data) in func.blocks.iter_enumerated() {
                    for (index, &inst_id) in data.instructions.iter().enumerate() {
                        if let InstKind::ICall { function: Callee::Function(callee), .. } =
                            func.inst(inst_id).kind
                        {
                            sites
                                .entry(callee)
                                .or_default()
                                .push(CallSite::Call(caller, block, index));
                        }
                    }
                    if let Some(&Terminator::TailCall { function, .. }) = data.terminator.as_ref() {
                        sites.entry(function).or_default().push(CallSite::Tail(caller));
                    }
                }
            }
            sites
        });
        call_sites.get(&func_id).map_or(&[], Vec::as_slice)
    }
}

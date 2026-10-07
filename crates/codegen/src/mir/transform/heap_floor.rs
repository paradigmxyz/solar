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
//! returned from an external function as integers is data. The compiler's own free-memory-pointer
//! reads and allocations read the word as the pointer too, and so do the builtins and semantic
//! operations whose lowering may allocate or encode at the pointer, unlike pure computations,
//! checks that revert with scratch data, and accesses to frames, storage slots, and memory that
//! already exists. An access that may cover a byte the writes have not replaced reads the word as
//! data, such as a scratch hash or revert data, when its address is computed from constants,
//! calldata, and parameters that may hold such an address: an external function's integer
//! arguments, or a parameter some call site passes one in. A return from the constructor, or one
//! from an external function whose results all have static ABI types, ends execution without
//! reading the slot: codegen returns runtime code from a fixed address and encodes static results
//! in a static buffer.
//!
//! A word read only as the pointer is clamped where it is stored, `mstore 64, clamped`. A word
//! that is also read as data keeps its value in the slot, and the clamp moves to each pointer read
//! that the search reaches. An assembly load uses the clamped word in place of the one it reads,
//! and the slot itself is clamped right before a compiler read, before a call that reads the slot
//! only as the pointer, and before a return to the dispatcher:
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
//! call, and copies or byte stores that overwrite the pointer, keep their bytes. An access at an
//! address derived from memory or the heap, or from a parameter that every call site fills with
//! one, is assumed not to cover the slot, and a loaded word stored elsewhere stays data even if a
//! later load uses it as a pointer. A slot clamped before a compiler read stays clamped for a later
//! read of it as data. Writes replace the word only when one block covers all of its bytes. See
//! CODEGEN-010.

use crate::mir::{
    AbiType, ArgIdx, BlockId, Builtin, Callee, EffectKind, Function, FunctionId, Immediate, InstId,
    InstKind, Instruction, InstructionMetadata, MemoryRegion, MirType, Module, Terminator, Value,
    ValueId,
    analysis::{AddressInput, absolute_address_inputs, absolute_address_inputs_with_params},
    memory::EvmMemoryLayout,
    pass::{MirPass, ModuleAnalyses},
    utils::IndexLists,
};
use alloy_primitives::U256;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::IndexVec,
    map::{FxHashMap, FxHashSet, FxIndexSet},
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
        let clamps = floor_clamps(module);
        let changed = !clamps.is_empty();
        for clamp in clamps {
            match clamp {
                Clamp::Store(func, block, inst) => {
                    clamp_store(&mut module.functions[func], block, inst);
                }
                Clamp::Load(func, block, inst) => {
                    clamp_load(&mut module.functions[func], block, inst);
                }
                Clamp::Slot(func, block, before) => {
                    clamp_slot(&mut module.functions[func], block, before);
                }
            }
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
/// pointer read when some read takes the word as data.
fn floor_clamps(module: &Module) -> FxIndexSet<Clamp> {
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
    clamps
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

/// Rewrites the users of `word = mload 64` to read `max(word, heap_floor)`, keeping the slot's
/// word.
fn clamp_load(func: &mut Function, block: BlockId, inst: InstId) {
    let word = func.inst_result_value(inst).expect("a load produces the slot's word");
    let metadata = func.inst(inst).metadata.clone();
    let (clamp, clamped) = floor_max(func, word, &metadata);
    // The clamp is not placed yet, so it keeps reading the loaded word.
    func.replace_uses(&FxHashMap::from_iter([(word, clamped)]));
    // word = mload 64
    // floor = heap_floor
    // below = lt word, floor
    // clamped = select below, floor, word
    let position = index_in_block(func, block, inst) + 1;
    func.blocks[block].instructions.splice(position..position, clamp);
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

/// Classifies the values of every function as absolute addresses for reads. A parameter may hold
/// one when it is an integer an external function takes from calldata, or when a call site passes
/// a word computed from constants, calldata, and such parameters.
fn read_address_inputs(module: &Module) -> IndexVec<FunctionId, IndexVec<ValueId, AddressInput>> {
    let mut params = module
        .functions
        .iter()
        .map(|func| {
            func.arg_indices()
                .map(|index| {
                    if func.is_external_entry() && matches!(func.arg_ty(index), MirType::Int(_)) {
                        AddressInput::Calldata
                    } else {
                        AddressInput::Other
                    }
                })
                .collect::<IndexVec<ArgIdx, _>>()
        })
        .collect::<IndexVec<FunctionId, _>>();
    let classify = |func_id: FunctionId, params: &IndexVec<FunctionId, IndexVec<ArgIdx, _>>| {
        absolute_address_inputs_with_params(&module.functions[func_id], |index| {
            params[func_id].get(index).copied().unwrap_or(AddressInput::Other)
        })
    };
    let mut inputs = module
        .functions
        .indices()
        .map(|func_id| classify(func_id, &params))
        .collect::<IndexVec<FunctionId, _>>();
    let mut pending = module.functions.indices().collect::<Vec<_>>();
    while let Some(caller) = pending.pop() {
        let func = &module.functions[caller];
        let mut raised = Vec::new();
        let mut pass = |callee: FunctionId, args: &[ValueId]| {
            for (index, &arg) in args.iter().enumerate() {
                if inputs[caller][arg] != AddressInput::Other
                    && let Some(param) = params[callee].get_mut(ArgIdx::new(index))
                    && *param == AddressInput::Other
                {
                    *param = AddressInput::Calldata;
                    raised.push(callee);
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
        for callee in raised {
            inputs[callee] = classify(callee, &params);
            pending.push(callee);
        }
    }
    inputs
}

/// How an instruction uses a word loaded from the slot, or a word computed from one.
fn word_use(func: &Function, kind: &InstKind, word: ValueId) -> WordUse {
    match *kind {
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
        | InstKind::CheckedBinary { .. }
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
        | InstKind::PtrToInt(..) => WordUse::Derives,
        InstKind::Lt(..)
        | InstKind::Gt(..)
        | InstKind::SLt(..)
        | InstKind::SGt(..)
        | InstKind::Eq(..)
        | InstKind::Ne(..)
        | InstKind::SLoad(_)
        | InstKind::SStore(..)
        | InstKind::TLoad(_)
        | InstKind::TStore(..)
        | InstKind::CalldataLoad(_) => WordUse::Data,
        // A word stored elsewhere is data.
        InstKind::MStore(address, _) | InstKind::MStore8(address, _)
            if address != word && !is_slot(func, address) =>
        {
            WordUse::Data
        }
        _ => WordUse::Pointer,
    }
}

/// How a read uses a word loaded from the slot, or a word computed from one.
enum WordUse {
    /// Computes another word from it.
    Derives,
    /// Uses it as data.
    Data,
    /// May use it as a pointer.
    Pointer,
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
    /// The same with parameters classified by what their callers pass, for every function, built
    /// on first use.
    read_inputs: Option<IndexVec<FunctionId, IndexVec<ValueId, AddressInput>>>,
    /// The values of each function's parameters, built on first use.
    arg_values: FxHashMap<FunctionId, IndexVec<ArgIdx, Vec<ValueId>>>,
    /// The readers of each function's values, built on first use.
    users: FxHashMap<FunctionId, IndexLists<ValueId, User>>,
    /// Whether each classified load of the slot reads the pointer.
    pointer_loads: FxHashMap<(FunctionId, InstId), bool>,
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
            read_inputs: None,
            arg_values: FxHashMap::default(),
            users: FxHashMap::default(),
            pointer_loads: FxHashMap::default(),
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
        if sites.is_empty() {
            // Any other function no call reaches returns to code the module does not show.
            reads.pointer = !returns_to_the_end(&self.module.functions[func_id]);
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
                Effect::LoadsPointer => {
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
            if sites.is_empty() && !returns_to_the_end(&self.module.functions[callee]) {
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
                if self.loaded_as_pointer(func_id, inst_id) {
                    Effect::LoadsPointer
                } else {
                    Effect::ReadsData
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
    /// `written`. A computed offset may when the function computes it from constants and
    /// calldata alone.
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
            return self.read_inputs(func_id)[offset] != AddressInput::Other;
        };
        slot_bytes(start, size.unwrap_or(u64::MAX)) & !written != 0
    }

    /// Returns which values of `func_id` may be absolute addresses where it reads memory: words
    /// computed from constants, calldata, and parameters that may hold one.
    fn read_inputs(&mut self, func_id: FunctionId) -> &IndexVec<ValueId, AddressInput> {
        let module = self.module;
        &self.read_inputs.get_or_insert_with(|| read_address_inputs(module))[func_id]
    }

    /// Returns the values of parameter `index` of `func_id`.
    fn arg_values(&mut self, func_id: FunctionId, index: usize) -> Vec<ValueId> {
        let module = self.module;
        let values =
            self.arg_values.entry(func_id).or_insert_with(|| module.functions[func_id].arg_uses());
        values.get(ArgIdx::new(index)).cloned().unwrap_or_default()
    }

    /// Queues the call results that receive a word `func_id` returns, following tail calls to
    /// the callers they return to. Returns false when the word leaves the module as a pointer:
    /// an external function encodes only a returned `scalar` as data.
    fn queue_returned(
        &mut self,
        func_id: FunctionId,
        scalar: bool,
        pending: &mut Vec<(FunctionId, ValueId)>,
    ) -> bool {
        let module = self.module;
        let mut returning = vec![func_id];
        let mut seen = FxHashSet::default();
        while let Some(callee) = returning.pop() {
            if !seen.insert(callee) {
                continue;
            }
            let sites = self.call_sites(callee).to_vec();
            if sites.is_empty() && !(scalar && module.functions[callee].is_external_entry()) {
                return false;
            }
            for site in sites {
                match site {
                    CallSite::Call(caller, block, index) => {
                        let caller_func = &module.functions[caller];
                        let call = caller_func.blocks[block].instructions[index];
                        pending.extend(caller_func.inst_result_value(call).map(|r| (caller, r)));
                    }
                    CallSite::Tail(caller) => returning.push(caller),
                }
            }
        }
        true
    }

    /// Returns whether the word a load reads from the slot is used as the pointer: it, or a word
    /// computed from it, addresses or sizes memory or becomes the pointer again, in its function,
    /// in the callees it is passed to, or in the callers it is returned to.
    fn loaded_as_pointer(&mut self, func_id: FunctionId, load: InstId) -> bool {
        if let Some(&pointer) = self.pointer_loads.get(&(func_id, load)) {
            return pointer;
        }
        let module = self.module;
        let mut seen = FxHashSet::default();
        let mut pending = module.functions[func_id]
            .inst_result_value(load)
            .map(|word| (func_id, word))
            .into_iter()
            .collect::<Vec<_>>();
        let pointer = 'words: {
            while let Some((func_id, word)) = pending.pop() {
                if !seen.insert((func_id, word)) {
                    continue;
                }
                let func = &module.functions[func_id];
                let users = self
                    .users
                    .entry(func_id)
                    .or_insert_with(|| value_users(func))
                    .get(word)
                    .to_vec();
                for user in users {
                    match user {
                        User::Inst(inst) => match (
                            word_use(func, &func.inst(inst).kind, word),
                            &func.inst(inst).kind,
                        ) {
                            (WordUse::Derives, _) => {
                                pending.extend(func.inst_result_value(inst).map(|v| (func_id, v)));
                            }
                            (WordUse::Data, _) => {}
                            // A callee uses the word as it uses the parameter.
                            (
                                WordUse::Pointer,
                                InstKind::ICall { function: Callee::Function(callee), args },
                            ) => {
                                for (index, _) in
                                    args.iter().enumerate().filter(|&(_, &arg)| arg == word)
                                {
                                    let values = self.arg_values(*callee, index);
                                    pending.extend(values.into_iter().map(|v| (*callee, v)));
                                }
                            }
                            (WordUse::Pointer, _) => break 'words true,
                        },
                        User::Terminator(block) => match func.blocks[block].terminator.as_ref() {
                            Some(
                                Terminator::Branch { .. }
                                | Terminator::Switch { .. }
                                | Terminator::SelfDestruct { .. },
                            ) => {}
                            // The callers use the word as they use the call's result.
                            Some(Terminator::Return { .. }) => {
                                let scalar = is_scalar(module, func.value_ty(word));
                                if !self.queue_returned(func_id, scalar, &mut pending) {
                                    break 'words true;
                                }
                            }
                            Some(Terminator::TailCall { function, args }) => {
                                for (index, _) in
                                    args.iter().enumerate().filter(|&(_, &arg)| arg == word)
                                {
                                    let values = self.arg_values(*function, index);
                                    pending.extend(values.into_iter().map(|v| (*function, v)));
                                }
                            }
                            _ => break 'words true,
                        },
                    }
                }
            }
            false
        };
        self.pointer_loads.insert((func_id, load), pointer);
        pointer
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

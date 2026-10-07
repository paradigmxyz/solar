//! Keeps the free memory pointer at or above the heap floor where inline assembly lowers it.
//!
//! Codegen places spill slots and internal-call frames below each entry's initial free memory
//! pointer (CODEGEN-009), where `solc` keeps those words on the stack. Memory-unsafe assembly can
//! hand that memory back to the heap by storing an absolute address into the pointer's slot:
//! Seaport moves the pointer to just past the event data it lays out at an address that calldata
//! sizes, and resets it to `0x80` after batch transfers. The next allocation then overwrites live
//! compiler words. This pass rewrites each such store, `mstore 64, v` with `v` computed from
//! constants and calldata alone, into
//!
//! ```text
//! floor = heap_floor
//! below = lt v, floor
//! clamped = select below, floor, v
//! mstore 64, clamped
//! ```
//!
//! where `heap_floor` is the initial pointer of every entry that can run the function, which
//! codegen fixes once it has placed the frames. A pointer derived from the heap, or loaded from
//! memory or a parameter, already lies above the floor in a valid program and stays as stored.
//!
//! A store that only fills the slot with data keeps its value: an error argument before a revert,
//! or a hash input before the pointer is restored. A forward search tells the two apart. It
//! follows the stored value through blocks, into internal calls, and from returns back to every
//! call site, until something reads the slot as the pointer, another store or a copy over the whole
//! slot replaces it, or execution ends. An `mload` of the slot reads it as the pointer when its
//! word, or a word computed from it, addresses or sizes memory, becomes the pointer again, or
//! leaves the function through a call or an internal return, as every allocation in assembly
//! does; a word only compared, stored, or returned from an external function is data. The
//! compiler's own free-memory-pointer reads and allocations read the slot as the pointer too, and
//! so do the builtins and semantic operations whose lowering may allocate or encode at the
//! pointer, unlike pure computations, checks that revert with scratch data, and accesses to
//! frames, storage slots, and memory that already exists. A return from the constructor, or one
//! from an external function that returns only scalar words, ends execution without reading the
//! slot: codegen returns runtime code from a fixed address and encodes scalar results in a static
//! buffer.
//!
//! The pass runs first, on semantic MIR, so the passes that forward the slot's stored value to
//! its loads or delete its dead stores see the clamped value. Clamping during codegen instead
//! left a load forwarded from the store with the raw address.
//!
//! NOTE: Only a full-word store to the constant slot whose value the storing function computes
//! from constants and calldata is clamped. An absolute address passed through a parameter, memory,
//! or a call, and copies or byte stores that overwrite the pointer, keep their bytes. See
//! CODEGEN-010.

use crate::mir::{
    BlockId, Builtin, Callee, EffectKind, Function, FunctionId, InstId, InstKind, Instruction,
    MirType, Module, Terminator, ValueId,
    analysis::{AddressInput, absolute_address_inputs},
    memory::EvmMemoryLayout,
    pass::{MirPass, ModuleAnalyses},
    utils::IndexLists,
};
use solar_data_structures::{
    bit_set::DenseBitSet,
    map::{FxHashMap, FxHashSet},
};

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
        let stores = floor_stores(module);
        let changed = !stores.is_empty();
        for (func_id, block, inst) in stores {
            clamp_store(&mut module.functions[func_id], block, inst);
        }
        changed
    }
}

/// The stores to the free memory pointer's slot whose value the function computes from constants
/// and calldata alone and that something can read back as the pointer.
fn floor_stores(module: &Module) -> Vec<(FunctionId, BlockId, InstId)> {
    let mut uses = PointerUses::new(module);
    let mut stores = Vec::new();
    for (func_id, func) in module.functions.iter_enumerated() {
        let mut inputs = None;
        for (block, data) in func.blocks.iter_enumerated() {
            for (index, &inst_id) in data.instructions.iter().enumerate() {
                if let InstKind::MStore(address, value) = func.inst(inst_id).kind
                    && is_slot(func, address)
                    && func.value_ty(value) == Some(MirType::I256)
                    && inputs.get_or_insert_with(|| absolute_address_inputs(func))[value]
                        != AddressInput::Other
                    && uses.store_may_be_used_as_pointer(func_id, block, index)
                {
                    stores.push((func_id, block, inst_id));
                }
            }
        }
    }
    stores
}

/// Rewrites `mstore 64, v` into `mstore 64, max(v, heap_floor)`.
fn clamp_store(func: &mut Function, block: BlockId, inst: InstId) {
    let InstKind::MStore(address, value) = func.inst(inst).kind else {
        unreachable!("a floor store is an `mstore`")
    };
    let metadata = func.inst(inst).metadata.clone();
    let emit = |func: &mut Function, kind, ty| {
        let mut instruction = Instruction::new(kind, Some(ty));
        instruction.metadata.copy_debug_context(&metadata);
        func.alloc_value_inst(instruction)
    };
    // floor = heap_floor
    // below = lt v, floor
    // clamped = select below, floor, v
    let (floor_inst, floor) = emit(func, InstKind::HeapFloor, MirType::I256);
    let (below_inst, below) = emit(func, InstKind::Lt(value, floor), MirType::I1);
    let (clamped_inst, clamped) = emit(func, InstKind::Select(below, floor, value), MirType::I256);
    let instructions = &mut func.blocks[block].instructions;
    let position =
        instructions.iter().position(|&id| id == inst).expect("the store is in its block");
    instructions.splice(position..position, [floor_inst, below_inst, clamped_inst]);
    // mstore 64, clamped
    func.inst_mut(inst).kind = InstKind::MStore(address, clamped);
}

/// Whether a value is the address of the free memory pointer's slot.
fn is_slot(func: &Function, address: ValueId) -> bool {
    func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT)
}

/// Whether a function returns to code that ends execution without reading the free memory
/// pointer: the constructor, whose runtime code codegen copies to a fixed address, or an
/// external function returning only scalar words, which codegen encodes in a static buffer.
fn returns_to_the_end(func: &Function) -> bool {
    func.attributes.is_constructor
        || func.is_external_entry()
            && func.blocks.iter().all(|block| match &block.terminator {
                Some(Terminator::Return { values }) => values
                    .iter()
                    .all(|&value| matches!(func.value_ty(value), Some(MirType::Int(_)))),
                _ => true,
            })
}

/// Whether a copy or a zeroing writes every byte of the free memory pointer's slot, replacing the
/// word stored there.
fn overwrites_slot(func: &Function, kind: &InstKind) -> bool {
    let (dest, size) = match *kind {
        InstKind::CalldataCopy(dest, _, size)
        | InstKind::CodeCopy(dest, _, size)
        | InstKind::ReturnDataCopy(dest, _, size)
        | InstKind::ExtCodeCopy(_, dest, _, size)
        | InstKind::DataCopy(_, dest, size)
        | InstKind::MCopy(dest, _, size)
        | InstKind::MemoryZero(dest, size) => (dest, size),
        _ => return false,
    };
    let slot = EvmMemoryLayout::FMP_SLOT;
    func.value_u64(dest).zip(func.value_u64(size)).is_some_and(|(dest, size)| {
        dest <= slot
            && dest.checked_add(size).is_some_and(|end| end >= slot + EvmMemoryLayout::WORD_SIZE)
    })
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

/// How a terminator uses a word loaded from the slot, or a word computed from one.
fn terminator_word_use(func: &Function, block: BlockId, word: ValueId) -> WordUse {
    match func.blocks[block].terminator.as_ref() {
        Some(Terminator::Branch { .. } | Terminator::Switch { .. }) => WordUse::Data,
        Some(Terminator::SelfDestruct { .. }) => WordUse::Data,
        // An external function encodes a returned integer as data.
        Some(Terminator::Return { .. })
            if func.is_external_entry() && matches!(func.value_ty(word), Some(MirType::Int(_))) =>
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

/// What happens to a value left in the free memory pointer slot from some point on.
#[derive(Clone, Copy)]
enum Flow {
    /// A path reads it as the pointer.
    Used,
    /// Every path overwrites it or ends execution first.
    Replaced,
    /// No path reads it as the pointer, and one returns it to the caller.
    Returned,
}

/// A call to a function: an internal call at an instruction, or a tail call ending a function.
#[derive(Clone, Copy)]
enum CallSite {
    Call(FunctionId, BlockId, usize),
    Tail(FunctionId),
}

/// Follows a value stored into the free memory pointer slot through blocks, internal calls, and
/// returns until it is read as the pointer, replaced, or execution ends.
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
            users: FxHashMap::default(),
            pointer_loads: FxHashMap::default(),
        }
    }

    /// Returns whether the value stored by the instruction at `index` in `block` can be read
    /// back as the free memory pointer, in its function or after it returns.
    fn store_may_be_used_as_pointer(
        &mut self,
        func_id: FunctionId,
        block: BlockId,
        index: usize,
    ) -> bool {
        match self.flow_from(func_id, block, index + 1) {
            Flow::Used => true,
            Flow::Replaced => false,
            Flow::Returned => self.used_after_return(func_id, &mut FxHashSet::default()),
        }
    }

    /// Returns whether a value that `func_id` returns in the slot can be read as the pointer
    /// after any of its call sites.
    fn used_after_return(
        &mut self,
        func_id: FunctionId,
        returned: &mut FxHashSet<FunctionId>,
    ) -> bool {
        if !returned.insert(func_id) {
            return false;
        }
        let sites = self.call_sites(func_id).to_vec();
        if sites.is_empty() {
            // Any other function no call reaches returns to code the module does not show.
            return !returns_to_the_end(&self.module.functions[func_id]);
        }
        sites.into_iter().any(|site| match site {
            CallSite::Call(caller, block, index) => {
                match self.flow_from(caller, block, index + 1) {
                    Flow::Used => true,
                    Flow::Replaced => false,
                    Flow::Returned => self.used_after_return(caller, returned),
                }
            }
            CallSite::Tail(caller) => self.used_after_return(caller, returned),
        })
    }

    /// Returns what a call to `func_id` does with the value its caller left in the slot.
    ///
    /// A call into a function still being followed passes the value through as far as the
    /// search knows: following that function's own paths finds any read the call could make,
    /// and the search goes on after the call. A flow that relied on that is remembered only once
    /// the outermost function it passed into is done.
    fn entry_flow(&mut self, func_id: FunctionId) -> Flow {
        if let Some(&flow) = self.entry_flows.get(&func_id) {
            return flow;
        }
        if let Some(&depth) = self.following.get(&func_id) {
            self.shallowest = self.shallowest.min(depth);
            return Flow::Returned;
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

    /// Follows the value from instruction `start` of `block` to the end of `func_id`.
    fn flow_from(&mut self, func_id: FunctionId, block: BlockId, start: usize) -> Flow {
        let module = self.module;
        let func = &module.functions[func_id];
        let mut visited = DenseBitSet::new_empty(func.blocks.len());
        let mut pending = vec![(block, start)];
        let mut returned = false;
        while let Some((block, start)) = pending.pop() {
            let data = &func.blocks[block];
            let mut replaced = false;
            for &inst_id in &data.instructions[start.min(data.instructions.len())..] {
                let inst = func.inst(inst_id);
                match inst.kind {
                    InstKind::MStore(address, _) if is_slot(func, address) => {
                        replaced = true;
                        break;
                    }
                    InstKind::SetFmp(_) => {
                        replaced = true;
                        break;
                    }
                    _ if overwrites_slot(func, &inst.kind) => {
                        replaced = true;
                        break;
                    }
                    InstKind::MLoad(address)
                        if is_slot(func, address) && self.loaded_as_pointer(func_id, inst_id) =>
                    {
                        return Flow::Used;
                    }
                    InstKind::ICall { function: Callee::Function(callee), .. } => {
                        match self.entry_flow(callee) {
                            Flow::Used => return Flow::Used,
                            Flow::Replaced => {
                                replaced = true;
                                break;
                            }
                            Flow::Returned => {}
                        }
                    }
                    // The compiler's own pointer reads and allocations, and the builtins and
                    // semantic operations whose lowering may allocate or encode at the pointer.
                    InstKind::Fmp | InstKind::Alloc { .. } => return Flow::Used,
                    _ if inst.unlowered_reason(func).is_some()
                        && lowered_reads_pointer(&inst.kind) =>
                    {
                        return Flow::Used;
                    }
                    _ => {}
                }
            }
            if replaced {
                continue;
            }
            match data.terminator.as_ref() {
                Some(
                    Terminator::Revert { .. }
                    | Terminator::RevertReturndata
                    | Terminator::ReturnData { .. }
                    | Terminator::Stop
                    | Terminator::Invalid
                    | Terminator::SelfDestruct { .. },
                ) => {}
                Some(
                    terminator @ (Terminator::Jump(_)
                    | Terminator::Branch { .. }
                    | Terminator::Switch { .. }),
                ) => terminator.for_each_successor(|successor| {
                    if visited.insert(successor) {
                        pending.push((successor, 0));
                    }
                }),
                Some(Terminator::Return { .. }) => returned = true,
                // The tail callee returns to this function's callers.
                Some(&Terminator::TailCall { function, .. }) => match self.entry_flow(function) {
                    Flow::Used => return Flow::Used,
                    Flow::Replaced => {}
                    Flow::Returned => returned = true,
                },
                None => return Flow::Used,
            }
        }
        if returned { Flow::Returned } else { Flow::Replaced }
    }

    /// Returns whether the word a load reads from the slot is used as the pointer: it, or a word
    /// computed from it, addresses or sizes memory, becomes the pointer again, or leaves the
    /// function through a call or an internal return.
    fn loaded_as_pointer(&mut self, func_id: FunctionId, load: InstId) -> bool {
        if let Some(&pointer) = self.pointer_loads.get(&(func_id, load)) {
            return pointer;
        }
        let module = self.module;
        let func = &module.functions[func_id];
        let users = self.users.entry(func_id).or_insert_with(|| value_users(func));
        let mut derived = FxHashSet::default();
        let mut pending = func.inst_result_value(load).into_iter().collect::<Vec<_>>();
        let mut pointer = false;
        'words: while let Some(word) = pending.pop() {
            if !derived.insert(word) {
                continue;
            }
            for &user in users.get(word) {
                let word_use = match user {
                    User::Inst(inst) => word_use(func, &func.inst(inst).kind, word),
                    User::Terminator(block) => terminator_word_use(func, block, word),
                };
                match (word_use, user) {
                    (WordUse::Derives, User::Inst(inst)) => {
                        pending.extend(func.inst_result_value(inst));
                    }
                    (WordUse::Data, _) => {}
                    _ => {
                        pointer = true;
                        break 'words;
                    }
                }
            }
        }
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

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
//! call site, until something reads the slot as the pointer, another store replaces it, or
//! execution ends. An `mload` of the slot reads it as the pointer, as every allocation in
//! assembly starts with one, and so do the compiler's own free-memory-pointer reads and
//! allocations, builtin calls, and the semantic operations whose lowering may allocate. A return
//! from the constructor, or one from an external function that returns only scalar words, ends
//! execution without reading the slot: codegen returns runtime code from a fixed address and
//! encodes scalar results in a static buffer.
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
    BlockId, Callee, Function, FunctionId, InstId, InstKind, Instruction, MirType, Module,
    Terminator,
    analysis::{AddressInput, absolute_address_inputs},
    memory::EvmMemoryLayout,
    pass::{MirPass, ModuleAnalyses},
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
                    && func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT)
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

/// Whether a function returns to code that ends execution without reading the free memory
/// pointer: the constructor, whose runtime code codegen copies to a fixed address, or an
/// external function returning only scalar words, which codegen encodes in a static buffer.
fn returns_to_the_end(func: &Function) -> bool {
    if func.attributes.is_constructor {
        return true;
    }
    let external =
        func.selector.is_some() || func.attributes.is_receive || func.attributes.is_fallback;
    external
        && func.blocks.iter().all(|block| match &block.terminator {
            Some(Terminator::Return { values }) => {
                values.iter().all(|&value| matches!(func.value_ty(value), Some(MirType::Int(_))))
            }
            _ => true,
        })
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
    /// The flow from the entry of each function, or `None` while it is being computed.
    entry_flows: FxHashMap<FunctionId, Option<Flow>>,
    /// The call sites of each function, built on first use.
    call_sites: Option<FxHashMap<FunctionId, Vec<CallSite>>>,
}

impl<'a> PointerUses<'a> {
    fn new(module: &'a Module) -> Self {
        Self { module, entry_flows: FxHashMap::default(), call_sites: None }
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
    fn entry_flow(&mut self, func_id: FunctionId) -> Flow {
        match self.entry_flows.get(&func_id) {
            Some(&Some(flow)) => return flow,
            // A recursive call is assumed to allocate.
            Some(None) => return Flow::Used,
            None => {}
        }
        self.entry_flows.insert(func_id, None);
        let flow = self.flow_from(func_id, BlockId::ENTRY, 0);
        self.entry_flows.insert(func_id, Some(flow));
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
                    InstKind::MStore(address, _)
                        if func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT) =>
                    {
                        replaced = true;
                        break;
                    }
                    InstKind::SetFmp(_) => {
                        replaced = true;
                        break;
                    }
                    InstKind::MLoad(address)
                        if func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT) =>
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
                    // The compiler's own pointer reads and allocations, builtins, and semantic
                    // operations whose lowering may allocate.
                    InstKind::Fmp | InstKind::Alloc { .. } | InstKind::ICall { .. } => {
                        return Flow::Used;
                    }
                    _ if inst.unlowered_reason(func).is_some() => return Flow::Used,
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

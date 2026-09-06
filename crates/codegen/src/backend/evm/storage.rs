//! Function activation and compiler-owned memory layout.
//!
//! This planner consumes physical MIR without emitting instructions. Each internal frame has the
//! retained two-word header, followed by argument words, return words, lowered locals and spills.
//! Static single-result activations can receive arguments on the stack even when values need
//! spill homes; their reserved memory remains in the ordinary frame-sharing plan.
//! Runtime functions outside recursive call-graph components receive static frames;
//! constructor callees and recursive functions receive activation-relative frames. Entry-local
//! absolute addresses remain rooted at `HEAP_START`, as required by retained MIR lowering.
//!
//! Scheduling reserves spill words before final layout. Static frames and deferred allocations sit
//! above entry locals and immutable staging. Entry spill regions overlap because entries do not
//! return to one another. Private single-result static activations share storage across sibling
//! call paths; longest-path frame ends keep every ancestor disjoint from its descendants, including
//! paths through dynamic activations. Address-exposed and multi-return frames stay distinct.
//! Deferred allocations from different runtime entries share a pool only when the final call graph
//! admits each entry exclusively through dispatcher tail calls. Their retained nonescape proof and
//! mutually exclusive entry lifetimes keep those pools disjoint in time; deployment and all other
//! functions retain separate pools. Repeated finalization rebuilds absolute addresses from relative
//! sizes so reservations cannot leave stale relocations.
//!
//! Dynamic frames whose addresses are exposed, whose return types reference memory, or whose sticky
//! `may_return_memory` attribute is set must remain allocated on return. The call emitter owns frame
//! setup, restoration, runtime overflow checks and the EVM return-label stack protocol. This module
//! checks every constant address/size calculation and never silently wraps a layout.

use crate::{
    analysis::{CallGraphInfo, MemoryCallSummaries},
    memory::EvmMemoryLayout,
    mir::{AllocationAlignment, Function, FunctionId, InstId, InstKind, Module, Terminator},
};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
use std::collections::VecDeque;

const WORD: u64 = EvmMemoryLayout::WORD_SIZE;

/// Offset of the suspended caller's frame pointer in a dynamic frame.
pub(crate) const PREVIOUS_FRAME_OFFSET: u64 = 0;
/// Offset of the free-memory pointer saved on entry to a dynamic frame.
pub(crate) const SAVED_FMP_OFFSET: u64 = WORD;

/// Origin of a function's frame-relative addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameBase {
    /// A per-function region whose address is known after layout.
    Static(u64),
    /// A new activation allocated by the calling convention.
    Dynamic,
}

/// The final location of an individual frame word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameAddress {
    /// A byte address in compiler-owned fixed memory.
    Absolute(u64),
    /// A byte offset from the active dynamic frame pointer.
    Relative(u64),
}

/// Memory requirements of one function, indexed by its MIR identity.
#[derive(Clone, Debug)]
pub(crate) struct FunctionStorage {
    /// Whether this artifact can execute the function.
    pub(crate) reachable: bool,
    /// Whether this function owns absolute low-memory entry locals.
    pub(crate) is_entry: bool,
    /// Whether a returning activation receives arguments on the stack before any entry spills.
    pub(crate) stack_arguments: bool,
    /// Static address or activation-relative addressing mode.
    pub(crate) base: FrameBase,
    /// Offset of the first argument word from the frame base.
    pub(crate) argument_offset: u64,
    /// Offset of the first return word from the frame base.
    pub(crate) return_offset: u64,
    /// Fixed scratch buffer for extra results of a stack activation.
    pub(crate) stack_return_base: u64,
    /// Offset of the first lowered mutable-local byte.
    pub(crate) local_offset: u64,
    /// Offset of the first scheduler spill word from the frame base.
    pub(crate) spill_offset: u64,
    /// Bytes to reserve for the entire function activation, including spills.
    pub(crate) frame_size: u64,
    /// Whether returning from a dynamic activation must preserve its allocation.
    pub(crate) retain_frame: bool,
    /// Whether the frame address can remain observable after a returning activation.
    pub(crate) address_exposed: bool,
    /// Sparse allocation-site offsets within this function's deferred-allocation area.
    pub(crate) deferred_allocations: FxHashMap<InstId, u64>,
    /// Absolute start of this function's deferred-allocation area after layout.
    pub(crate) deferred_base: u64,
    local_end: u64,
    spill_size: u64,
    deferred_size: u64,
}

impl FunctionStorage {
    /// Resolves a frame-relative byte offset without conflating static and dynamic addresses.
    pub(crate) fn address(&self, offset: u64) -> Result<FrameAddress, &'static str> {
        match self.base {
            FrameBase::Static(base) => Ok(FrameAddress::Absolute(add(base, offset)?)),
            FrameBase::Dynamic => Ok(FrameAddress::Relative(offset)),
        }
    }

    /// Extra result buffers are consumed immediately after each return, so stack activations share a fixed per-function buffer.
    pub(crate) fn return_address(&self, index: usize) -> Result<FrameAddress, &'static str> {
        if self.stack_arguments {
            Ok(FrameAddress::Absolute(add(self.stack_return_base, words(index)?)?))
        } else {
            self.address(add(self.return_offset, words(index)?)?)
        }
    }

    /// Resolves a reserved spill word, rejecting a scheduler/planner mismatch.
    pub(crate) fn spill_address(&self, index: usize) -> Result<FrameAddress, &'static str> {
        let offset = words(index)?;
        if offset >= self.spill_size {
            return Err("EVM spill index exceeds the reserved frame region");
        }
        self.address(add(self.spill_offset, offset)?)
    }

    /// Resolves a deferred allocation after final layout.
    pub(crate) fn allocation_address(&self, inst: InstId) -> Result<u64, &'static str> {
        let offset = self
            .deferred_allocations
            .get(&inst)
            .ok_or("EVM allocation has no deferred storage reservation")?;
        add(self.deferred_base, *offset)
    }
}

/// Storage owned by one runtime or deployment artifact.
#[derive(Clone, Debug)]
pub(crate) struct ModulePlan {
    /// Function layouts use the complete stable MIR function ID domain.
    pub(crate) functions: IndexVec<FunctionId, FunctionStorage>,
    /// First immutable staging byte, matching retained `immutable.rs`.
    pub(crate) immutable_staging_base: u64,
    /// First byte after the immutable staging words.
    pub(crate) immutable_staging_end: u64,
    /// First byte available for heap allocation after final layout.
    pub(crate) fixed_memory_end: u64,
    /// Largest dynamic activation, used when a static caller has a dynamic ancestor.
    pub(crate) max_dynamic_frame_size: u64,
    /// Start of the copied appended constructor arguments; their dynamic size is a runtime fact.
    pub(crate) constructor_arg_base: u64,
    reserved_end: u64,
    callees: IndexVec<FunctionId, Box<[FunctionId]>>,
    shared_deferred_entries: DenseBitSet<FunctionId>,
}

impl ModulePlan {
    /// Plans one artifact; deployment callees always use dynamic activations.
    pub(crate) fn new(module: &Module, deployment: bool) -> Result<Self, &'static str> {
        let calls = CallGraphInfo::new(module);
        let memory_effects = MemoryCallSummaries::new(module);
        let constructors = module
            .functions
            .iter_enumerated()
            .filter_map(|(id, function)| function.attributes.is_constructor.then_some(id))
            .collect::<Vec<_>>();
        let mut constructor_reachable = calls.reachable_callees_from(constructors.iter().copied());
        for &constructor in &constructors {
            constructor_reachable.insert(constructor);
        }
        let mut roots = DenseBitSet::new_empty(module.functions.len());
        if deployment {
            roots.union(&constructor_reachable);
        } else {
            if let Some(entry) = module.dispatch_entry() {
                roots.insert(entry);
            }
            for (id, function) in module.functions.iter_enumerated() {
                if !function.attributes.is_constructor && is_entry(function) {
                    roots.insert(id);
                }
            }
        }
        let mut reachable = calls.reachable_callees_from(roots.iter());
        reachable.union(&roots);

        let constructor_end =
            constructors.first().map_or(Ok(EvmMemoryLayout::HEAP_START), |&id| {
                add(EvmMemoryLayout::HEAP_START, module.functions[id].internal_frame_size)
            })?;
        let immutable_staging_base =
            align(constructor_end.max(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT + WORD))?;
        let immutable_staging_end = add(immutable_staging_base, words(module.immutable_count())?)?;
        let mut reserved_end = EvmMemoryLayout::HEAP_START;
        if deployment && module.immutable_count() != 0 {
            reserved_end = immutable_staging_end;
        }
        let mut functions = IndexVec::with_capacity(module.functions.len());
        for (id, function) in module.functions.iter_enumerated() {
            let entry = is_entry(function) || module.dispatch_entry() == Some(id);
            let mut storage = function_storage(function, entry)?;
            storage.reachable = reachable.contains(id);
            storage.retain_frame |=
                memory_effects.get(id).is_none_or(|summary| summary.may_reset_fmp());
            storage.base = if entry {
                FrameBase::Static(EvmMemoryLayout::HEAP_START)
            } else if deployment
                || calls.is_recursive(id)
                || (storage.address_exposed
                    && function.blocks.iter().any(|block| {
                        matches!(
                            block.terminator,
                            Some(Terminator::Return { .. } | Terminator::Stop)
                        )
                    }))
            {
                FrameBase::Dynamic
            } else {
                // Absolute bases are assigned together after all spill reservations.
                FrameBase::Static(0)
            };
            if storage.reachable {
                if entry {
                    reserved_end =
                        reserved_end.max(add(EvmMemoryLayout::HEAP_START, storage.local_end)?);
                } else if storage.base == FrameBase::Dynamic && !storage.stack_arguments {
                    reserved_end =
                        reserved_end.max(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT + WORD);
                }
            }
            functions.push(storage);
        }
        let mut shared_deferred_entries = if !deployment && module.dispatch_entry().is_some() {
            roots
        } else {
            DenseBitSet::new_empty(module.functions.len())
        };
        if let Some(dispatch) = module.dispatch_entry() {
            shared_deferred_entries.remove(dispatch);
        }
        let mut callees = IndexVec::new();
        for (id, function) in module.iter_functions() {
            let mut targets = function
                .instructions()
                .filter_map(|inst| {
                    if let InstKind::ICall { function, .. } = function.inst(inst).kind {
                        shared_deferred_entries.remove(function);
                        Some(function)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            targets.extend(function.blocks.iter().filter_map(|block| {
                if let Some(Terminator::TailCall { function, .. }) = block.terminator {
                    if Some(id) != module.dispatch_entry() {
                        shared_deferred_entries.remove(function);
                    }
                    Some(function)
                } else {
                    None
                }
            }));
            targets.sort_unstable();
            targets.dedup();
            callees.push(targets.into_boxed_slice());
        }
        let mut plan = Self {
            functions,
            immutable_staging_base,
            immutable_staging_end,
            fixed_memory_end: reserved_end,
            max_dynamic_frame_size: 0,
            constructor_arg_base: reserved_end,
            reserved_end: align(reserved_end)?,
            callees,
            shared_deferred_entries,
        };
        plan.finalize()?;
        Ok(plan)
    }

    /// Reserves the scheduler's required spill words; call `finalize` before reading addresses.
    pub(crate) fn reserve_spills(
        &mut self,
        function: FunctionId,
        count: usize,
    ) -> Result<(), &'static str> {
        self.functions[function].spill_size = words(count)?;
        let frame = &mut self.functions[function];
        if count != 0
            && !(matches!(frame.base, FrameBase::Static(_))
                && frame.local_offset - frame.return_offset <= WORD)
        {
            frame.stack_arguments = false;
        }
        Ok(())
    }

    /// Recomputes absolute addresses after scheduler reservations without accumulating state.
    pub(crate) fn finalize(&mut self) -> Result<(), &'static str> {
        if self.functions.iter().any(|function| {
            function.reachable && !function.stack_arguments && function.base == FrameBase::Dynamic
        }) {
            self.reserved_end =
                self.reserved_end.max(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT + WORD);
        }
        let mut end = self.reserved_end;
        for function in &mut self.functions {
            if !function.reachable {
                continue;
            }
            function.spill_offset = if function.is_entry {
                self.reserved_end - EvmMemoryLayout::HEAP_START
            } else {
                function.local_end
            };
            function.frame_size = if function.stack_arguments && function.spill_size == 0 {
                0
            } else {
                add(function.spill_offset, function.spill_size)?
            };
            if function.is_entry {
                end = end.max(add(self.reserved_end, function.spill_size)?);
            }
        }
        for function in &mut self.functions {
            if function.reachable
                && !function.is_entry
                && function.frame_size != 0
                && matches!(function.base, FrameBase::Static(_))
                && !shareable_frame(function)
            {
                function.base = FrameBase::Static(end);
                end = add(end, function.frame_size)?;
            }
        }
        let offsets = self.shared_frame_offsets()?;
        let shared_base = end;
        for (id, function) in self.functions.iter_mut_enumerated() {
            if shareable_frame(function) {
                let base = add(shared_base, offsets[id])?;
                function.base = FrameBase::Static(base);
                end = end.max(add(base, function.frame_size)?);
            }
        }
        for function in &mut self.functions {
            if function.reachable
                && function.stack_arguments
                && function.local_offset - function.return_offset > WORD
            {
                function.stack_return_base = end;
                end = add(end, function.local_offset - function.return_offset)?;
            }
        }
        let shared_deferred_base = end;
        let shared_deferred_size = self
            .shared_deferred_entries
            .iter()
            .map(|id| self.functions[id].deferred_size)
            .max()
            .unwrap_or(0);
        end = add(end, shared_deferred_size)?;
        for (id, function) in self.functions.iter_mut_enumerated() {
            if function.reachable {
                if self.shared_deferred_entries.contains(id) {
                    function.deferred_base = shared_deferred_base;
                } else {
                    function.deferred_base = end;
                    end = add(end, function.deferred_size)?;
                }
            }
        }
        self.max_dynamic_frame_size = self
            .functions
            .iter()
            .filter(|function| {
                function.reachable
                    && !function.stack_arguments
                    && function.base == FrameBase::Dynamic
            })
            .map(|function| function.frame_size)
            .max()
            .unwrap_or(0);
        self.fixed_memory_end = align(end)?;
        self.constructor_arg_base = self.fixed_memory_end;
        Ok(())
    }

    /// Propagates maximum live ancestor ends; recursive components have zero static weight.
    fn shared_frame_offsets(&self) -> Result<IndexVec<FunctionId, u64>, &'static str> {
        let mut offsets = IndexVec::from_vec(vec![0; self.functions.len()]);
        let mut queued = DenseBitSet::new_empty(self.functions.len());
        let mut pending = VecDeque::new();
        for (id, function) in self.functions.iter_enumerated() {
            if function.reachable {
                queued.insert(id);
                pending.push_back(id);
            }
        }
        while let Some(id) = pending.pop_front() {
            queued.remove(id);
            let frame = &self.functions[id];
            let end = add(offsets[id], if shareable_frame(frame) { frame.frame_size } else { 0 })?;
            for &callee in &self.callees[id] {
                if offsets[callee] < end {
                    offsets[callee] = end;
                    if queued.insert(callee) {
                        pending.push_back(callee);
                    }
                }
            }
        }
        Ok(offsets)
    }
}

/// Multi-return pointers can expose frame storage beyond the activation's return.
fn shareable_frame(frame: &FunctionStorage) -> bool {
    frame.reachable
        && !frame.is_entry
        && frame.frame_size != 0
        && !frame.address_exposed
        && frame.local_offset - frame.return_offset <= WORD
        && matches!(frame.base, FrameBase::Static(_))
}

fn function_storage(function: &Function, entry: bool) -> Result<FunctionStorage, &'static str> {
    let argument_offset = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE;
    let return_offset = add(argument_offset, words(function.params.len())?)?;
    let local_offset = add(return_offset, words(function.returns.len())?)?;
    let local_end = align(if entry {
        function.internal_frame_size.max(function.external_static_return_size)
    } else {
        add(local_offset, function.internal_frame_size)?
    })?;
    let mut deferred_allocations = FxHashMap::default();
    let mut deferred_size = 0;
    for inst in function.instructions() {
        let instruction = function.inst(inst);
        if let InstKind::Alloc { size, semantics, .. } = instruction.kind {
            if !instruction.metadata.deferred_alloc() {
                return Err("EVM storage planning requires lowered allocations");
            }
            let size =
                function.value_u64(size).ok_or("EVM deferred allocation size is not constant")?;
            let size =
                if semantics.alignment == AllocationAlignment::Word { align(size)? } else { size };
            let offset = align(deferred_size)?;
            deferred_size = add(offset, size)?;
            deferred_allocations.insert(inst, offset);
        }
    }
    let address_exposed = frame_address_escapes(function);
    Ok(FunctionStorage {
        reachable: false,
        is_entry: entry,
        stack_arguments: !entry
            && !address_exposed
            && function.internal_frame_size == 0
            && function.params.len() <= 12
            && !function
                .instructions()
                .any(|inst| matches!(function.inst(inst).kind, InstKind::InternalFrameAddr(_)))
            && function.blocks.iter().any(|block| {
                matches!(block.terminator, Some(Terminator::Return { .. } | Terminator::Stop))
            }),
        base: FrameBase::Dynamic,
        argument_offset,
        return_offset,
        stack_return_base: 0,
        local_offset,
        spill_offset: local_end,
        frame_size: local_end,
        address_exposed,
        retain_frame: address_exposed
            || function.attributes.may_return_memory
            || function.returns.iter().any(|ty| ty.is_memory_reference()),
        deferred_allocations,
        deferred_base: 0,
        local_end,
        spill_size: 0,
        deferred_size: align(deferred_size)?,
    })
}

/// Treats frame-address arithmetic as derivation and pointer-valued stores/calls as escapes.
/// Ordinary loads and stores through a frame address do not expose the address itself.
fn frame_address_escapes(function: &Function) -> bool {
    let mut addresses = DenseBitSet::new_empty(function.num_values());
    for inst in function.instructions() {
        if matches!(function.inst(inst).kind, InstKind::InternalFrameAddr(_))
            && let Some(result) = function.inst_result_value(inst)
        {
            addresses.insert(result);
        }
    }
    if addresses.is_empty() {
        return false;
    }
    loop {
        let mut changed = false;
        for inst in function.instructions() {
            let instruction = function.inst(inst);
            if !instruction.operands().iter().any(|&value| addresses.contains(value)) {
                continue;
            }
            match &instruction.kind {
                InstKind::Add(..) | InstKind::Sub(..) | InstKind::Phi(_) => {
                    if let Some(result) = function.inst_result_value(inst) {
                        changed |= addresses.insert(result);
                    }
                }
                InstKind::Select(condition, _, _) if !addresses.contains(*condition) => {
                    if let Some(result) = function.inst_result_value(inst) {
                        changed |= addresses.insert(result);
                    }
                }
                InstKind::MLoad(_) => {}
                InstKind::MStore(_, value) | InstKind::MStore8(_, value)
                    if !addresses.contains(*value) => {}
                InstKind::MCopy(_, _, size) | InstKind::Keccak256(_, size)
                    if !addresses.contains(*size) => {}
                _ => return true,
            }
        }
        if !changed {
            break;
        }
    }
    function.blocks.iter().any(|block| {
        block
            .terminator
            .as_ref()
            .is_some_and(|term| term.operands().iter().any(|&value| addresses.contains(value)))
    })
}

fn is_entry(function: &Function) -> bool {
    function.selector.is_some()
        || function.attributes.is_constructor
        || function.attributes.is_receive
        || function.attributes.is_fallback
}

fn words(count: usize) -> Result<u64, &'static str> {
    u64::try_from(count)
        .ok()
        .and_then(|count| count.checked_mul(WORD))
        .ok_or("EVM frame word count exceeds the allocation limit")
}

fn add(base: u64, size: u64) -> Result<u64, &'static str> {
    base.checked_add(size).ok_or("EVM memory layout exceeds the allocation limit")
}

fn align(size: u64) -> Result<u64, &'static str> {
    EvmMemoryLayout::align_word(size).ok_or("EVM memory alignment exceeds the allocation limit")
}

#[cfg(test)]
mod tests {
    use super::{add, align, frame_address_escapes, words};
    use crate::mir::{Function, FunctionBuilder};
    use solar_interface::Ident;

    #[test]
    fn frame_load_is_local_but_returning_address_escapes() {
        let mut function = Function::new(Ident::DUMMY);
        let pointer = {
            let mut builder = FunctionBuilder::new(&mut function);
            let pointer = builder.internal_frame_addr(64);
            let value = builder.mload(pointer);
            builder.ret([value]);
            pointer
        };
        assert!(!frame_address_escapes(&function));
        FunctionBuilder::new(&mut function).ret([pointer]);
        assert!(frame_address_escapes(&function));
    }

    #[test]
    fn checked_word_layout_boundaries() {
        assert_eq!(words(0), Ok(0));
        assert_eq!(words(3), Ok(96));
        assert_eq!(align(0), Ok(0));
        assert_eq!(align(33), Ok(64));
        assert_eq!(align(u64::MAX - 31), Ok(u64::MAX - 31));
        assert!(align(u64::MAX - 30).is_err());
        assert_eq!(add(u64::MAX - 32, 32), Ok(u64::MAX));
        assert!(add(u64::MAX, 1).is_err());
        if usize::BITS == 64 {
            assert!(words(usize::MAX).is_err());
        }
    }
}

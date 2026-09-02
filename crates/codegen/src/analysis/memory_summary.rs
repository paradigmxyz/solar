//! Interprocedural memory and pointer-capture summaries.
//!
//! Summaries are computed to a fixpoint over internal-call edges. Missing
//! bodies stay fully conservative; recursive groups converge because every
//! fact only moves from false to true.

use super::{AddressSpace, AliasAnalysis};
use crate::{
    memory::EvmMemoryLayout,
    mir::{ArgIdx, Function, FunctionId, InstId, InstKind, Module, Terminator, Value, ValueId},
};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashSet};
use std::collections::VecDeque;

/// Conservative memory effects and pointer captures for one MIR function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FunctionMemorySummary {
    /// Read address spaces as a bit per [`space_index`].
    reads: u8,
    /// Written address spaces as a bit per [`space_index`].
    writes: u8,
    may_reset_fmp: bool,
    /// Whether the function may move the free-memory pointer below its current value.
    may_recycle_fmp: bool,
    /// Whether the function may read or write the free-memory pointer directly.
    may_observe_fmp: bool,
    /// Whether the function may read `msize`.
    may_observe_msize: bool,
    /// Parameters whose pointer value may escape the call.
    captures: DenseBitSet<ArgIdx>,
    /// Parameters whose pointer value the function may relate to the heap: a value derived
    /// from the parameter meets one derived from the free-memory pointer or `msize` in a
    /// single instruction.
    observes: DenseBitSet<ArgIdx>,
}

impl FunctionMemorySummary {
    fn empty(params: usize) -> Self {
        Self {
            reads: 0,
            writes: 0,
            may_reset_fmp: false,
            may_recycle_fmp: false,
            may_observe_fmp: false,
            may_observe_msize: false,
            captures: DenseBitSet::new_empty(params),
            observes: DenseBitSet::new_empty(params),
        }
    }

    fn conservative(params: usize) -> Self {
        Self {
            reads: 0b1111,
            writes: 0b1111,
            may_reset_fmp: true,
            may_recycle_fmp: true,
            may_observe_fmp: true,
            may_observe_msize: true,
            captures: DenseBitSet::new_filled(params),
            observes: DenseBitSet::new_filled(params),
        }
    }

    /// Returns whether the function may read an address space.
    #[must_use]
    pub(crate) const fn reads(&self, space: AddressSpace) -> bool {
        self.reads & (1 << space_index(space)) != 0
    }

    /// Returns whether the function may write an address space.
    #[must_use]
    pub(crate) const fn writes(&self, space: AddressSpace) -> bool {
        self.writes & (1 << space_index(space)) != 0
    }

    /// Returns whether the function may recycle or arbitrarily replace the FMP.
    #[must_use]
    pub(crate) const fn may_reset_fmp(&self) -> bool {
        self.may_reset_fmp
    }

    /// Returns whether the function may recycle the free-memory pointer.
    #[must_use]
    pub(crate) const fn may_recycle_fmp(&self) -> bool {
        self.may_recycle_fmp
    }

    /// Returns whether the function may read or write the free-memory pointer directly.
    ///
    /// Together with [`Self::observes_param`], such a function can derive aliases from where a
    /// pointer argument lies relative to the heap, so moving that argument's object out of the
    /// heap is observable to it.
    #[must_use]
    pub(crate) const fn may_observe_fmp(&self) -> bool {
        self.may_observe_fmp
    }

    /// Returns whether the function may read `msize`, which an elided allocation would change.
    #[must_use]
    pub(crate) const fn may_observe_msize(&self) -> bool {
        self.may_observe_msize
    }

    /// Returns whether the function may relate a parameter's pointer value to the heap.
    ///
    /// Dereferencing the pointer, or comparing it with values derived from itself, is
    /// placement-agnostic; only an instruction that also consumes a value derived from the
    /// free-memory pointer or `msize` can observe where the object lies relative to the heap.
    #[must_use]
    pub(crate) fn observes_param(&self, index: ArgIdx) -> bool {
        index.index() >= self.observes.domain_size() || self.observes.contains(index)
    }

    /// Returns whether a parameter's pointer value may escape the call.
    #[must_use]
    pub(crate) fn captures_param(&self, index: ArgIdx) -> bool {
        index.index() >= self.captures.domain_size() || self.captures.contains(index)
    }

    fn merge_effects(&mut self, other: &Self) {
        self.reads |= other.reads;
        self.writes |= other.writes;
        self.may_reset_fmp |= other.may_reset_fmp;
        self.may_recycle_fmp |= other.may_recycle_fmp;
        self.may_observe_fmp |= other.may_observe_fmp;
        self.may_observe_msize |= other.may_observe_msize;
    }
}

/// Cached module-level summaries for all internal-call targets.
#[derive(Clone, Debug)]
pub(crate) struct MemoryCallSummaries {
    summaries: IndexVec<FunctionId, FunctionMemorySummary>,
}

impl MemoryCallSummaries {
    /// Computes summaries to a monotone fixpoint over the module call graph.
    #[must_use]
    pub(crate) fn new(module: &Module) -> Self {
        if !module.functions.iter().any(|func| {
            func.instructions()
                .any(|inst_id| matches!(func.inst(inst_id).kind, InstKind::InternalCall { .. }))
                || func
                    .blocks
                    .iter()
                    .any(|block| matches!(block.terminator, Some(Terminator::TailCall { .. })))
        }) {
            return Self { summaries: IndexVec::new() };
        }

        let sources = module.functions.iter().map(parameter_sources).collect::<IndexVec<_, _>>();
        let mut local = IndexVec::with_capacity(module.functions.len());
        for (func_id, func) in module.functions.iter_enumerated() {
            local.push(local_summary(func, &sources[func_id]));
        }
        let mut summaries = local.clone();

        let mut callers = IndexVec::from_vec(vec![Vec::new(); module.functions.len()]);
        for (caller, func) in module.functions.iter_enumerated() {
            for inst_id in func.instructions() {
                if let InstKind::InternalCall { function, .. } = func.inst(inst_id).kind
                    && let Some(function_callers) = callers.get_mut(function)
                {
                    function_callers.push(caller);
                }
            }
            for block in &func.blocks {
                if let Some(Terminator::TailCall { function, .. }) = &block.terminator
                    && let Some(function_callers) = callers.get_mut(*function)
                {
                    function_callers.push(caller);
                }
            }
        }
        for function_callers in &mut callers {
            function_callers.sort_unstable();
            function_callers.dedup();
        }

        let mut queued = DenseBitSet::new_filled(module.functions.len());
        let mut worklist = module.functions.indices().collect::<VecDeque<_>>();
        while let Some(func_id) = worklist.pop_front() {
            queued.remove(func_id);
            let func = &module.functions[func_id];
            let mut summary = local[func_id].clone();
            for block in &func.blocks {
                for &inst_id in &block.instructions {
                    if let InstKind::InternalCall { function, ref args, .. } =
                        func.inst(inst_id).kind
                    {
                        merge_call(
                            &mut summary,
                            func,
                            summaries.get(function),
                            args,
                            &sources[func_id],
                        );
                    }
                }
                if let Some(Terminator::TailCall { function, args }) = &block.terminator {
                    merge_call(
                        &mut summary,
                        func,
                        summaries.get(*function),
                        args,
                        &sources[func_id],
                    );
                }
            }

            if summary != summaries[func_id] {
                summaries[func_id] = summary;
                for &caller in &callers[func_id] {
                    if queued.insert(caller) {
                        worklist.push_back(caller);
                    }
                }
            }
        }

        Self { summaries }
    }

    /// Returns a function summary, if the target belongs to this module.
    #[must_use]
    pub(crate) fn get(&self, function: FunctionId) -> Option<&FunctionMemorySummary> {
        self.summaries.get(function)
    }
}

fn merge_call(
    summary: &mut FunctionMemorySummary,
    func: &Function,
    callee: Option<&FunctionMemorySummary>,
    args: &[ValueId],
    sources: &IndexVec<ValueId, DenseBitSet<ArgIdx>>,
) {
    let conservative;
    let callee = if let Some(callee) = callee {
        callee
    } else {
        conservative = FunctionMemorySummary::conservative(args.len());
        &conservative
    };
    summary.merge_effects(callee);
    for (index, &arg) in args.iter().enumerate() {
        if callee.captures_param(ArgIdx::new(index)) {
            capture_sources(summary, func, sources, arg);
        }
        if callee.observes_param(ArgIdx::new(index)) {
            observe_sources(summary, func, sources, arg);
        }
    }
}

const fn space_index(space: AddressSpace) -> usize {
    match space {
        AddressSpace::Memory => 0,
        AddressSpace::Storage => 1,
        AddressSpace::Transient => 2,
        AddressSpace::Immutable => 3,
    }
}

fn local_summary(
    func: &Function,
    sources: &IndexVec<ValueId, DenseBitSet<ArgIdx>>,
) -> FunctionMemorySummary {
    if func.blocks.is_empty() {
        return FunctionMemorySummary::conservative(func.params.len());
    }

    let mut summary = FunctionMemorySummary::empty(func.params.len());
    let aa = AliasAnalysis::new(func);
    let heap_derived = heap_derived_values(func);
    for block in &func.blocks {
        for &inst_id in &block.instructions {
            let kind = &func.inst(inst_id).kind;
            if let InstKind::InternalCall { returns, .. } = kind {
                // Callee effects merge through the call graph, but a multi-result
                // call also writes the caller-side multi-return buffer during
                // backend lowering. That traffic exists in no MIR body, so it
                // must be a local memory effect of the calling function.
                if *returns > 1 {
                    summary.reads |= 1 << space_index(AddressSpace::Memory);
                    summary.writes |= 1 << space_index(AddressSpace::Memory);
                }
                continue;
            }
            let effects = aa.instruction_mod_ref(func, inst_id);
            for space in [
                AddressSpace::Memory,
                AddressSpace::Storage,
                AddressSpace::Transient,
                AddressSpace::Immutable,
            ] {
                summary.reads |= (effects.reads_space(space) as u8) << space_index(space);
                summary.writes |= (effects.writes_space(space) as u8) << space_index(space);
            }
            summary.may_reset_fmp |= aa.instruction_may_reset_fmp(func, inst_id);
            summary.may_recycle_fmp |= instruction_may_recycle_fmp(func, inst_id);
            summary.may_observe_fmp |= instruction_observes_fmp(func, inst_id);
            summary.may_observe_msize |= matches!(kind, InstKind::MSize);
            // An instruction that consumes both a pointer-derived value and a heap-derived one
            // can relate the object to the heap, whatever the positions: comparisons, pointer
            // arithmetic against the free-memory pointer, or storing one through the other.
            let operands = kind.operands();
            if operands.iter().any(|operand| heap_derived.contains(*operand)) {
                for operand in operands {
                    observe_sources(&mut summary, func, sources, operand);
                }
            }

            match kind {
                InstKind::MStore(_, value)
                | InstKind::MStore8(_, value)
                | InstKind::SStore(_, value)
                | InstKind::TStore(_, value)
                | InstKind::SetFmp(value)
                | InstKind::MemoryObjectStoreField { value, .. }
                | InstKind::MemoryObjectStoreElement { value, .. }
                | InstKind::MemoryObjectStoreByte { value, .. }
                | InstKind::MemoryObjectStoreWord { value, .. }
                | InstKind::FrameStore { value, .. } => {
                    capture_sources(&mut summary, func, sources, *value);
                }
                InstKind::MemorySliceLoadWord { slice, offset } => {
                    capture_sources(&mut summary, func, sources, *slice);
                    capture_sources(&mut summary, func, sources, *offset);
                }
                InstKind::CalldataSliceLoadWord { slice, offset } => {
                    capture_sources(&mut summary, func, sources, *slice);
                    capture_sources(&mut summary, func, sources, *offset);
                }
                _ => {}
            }
        }

        match &block.terminator {
            Some(Terminator::Return { values }) => {
                for &value in values {
                    capture_sources(&mut summary, func, sources, value);
                }
            }
            Some(Terminator::TailCall { .. }) | None => {}
            Some(term) => {
                let operands = term.operands();
                if operands.iter().any(|operand| heap_derived.contains(*operand)) {
                    for operand in operands {
                        observe_sources(&mut summary, func, sources, operand);
                    }
                }
            }
        }
    }
    summary
}

/// Values derived from the free-memory pointer or `msize`, through any computation except a
/// load: a word read through such an address is data, not a heap position. Internal call
/// results count as heap-derived, since a callee may return a heap position.
fn heap_derived_values(func: &Function) -> DenseBitSet<ValueId> {
    let mut derived = DenseBitSet::new_empty(func.num_values());
    let mut users = IndexVec::from_vec(vec![Vec::new(); func.num_values()]);
    let mut worklist = Vec::new();
    for inst_id in func.instructions() {
        let Some(result) = func.inst_result_value(inst_id) else { continue };
        let kind = &func.inst(inst_id).kind;
        let root = match kind {
            InstKind::Fmp | InstKind::MSize | InstKind::InternalCall { .. } => true,
            InstKind::MLoad(address) => func.value_u64(*address) == Some(EvmMemoryLayout::FMP_SLOT),
            _ => false,
        };
        if root {
            derived.insert(result);
            worklist.push(result);
            continue;
        }
        if instruction_loads_data(kind) {
            continue;
        }
        for operand in kind.operands() {
            users[operand].push(result);
        }
    }
    while let Some(value) = worklist.pop() {
        for &user in &users[value] {
            if derived.insert(user) {
                worklist.push(user);
            }
        }
    }
    derived
}

fn observe_sources(
    summary: &mut FunctionMemorySummary,
    func: &Function,
    sources: &IndexVec<ValueId, DenseBitSet<ArgIdx>>,
    value: ValueId,
) {
    if let Value::Arg(index) = func.value(value)
        && index.index() < summary.observes.domain_size()
    {
        summary.observes.insert(*index);
    }
    summary.observes.union(&sources[value]);
}

/// Returns whether an instruction reads the free-memory pointer or the memory size directly.
///
/// Compiler-owned allocations are still abstract here and cannot relate a pointer argument to
/// the heap; only source-visible pointer reads and writes can.
fn instruction_observes_fmp(func: &Function, inst_id: InstId) -> bool {
    match func.inst(inst_id).kind {
        InstKind::Fmp | InstKind::SetFmp(_) | InstKind::MSize => true,
        InstKind::MLoad(address) | InstKind::MStore(address, _) => {
            func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT)
        }
        _ => false,
    }
}

fn instruction_may_recycle_fmp(func: &Function, inst_id: InstId) -> bool {
    match func.inst(inst_id).kind {
        InstKind::SetFmp(_) => true,
        InstKind::MStore(address, value) => {
            if func.value_u64(address) != Some(EvmMemoryLayout::FMP_SLOT) {
                return false;
            }
            !is_monotonic_fmp_advance(func, value)
        }
        InstKind::MStore8(address, _)
        | InstKind::MCopy(address, _, _)
        | InstKind::MemoryZero(address, _)
        | InstKind::CalldataCopy(address, _, _)
        | InstKind::CodeCopy(address, _, _)
        | InstKind::ReturnDataCopy(address, _, _) => {
            func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT)
        }
        InstKind::ExtCodeCopy(_, address, _, _) => {
            func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT)
        }
        InstKind::Call { ret_offset, .. }
        | InstKind::CallCode { ret_offset, .. }
        | InstKind::StaticCall { ret_offset, .. }
        | InstKind::DelegateCall { ret_offset, .. } => {
            func.value_u64(ret_offset) == Some(EvmMemoryLayout::FMP_SLOT)
        }
        _ => false,
    }
}

fn is_monotonic_fmp_advance(func: &Function, value: ValueId) -> bool {
    let Value::Inst(inst_id) = func.value(value) else { return false };
    if !matches!(func.inst(*inst_id).kind, InstKind::Add(_, _)) {
        return false;
    }
    let mut visiting = FxHashSet::default();
    is_fmp_derived(func, value, &mut visiting, None, false)
}

/// Proves that a value is the free-memory pointer plus only nonnegative offsets.
///
/// Loop-carried pointer increments form cyclic phi dependencies. A cycle is
/// accepted only when the phi that anchors it also has a separately proven FMP
/// input; unrelated cycles remain rejected.
fn is_fmp_derived(
    func: &Function,
    value: ValueId,
    visiting: &mut FxHashSet<ValueId>,
    anchor: Option<ValueId>,
    supported: bool,
) -> bool {
    if anchor == Some(value) && supported && visiting.contains(&value) {
        return true;
    }
    if !visiting.insert(value) {
        return false;
    }

    let result = match func.value(value) {
        Value::Inst(inst_id) => match func.inst(*inst_id).kind.clone() {
            InstKind::MLoad(address) => func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT),
            InstKind::Add(first, second) => {
                (is_fmp_derived(func, first, visiting, anchor, supported)
                    && is_nonnegative_offset(func, second))
                    || (is_fmp_derived(func, second, visiting, anchor, supported)
                        && is_nonnegative_offset(func, first))
            }
            InstKind::Phi(incoming) if !incoming.is_empty() => {
                let is_root = anchor.is_none();
                let externally_supported = is_root
                    && incoming
                        .iter()
                        .any(|&(_, value)| is_fmp_derived(func, value, visiting, None, false));
                let anchor = anchor.or(is_root.then_some(value));
                let supported = supported || externally_supported;
                incoming
                    .iter()
                    .all(|&(_, value)| is_fmp_derived(func, value, visiting, anchor, supported))
            }
            _ => false,
        },
        _ => false,
    };

    visiting.remove(&value);
    result
}

fn is_nonnegative_offset(func: &Function, value: ValueId) -> bool {
    let Value::Inst(inst_id) = func.value(value) else { return true };
    !matches!(
        func.inst(*inst_id).kind,
        InstKind::Sub(_, _)
            | InstKind::SDiv(_, _)
            | InstKind::SMod(_, _)
            | InstKind::Sar(_, _)
            | InstKind::Not(_)
            | InstKind::SetFmp(_)
            | InstKind::MStore(_, _)
            | InstKind::MStore8(_, _)
            | InstKind::MCopy(_, _, _)
            | InstKind::MemoryZero(_, _)
            | InstKind::CalldataCopy(_, _, _)
            | InstKind::CodeCopy(_, _, _)
            | InstKind::ReturnDataCopy(_, _, _)
            | InstKind::ExtCodeCopy(_, _, _, _)
            | InstKind::Call { .. }
            | InstKind::CallCode { .. }
            | InstKind::StaticCall { .. }
            | InstKind::DelegateCall { .. }
    )
}

fn capture_sources(
    summary: &mut FunctionMemorySummary,
    func: &Function,
    sources: &IndexVec<ValueId, DenseBitSet<ArgIdx>>,
    value: ValueId,
) {
    if let Value::Arg(index) = func.value(value)
        && index.index() < summary.captures.domain_size()
    {
        summary.captures.insert(*index);
    }
    summary.captures.union(&sources[value]);
}

/// Tracks which parameters a value is derived from.
///
/// Capture summaries follow pointer-preserving computations: a helper can
/// return an arithmetic or bitwise identity of a pointer parameter. Direct
/// argument sources are handled lazily while propagating or capturing.
fn parameter_sources(func: &Function) -> IndexVec<ValueId, DenseBitSet<ArgIdx>> {
    let params = func.params.len();
    let mut sources = IndexVec::from_vec(vec![DenseBitSet::new_empty(params); func.num_values()]);
    if params == 0 {
        return sources;
    }

    let mut users = IndexVec::from_vec(vec![Vec::new(); func.num_values()]);
    let mut queued = DenseBitSet::new_empty(func.num_values());
    let mut worklist = VecDeque::new();
    for inst_id in func.instructions() {
        let Some(result) = func.inst_result_value(inst_id) else { continue };
        let mut add_user = |operand: ValueId| {
            users[operand].push(result);
            if let Value::Arg(index) = func.value(operand)
                && index.index() < params
                && sources[operand].insert(*index)
                && queued.insert(operand)
            {
                worklist.push_back(operand);
            }
        };
        let kind = &func.inst(inst_id).kind;
        if instruction_loads_data(kind) || instruction_compares_values(kind) {
            continue;
        }
        for operand in kind.operands() {
            add_user(operand);
        }
    }

    while let Some(value) = worklist.pop_front() {
        queued.remove(value);
        let propagated = sources[value].clone();
        for &user in &users[value] {
            if sources[user].union(&propagated) && queued.insert(user) {
                worklist.push_back(user);
            }
        }
    }
    sources
}

fn instruction_compares_values(kind: &InstKind) -> bool {
    matches!(
        kind,
        InstKind::Lt(_, _)
            | InstKind::Gt(_, _)
            | InstKind::SLt(_, _)
            | InstKind::SGt(_, _)
            | InstKind::Eq(_, _)
            | InstKind::IsZero(_)
    )
}

fn instruction_loads_data(kind: &InstKind) -> bool {
    matches!(
        kind,
        InstKind::MLoad(_)
            | InstKind::CalldataLoad(_)
            | InstKind::SLoad(_)
            | InstKind::TLoad(_)
            | InstKind::Keccak256(_, _)
            | InstKind::MemoryObjectLoadField { .. }
            | InstKind::MemoryObjectLoadElement { .. }
            | InstKind::MemoryObjectLoadByte { .. }
            | InstKind::MemoryObjectLen(_, _)
            | InstKind::MemorySliceLoadWord { .. }
            | InstKind::CalldataSliceLoadWord { .. }
            | InstKind::SliceLen(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{FunctionBuilder, MirType};
    use solar_interface::{Ident, sym};

    #[test]
    fn propagates_captures_and_fmp_resets() {
        let mut module = Module::new(Ident::DUMMY);

        let mut reader = Function::new(Ident::with_dummy_span(sym::memory_read));
        {
            let mut builder = FunctionBuilder::new(&mut reader);
            let ptr = builder.add_param(MirType::MemPtr);
            let value = builder.mload(ptr);
            builder.ret([value]);
        }
        reader.returns.push(MirType::uint256());
        let reader = module.add_function(reader);

        let mut returning = Function::new(Ident::with_dummy_span(sym::ret));
        {
            let mut builder = FunctionBuilder::new(&mut returning);
            let ptr = builder.add_param(MirType::MemPtr);
            builder.ret([ptr]);
        }
        returning.returns.push(MirType::MemPtr);
        let returning = module.add_function(returning);

        let mut obfuscated = Function::new(Ident::with_dummy_span(sym::ret));
        {
            let mut builder = FunctionBuilder::new(&mut obfuscated);
            let ptr = builder.add_param(MirType::MemPtr);
            let zero = builder.imm(0);
            let value = builder.xor(ptr, zero);
            builder.ret([value]);
        }
        obfuscated.returns.push(MirType::MemPtr);
        let obfuscated = module.add_function(obfuscated);

        let mut resetter = Function::new(Ident::with_dummy_span(sym::fmp));
        {
            let mut builder = FunctionBuilder::new(&mut resetter);
            let ptr = builder.add_param(MirType::MemPtr);
            builder.set_fmp(ptr);
            builder.ret([]);
        }
        let resetter = module.add_function(resetter);

        let mut reader_caller = Function::new(Ident::with_dummy_span(sym::internal_call));
        {
            let mut builder = FunctionBuilder::new(&mut reader_caller);
            let ptr = builder.add_param(MirType::MemPtr);
            builder.internal_call_void(reader, vec![ptr], 1);
            builder.ret([]);
        }
        let reader_caller = module.add_function(reader_caller);

        let mut returning_caller = Function::new(Ident::with_dummy_span(sym::result_ty));
        {
            let mut builder = FunctionBuilder::new(&mut returning_caller);
            let ptr = builder.add_param(MirType::MemPtr);
            builder.internal_call_void(returning, vec![ptr], 1);
            builder.ret([]);
        }
        let returning_caller = module.add_function(returning_caller);

        let summaries = MemoryCallSummaries::new(&module);
        assert!(!summaries.get(reader_caller).unwrap().captures_param(ArgIdx::new(0)));
        assert!(summaries.get(returning_caller).unwrap().captures_param(ArgIdx::new(0)));
        assert!(summaries.get(obfuscated).unwrap().captures_param(ArgIdx::new(0)));
        assert!(summaries.get(resetter).unwrap().may_reset_fmp());
    }
}

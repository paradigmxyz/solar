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
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec};
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
    /// Parameters whose pointer value may escape the call.
    captures: DenseBitSet<ArgIdx>,
}

impl FunctionMemorySummary {
    fn empty(params: usize) -> Self {
        Self {
            reads: 0,
            writes: 0,
            may_reset_fmp: false,
            may_recycle_fmp: false,
            captures: DenseBitSet::new_empty(params),
        }
    }

    fn conservative(params: usize) -> Self {
        Self {
            reads: 0b1111,
            writes: 0b1111,
            may_reset_fmp: true,
            may_recycle_fmp: true,
            captures: DenseBitSet::new_filled(params),
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
    for block in &func.blocks {
        for &inst_id in &block.instructions {
            let kind = &func.inst(inst_id).kind;
            if matches!(kind, InstKind::InternalCall { .. }) {
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

        if let Some(Terminator::Return { values }) = &block.terminator {
            for &value in values {
                capture_sources(&mut summary, func, sources, value);
            }
        }
    }
    summary
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
    let InstKind::Add(first, second) = func.inst(*inst_id).kind else { return false };
    (is_fmp_load(func, first) && is_nonnegative_offset(func, second))
        || (is_fmp_load(func, second) && is_nonnegative_offset(func, first))
}

fn is_fmp_load(func: &Function, value: ValueId) -> bool {
    let Value::Inst(inst_id) = func.value(value) else { return false };
    let InstKind::MLoad(address) = func.inst(*inst_id).kind else { return false };
    func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT)
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

/// Tracks which parameters a value is derived from. Only pointer-preserving
/// operations propagate sources; loading pointer bits through memory is
/// deliberately not guessed, and storing a parameter is already a capture.
/// Direct argument sources are handled lazily while propagating or capturing.
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
        match &func.inst(inst_id).kind {
            InstKind::Add(first, second)
            | InstKind::Sub(first, second)
            | InstKind::MakeSlice { ptr: first, len: second, .. } => {
                add_user(*first);
                add_user(*second);
            }
            InstKind::Select(_, first, second) => {
                add_user(*first);
                add_user(*second);
            }
            InstKind::Phi(incoming) => {
                for &(_, value) in incoming {
                    add_user(value);
                }
            }
            InstKind::SlicePtr(value)
            | InstKind::MemoryObjectData(value, _)
            | InstKind::MemoryObjectFieldAddr { object: value, .. } => add_user(*value),
            InstKind::MemoryObjectElementAddr { object, index, .. } => {
                add_user(*object);
                add_user(*index);
            }
            _ => {}
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
        assert!(summaries.get(resetter).unwrap().may_reset_fmp());
    }
}

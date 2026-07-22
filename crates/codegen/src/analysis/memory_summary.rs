//! Interprocedural memory and pointer-capture summaries.
//!
//! Summaries are computed to a fixpoint over internal-call edges. Missing
//! bodies stay fully conservative; recursive groups converge because every
//! fact only moves from false to true.

use super::{AddressSpace, AliasAnalysis};
use crate::mir::{Function, FunctionId, InstKind, Module, Terminator, Value, ValueId};
use solar_data_structures::{index::IndexVec, map::FxHashSet};

/// Conservative memory effects and pointer captures for one MIR function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FunctionMemorySummary {
    reads: [bool; 3],
    writes: [bool; 3],
    may_reset_fmp: bool,
    captures: Vec<bool>,
}

impl FunctionMemorySummary {
    fn empty(params: usize) -> Self {
        Self {
            reads: [false; 3],
            writes: [false; 3],
            may_reset_fmp: false,
            captures: vec![false; params],
        }
    }

    fn conservative(params: usize) -> Self {
        Self {
            reads: [true; 3],
            writes: [true; 3],
            may_reset_fmp: true,
            captures: vec![true; params],
        }
    }

    /// Returns whether the function may read an address space.
    #[must_use]
    pub(crate) const fn reads(&self, space: AddressSpace) -> bool {
        self.reads[space_index(space)]
    }

    /// Returns whether the function may write an address space.
    #[must_use]
    pub(crate) const fn writes(&self, space: AddressSpace) -> bool {
        self.writes[space_index(space)]
    }

    /// Returns whether the function may recycle or arbitrarily replace the FMP.
    #[must_use]
    pub(crate) const fn may_reset_fmp(&self) -> bool {
        self.may_reset_fmp
    }

    /// Returns whether a parameter's pointer value may escape the call.
    #[must_use]
    pub(crate) fn captures_param(&self, index: usize) -> bool {
        self.captures.get(index).copied().unwrap_or(true)
    }

    fn merge_effects(&mut self, other: &Self) {
        for index in 0..3 {
            self.reads[index] |= other.reads[index];
            self.writes[index] |= other.writes[index];
        }
        self.may_reset_fmp |= other.may_reset_fmp;
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
        let mut local = IndexVec::with_capacity(module.functions.len());
        for func in &module.functions {
            local.push(local_summary(func));
        }
        let mut summaries = local.clone();

        loop {
            let previous = summaries.clone();
            for (func_id, func) in module.functions.iter_enumerated() {
                if func.blocks.is_empty() {
                    summaries[func_id] = FunctionMemorySummary::conservative(func.params.len());
                    continue;
                }

                let mut summary = local[func_id].clone();
                let sources = parameter_sources(func);
                for block in &func.blocks {
                    for &inst_id in &block.instructions {
                        if let InstKind::InternalCall { function, ref args, .. } =
                            func.instructions[inst_id].kind
                        {
                            let callee = previous
                                .get(function)
                                .cloned()
                                .unwrap_or_else(|| FunctionMemorySummary::conservative(args.len()));
                            summary.merge_effects(&callee);
                            for (index, &arg) in args.iter().enumerate() {
                                if callee.captures_param(index) {
                                    capture_sources(&mut summary, &sources[arg]);
                                }
                            }
                        }
                    }
                    if let Some(Terminator::TailCall { function, args }) = &block.terminator {
                        let callee = previous
                            .get(*function)
                            .cloned()
                            .unwrap_or_else(|| FunctionMemorySummary::conservative(args.len()));
                        summary.merge_effects(&callee);
                        for (index, &arg) in args.iter().enumerate() {
                            if callee.captures_param(index) {
                                capture_sources(&mut summary, &sources[arg]);
                            }
                        }
                    }
                }
                summaries[func_id] = summary;
            }
            if summaries == previous {
                break;
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

const fn space_index(space: AddressSpace) -> usize {
    match space {
        AddressSpace::Memory => 0,
        AddressSpace::Storage => 1,
        AddressSpace::Transient => 2,
    }
}

fn local_summary(func: &Function) -> FunctionMemorySummary {
    if func.blocks.is_empty() {
        return FunctionMemorySummary::conservative(func.params.len());
    }

    let mut summary = FunctionMemorySummary::empty(func.params.len());
    let sources = parameter_sources(func);
    let aa = AliasAnalysis::new(func);
    for block in &func.blocks {
        for &inst_id in &block.instructions {
            let kind = &func.instructions[inst_id].kind;
            if matches!(kind, InstKind::InternalCall { .. }) {
                continue;
            }
            let effects = aa.instruction_mod_ref(func, inst_id);
            for space in [AddressSpace::Memory, AddressSpace::Storage, AddressSpace::Transient] {
                summary.reads[space_index(space)] |= effects.reads_space(space);
                summary.writes[space_index(space)] |= effects.writes_space(space);
            }
            summary.may_reset_fmp |= aa.instruction_may_reset_fmp(func, inst_id);

            match kind {
                InstKind::MStore(_, value)
                | InstKind::MStore8(_, value)
                | InstKind::SStore(_, value)
                | InstKind::TStore(_, value)
                | InstKind::SetFmp(value) => capture_sources(&mut summary, &sources[*value]),
                _ => {}
            }
        }

        if let Some(Terminator::Return { values }) = &block.terminator {
            for &value in values {
                capture_sources(&mut summary, &sources[value]);
            }
        }
    }
    summary
}

fn capture_sources(summary: &mut FunctionMemorySummary, sources: &FxHashSet<usize>) {
    for &source in sources {
        if let Some(captured) = summary.captures.get_mut(source) {
            *captured = true;
        }
    }
}

/// Tracks which parameters a value is derived from. Only pointer-preserving
/// operations propagate sources; loading pointer bits through memory is
/// deliberately not guessed, and storing a parameter is already a capture.
fn parameter_sources(func: &Function) -> IndexVec<ValueId, FxHashSet<usize>> {
    let mut sources = IndexVec::with_capacity(func.values.len());
    for _ in 0..func.values.len() {
        sources.push(FxHashSet::default());
    }
    for (value_id, value) in func.values.iter_enumerated() {
        if let Value::Arg { index, .. } = value {
            sources[value_id].insert(*index as usize);
        }
    }

    loop {
        let mut changed = false;
        for (value_id, value) in func.values.iter_enumerated() {
            let Value::Inst(inst_id) = value else { continue };
            let operands = match &func.instructions[*inst_id].kind {
                InstKind::Add(first, second)
                | InstKind::Sub(first, second)
                | InstKind::MakeSlice { ptr: first, len: second, .. } => {
                    vec![*first, *second]
                }
                InstKind::Select(_, first, second) => vec![*first, *second],
                InstKind::Phi(incoming) => incoming.iter().map(|(_, value)| *value).collect(),
                InstKind::SlicePtr(value)
                | InstKind::MemoryObjectData(value, _)
                | InstKind::MemoryObjectFieldAddr { object: value, .. } => vec![*value],
                InstKind::MemoryObjectElementAddr { object, index, .. } => {
                    vec![*object, *index]
                }
                _ => continue,
            };
            let mut propagated = FxHashSet::default();
            for operand in operands {
                propagated.extend(sources[operand].iter().copied());
            }
            let before = sources[value_id].len();
            sources[value_id].extend(propagated);
            changed |= sources[value_id].len() != before;
        }
        if !changed {
            break;
        }
    }
    sources
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{FunctionBuilder, MirType};
    use solar_interface::Ident;

    #[test]
    fn propagates_captures_and_fmp_resets() {
        let mut module = Module::new(Ident::DUMMY);

        let mut reader = Function::new(Ident::DUMMY);
        {
            let mut builder = FunctionBuilder::new(&mut reader);
            let ptr = builder.add_param(MirType::MemPtr);
            let value = builder.mload(ptr);
            builder.ret([value]);
        }
        reader.returns.push(MirType::uint256());
        let reader = module.add_function(reader);

        let mut returning = Function::new(Ident::DUMMY);
        {
            let mut builder = FunctionBuilder::new(&mut returning);
            let ptr = builder.add_param(MirType::MemPtr);
            builder.ret([ptr]);
        }
        returning.returns.push(MirType::MemPtr);
        let returning = module.add_function(returning);

        let mut resetter = Function::new(Ident::DUMMY);
        {
            let mut builder = FunctionBuilder::new(&mut resetter);
            let ptr = builder.add_param(MirType::MemPtr);
            builder.set_fmp(ptr);
            builder.ret([]);
        }
        let resetter = module.add_function(resetter);

        let mut reader_caller = Function::new(Ident::DUMMY);
        {
            let mut builder = FunctionBuilder::new(&mut reader_caller);
            let ptr = builder.add_param(MirType::MemPtr);
            builder.internal_call_void(reader, vec![ptr], 1);
            builder.ret([]);
        }
        let reader_caller = module.add_function(reader_caller);

        let mut returning_caller = Function::new(Ident::DUMMY);
        {
            let mut builder = FunctionBuilder::new(&mut returning_caller);
            let ptr = builder.add_param(MirType::MemPtr);
            builder.internal_call_void(returning, vec![ptr], 1);
            builder.ret([]);
        }
        let returning_caller = module.add_function(returning_caller);

        let summaries = MemoryCallSummaries::new(&module);
        assert!(!summaries.get(reader_caller).unwrap().captures_param(0));
        assert!(summaries.get(returning_caller).unwrap().captures_param(0));
        assert!(summaries.get(resetter).unwrap().may_reset_fmp());
    }
}

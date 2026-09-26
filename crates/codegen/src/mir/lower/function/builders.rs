//! Uses of a `solar:core/v1/Buffers.sol` builder after `Buffers.finish` emptied it.
//!
//! `finish` returns what a builder holds without a copy and leaves the builder empty, so appending
//! afterwards starts a new output instead of extending the one returned. That is well defined, and
//! other compilers run it as written, but a builder is meant to be finished once, after its last
//! append: this compiler rejects every use of a builder on a path after `finish` emptied it.
//!
//! The check runs once the contract is lowered, over the SSA values of each function. A call to
//! `finish` empties the builder it is passed, and so does a call to a function that may empty a
//! builder parameter, summarized over the call graph to a fixed point. The emptied builders flow
//! forward through the CFG, into a phi along each edge that carries one, until the instruction
//! defining a value runs again, as a loop's next iteration does; any other use of one is
//! reported: a call it is passed to, a store of it, or a return of it. Paths are not told apart
//! by their conditions, so a builder a loop finishes in its last iteration is still emptied on
//! the back edge.
//!
//! NOTE: builders are followed as values, not through memory: a builder stored in memory and
//! loaded again, or one a function empties and then returns, is not followed.

use super::*;
use crate::mir::{BlockId, InstId, Module, analysis::CfgInfo};
use solar_data_structures::{
    index::{IndexVec, index_vec},
    smallvec::SmallVec,
};
use solar_interface::source_map::FileName;

/// The module that owns the builders.
const BUFFERS: &str = "solar:core/v1/Buffers.sol";

/// The functions of `mir_ids` that are `Buffers.finish`.
pub(in crate::mir::lower) fn finish_functions(
    gcx: Gcx<'_>,
    mir_ids: &FxHashMap<hir::FunctionId, FunctionId>,
) -> FxHashSet<FunctionId> {
    mir_ids
        .iter()
        .filter(|&(&id, _)| {
            let function = gcx.hir.function(id);
            matches!(&gcx.hir.source(function.source).file.name, FileName::Custom(path) if path == BUFFERS)
                && function.name.is_some_and(|name| name.name == sym::finish)
        })
        .map(|(_, &mir_id)| mir_id)
        .collect()
}

/// Rejects every use of a builder after a call that may have emptied it with `finish`.
pub(in crate::mir::lower) fn check_builder_finishes(
    gcx: Gcx<'_>,
    module: &Module,
    finish: &FxHashSet<FunctionId>,
) {
    if finish.is_empty() {
        return;
    }
    let summaries = summarize(module, finish);
    for func in &module.functions {
        check_function(gcx, func, finish, &summaries);
    }
}

/// The parameters each function may empty with `finish`, by index.
type Summaries = FxHashMap<FunctionId, SmallVec<[usize; 2]>>;

/// Summarizes every function of `module` to a fixed point over the call graph; summaries only
/// grow, so the iteration ends.
fn summarize(module: &Module, finish: &FxHashSet<FunctionId>) -> Summaries {
    let mut summaries =
        finish.iter().map(|&id| (id, SmallVec::from_slice(&[0]))).collect::<Summaries>();
    loop {
        let mut changed = false;
        for (id, func) in module.functions.iter_enumerated() {
            if finish.contains(&id) {
                continue;
            }
            let mut emptied = summaries.get(&id).cloned().unwrap_or_default();
            for (_, values) in emptied_operands(func, finish, &summaries) {
                for param in values.into_iter().flat_map(|value| parameters_of(func, value)) {
                    if !emptied.contains(&param) {
                        emptied.push(param);
                    }
                }
            }
            emptied.sort_unstable();
            if !emptied.is_empty() && summaries.get(&id) != Some(&emptied) {
                summaries.insert(id, emptied);
                changed = true;
            }
        }
        if !changed {
            return summaries;
        }
    }
}

/// Every call in `func` that may empty a builder, with the builders it may empty.
fn emptied_operands(
    func: &Function,
    finish: &FxHashSet<FunctionId>,
    summaries: &Summaries,
) -> Vec<(InstId, SmallVec<[ValueId; 1]>)> {
    let mut calls = Vec::new();
    for inst in func.instructions() {
        let InstKind::ICall { function: crate::mir::Callee::Function(callee), args } =
            &func.inst(inst).kind
        else {
            continue;
        };
        let emptied = match summaries.get(callee) {
            Some(params) => params.iter().filter_map(|&param| args.get(param).copied()).collect(),
            None if finish.contains(callee) => args.first().copied().into_iter().collect(),
            None => continue,
        };
        calls.push((inst, emptied));
    }
    calls
}

/// The parameters `value` may be, through phis, selects, and casts.
fn parameters_of(func: &Function, value: ValueId) -> SmallVec<[usize; 2]> {
    let mut params = SmallVec::new();
    let mut seen = FxHashSet::default();
    let mut stack = vec![value];
    while let Some(value) = stack.pop() {
        if !seen.insert(value) {
            continue;
        }
        match *func.value(value) {
            Value::Arg(index) => params.push(index.index()),
            Value::Inst(inst) => match &func.inst(inst).kind {
                InstKind::Phi(incoming) => stack.extend(incoming.iter().map(|&(_, value)| value)),
                &InstKind::Select(_, first, second) => stack.extend([first, second]),
                &InstKind::Bitcast(value)
                | &InstKind::IntToPtr(value)
                | &InstKind::PtrToInt(value, _) => stack.push(value),
                _ => {}
            },
            _ => {}
        }
    }
    params
}

/// Reports the uses of builders `func` empties.
fn check_function(
    gcx: Gcx<'_>,
    func: &Function,
    finish: &FxHashSet<FunctionId>,
    summaries: &Summaries,
) {
    let calls = emptied_operands(func, finish, summaries)
        .into_iter()
        .filter(|(_, emptied)| !emptied.is_empty())
        .collect::<FxHashMap<_, _>>();
    if calls.is_empty() {
        return;
    }
    let cfg = CfgInfo::new(func);
    let mut predecessors = index_vec![SmallVec::<[BlockId; 2]>::new(); func.blocks.len()];
    for &block in cfg.rpo() {
        for &successor in cfg.successors(block) {
            predecessors[successor].push(block);
        }
    }
    // The emptied builders where each block ends, with the call that emptied each.
    let mut exits: IndexVec<BlockId, Option<FxHashMap<ValueId, InstId>>> =
        index_vec![None; func.blocks.len()];
    loop {
        let mut changed = false;
        for &block in cfg.rpo() {
            let mut emptied = entry_state(func, &predecessors[block], &exits, block);
            scan_block(func, block, &calls, &mut emptied, &mut |_, _| {});
            if exits[block].as_ref() != Some(&emptied) {
                exits[block] = Some(emptied);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut reported = FxHashSet::default();
    for &block in cfg.rpo() {
        let mut emptied = entry_state(func, &predecessors[block], &exits, block);
        scan_block(func, block, &calls, &mut emptied, &mut |span, call| {
            let emptied = func.inst(call).metadata.source_span();
            if !reported.insert((span, emptied)) {
                return;
            }
            let mut err = gcx.dcx().err("this uses a builder after `finish` emptied it");
            if let Some(span) = span.or(emptied) {
                err = err.span(span);
            }
            if let Some(emptied) = emptied {
                let note = if span == Some(emptied) {
                    "an earlier iteration empties the builder here"
                } else {
                    "the builder is emptied here"
                };
                err = err.span_note(emptied, note);
            }
            err.help(
                "finish a builder once, after its last append, and start another with \
                 `Buffers.create`",
            )
            .emit();
        });
    }
}

/// The emptied builders where `block` starts: what any of its `predecessors` left, and the phis of
/// the block whose value along some edge is an emptied builder.
fn entry_state(
    func: &Function,
    predecessors: &[BlockId],
    exits: &IndexVec<BlockId, Option<FxHashMap<ValueId, InstId>>>,
    block: BlockId,
) -> FxHashMap<ValueId, InstId> {
    let phis = func.blocks[block]
        .instructions
        .iter()
        .filter(|&&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)))
        .filter_map(|&inst| func.inst_result_value(inst))
        .collect::<SmallVec<[ValueId; 4]>>();
    let mut state = FxHashMap::default();
    for &predecessor in predecessors {
        let Some(exit) = &exits[predecessor] else { continue };
        // A phi is defined again on entry, so only its value along the edge counts.
        for (&value, &call) in exit {
            if !phis.contains(&value) {
                state.entry(value).or_insert(call);
            }
        }
        for &inst in &func.blocks[block].instructions {
            let InstKind::Phi(incoming) = &func.inst(inst).kind else { continue };
            if let Some(result) = func.inst_result_value(inst)
                && let Some(&call) = incoming
                    .iter()
                    .filter(|&&(from, _)| from == predecessor)
                    .find_map(|(_, value)| exit.get(value))
            {
                state.entry(result).or_insert(call);
            }
        }
    }
    state
}

/// Runs `block` over the emptied builders `state`, calling `report` with each use of one: the
/// span of the use and the call that emptied the builder.
fn scan_block(
    func: &Function,
    block: BlockId,
    calls: &FxHashMap<InstId, SmallVec<[ValueId; 1]>>,
    state: &mut FxHashMap<ValueId, InstId>,
    report: &mut impl FnMut(Option<Span>, InstId),
) {
    let data = &func.blocks[block];
    for &inst in &data.instructions {
        let instruction = func.inst(inst);
        match &instruction.kind {
            // A phi takes its value along an edge, which the block's entry state accounts for.
            InstKind::Phi(_) => continue,
            // A cast or a select only renames a builder.
            InstKind::Bitcast(_)
            | InstKind::IntToPtr(_)
            | InstKind::PtrToInt(..)
            | InstKind::Select(..) => {
                let call = instruction
                    .kind
                    .operands()
                    .iter()
                    .find_map(|operand| state.get(operand))
                    .copied();
                if let Some(result) = func.inst_result_value(inst) {
                    state.remove(&result);
                    if let Some(call) = call {
                        state.insert(result, call);
                    }
                }
                continue;
            }
            _ => {}
        }
        for operand in instruction.kind.operands() {
            if let Some(&call) = state.get(&operand) {
                report(instruction.metadata.source_span(), call);
            }
        }
        // The instruction defines its result anew, as in a loop's next iteration.
        if let Some(result) = func.inst_result_value(inst) {
            state.remove(&result);
        }
        if let Some(emptied) = calls.get(&inst) {
            for &value in emptied {
                state.entry(value).or_insert(inst);
            }
        }
    }
    if let Some(terminator) = &data.terminator {
        let span = data.terminator_metadata.source_span();
        terminator.for_each_operand(|operand| {
            if let Some(&call) = state.get(&operand) {
                report(span, call);
            }
        });
    }
}

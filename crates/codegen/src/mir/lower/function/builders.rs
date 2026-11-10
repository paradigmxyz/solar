//! Uses of a `solar:core/Buffers.sol` builder after `Buffers.finish` emptied it.
//!
//! `finish` returns what a builder holds without a copy and leaves the builder empty, so appending
//! afterwards starts a new output instead of extending the one returned. That is well defined, and
//! other compilers run it as written, but a builder is meant to be finished once, after its last
//! append: this compiler rejects every use of a builder on a path after `finish` emptied it.
//!
//! The check runs once the contract is lowered, on a disposable copy of each entry's MIR with
//! user helpers expanded. This preserves argument aliases, conditional finishes and reverting
//! paths without modifying executable IR. A call to `finish` empties its actual argument. The
//! emptied builders flow forward through the CFG, into a phi along each edge that carries one,
//! until the instruction defining a value runs again, as a loop's next iteration does; any other
//! use of one is reported: a call it is passed to, a store of it, or a return of it.
//!
//! Each path carries the equalities its branches proved, between values up to a constant offset,
//! and skips a branch they decide. A builder a loop finishes when `i + 1 == n`, or `i == n - 1`,
//! is therefore not emptied on the back edge, where the incremented `i` equals `n` and `i < n`
//! fails, and one finished under a condition is not emptied where the same condition, or its
//! negation, is tested again. Back edges widen states with the same emptied references; forward
//! joins retain separate facts until their path budget is exhausted. Facts have a separate fixed
//! size budget so sequential branches cannot accumulate quadratic state.
//!
//! Builders are followed as values, never through memory: type checking keeps a builder out of
//! struct fields, array elements, and mapping values, so none is stored and loaded again, and
//! returning an emptied builder is itself a use.
//!
//! NOTE: inline assembly can still move a builder's pointer through memory unseen.

use super::*;
use crate::mir::{
    BlockId, CheckedOp, InstId, Module, Terminator,
    analysis::{AliasAnalysis, Location, LocationSize},
    pass::MirPass,
};
use solar_data_structures::{
    index::{IndexVec, index_vec},
    smallvec::SmallVec,
};
use solar_interface::source_map::FileName;
use solar_sema::hir::Visit;
use std::{collections::VecDeque, ops::ControlFlow};

/// The module that owns the builders.
const BUFFERS: &str = "solar:core/Buffers.sol";

/// The functions of `mir_ids` that are `Buffers.finish`.
pub(in crate::mir::lower) fn finish_functions(
    gcx: Gcx<'_>,
    mir_ids: &FxHashMap<hir::FunctionId, FunctionId>,
) -> FxHashSet<FunctionId> {
    let source_map = gcx.sess.source_map();
    if source_map.get_file(FileName::Custom(BUFFERS.to_owned())).is_none()
        && source_map.get_file(FileName::Real(BUFFERS.into())).is_none()
    {
        return FxHashSet::default();
    }
    mir_ids
        .iter()
        .filter(|&(&id, _)| {
            let function = gcx.hir.function(id);
            solar_sema::core::module_path(&gcx.hir.source(function.source).file.name)
                == Some(BUFFERS)
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
    mir_ids: &FxHashMap<hir::FunctionId, FunctionId>,
    contract: hir::ContractId,
) {
    if finish.is_empty() {
        return;
    }
    let user_assembly = mir_ids
        .iter()
        .filter_map(|(&id, &mir_id)| {
            let function = gcx.hir.function(id);
            let mut assembly = UserAssembly { gcx, contract, seen: FxHashSet::default() };
            (!solar_sema::core::is_core_file(&gcx.hir.source(function.source).file.name)
                && (function.is_yul || assembly.visit_nested_function(id).is_break()))
            .then_some(mir_id)
        })
        .collect::<Vec<_>>();
    let builder_returns = mir_ids
        .iter()
        .filter_map(|(&id, &mir_id)| {
            gcx.hir
                .function(id)
                .returns
                .iter()
                .any(|&ret| {
                    let TyKind::Struct(id) = gcx.type_of_item(ret.into()).peel_refs().kind else {
                        return false;
                    };
                    solar_sema::core::module_path(
                        &gcx.hir.source(gcx.hir.strukt(id).source).file.name,
                    ) == Some(BUFFERS)
                })
                .then_some(mir_id)
        })
        .collect::<FxHashSet<_>>();
    let graph = crate::mir::analysis::CallGraphInfo::new(module);
    let mut callers = index_vec![Vec::new(); module.functions.len()];
    for (id, _) in module.functions.iter_enumerated() {
        for callee in graph.callees(id) {
            callers[callee].push(id);
        }
    }
    // Reverse propagation visits each edge once, rather than rescanning the whole module for
    // every level of a call chain.
    let relevant = calling_closure(&callers, finish.iter().copied());
    let assembly = calling_closure(&callers, user_assembly);
    let mut analysis = module.clone();
    let mut checked = FxHashMap::default();
    let mut budget = 16_384usize;
    let calls = CallContext {
        module,
        graph: &graph,
        relevant: &relevant,
        finish,
        builder_returns: &builder_returns,
    };
    for root in &module.external_entries() {
        if !relevant.contains(root) {
            continue;
        }
        let mut function = module.function(root).clone();
        let mut uses = AnalysisUses::default();
        if expand_calls(gcx, &calls, &mut function, &mut budget, &mut uses) {
            *analysis.function_mut(root) = function;
            checked.insert(root, uses);
        }
    }
    // Resolve single-edge and loop-invariant aliases before checking each entry. This changes
    // only the analysis copy, never the module that code generation consumes.
    crate::mir::transform::cfg_simplify::SimplifyTrivialPhis.run_pass(
        gcx,
        &mut analysis,
        &mut Default::default(),
    );
    for root in module.external_entries().iter() {
        if let Some(uses) = checked.get(&root) {
            check_function(
                gcx,
                &analysis,
                analysis.function(root),
                finish,
                !assembly.contains(root),
                uses,
            );
        }
    }
}

/// Call facts shared by every entry's disposable expansion.
struct CallContext<'a> {
    module: &'a Module,
    graph: &'a crate::mir::analysis::CallGraphInfo,
    relevant: &'a DenseBitSet<FunctionId>,
    finish: &'a FxHashSet<FunctionId>,
    builder_returns: &'a FxHashSet<FunctionId>,
}

/// Checks helper bodies in their actual calling context: aliases, conditions and failing paths
/// must not become an unconditional "may finish" summary. This is a disposable analysis copy;
/// neither inlining hints nor optimization objectives change which calls validation follows.
/// Clones and path facts have explicit budgets. Unsupported recursion or excessive expansion is
/// rejected rather than silently accepting an unchecked use.
fn expand_calls(
    gcx: Gcx<'_>,
    calls: &CallContext<'_>,
    function: &mut Function,
    budget: &mut usize,
    uses: &mut AnalysisUses,
) -> bool {
    let mut replacements = FxHashMap::default();
    let mut block = 0;
    while block < function.blocks.len() {
        let id = BlockId::from_usize(block);
        for index in 0..function.blocks[id].instructions.len() {
            let inst = function.blocks[id].instructions[index];
            let InstKind::ICall { function: crate::mir::Callee::Function(callee), .. } =
                function.inst(inst).kind
            else {
                continue;
            };
            if calls.finish.contains(&callee) {
                continue;
            }
            let callee_func = calls.module.function(callee);
            // Compiler-owned buffer operations are the abstraction boundary. User helpers and
            // dispatchers are expanded, including helpers that merely return a builder alias.
            let file = gcx.sess.source_map().lookup_source_file(callee_func.declaration_span.lo());
            let core = solar_sema::core::is_core_file(&file.name);
            if core
                && !(callee_func.return_type() == MirType::MEMPTR
                    && callee_func.params.iter().all(|&ty| ty != MirType::MEMPTR))
            {
                continue;
            }
            if calls.graph.is_recursive(callee)
                && !calls.relevant.contains(callee)
                && !calls.builder_returns.contains(&callee)
                && !callee_func.attributes.is_function_pointer_dispatcher
            {
                continue;
            }
            let count = callee_func.instructions().count().saturating_add(callee_func.blocks.len());
            if calls.graph.is_recursive(callee) || count > *budget {
                gcx.dcx().err("cannot prove builder lifetime through this call")
                    .span(function.inst(inst).metadata.source_span().unwrap_or(function.declaration_span))
                    .help("avoid recursive builder helpers or reduce helper expansion in this entry point")
                    .emit();
                return false;
            }
            *budget -= count;
            let InstKind::ICall { args, .. } = &function.inst(inst).kind else { unreachable!() };
            let call = (function.inst(inst).metadata.source_span(), args.iter().copied().collect());
            let continuation = BlockId::from_usize(function.blocks.len());
            // Existing return-use records follow the suffix when a block splits at a call.
            if let Some(events) = uses.remove(&id) {
                uses.insert(continuation, events);
            }
            uses.entry(id).or_default().push(call);
            // icall callee(args) => cloned callee CFG; return edges => continuation phis
            let Some(renamed) = crate::mir::transform::inline::inline_analysis_call(
                function,
                id,
                index,
                callee_func,
                true,
                |block, values, span| {
                    uses.entry(block).or_default().push((span, values.iter().copied().collect()))
                },
            ) else {
                gcx.dcx()
                    .err("cannot check builder lifetime through this call")
                    .span(function.declaration_span)
                    .emit();
                return false;
            };
            replacements.extend(renamed);
            break; // The suffix and cloned body are newly appended blocks.
        }
        block += 1;
    }
    function.replace_uses_canonicalized(&replacements);
    for events in uses.values_mut() {
        for (_, values) in events {
            for value in values {
                *value = crate::mir::utils::resolve_replacement(*value, &replacements);
            }
        }
    }
    crate::mir::utils::rebuild_predecessors(function);
    true
}

/// Calls that empty a builder. Helper calls have already been expanded in an analysis copy.
fn emptied_operands(
    func: &Function,
    finish: &FxHashSet<FunctionId>,
) -> FxHashMap<InstId, SmallVec<[ValueId; 1]>> {
    func.instructions()
        .filter_map(|inst| {
            let InstKind::ICall { function: crate::mir::Callee::Function(callee), args } =
                &func.inst(inst).kind
            else {
                return None;
            };
            (finish.contains(callee) && !args.is_empty())
                .then(|| (inst, SmallVec::from_slice(&args[..1])))
        })
        .collect()
}

/// Source-level calls and returns remain uses even when the analysis expands their bodies.
/// Records sit at the ends of their blocks and never enter executable IR.
type AnalysisUses = FxHashMap<BlockId, Vec<(Option<Span>, SmallVec<[ValueId; 4]>)>>;

/// Reports the uses of builders `func` empties.
fn check_function(
    gcx: Gcx<'_>,
    module: &Module,
    func: &Function,
    finish: &FxHashSet<FunctionId>,
    typed_heap: bool,
    uses: &AnalysisUses,
) {
    let calls = emptied_operands(func, finish);
    if calls.is_empty() {
        return;
    }
    let Some(entries) = path_states(
        func,
        &calls,
        if typed_heap { stable_argument_lengths(gcx, module, func) } else { Arc::default() },
        uses,
    ) else {
        gcx.dcx()
            .err("builder lifetime analysis exceeds its finished-builder budget")
            .span(func.declaration_span)
            .help("build repeated outputs in a loop, or split entry points")
            .emit();
        return;
    };
    let mut reported = FxHashSet::default();
    for (block, states) in entries.iter_enumerated() {
        for (state, _) in states {
            let mut state = state.clone();
            scan_block(func, block, &calls, uses, &mut state, &mut |span, call| {
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
}

/// Repeated loads of an incoming array's length can share a symbolic fact only if no write
/// can change the header. In particular, fresh builders own their backing allocations:
/// `Buffers` copies chunks into them and never adopts an incoming array. Their operations
/// cannot change an entry argument's header. An assembly-forged builder is not a fresh object
/// and gets no such exception. All other writes and calls use the ordinary alias analysis.
/// Candidate headers are bounded independently of source size, keeping this proof linear.
fn stable_argument_lengths(
    gcx: Gcx<'_>,
    module: &Module,
    func: &Function,
) -> Arc<FxHashMap<ValueId, ValueId>> {
    let aa = AliasAnalysis::new(func);
    let mut candidates = FxHashMap::default();
    for inst in func.instructions() {
        if let InstKind::Load(address, PtrLocation::Memory) = func.inst(inst).kind
            && let Some((object, 0)) = func.object_address(address)
            && matches!(func.value(object), Value::Arg(_))
            && let Some(location) = aa.memory_location(
                func,
                inst,
                address,
                LocationSize::Const(EvmMemoryLayout::WORD_SIZE),
            )
            && (candidates.contains_key(&object) || candidates.len() < 64)
        {
            candidates.insert(object, Location::Memory(location));
        }
    }
    for inst in func.instructions() {
        if candidates.is_empty() {
            break;
        }
        if let InstKind::ICall { function: crate::mir::Callee::Function(callee), args } =
            &func.inst(inst).kind
        {
            let callee = module.function(*callee);
            let file = gcx.sess.source_map().lookup_source_file(callee.declaration_span.lo());
            if solar_sema::core::module_path(&file.name) == Some(BUFFERS)
                && args.first().is_some_and(|&value| {
                    func.object_address(value).is_some_and(|(object, offset)| offset == 0
                        && matches!(func.value(object), Value::Inst(inst) if matches!(func.inst(*inst).kind, InstKind::Alloc { .. })))
                })
            { continue; }
        }
        // Allocation only advances the heap and initializes its own fresh object. Entry
        // arguments were allocated earlier; without unsafe assembly this cannot reuse them.
        if matches!(func.inst(inst).kind, InstKind::Alloc { .. }) {
            continue;
        }
        if let InstKind::Store(address, _, PtrLocation::Memory) = func.inst(inst).kind
            && let Some((object, offset)) = func.object_address(address)
            && let Value::Inst(alloc) = *func.value(object)
            && let InstKind::Alloc { size, .. } = func.inst(alloc).kind
            && (offset == 0
                || func.value_u64(size).is_some_and(|size| {
                    offset.checked_add(EvmMemoryLayout::WORD_SIZE).is_some_and(|end| end <= size)
                }))
        {
            // Source-level allocation headers and in-bounds fields cannot overlap objects
            // passed into this entry. This proof is disabled for all user inline assembly,
            // including memory-safe assembly, which can manufacture aliases explicitly.
            continue;
        }
        let effects = aa.instruction_mod_ref(func, inst);
        candidates.retain(|_, &mut location| !effects.may_write(&aa, location));
    }
    let mut loads = FxHashMap::default();
    for inst in func.instructions() {
        if let InstKind::Load(address, PtrLocation::Memory) = func.inst(inst).kind
            && let Some((object, 0)) = func.object_address(address)
            && candidates.contains_key(&object)
            && let Some(value) = func.inst_result_value(inst)
        {
            loads.insert(value, object);
        }
    }
    Arc::new(loads)
}

/// At most this many path states reach a block before they merge into one.
const PATH_STATES: usize = 8;

/// What one path knows where a block starts: the builders it emptied, with the call that emptied
/// each, and the equalities its branches proved.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PathState {
    emptied: FxHashMap<ValueId, InstId>,
    facts: Facts,
}

impl PathState {
    /// Whether `self` covers `other`: everything `other` emptied is emptied here, and everything
    /// this state knows, `other` knows too, so this state reaches every use `other` reaches.
    fn covers(&self, other: &Self) -> bool {
        other.emptied.keys().all(|value| self.emptied.contains_key(value))
            && other.facts.implies(&self.facts)
    }

    /// The state that covers both `self` and `other`.
    fn join(&self, other: &Self) -> Self {
        let mut emptied = self.emptied.clone();
        for (&value, &call) in &other.emptied {
            emptied.entry(value).or_insert(call);
        }
        Self { emptied, facts: self.facts.meet(&other.facts) }
    }
}

/// The path states that reach each block, found to a fixed point, each with whether it was run.
///
/// A path's branches add facts: the taken side of `jumpi c` knows `c`, and an `eq` or `ne`
/// condition also knows its operands equal on the side that says so. A branch the facts decide
/// sends the path one way only, so a builder a loop's last iteration finishes stops at the loop's
/// exit: its `i + 1 == n` makes the next `i < n` false. A state covered by one already at a block
/// adds nothing, back edges join states that emptied the same builders, and past [`PATH_STATES`]
/// the states of a block join into one, so the search ends.
fn path_states(
    func: &Function,
    calls: &FxHashMap<InstId, SmallVec<[ValueId; 1]>>,
    stable_loads: Arc<FxHashMap<ValueId, ValueId>>,
    uses: &AnalysisUses,
) -> Option<IndexVec<BlockId, Vec<(PathState, bool)>>> {
    let mut entries = index_vec![Vec::<(PathState, bool)>::new(); func.blocks.len()];
    let mut worklist = VecDeque::from([BlockId::ENTRY]);
    let mut widened = DenseBitSet::new_empty(func.blocks.len());
    let cfg = crate::mir::analysis::CfgInfo::new(func);
    entries[BlockId::ENTRY].push((
        PathState { facts: Facts { stable_loads, ..Default::default() }, ..Default::default() },
        false,
    ));
    while let Some(block) = worklist.pop_front() {
        while let Some(index) = entries[block].iter().position(|(_, ran)| !ran) {
            entries[block][index].1 = true;
            let mut state = entries[block][index].0.clone();
            scan_block(func, block, calls, uses, &mut state, &mut |_, _| {});
            if state.emptied.len() > 64 {
                return None;
            }
            for (successor, state) in successor_states(func, block, state) {
                if add_state(
                    &mut entries[successor],
                    state,
                    widened.contains(successor),
                    cfg.dominators().dominates(successor, block),
                ) {
                    if entries[successor].len() > PATH_STATES {
                        let joined = entries[successor]
                            .iter()
                            .skip(1)
                            .fold(entries[successor][0].0.clone(), |joined, (state, _)| {
                                joined.join(state)
                            });
                        entries[successor] = vec![(joined, false)];
                        widened.insert(successor);
                    }
                    worklist.push_back(successor);
                }
            }
        }
    }
    Some(entries)
}

/// Adds `state` to the states of a block, unless one there covers it; returns whether the block
/// has a new state to run.
///
/// On a back edge, states that emptied the same builders join, so facts that change on each
/// lap widen away while relations every lap shares survive. Forward joins keep distinct branch
/// conditions until their path budget is exhausted.
fn add_state(
    states: &mut Vec<(PathState, bool)>,
    state: PathState,
    widened: bool,
    backedge: bool,
) -> bool {
    if states.iter().any(|(existing, _)| existing.covers(&state)) {
        return false;
    }
    if backedge
        && let Some((existing, ran)) = states.iter_mut().find(|(existing, _)| {
            existing.emptied.len() == state.emptied.len()
                && existing.emptied.keys().all(|value| state.emptied.contains_key(value))
        })
    {
        *existing = existing.join(&state);
        *ran = false;
        return true;
    }
    states.retain(|(existing, _)| !state.covers(existing));
    states.push((state, false));
    if widened {
        let joined = states.iter().skip(1).fold(states[0].0.clone(), |a, (b, _)| a.join(b));
        *states = vec![(joined, false)];
    }
    true
}

/// The states `state`, at the end of `block`, enters each successor with: along the edges its
/// facts leave open, with what each edge's branch proves, and through the successor's phis.
fn successor_states(
    func: &Function,
    block: BlockId,
    state: PathState,
) -> SmallVec<[(BlockId, PathState); 2]> {
    let mut states = SmallVec::new();
    match &func.blocks[block].terminator {
        Some(Terminator::Jump(target)) => {
            states.push((*target, enter(func, block, *target, state)))
        }
        Some(Terminator::Branch { condition, then_block, else_block }) => {
            let (condition, then_block, else_block) = (*condition, *then_block, *else_block);
            match state.facts.truth(func, condition) {
                Some(true) => states.push((then_block, enter(func, block, then_block, state))),
                Some(false) => states.push((else_block, enter(func, block, else_block, state))),
                None => {
                    for (target, holds) in [(then_block, true), (else_block, false)] {
                        let mut state = state.clone();
                        if state.facts.assume(func, condition, holds) {
                            states.push((target, enter(func, block, target, state)));
                        }
                    }
                }
            }
        }
        Some(terminator) => {
            for &target in terminator.successors().iter() {
                states.push((target, enter(func, block, target, state.clone())));
            }
        }
        None => {}
    }
    states
}

/// `state` entering `target` from `from`: each phi of `target` takes its value along the edge,
/// which it is defined anew as unless it merges itself there.
fn enter(func: &Function, from: BlockId, target: BlockId, mut state: PathState) -> PathState {
    let phis = func.blocks[target]
        .instructions
        .iter()
        .filter_map(|&inst| {
            let InstKind::Phi(incoming) = &func.inst(inst).kind else { return None };
            let value = incoming.iter().find(|&&(block, _)| block == from)?.1;
            Some((func.inst_result_value(inst)?, value))
        })
        .collect::<SmallVec<[(ValueId, ValueId); 4]>>();
    let redefined = phis
        .iter()
        .filter(|&&(phi, value)| phi != value)
        .map(|&(phi, _)| phi)
        .collect::<SmallVec<[ValueId; 4]>>();
    // Every phi is defined at once, from the values before any of them is.
    let emptied = phis
        .iter()
        .map(|&(phi, value)| (phi, state.emptied.get(&value).copied()))
        .collect::<SmallVec<[_; 4]>>();
    let forms = phis
        .iter()
        .filter(|&&(phi, value)| phi != value)
        .map(|&(phi, value)| (phi, state.facts.resolve_avoiding(func, value, &redefined)))
        .collect::<SmallVec<[_; 4]>>();
    for &phi in &redefined {
        state.emptied.remove(&phi);
        state.facts.kill(phi);
    }
    for (phi, call) in emptied {
        if let Some(call) = call {
            state.emptied.insert(phi, call);
        }
    }
    for (phi, form) in forms {
        if let Some(form) = form {
            state.facts.union((Atom::Value(phi), U256::ZERO), form);
        }
    }
    state
}

/// Runs `block` over the path `state`, calling `report` with each use of an emptied builder: the
/// span of the use and the call that emptied the builder.
fn scan_block(
    func: &Function,
    block: BlockId,
    calls: &FxHashMap<InstId, SmallVec<[ValueId; 1]>>,
    uses: &AnalysisUses,
    state: &mut PathState,
    report: &mut impl FnMut(Option<Span>, InstId),
) {
    let data = &func.blocks[block];
    for &inst in &data.instructions {
        let instruction = func.inst(inst);
        let result = func.inst_result_value(inst);
        match &instruction.kind {
            // A phi takes its value along an edge, which the block's entry state accounts for.
            InstKind::Phi(_) => continue,
            // A cast or a select only renames a builder.
            InstKind::IntToPtr(..) | InstKind::PtrToInt(..) | InstKind::Select(..) => {
                let call = instruction
                    .kind
                    .operands()
                    .iter()
                    .find_map(|operand| state.emptied.get(operand))
                    .copied();
                if let Some(result) = result {
                    state.emptied.remove(&result);
                    state.facts.kill(result);
                    if let Some(call) = call {
                        state.emptied.insert(result, call);
                    }
                    let alias = match instruction.kind {
                        InstKind::IntToPtr(value) | InstKind::PtrToInt(value, _) => Some(value),
                        InstKind::Select(condition, a, b) => state
                            .facts
                            .truth(func, condition)
                            .map(|holds| if holds { a } else { b }),
                        _ => None,
                    };
                    if let Some(alias) = alias {
                        state.facts.union(
                            (Atom::Value(result), U256::ZERO),
                            state.facts.resolve(func, alias),
                        );
                    }
                }
                continue;
            }
            _ => {}
        }
        for operand in instruction.kind.operands() {
            if let Some(call) = emptied_alias(func, state, operand) {
                report(instruction.metadata.source_span(), call);
            }
        }
        // The instruction defines its result anew, as in a loop's next iteration.
        if let Some(result) = result {
            state.emptied.remove(&result);
            state.facts.kill(result);
        }
        if let Some(emptied) = calls.get(&inst) {
            for &value in emptied {
                for alias in builder_aliases(func, &state.facts, value) {
                    state.emptied.entry(alias).or_insert(inst);
                }
                state.emptied.entry(value).or_insert(inst);
            }
        }
    }
    for (span, values) in uses.get(&block).into_iter().flatten() {
        for &value in values {
            if let Some(call) = emptied_alias(func, state, value) {
                report(*span, call);
            }
        }
    }
    if let Some(terminator) = &data.terminator {
        let span = data.terminator_metadata.source_span();
        terminator.visit_operands(|operand| {
            if let Some(call) = emptied_alias(func, state, operand) {
                report(span, call);
            }
        });
    }
}

/// An emptied builder may be named by a phi, cast or helper return as well as its original SSA
/// value.
fn emptied_alias(func: &Function, state: &PathState, value: ValueId) -> Option<InstId> {
    builder_aliases(func, &state.facts, value)
        .into_iter()
        .find_map(|alias| state.emptied.get(&alias).copied())
        .or_else(|| state.emptied.get(&value).copied())
}

/// Roots of a builder reference. Facts choose a phi's incoming alias on a known path; when
/// widening forgets that choice, both incoming aliases remain possible. Forgetting scalar facts
/// must never forget that a reference can still name an emptied builder.
fn builder_aliases(func: &Function, facts: &Facts, value: ValueId) -> SmallVec<[ValueId; 2]> {
    let mut roots = SmallVec::new();
    let mut seen = FxHashSet::default();
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        if !seen.insert(value) {
            continue;
        }
        let (leader, offset) = facts.resolve(func, value);
        if let Atom::Value(alias) = leader
            && alias != value
            && offset.is_zero()
        {
            pending.push(alias);
            continue;
        }
        match func.value(value) {
            Value::Inst(inst) => match &func.inst(*inst).kind {
                InstKind::Phi(incoming) => pending.extend(incoming.iter().map(|&(_, value)| value)),
                &InstKind::Select(condition, a, b) => match facts.truth(func, condition) {
                    Some(true) => pending.push(a),
                    Some(false) => pending.push(b),
                    None => pending.extend([a, b]),
                },
                &InstKind::IntToPtr(value) | &InstKind::PtrToInt(value, _) => pending.push(value),
                _ => roots.push(value),
            },
            _ => roots.push(value),
        }
    }
    roots
}

/// A value the facts relate: a constant, or an SSA value that is not a constant offset of another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Atom {
    Const,
    Value(ValueId),
    Header(ValueId),
}

/// `value` as an atom plus a constant, through additions and subtractions of constants, checked
/// ones included: a checked operation either has the wrapped result or fails.
fn form(func: &Function, value: ValueId) -> (Atom, U256) {
    let mut value = value;
    let mut offset = U256::ZERO;
    for _ in 0..16 {
        if let Some(constant) = func.value_u256(value) {
            return (Atom::Const, offset.wrapping_add(constant));
        }
        let Value::Inst(inst) = *func.value(value) else { break };
        let (operand, delta) = match func.inst(inst).kind {
            InstKind::Add(lhs, rhs) => match (func.value_u256(lhs), func.value_u256(rhs)) {
                (_, Some(constant)) => (lhs, constant),
                (Some(constant), _) => (rhs, constant),
                _ => break,
            },
            InstKind::Sub(lhs, rhs) => match func.value_u256(rhs) {
                Some(constant) => (lhs, constant.wrapping_neg()),
                None => break,
            },
            InstKind::CheckedBinary { op, lhs, rhs, .. } => match (op, func.value_u256(rhs)) {
                (CheckedOp::Add, Some(constant)) => (lhs, constant),
                (CheckedOp::Sub, Some(constant)) => (lhs, constant.wrapping_neg()),
                _ => break,
            },
            _ => break,
        };
        value = operand;
        offset = offset.wrapping_add(delta);
    }
    (Atom::Value(value), offset)
}

/// Values a path proved equal up to a constant, as classes: each member maps to its class's
/// leader and its offset from it, `member = leader + offset` in wrapping arithmetic. A class with
/// a constant member leads with the constant.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Facts {
    stable_loads: Arc<FxHashMap<ValueId, ValueId>>,
    members: FxHashMap<Atom, (Atom, U256)>,
}

impl Facts {
    fn form(&self, func: &Function, value: ValueId) -> (Atom, U256) {
        let (atom, offset) = form(func, value);
        let atom = match atom {
            Atom::Value(value) if let Some(&object) = self.stable_loads.get(&value) => {
                Atom::Header(object)
            }
            _ => atom,
        };
        (atom, offset)
    }

    fn find(&self, atom: Atom) -> (Atom, U256) {
        self.members.get(&atom).copied().unwrap_or((atom, U256::ZERO))
    }

    /// `value` as its class's leader plus a constant.
    fn resolve(&self, func: &Function, value: ValueId) -> (Atom, U256) {
        let (atom, offset) = self.form(func, value);
        let (leader, base) = self.find(atom);
        (leader, base.wrapping_add(offset))
    }

    /// [`Self::resolve`], led by a member other than the `avoided` values, when there is one.
    fn resolve_avoiding(
        &self,
        func: &Function,
        value: ValueId,
        avoided: &[ValueId],
    ) -> Option<(Atom, U256)> {
        let (leader, offset) = self.resolve(func, value);
        let avoid = |atom: Atom| matches!(atom, Atom::Value(value) if avoided.contains(&value));
        if !avoid(leader) {
            return Some((leader, offset));
        }
        // leader + offset = member - member_offset + offset
        let (&member, &(_, member_offset)) = self
            .members
            .iter()
            .filter(|&(&member, &(class, _))| class == leader && !avoid(member))
            .min_by_key(|&(&member, _)| member)?;
        Some((member, offset.wrapping_sub(member_offset)))
    }

    /// Records `a = b`, each an atom plus a constant; returns false when the facts contradict it.
    fn union(&mut self, a: (Atom, U256), b: (Atom, U256)) -> bool {
        let (a_leader, a_base) = self.find(a.0);
        let (b_leader, b_base) = self.find(b.0);
        let a_offset = a_base.wrapping_add(a.1);
        let b_offset = b_base.wrapping_add(b.1);
        if a_leader == b_leader {
            return a_offset == b_offset;
        }
        // a_leader + a_offset = b_leader + b_offset
        let (keep, merged, delta) = if a_leader == Atom::Const {
            (a_leader, b_leader, a_offset.wrapping_sub(b_offset))
        } else {
            (b_leader, a_leader, b_offset.wrapping_sub(a_offset))
        };
        // Forgetting a relation can only open more paths, never hide an emptied builder.
        // Bound the state as well as the number of paths: otherwise sequential branches retain
        // a growing map at every block, consuming quadratic memory.
        if self.members.len() >= 64 {
            self.members.clear();
        }
        // merged = keep + delta
        let moved = self
            .members
            .iter()
            .filter(|&(_, &(class, _))| class == merged)
            .map(|(&member, &(_, offset))| (member, offset))
            .collect::<SmallVec<[_; 4]>>();
        for (member, offset) in moved {
            self.members.insert(member, (keep, offset.wrapping_add(delta)));
        }
        self.members.insert(merged, (keep, delta));
        self.members.insert(keep, (keep, U256::ZERO));
        true
    }

    /// Forgets `value`, which is being defined anew.
    fn kill(&mut self, value: ValueId) {
        let atom = Atom::Value(value);
        let Some((leader, _)) = self.members.remove(&atom) else { return };
        let class = self
            .members
            .iter()
            .filter(|&(_, &(class, _))| class == leader)
            .map(|(&member, &(_, offset))| (member, offset))
            .collect::<SmallVec<[_; 4]>>();
        if leader == atom
            && let Some(&(new_leader, base)) = class.iter().min_by_key(|&&(member, _)| member)
        {
            for &(member, offset) in &class {
                self.members.insert(member, (new_leader, offset.wrapping_sub(base)));
            }
        }
        // A class left with its leader alone says nothing.
        if class.len() == 1 {
            self.members.remove(&class[0].0);
        }
    }

    /// Whether `value` is nonzero on every path with these facts, when they decide it: for a
    /// condition, whether it holds.
    fn truth(&self, func: &Function, value: ValueId) -> Option<bool> {
        self.truth_at(func, value, 0)
    }

    fn truth_at(&self, func: &Function, value: ValueId, depth: usize) -> Option<bool> {
        let (leader, offset) = self.resolve(func, value);
        if leader == Atom::Const {
            return Some(!offset.is_zero());
        }
        let Value::Inst(inst) = *func.value(value) else { return None };
        if depth > 8 {
            return None;
        }
        match func.inst(inst).kind {
            InstKind::Eq(lhs, rhs) => self.equality(func, lhs, rhs, depth),
            InstKind::Ne(lhs, rhs) => self.equality(func, lhs, rhs, depth).map(|equal| !equal),
            InstKind::Lt(lhs, rhs) | InstKind::Gt(lhs, rhs) => {
                let (a, b) = (self.resolve(func, lhs), self.resolve(func, rhs));
                if a.0 == Atom::Const && b.0 == Atom::Const {
                    Some(if matches!(func.inst(inst).kind, InstKind::Lt(..)) {
                        a.1 < b.1
                    } else {
                        a.1 > b.1
                    })
                } else {
                    self.difference(func, lhs, rhs)
                        .filter(|difference| difference.is_zero())
                        .map(|_| false)
                }
            }
            _ => None,
        }
    }

    /// `lhs - rhs`, when the facts relate the two.
    fn difference(&self, func: &Function, lhs: ValueId, rhs: ValueId) -> Option<U256> {
        let (lhs_leader, lhs_offset) = self.resolve(func, lhs);
        let (rhs_leader, rhs_offset) = self.resolve(func, rhs);
        (lhs_leader == rhs_leader).then(|| lhs_offset.wrapping_sub(rhs_offset))
    }

    /// Whether `lhs == rhs`, when the facts decide it: by relating the operands, by comparing a
    /// value with zero, or by a decided `eq` or `ne` of the same operands.
    fn equality(&self, func: &Function, lhs: ValueId, rhs: ValueId, depth: usize) -> Option<bool> {
        if let Some(difference) = self.difference(func, lhs, rhs) {
            return Some(difference.is_zero());
        }
        let zero = (Atom::Const, U256::ZERO);
        if self.resolve(func, rhs) == zero
            && let Some(nonzero) = self.truth_at(func, lhs, depth + 1)
        {
            return Some(!nonzero);
        }
        if self.resolve(func, lhs) == zero
            && let Some(nonzero) = self.truth_at(func, rhs, depth + 1)
        {
            return Some(!nonzero);
        }
        let operands = (self.resolve(func, lhs), self.resolve(func, rhs));
        self.members.iter().find_map(|(&member, &(leader, offset))| {
            let (Atom::Value(condition), Atom::Const) = (member, leader) else { return None };
            let Value::Inst(inst) = *func.value(condition) else { return None };
            let (equal, a, b) = match func.inst(inst).kind {
                InstKind::Eq(a, b) => (true, a, b),
                InstKind::Ne(a, b) => (false, a, b),
                _ => return None,
            };
            let same = (self.resolve(func, a), self.resolve(func, b));
            (same == operands || (same.1, same.0) == operands).then(|| (!offset.is_zero()) == equal)
        })
    }

    /// Records that `condition` is `holds`; returns false when the facts contradict it.
    fn assume(&mut self, func: &Function, condition: ValueId, holds: bool) -> bool {
        let value = U256::from(u8::from(holds));
        if !self.union(self.form(func, condition), (Atom::Const, value)) {
            return false;
        }
        let Value::Inst(inst) = *func.value(condition) else { return true };
        // A boolean that is not zero is one.
        let boolean_nonzero = |facts: &mut Self, operand: ValueId| {
            func.value_ty(operand) != Some(MirType::I1)
                || facts.union(facts.form(func, operand), (Atom::Const, U256::from(1)))
        };
        match func.inst(inst).kind {
            InstKind::Eq(lhs, rhs) if holds => {
                self.union(self.form(func, lhs), self.form(func, rhs))
            }
            InstKind::Ne(lhs, rhs) if !holds => {
                self.union(self.form(func, lhs), self.form(func, rhs))
            }
            InstKind::Eq(lhs, rhs) | InstKind::Ne(lhs, rhs) => {
                // The operand compared with zero is not zero.
                if self.resolve(func, rhs) == (Atom::Const, U256::ZERO) {
                    boolean_nonzero(self, lhs)
                } else if self.resolve(func, lhs) == (Atom::Const, U256::ZERO) {
                    boolean_nonzero(self, rhs)
                } else {
                    true
                }
            }
            _ => true,
        }
    }

    /// Whether these facts give every relation `other` gives.
    fn implies(&self, other: &Self) -> bool {
        other.members.iter().all(|(&member, &(leader, offset))| {
            let (member_leader, member_offset) = self.find(member);
            let (leader_leader, leader_offset) = self.find(leader);
            member_leader == leader_leader && member_offset.wrapping_sub(leader_offset) == offset
        })
    }

    /// The relations both `self` and `other` give.
    fn meet(&self, other: &Self) -> Self {
        let mut met = Self { stable_loads: Arc::clone(&self.stable_loads), ..Self::default() };
        let members = self.members.iter().map(|(&member, &relation)| (member, relation));
        let members = members.collect::<Vec<_>>();
        for &(member, (leader, offset)) in &members {
            for &(peer, (peer_leader, peer_offset)) in &members {
                if peer <= member || peer_leader != leader {
                    continue;
                }
                // member = peer + (offset - peer_offset) here; keep it when `other` agrees.
                let difference = offset.wrapping_sub(peer_offset);
                let (a_leader, a_offset) = other.find(member);
                let (b_leader, b_offset) = other.find(peer);
                if a_leader == b_leader && a_offset.wrapping_sub(b_offset) == difference {
                    met.union((member, U256::ZERO), (peer, difference));
                }
            }
        }
        met
    }
}

/// Assembly can manufacture aliases that the source-level ownership proof cannot establish.
struct UserAssembly<'gcx> {
    gcx: Gcx<'gcx>,
    contract: hir::ContractId,
    seen: FxHashSet<hir::FunctionId>,
}

impl<'gcx> Visit<'gcx> for UserAssembly<'gcx> {
    type BreakValue = ();

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_modifier(
        &mut self,
        modifier: &'gcx hir::Modifier<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        if let Some(id) = self.gcx.resolve_modifier_target(self.contract, modifier)
            && self.seen.insert(id)
        {
            self.visit_nested_function(id)?;
        }
        self.walk_modifier(modifier)
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
        if matches!(stmt.kind, hir::StmtKind::AssemblyBlock(..)) {
            return ControlFlow::Break(());
        }
        self.walk_stmt(stmt)
    }
}

/// The reverse call closure, with one visit per function and edge.
fn calling_closure(
    callers: &IndexVec<FunctionId, Vec<FunctionId>>,
    roots: impl IntoIterator<Item = FunctionId>,
) -> DenseBitSet<FunctionId> {
    let mut reached = DenseBitSet::new_empty(callers.len());
    let mut pending = roots.into_iter().collect::<Vec<_>>();
    while let Some(id) = pending.pop() {
        if reached.insert(id) {
            pending.extend(&callers[id]);
        }
    }
    reached
}

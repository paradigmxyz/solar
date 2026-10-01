//! Uses of a `solar:core/Buffers.sol` builder after `Buffers.finish` emptied it.
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
//! reported: a call it is passed to, a store of it, or a return of it.
//!
//! Each path carries the equalities its branches proved, between values up to a constant offset,
//! and skips a branch they decide. A builder a loop finishes when `i + 1 == n`, or `i == n - 1`,
//! is therefore not emptied on the back edge, where the incremented `i` equals `n` and `i < n`
//! fails, and one finished under a condition is not emptied where the same condition, or its
//! negation, is tested again. Facts that differ from lap to lap of a loop widen away.
//!
//! Builders are followed as values, never through memory: type checking keeps a builder out of
//! struct fields, array elements, and mapping values, so none is stored and loaded again, and
//! returning an emptied builder is itself a use.
//!
//! NOTE: inline assembly can still move a builder's pointer through memory unseen.

use super::*;
use crate::mir::{BlockId, CheckedOp, InstId, Module, Terminator};
use solar_data_structures::{
    index::{IndexVec, index_vec},
    smallvec::SmallVec,
};
use solar_interface::source_map::FileName;
use std::collections::VecDeque;

/// The module that owns the builders.
const BUFFERS: &str = "solar:core/Buffers.sol";

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
    let entries = path_states(func, &calls);
    let mut reported = FxHashSet::default();
    for (block, states) in entries.iter_enumerated() {
        for (state, _) in states {
            let mut state = state.clone();
            scan_block(func, block, &calls, &mut state, &mut |span, call| {
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
/// adds nothing, states that emptied the same builders join, and past [`PATH_STATES`] the states
/// of a block join into one, so the search ends.
fn path_states(
    func: &Function,
    calls: &FxHashMap<InstId, SmallVec<[ValueId; 1]>>,
) -> IndexVec<BlockId, Vec<(PathState, bool)>> {
    let mut entries = index_vec![Vec::<(PathState, bool)>::new(); func.blocks.len()];
    let mut worklist = VecDeque::from([BlockId::ENTRY]);
    entries[BlockId::ENTRY].push((PathState::default(), false));
    while let Some(block) = worklist.pop_front() {
        while let Some(index) = entries[block].iter().position(|(_, ran)| !ran) {
            entries[block][index].1 = true;
            let mut state = entries[block][index].0.clone();
            scan_block(func, block, calls, &mut state, &mut |_, _| {});
            for (successor, state) in successor_states(func, block, state) {
                if add_state(&mut entries[successor], state) {
                    worklist.push_back(successor);
                }
            }
        }
    }
    entries
}

/// Adds `state` to the states of a block, unless one there covers it; returns whether the block
/// has a new state to run.
///
/// A state that emptied the same builders as one already there joins it, so facts that change on
/// every lap of a loop, such as a counter's value, widen away instead of making new states, while
/// the relations every lap shares survive.
fn add_state(states: &mut Vec<(PathState, bool)>, state: PathState) -> bool {
    if states.iter().any(|(existing, _)| existing.covers(&state)) {
        return false;
    }
    if let Some((existing, ran)) = states.iter_mut().find(|(existing, _)| {
        existing.emptied.len() == state.emptied.len()
            && existing.emptied.keys().all(|value| state.emptied.contains_key(value))
    }) {
        *existing = existing.join(&state);
        *ran = false;
        return true;
    }
    states.retain(|(existing, _)| !state.covers(existing));
    states.push((state, false));
    if states.len() > PATH_STATES {
        let joined = states
            .iter()
            .skip(1)
            .fold(states[0].0.clone(), |joined, (state, _)| joined.join(state));
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
            InstKind::Bitcast(_)
            | InstKind::IntToPtr(_)
            | InstKind::PtrToInt(..)
            | InstKind::Select(..) => {
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
                }
                continue;
            }
            _ => {}
        }
        for operand in instruction.kind.operands() {
            if let Some(&call) = state.emptied.get(&operand) {
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
                state.emptied.entry(value).or_insert(inst);
            }
        }
    }
    if let Some(terminator) = &data.terminator {
        let span = data.terminator_metadata.source_span();
        terminator.visit_operands(|operand| {
            if let Some(&call) = state.emptied.get(&operand) {
                report(span, call);
            }
        });
    }
}

/// A value the facts relate: a constant, or an SSA value that is not a constant offset of another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Atom {
    Const,
    Value(ValueId),
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
    members: FxHashMap<Atom, (Atom, U256)>,
}

impl Facts {
    fn find(&self, atom: Atom) -> (Atom, U256) {
        self.members.get(&atom).copied().unwrap_or((atom, U256::ZERO))
    }

    /// `value` as its class's leader plus a constant.
    fn resolve(&self, func: &Function, value: ValueId) -> (Atom, U256) {
        let (atom, offset) = form(func, value);
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
            // Equal operands decide an ordering; others can wrap.
            InstKind::Lt(lhs, rhs) | InstKind::Gt(lhs, rhs) => self
                .difference(func, lhs, rhs)
                .filter(|difference| difference.is_zero())
                .map(|_| false),
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
        if !self.union(form(func, condition), (Atom::Const, value)) {
            return false;
        }
        let Value::Inst(inst) = *func.value(condition) else { return true };
        // A boolean that is not zero is one.
        let boolean_nonzero = |facts: &mut Self, operand: ValueId| {
            func.value_ty(operand) != Some(MirType::I1)
                || facts.union(form(func, operand), (Atom::Const, U256::from(1)))
        };
        match func.inst(inst).kind {
            InstKind::Eq(lhs, rhs) if holds => self.union(form(func, lhs), form(func, rhs)),
            InstKind::Ne(lhs, rhs) if !holds => self.union(form(func, lhs), form(func, rhs)),
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
        let mut met = Self::default();
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

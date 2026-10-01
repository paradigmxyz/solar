//! Intraprocedural worklist solver over the MIR control-flow graph.
//!
//! An [`Analysis`] supplies a domain, a boundary state, and transfer functions for
//! instructions, terminators, and CFG edges. [`solve`] computes the least fixed point of the
//! induced equations with a worklist ordered by reverse postorder (postorder for backward
//! problems), so acyclic regions converge in one sweep. Retreating edges identify loop
//! headers; after [`WIDEN_DELAY`] joins into a header the solver widens instead, which bounds
//! the iteration count for domains with infinite ascending chains such as intervals.
//!
//! Phis are not ordinary instructions in SSA dataflow: their meaning depends on the incoming
//! edge. The solver therefore skips them in the block body and calls [`Analysis::apply_phi`]
//! once per phi while propagating along each edge, after [`Analysis::apply_edge`] has refined
//! the edge state. Edge refinement is where path sensitivity lives: a forward analysis that
//! learns from `jumpi cond` can assume `cond` on the taken edge and its negation on the other,
//! and an analysis over [`Reachable`](super::lattice::Reachable) can mark an infeasible edge
//! unreachable so its target never sees that state.
//!
//! [`Results`] retain one state per block (the entry state for forward problems, the exit
//! state for backward ones). [`replay`] reconstructs per-instruction states on demand for
//! fact dumps and for clients that record events only once the fixed point is known.

use super::lattice::JoinSemiLattice;
use crate::mir::{BlockId, Function, InstId, InstKind, Terminator, ValueId, analysis::CfgInfo};
use smallvec::SmallVec;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec};
use std::collections::BTreeSet;

/// Number of joins into a loop header before the solver switches to widening.
pub(crate) const WIDEN_DELAY: u32 = 2;

/// The direction in which facts propagate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    /// From the function entry towards its exits.
    Forward,
    /// From the function exits towards its entry.
    Backward,
}

/// Branch facts that hold whenever control follows one CFG edge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EdgeCondition {
    /// The edge is taken unconditionally, or under several conditions that are not
    /// distinguished, such as both arms of a branch targeting the same block.
    Always,
    /// The branch `condition` evaluated to `taken`.
    Branch {
        /// The `i1` branch condition.
        condition: ValueId,
        /// Whether the condition was true on this edge.
        taken: bool,
    },
    /// A switch on `value` selected this edge.
    Switch {
        /// The switch scrutinee.
        value: ValueId,
        /// Case values that select this edge.
        cases: SmallVec<[ValueId; 2]>,
        /// Whether the default edge also targets this block.
        is_default: bool,
        /// Every case value of the switch, needed to describe the default edge.
        all_cases: SmallVec<[ValueId; 4]>,
    },
}

/// One logical CFG edge together with the branch facts that hold on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Edge {
    /// Source block.
    pub(crate) from: BlockId,
    /// Target block.
    pub(crate) to: BlockId,
    /// Facts implied by taking the edge.
    pub(crate) condition: EdgeCondition,
}

/// A program point at which a transfer function applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ProgramPoint {
    /// A non-terminator instruction, including phis.
    Instruction(BlockId, InstId),
    /// The terminator of a block.
    Terminator(BlockId),
}

/// A dataflow problem over one function's CFG.
pub(crate) trait Analysis {
    /// The abstract state at each program point.
    type Domain: JoinSemiLattice + PartialEq;

    /// The propagation direction.
    const DIRECTION: Direction = Direction::Forward;

    /// Returns the state that describes no execution.
    fn bottom(&self, func: &Function) -> Self::Domain;

    /// Initializes a boundary block: the entry for forward problems, each block without
    /// successors for backward problems.
    fn initialize_boundary(&mut self, func: &Function, block: BlockId, state: &mut Self::Domain);

    /// Applies one non-phi instruction.
    fn apply_instruction(
        &mut self,
        func: &Function,
        block: BlockId,
        inst: InstId,
        state: &mut Self::Domain,
    );

    /// Applies a block terminator.
    fn apply_terminator(&mut self, _func: &Function, _block: BlockId, _state: &mut Self::Domain) {}

    /// Refines the state that flows along `edge`.
    fn apply_edge(&mut self, _func: &Function, _edge: &Edge, _state: &mut Self::Domain) {}

    /// Binds one phi of `edge.to` to its incoming value on `edge`.
    /// Read incoming values from `source`, the snapshot before any phi on this edge is bound.
    fn apply_phi(
        &mut self,
        _func: &Function,
        _phi: InstId,
        _incoming: ValueId,
        _edge: &Edge,
        _source: &Self::Domain,
        _state: &mut Self::Domain,
    ) {
    }
}

/// Fixed-point states of one analysis, one per block.
#[derive(Clone, Debug)]
pub(crate) struct Results<D> {
    /// Forward problems store block entry states; backward problems store block exit states.
    states: IndexVec<BlockId, D>,
    /// Blocks the solver visited at least once.
    visited: DenseBitSet<BlockId>,
}

impl<D> Results<D> {
    /// Returns the stored state of `block`.
    pub(crate) fn state(&self, block: BlockId) -> &D {
        &self.states[block]
    }

    /// Returns whether the solver reached `block` with a non-bottom state.
    pub(crate) fn is_visited(&self, block: BlockId) -> bool {
        self.visited.contains(block)
    }
}

/// Collects the logical outgoing edges of `block`, merging duplicate targets.
pub(crate) fn outgoing_edges(func: &Function, block: BlockId) -> SmallVec<[Edge; 2]> {
    let mut edges = SmallVec::new();
    match &func.blocks[block].terminator {
        Some(Terminator::Jump(target)) => {
            edges.push(Edge { from: block, to: *target, condition: EdgeCondition::Always });
        }
        &Some(Terminator::Branch { condition, then_block, else_block }) => {
            if then_block == else_block {
                edges.push(Edge { from: block, to: then_block, condition: EdgeCondition::Always });
            } else {
                edges.push(Edge {
                    from: block,
                    to: then_block,
                    condition: EdgeCondition::Branch { condition, taken: true },
                });
                edges.push(Edge {
                    from: block,
                    to: else_block,
                    condition: EdgeCondition::Branch { condition, taken: false },
                });
            }
        }
        Some(Terminator::Switch { value, default, cases }) => {
            let all_cases = cases.iter().map(|&(case, _)| case).collect::<SmallVec<[_; 4]>>();
            let mut targets = SmallVec::<[BlockId; 4]>::new();
            for &(_, target) in cases {
                if !targets.contains(&target) {
                    targets.push(target);
                }
            }
            if !targets.contains(default) {
                targets.push(*default);
            }
            for target in targets {
                let matched = cases
                    .iter()
                    .filter(|&&(_, case_target)| case_target == target)
                    .map(|&(case, _)| case)
                    .collect();
                edges.push(Edge {
                    from: block,
                    to: target,
                    condition: EdgeCondition::Switch {
                        value: *value,
                        cases: matched,
                        is_default: target == *default,
                        all_cases: all_cases.clone(),
                    },
                });
            }
        }
        _ => {}
    }
    edges
}

/// Returns the leading phi instructions of `block`.
pub(crate) fn block_phis(func: &Function, block: BlockId) -> impl Iterator<Item = InstId> + '_ {
    func.blocks[block]
        .instructions
        .iter()
        .copied()
        .take_while(move |&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)))
}

/// Returns the incoming value of `phi` from `pred`, if present.
pub(crate) fn phi_incoming(func: &Function, phi: InstId, pred: BlockId) -> Option<ValueId> {
    let InstKind::Phi(incoming) = &func.inst(phi).kind else { return None };
    incoming.iter().find(|&&(block, _)| block == pred).map(|&(_, value)| value)
}

/// Computes the least fixed point of `analysis` over `func`.
pub(crate) fn solve<A: Analysis>(
    func: &Function,
    cfg: &CfgInfo,
    analysis: &mut A,
) -> Results<A::Domain> {
    match A::DIRECTION {
        Direction::Forward => solve_forward(func, cfg, analysis),
        Direction::Backward => solve_backward(func, cfg, analysis),
    }
}

fn rpo_positions(func: &Function, cfg: &CfgInfo) -> IndexVec<BlockId, usize> {
    let mut positions = IndexVec::from_vec(vec![usize::MAX; func.blocks.len()]);
    for (position, &block) in cfg.rpo().iter().enumerate() {
        positions[block] = position;
    }
    positions
}

fn solve_forward<A: Analysis>(
    func: &Function,
    cfg: &CfgInfo,
    analysis: &mut A,
) -> Results<A::Domain> {
    let positions = rpo_positions(func, cfg);
    let rpo = cfg.rpo();
    let mut states =
        IndexVec::from_vec((0..func.blocks.len()).map(|_| analysis.bottom(func)).collect());
    let mut visited = DenseBitSet::new_empty(func.blocks.len());
    let mut joins = IndexVec::from_vec(vec![0u32; func.blocks.len()]);
    let mut worklist = BTreeSet::new();
    analysis.initialize_boundary(func, BlockId::ENTRY, &mut states[BlockId::ENTRY]);
    worklist.insert(positions[BlockId::ENTRY]);

    while let Some(position) = worklist.pop_first() {
        let block = rpo[position];
        visited.insert(block);
        let mut state = states[block].clone();
        for &inst in &func.blocks[block].instructions {
            if !matches!(func.inst(inst).kind, InstKind::Phi(_)) {
                analysis.apply_instruction(func, block, inst, &mut state);
            }
        }
        analysis.apply_terminator(func, block, &mut state);
        for edge in outgoing_edges(func, block) {
            let mut edge_state = state.clone();
            analysis.apply_edge(func, &edge, &mut edge_state);
            let source = edge_state.clone();
            for phi in block_phis(func, edge.to) {
                if let Some(incoming) = phi_incoming(func, phi, block) {
                    analysis.apply_phi(func, phi, incoming, &edge, &source, &mut edge_state);
                }
            }
            let target = edge.to;
            let retreating = positions[target] <= position;
            let changed = if retreating && joins[target] >= WIDEN_DELAY {
                states[target].widen(&edge_state)
            } else {
                states[target].join(&edge_state)
            };
            if retreating {
                joins[target] += 1;
            }
            if changed || !visited.contains(target) {
                worklist.insert(positions[target]);
            }
        }
    }

    Results { states, visited }
}

fn solve_backward<A: Analysis>(
    func: &Function,
    cfg: &CfgInfo,
    analysis: &mut A,
) -> Results<A::Domain> {
    // Backward problems visit blocks in postorder, the reverse of RPO.
    let positions = rpo_positions(func, cfg);
    let rpo = cfg.rpo();
    let mut states =
        IndexVec::from_vec((0..func.blocks.len()).map(|_| analysis.bottom(func)).collect());
    let mut visited = DenseBitSet::new_empty(func.blocks.len());
    let mut joins = IndexVec::from_vec(vec![0u32; func.blocks.len()]);
    let mut worklist = BTreeSet::new();
    for &block in rpo {
        if outgoing_edges(func, block).is_empty() {
            analysis.initialize_boundary(func, block, &mut states[block]);
            worklist.insert(std::cmp::Reverse(positions[block]));
        }
    }

    while let Some(std::cmp::Reverse(position)) = worklist.pop_first() {
        let block = rpo[position];
        visited.insert(block);
        let mut state = states[block].clone();
        analysis.apply_terminator(func, block, &mut state);
        for &inst in func.blocks[block].instructions.iter().rev() {
            if !matches!(func.inst(inst).kind, InstKind::Phi(_)) {
                analysis.apply_instruction(func, block, inst, &mut state);
            }
        }
        for &pred in &func.blocks[block].predecessors {
            if positions[pred] == usize::MAX {
                continue;
            }
            let Some(edge) = outgoing_edges(func, pred).into_iter().find(|edge| edge.to == block)
            else {
                continue;
            };
            let mut edge_state = state.clone();
            for phi in block_phis(func, block) {
                if let Some(incoming) = phi_incoming(func, phi, pred) {
                    analysis.apply_phi(func, phi, incoming, &edge, &state, &mut edge_state);
                }
            }
            analysis.apply_edge(func, &edge, &mut edge_state);
            let retreating = positions[pred] >= position;
            let changed = if retreating && joins[pred] >= WIDEN_DELAY {
                states[pred].widen(&edge_state)
            } else {
                states[pred].join(&edge_state)
            };
            if retreating {
                joins[pred] += 1;
            }
            if changed || !visited.contains(pred) {
                worklist.insert(std::cmp::Reverse(positions[pred]));
            }
        }
    }

    Results { states, visited }
}

/// Refines a forward fixed point with `rounds` descending iterations.
///
/// Widening can overshoot, for example past a loop bound. Starting from a post-fixpoint,
/// recomputing every block entry from its predecessors without widening yields smaller states
/// that still over-approximate the least fixed point, because the transfer functions are
/// monotone.
pub(crate) fn narrow<A: Analysis>(
    func: &Function,
    cfg: &CfgInfo,
    analysis: &mut A,
    results: &mut Results<A::Domain>,
    rounds: usize,
) {
    debug_assert_eq!(A::DIRECTION, Direction::Forward, "narrowing refines forward problems only");
    for _ in 0..rounds {
        let mut changed = false;
        for &block in cfg.rpo() {
            if block == BlockId::ENTRY || !results.visited.contains(block) {
                continue;
            }
            let mut entry = analysis.bottom(func);
            for &pred in &func.blocks[block].predecessors {
                if !results.visited.contains(pred) {
                    continue;
                }
                let mut state = results.states[pred].clone();
                for &inst in &func.blocks[pred].instructions {
                    if !matches!(func.inst(inst).kind, InstKind::Phi(_)) {
                        analysis.apply_instruction(func, pred, inst, &mut state);
                    }
                }
                analysis.apply_terminator(func, pred, &mut state);
                for edge in outgoing_edges(func, pred).into_iter().filter(|edge| edge.to == block) {
                    let mut edge_state = state.clone();
                    analysis.apply_edge(func, &edge, &mut edge_state);
                    let source = edge_state.clone();
                    for phi in block_phis(func, block) {
                        if let Some(incoming) = phi_incoming(func, phi, pred) {
                            analysis.apply_phi(
                                func,
                                phi,
                                incoming,
                                &edge,
                                &source,
                                &mut edge_state,
                            );
                        }
                    }
                    entry.join(&edge_state);
                }
            }
            if results.states[block] != entry {
                results.states[block] = entry;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Replays a forward analysis over its fixed point in reverse postorder.
///
/// `visit` observes the state immediately before each program point and may record events
/// in the analysis. Phis are reported with the block entry state, which already binds them.
pub(crate) fn replay<A: Analysis>(
    func: &Function,
    cfg: &CfgInfo,
    analysis: &mut A,
    results: &Results<A::Domain>,
    mut visit: impl FnMut(&mut A, ProgramPoint, &A::Domain),
) {
    debug_assert_eq!(A::DIRECTION, Direction::Forward, "replay walks forward problems only");
    for &block in cfg.rpo() {
        if !results.is_visited(block) {
            continue;
        }
        let mut state = results.state(block).clone();
        for &inst in &func.blocks[block].instructions {
            visit(analysis, ProgramPoint::Instruction(block, inst), &state);
            if !matches!(func.inst(inst).kind, InstKind::Phi(_)) {
                analysis.apply_instruction(func, block, inst, &mut state);
            }
        }
        visit(analysis, ProgramPoint::Terminator(block), &state);
    }
}

/// Returns the state at the end of `block`, after its terminator's transfer.
pub(crate) fn block_exit_state<A: Analysis>(
    func: &Function,
    analysis: &mut A,
    results: &Results<A::Domain>,
    block: BlockId,
) -> A::Domain {
    let mut state = results.state(block).clone();
    for &inst in &func.blocks[block].instructions {
        if !matches!(func.inst(inst).kind, InstKind::Phi(_)) {
            analysis.apply_instruction(func, block, inst, &mut state);
        }
    }
    analysis.apply_terminator(func, block, &mut state);
    state
}

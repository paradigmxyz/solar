//! The test vectors of an MC/DC decision.
//!
//! The conditions of a decision form a graph, like a binary decision diagram: each condition, in
//! the order the source evaluates them, leads on to the next condition or ends the decision with
//! an outcome, depending on its own outcome. A path through the graph is one test vector, and
//! Ball–Larus numbering gives each path a number below the count of paths without a table of
//! them: one edge of a condition adds 0 and the other adds the paths of the first edge's target,
//! so the numbers along a path sum to its index. Only the edge that adds needs a probe.
//!
//! LLVM's regions give each condition the IDs of the conditions its outcomes lead to, or `-1`,
//! and take the value of the last condition evaluated as the outcome of the decision. So no `!`
//! may sit between a condition and the decision: the graph pushes each `!` down to the leaves,
//! as `!(x && y)` is `!x || !y`, `!(x || y)` is `!x && !y`, and `!!x` is `x`, which keeps the
//! order of evaluation and the short circuits. A leaf under an odd number of `!` is a negated
//! condition: it holds when its expression is false. The probe of a leaf sits where its
//! expression is true, so that edge is the one that adds: the true edge of a condition, and the
//! false edge of a negated one.
//!
//! A decision with more test vectors than [`MAX_VECTORS`] gets no MC/DC probes.

use solar_data_structures::map::FxHashMap;

/// The most test vectors of a decision with MC/DC probes.
pub(super) const MAX_VECTORS: usize = 256;

/// The shape of a decision, with the leaves keyed by their expressions.
pub(super) enum Cond {
    Leaf(usize),
    Not(Box<Self>),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
}

/// Where evaluation goes after a condition.
#[derive(Clone, Copy)]
enum Next {
    Cond(usize),
    Outcome(bool),
}

impl Next {
    /// Returns the condition ID LLVM's regions use, with `-1` for the end of the decision.
    fn id(self) -> i32 {
        match self {
            Self::Cond(cond) => cond as i32,
            Self::Outcome(_) => -1,
        }
    }
}

/// A condition, in the order of evaluation.
#[derive(Clone, Copy)]
struct Edges {
    leaf: usize,
    /// Whether the condition holds when its leaf is false.
    negated: bool,
    when_false: Next,
    when_true: Next,
    /// The paths that start at the target of the edge where the leaf is false, which the edge
    /// where it is true adds.
    weight: usize,
}

/// The graph of a decision.
pub(super) struct Graph {
    edges: Vec<Edges>,
    /// The test vectors, in the order of their indexes.
    pub(super) vectors: Vec<String>,
}

/// What a condition adds, and where it leads.
pub(super) struct Condition {
    pub(super) id: u32,
    pub(super) false_next: i32,
    pub(super) true_next: i32,
    /// The index the leaf adds when it is true.
    pub(super) weight: u32,
}

impl Graph {
    /// Returns the number of conditions.
    pub(super) fn conditions(&self) -> usize {
        self.edges.len()
    }

    /// Returns the conditions, each with its leaf.
    pub(super) fn iter(&self) -> impl Iterator<Item = (usize, Condition)> + '_ {
        self.edges.iter().enumerate().map(|(id, edges)| {
            let condition = Condition {
                id: id as u32,
                false_next: edges.when_false.id(),
                true_next: edges.when_true.id(),
                weight: edges.weight as u32,
            };
            (edges.leaf, condition)
        })
    }
}

/// Builds the graph of `root`, unless the decision has more than [`MAX_VECTORS`] test vectors.
pub(super) fn graph(root: &Cond) -> Option<Graph> {
    let mut leaves = Vec::new();
    collect(root, &mut leaves);
    let index = leaves.iter().enumerate().map(|(index, &leaf)| (leaf, index)).collect();
    let mut edges = vec![
        Edges {
            leaf: 0,
            negated: false,
            when_false: Next::Outcome(false),
            when_true: Next::Outcome(true),
            weight: 0
        };
        leaves.len()
    ];
    build(root, Next::Outcome(true), Next::Outcome(false), false, &index, &mut edges);

    // A condition leads only to later conditions, so number the paths from the last.
    let mut paths = vec![0; edges.len()];
    for cond in (0..edges.len()).rev() {
        let count = |next| match next {
            Next::Cond(next) => paths[next],
            Next::Outcome(_) => 1,
        };
        let Edges { negated, when_false, when_true, .. } = edges[cond];
        let (leaf_false, leaf_true) =
            if negated { (when_true, when_false) } else { (when_false, when_true) };
        let weight = count(leaf_false);
        paths[cond] = (weight + count(leaf_true)).min(MAX_VECTORS + 1);
        edges[cond].weight = weight;
    }
    if paths[0] > MAX_VECTORS {
        return None;
    }

    let mut vectors = vec![String::new(); paths[0]];
    let mut state = vec!['-'; edges.len()];
    vector(&edges, Next::Cond(0), 0, &mut state, &mut vectors);
    Some(Graph { edges, vectors })
}

fn collect(cond: &Cond, leaves: &mut Vec<usize>) {
    match cond {
        Cond::Leaf(leaf) => leaves.push(*leaf),
        Cond::Not(cond) => collect(cond, leaves),
        Cond::And(lhs, rhs) | Cond::Or(lhs, rhs) => {
            collect(lhs, leaves);
            collect(rhs, leaves);
        }
    }
}

/// Wires `cond`, whose evaluation goes on to `when_true` when it is true and to `when_false`
/// when it is false, under an odd number of `!` if `negated`, and returns where it starts.
fn build(
    cond: &Cond,
    when_true: Next,
    when_false: Next,
    negated: bool,
    index: &FxHashMap<usize, usize>,
    edges: &mut [Edges],
) -> Next {
    match cond {
        Cond::Leaf(leaf) => {
            let cond = index[leaf];
            let (when_true, when_false) =
                if negated { (when_false, when_true) } else { (when_true, when_false) };
            edges[cond] = Edges { leaf: *leaf, negated, when_false, when_true, weight: 0 };
            Next::Cond(cond)
        }
        Cond::Not(cond) => build(cond, when_false, when_true, !negated, index, edges),
        Cond::And(lhs, rhs) => {
            let rhs = build(rhs, when_true, when_false, negated, index, edges);
            build(lhs, rhs, when_false, negated, index, edges)
        }
        Cond::Or(lhs, rhs) => {
            let rhs = build(rhs, when_true, when_false, negated, index, edges);
            build(lhs, when_true, rhs, negated, index, edges)
        }
    }
}

/// Writes the vector of each path from `next`, whose index so far is `index`.
fn vector(
    edges: &[Edges],
    next: Next,
    index: usize,
    state: &mut Vec<char>,
    vectors: &mut Vec<String>,
) {
    match next {
        Next::Outcome(outcome) => {
            let mut vector = state.iter().collect::<String>();
            vector.push(if outcome { 'T' } else { 'F' });
            vectors[index] = vector;
        }
        Next::Cond(cond) => {
            let edge = edges[cond];
            let (true_weight, false_weight) =
                if edge.negated { (0, edge.weight) } else { (edge.weight, 0) };
            state[cond] = 'T';
            vector(edges, edge.when_true, index + true_weight, state, vectors);
            state[cond] = 'F';
            vector(edges, edge.when_false, index + false_weight, state, vectors);
            state[cond] = '-';
        }
    }
}

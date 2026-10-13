//! Directed graph algorithms.

use crate::index::{Idx, IndexVec};

/// Returns the strongly connected components of the directed graph with nodes `0..len` and the
/// given successors.
///
/// Components come in reverse topological order: no edge goes from a component to a later one.
///
/// This is Tarjan's algorithm with an explicit stack, so deep graphs cannot overflow the call
/// stack. `successors` is called once per node.
pub fn sccs<I: Idx, S: IntoIterator<Item = I>>(
    len: usize,
    mut successors: impl FnMut(I) -> S,
) -> Vec<Vec<I>> {
    const UNVISITED: u32 = u32::MAX;
    const DONE: u32 = u32::MAX - 1;

    // Visit order of each node while it is on `stack`; `UNVISITED` before and `DONE` after.
    let mut index = IndexVec::<I, u32>::from_vec(vec![UNVISITED; len]);
    let mut low = IndexVec::<I, u32>::from_vec(vec![0; len]);
    let mut next_index = 0;
    let mut stack = Vec::new();
    let mut walk = Vec::new();
    let mut sccs = Vec::new();
    for root in (0..len).map(I::from_usize) {
        if index[root] != UNVISITED {
            continue;
        }
        index[root] = next_index;
        low[root] = next_index;
        next_index += 1;
        stack.push(root);
        walk.push((root, successors(root).into_iter()));
        while let Some((node, succs)) = walk.last_mut() {
            let node = *node;
            if let Some(succ) = succs.next() {
                match index[succ] {
                    UNVISITED => {
                        index[succ] = next_index;
                        low[succ] = next_index;
                        next_index += 1;
                        stack.push(succ);
                        walk.push((succ, successors(succ).into_iter()));
                    }
                    DONE => {}
                    succ_index => low[node] = low[node].min(succ_index),
                }
                continue;
            }

            walk.pop();
            if let Some(&(parent, _)) = walk.last() {
                low[parent] = low[parent].min(low[node]);
            }
            if low[node] == index[node] {
                let start = stack.iter().rposition(|&n| n == node).unwrap();
                let scc = stack.split_off(start);
                for &n in &scc {
                    index[n] = DONE;
                }
                sccs.push(scc);
            }
        }
    }
    sccs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::define_nonmax_u32_index_type;

    define_nonmax_u32_index_type! {
        struct Node;
    }

    fn components(len: usize, edges: impl Fn(usize) -> Vec<usize>) -> Vec<Vec<usize>> {
        let sccs = sccs(len, |n: Node| edges(n.index()).into_iter().map(Node::from_usize));
        sccs.into_iter().map(|scc| scc.into_iter().map(Node::index).collect()).collect()
    }

    #[test]
    fn reverse_topological_order() {
        // 0 -> 1 <-> 2 -> 3 -> 3, 4 -> 0
        let edges = [vec![1], vec![2], vec![1, 3], vec![3], vec![0]];
        let sccs = components(edges.len(), |n| edges[n].clone());
        assert_eq!(sccs, [vec![3], vec![1, 2], vec![0], vec![4]]);
    }

    #[test]
    fn deep_chain() {
        let len = 1_000_000;
        let sccs = components(len, |n| if n + 1 < len { vec![n + 1] } else { vec![] });
        assert_eq!(sccs.len(), len);
        assert_eq!(sccs[0], [len - 1]);
    }
}

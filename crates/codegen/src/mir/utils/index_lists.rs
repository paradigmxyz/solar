//! Per-index lists stored contiguously, for adjacency such as predecessors or value users.
//!
//! One flat allocation replaces a vector per index. Build from `(index, item)` pairs with a
//! counting sort, which keeps each index's items in pair order.

use solar_data_structures::index::Idx;
use std::marker::PhantomData;

/// Per-index lists stored contiguously in index order.
#[derive(Clone, Debug)]
pub(crate) struct IndexLists<I, T> {
    /// The list of index `i` is `items[starts[i]..starts[i + 1]]`.
    starts: Vec<u32>,
    items: Vec<T>,
    marker: PhantomData<I>,
}

impl<I: Idx, T: Copy> IndexLists<I, T> {
    /// Groups `(index, item)` pairs over `len` indices, keeping each index's items in pair order.
    pub(crate) fn new<P: Iterator<Item = (I, T)>>(len: usize, pairs: impl Fn() -> P) -> Self {
        let mut starts = vec![0u32; len + 1];
        for (index, _) in pairs() {
            starts[index.index() + 1] += 1;
        }
        for index in 1..=len {
            starts[index] += starts[index - 1];
        }
        let Some((_, filler)) = pairs().next() else {
            return Self { starts, items: Vec::new(), marker: PhantomData };
        };
        // Each start advances to the next index's start while filling, then shifts back.
        let mut items = vec![filler; starts[len] as usize];
        for (index, item) in pairs() {
            let slot = &mut starts[index.index()];
            items[*slot as usize] = item;
            *slot += 1;
        }
        starts.copy_within(..len, 1);
        starts[0] = 0;
        Self { starts, items, marker: PhantomData }
    }

    /// Returns the number of indices.
    pub(crate) fn len(&self) -> usize {
        self.starts.len() - 1
    }

    /// Returns the list of `index`, or an empty list outside the domain.
    pub(crate) fn get(&self, index: I) -> &[T] {
        match self.starts.get(index.index()..index.index() + 2) {
            Some(&[start, end]) => &self.items[start as usize..end as usize],
            _ => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::BlockId;

    #[test]
    fn groups_in_pair_order() {
        let b = BlockId::from_usize;
        let pairs = [(b(2), 10), (b(0), 11), (b(2), 12), (b(0), 13)];
        let lists = IndexLists::new(4, || pairs.iter().copied());
        assert_eq!(lists.len(), 4);
        assert_eq!(lists.get(b(0)), [11, 13]);
        assert!(lists.get(b(1)).is_empty());
        assert_eq!(lists.get(b(2)), [10, 12]);
        assert!(lists.get(b(3)).is_empty());
        assert!(lists.get(b(4)).is_empty());
        assert!(IndexLists::new(2, std::iter::empty::<(BlockId, u8)>).get(b(1)).is_empty());
    }
}

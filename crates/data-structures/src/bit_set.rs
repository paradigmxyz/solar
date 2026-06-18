//! Indexed bitsets.
//!
//! Adapted from `rustc_index::bit_set`.

use std::{
    fmt, iter,
    marker::PhantomData,
    ops::{Bound, Range, RangeBounds},
    slice,
};

use crate::index::Idx;

type Word = u64;
const WORD_BYTES: usize = size_of::<Word>();
const WORD_BITS: usize = WORD_BYTES * 8;

/// Bitwise set relations.
pub trait BitRelations<Rhs> {
    /// Sets `self = self | other`, returning true if this set changed.
    fn union(&mut self, other: &Rhs) -> bool;

    /// Sets `self = self - other`, returning true if this set changed.
    fn subtract(&mut self, other: &Rhs) -> bool;

    /// Sets `self = self & other`, returning true if this set changed.
    fn intersect(&mut self, other: &Rhs) -> bool;
}

#[inline]
fn inclusive_start_end<T: Idx>(
    range: impl RangeBounds<T>,
    domain: usize,
) -> Option<(usize, usize)> {
    let start = match range.start_bound().cloned() {
        Bound::Included(start) => start.index(),
        Bound::Excluded(start) => start.index() + 1,
        Bound::Unbounded => 0,
    };
    let end = match range.end_bound().cloned() {
        Bound::Included(end) => end.index(),
        Bound::Excluded(end) => end.index().checked_sub(1)?,
        Bound::Unbounded => domain - 1,
    };
    assert!(end < domain);
    if start > end {
        return None;
    }
    Some((start, end))
}

macro_rules! bit_relations_inherent_impls {
    () => {
        /// Sets `self = self | other`, returning true if this set changed.
        pub fn union<Rhs>(&mut self, other: &Rhs) -> bool
        where
            Self: BitRelations<Rhs>,
        {
            <Self as BitRelations<Rhs>>::union(self, other)
        }

        /// Sets `self = self - other`, returning true if this set changed.
        pub fn subtract<Rhs>(&mut self, other: &Rhs) -> bool
        where
            Self: BitRelations<Rhs>,
        {
            <Self as BitRelations<Rhs>>::subtract(self, other)
        }

        /// Sets `self = self & other`, returning true if this set changed.
        pub fn intersect<Rhs>(&mut self, other: &Rhs) -> bool
        where
            Self: BitRelations<Rhs>,
        {
            <Self as BitRelations<Rhs>>::intersect(self, other)
        }
    };
}

/// A fixed-size bitset type with a dense representation.
///
/// `T` is an index type. All operations that involve an element panic if the
/// element is equal to or greater than the domain size. Operations that involve
/// two bitsets panic if their domain sizes differ.
#[derive(Eq, PartialEq, Hash)]
pub struct DenseBitSet<T> {
    domain_size: usize,
    words: Vec<Word>,
    marker: PhantomData<T>,
}

impl<T> DenseBitSet<T> {
    /// Gets the domain size.
    #[inline]
    pub const fn domain_size(&self) -> usize {
        self.domain_size
    }
}

impl<T: Idx> DenseBitSet<T> {
    /// Creates a new, empty bitset with the given domain size.
    #[inline]
    pub fn new_empty(domain_size: usize) -> Self {
        Self { domain_size, words: vec![0; num_words(domain_size)], marker: PhantomData }
    }

    /// Creates a new, filled bitset with the given domain size.
    #[inline]
    pub fn new_filled(domain_size: usize) -> Self {
        let mut result =
            Self { domain_size, words: vec![!0; num_words(domain_size)], marker: PhantomData };
        result.clear_excess_bits();
        result
    }

    /// Clears all elements.
    #[inline]
    pub fn clear(&mut self) {
        self.words.fill(0);
    }

    fn clear_excess_bits(&mut self) {
        clear_excess_bits_in_final_word(self.domain_size, &mut self.words);
    }

    /// Counts the number of set bits.
    #[inline]
    pub fn count(&self) -> usize {
        count_ones(&self.words)
    }

    /// Returns true if the set contains `elem`.
    #[inline]
    pub fn contains(&self, elem: T) -> bool {
        assert!(elem.index() < self.domain_size);
        let (word_index, mask) = word_index_and_mask(elem);
        (self.words[word_index] & mask) != 0
    }

    /// Returns true if this set is a non-strict superset of `other`.
    #[inline]
    pub fn superset(&self, other: &Self) -> bool {
        assert_eq!(self.domain_size, other.domain_size);
        self.words.iter().zip(&other.words).all(|(a, b)| (a & b) == *b)
    }

    /// Returns true if the set is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|word| *word == 0)
    }

    /// Inserts `elem`, returning true if this set changed.
    #[inline]
    pub fn insert(&mut self, elem: T) -> bool {
        assert!(
            elem.index() < self.domain_size,
            "inserting element at index {} but domain size is {}",
            elem.index(),
            self.domain_size,
        );
        let (word_index, mask) = word_index_and_mask(elem);
        let word = self.words[word_index];
        let new_word = word | mask;
        self.words[word_index] = new_word;
        new_word != word
    }

    /// Inserts each element in `elems`.
    #[inline]
    pub fn insert_range(&mut self, elems: impl RangeBounds<T>) {
        let Some((start, end)) = inclusive_start_end(elems, self.domain_size) else {
            return;
        };

        let (start_word_index, start_mask) = word_index_and_mask_usize(start);
        let (end_word_index, end_mask) = word_index_and_mask_usize(end);

        for word_index in (start_word_index + 1)..end_word_index {
            self.words[word_index] = !0;
        }

        if start_word_index != end_word_index {
            self.words[start_word_index] |= !(start_mask - 1);
            self.words[end_word_index] |= end_mask | (end_mask - 1);
        } else {
            self.words[start_word_index] |= end_mask | (end_mask - start_mask);
        }
    }

    /// Inserts all elements in the domain.
    pub fn insert_all(&mut self) {
        self.words.fill(!0);
        self.clear_excess_bits();
    }

    /// Returns true if any bit in `elems` is set.
    #[inline]
    pub fn contains_any(&self, elems: impl RangeBounds<T>) -> bool {
        let Some((start, end)) = inclusive_start_end(elems, self.domain_size) else {
            return false;
        };
        let (start_word_index, start_mask) = word_index_and_mask_usize(start);
        let (end_word_index, end_mask) = word_index_and_mask_usize(end);

        if start_word_index == end_word_index {
            self.words[start_word_index] & (end_mask | (end_mask - start_mask)) != 0
        } else if self.words[start_word_index] & !(start_mask - 1) != 0 {
            true
        } else {
            let remaining = start_word_index + 1..end_word_index;
            remaining.start <= remaining.end
                && (self.words[remaining].iter().any(|&word| word != 0)
                    || self.words[end_word_index] & (end_mask | (end_mask - 1)) != 0)
        }
    }

    /// Removes `elem`, returning true if this set changed.
    #[inline]
    pub fn remove(&mut self, elem: T) -> bool {
        assert!(elem.index() < self.domain_size);
        let (word_index, mask) = word_index_and_mask(elem);
        let word = self.words[word_index];
        let new_word = word & !mask;
        self.words[word_index] = new_word;
        new_word != word
    }

    /// Iterates over the indices of set bits in sorted order.
    #[inline]
    pub fn iter(&self) -> BitIter<'_, T> {
        BitIter::new(&self.words)
    }

    /// Returns the last set bit in `range`.
    pub fn last_set_in(&self, range: impl RangeBounds<T>) -> Option<T> {
        let (start, end) = inclusive_start_end(range, self.domain_size)?;
        let (start_word_index, _) = word_index_and_mask_usize(start);
        let (end_word_index, end_mask) = word_index_and_mask_usize(end);

        let end_word = self.words[end_word_index] & (end_mask | (end_mask - 1));
        if end_word != 0 {
            let pos = max_bit(end_word) + WORD_BITS * end_word_index;
            if start <= pos {
                return Some(T::from_usize(pos));
            }
        }

        if let Some(offset) =
            self.words[start_word_index..end_word_index].iter().rposition(|&word| word != 0)
        {
            let word_idx = start_word_index + offset;
            let pos = max_bit(self.words[word_idx]) + WORD_BITS * word_idx;
            if start <= pos {
                return Some(T::from_usize(pos));
            }
        }

        None
    }

    bit_relations_inherent_impls! {}

    /// Sets `self = self | !other`.
    pub fn union_not(&mut self, other: &Self) {
        assert_eq!(self.domain_size, other.domain_size);
        update_words(&mut self.words, &other.words, |a, b| a | !b);
        self.clear_excess_bits();
    }
}

impl<T: Idx> BitRelations<Self> for DenseBitSet<T> {
    #[inline]
    fn union(&mut self, other: &Self) -> bool {
        assert_eq!(self.domain_size, other.domain_size);
        update_words(&mut self.words, &other.words, |a, b| a | b)
    }

    #[inline]
    fn subtract(&mut self, other: &Self) -> bool {
        assert_eq!(self.domain_size, other.domain_size);
        update_words(&mut self.words, &other.words, |a, b| a & !b)
    }

    #[inline]
    fn intersect(&mut self, other: &Self) -> bool {
        assert_eq!(self.domain_size, other.domain_size);
        update_words(&mut self.words, &other.words, |a, b| a & b)
    }
}

impl<T: Idx> From<GrowableBitSet<T>> for DenseBitSet<T> {
    fn from(bit_set: GrowableBitSet<T>) -> Self {
        bit_set.bit_set
    }
}

impl<T> Clone for DenseBitSet<T> {
    fn clone(&self) -> Self {
        Self { domain_size: self.domain_size, words: self.words.clone(), marker: PhantomData }
    }

    fn clone_from(&mut self, from: &Self) {
        self.domain_size = from.domain_size;
        self.words.clone_from(&from.words);
    }
}

impl<T: Idx> fmt::Debug for DenseBitSet<T> {
    fn fmt(&self, w: &mut fmt::Formatter<'_>) -> fmt::Result {
        w.debug_list().entries(self.iter()).finish()
    }
}

/// Iterator over the set bits in a bitset.
pub struct BitIter<'a, T: Idx> {
    word: Word,
    offset: usize,
    iter: slice::Iter<'a, Word>,
    marker: PhantomData<T>,
}

impl<'a, T: Idx> BitIter<'a, T> {
    #[inline]
    fn new(words: &'a [Word]) -> Self {
        Self {
            word: 0,
            offset: usize::MAX - (WORD_BITS - 1),
            iter: words.iter(),
            marker: PhantomData,
        }
    }
}

impl<T: Idx> Iterator for BitIter<'_, T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        loop {
            if self.word != 0 {
                let bit_pos = self.word.trailing_zeros() as usize;
                self.word ^= 1 << bit_pos;
                return Some(T::from_usize(bit_pos + self.offset));
            }

            self.word = *self.iter.next()?;
            self.offset = self.offset.wrapping_add(WORD_BITS);
        }
    }
}

/// A resizable bitset type with a dense representation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GrowableBitSet<T: Idx> {
    bit_set: DenseBitSet<T>,
}

impl<T: Idx> Default for GrowableBitSet<T> {
    fn default() -> Self {
        Self::new_empty()
    }
}

impl<T: Idx> GrowableBitSet<T> {
    /// Ensures that the set can hold at least `min_domain_size` elements.
    pub fn ensure(&mut self, min_domain_size: usize) {
        if self.bit_set.domain_size < min_domain_size {
            self.bit_set.domain_size = min_domain_size;
        }

        let min_num_words = num_words(min_domain_size);
        if self.bit_set.words.len() < min_num_words {
            self.bit_set.words.resize(min_num_words, 0);
        }
    }

    /// Creates a new, empty growable bitset.
    pub fn new_empty() -> Self {
        Self { bit_set: DenseBitSet::new_empty(0) }
    }

    /// Creates a new, empty growable bitset with the given capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self { bit_set: DenseBitSet::new_empty(capacity) }
    }

    /// Inserts `elem`, returning true if this set changed.
    #[inline]
    pub fn insert(&mut self, elem: T) -> bool {
        self.ensure(elem.index() + 1);
        self.bit_set.insert(elem)
    }

    /// Inserts each element in `elems`.
    #[inline]
    pub fn insert_range(&mut self, elems: Range<T>) {
        self.ensure(elems.end.index());
        self.bit_set.insert_range(elems);
    }

    /// Removes `elem`, returning true if this set changed.
    #[inline]
    pub fn remove(&mut self, elem: T) -> bool {
        self.ensure(elem.index() + 1);
        self.bit_set.remove(elem)
    }

    /// Clears all elements.
    #[inline]
    pub fn clear(&mut self) {
        self.bit_set.clear();
    }

    /// Counts the number of set bits.
    #[inline]
    pub fn count(&self) -> usize {
        self.bit_set.count()
    }

    /// Returns true if the set is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.bit_set.is_empty()
    }

    /// Returns true if the set contains `elem`.
    #[inline]
    pub fn contains(&self, elem: T) -> bool {
        let (word_index, mask) = word_index_and_mask(elem);
        self.bit_set.words.get(word_index).is_some_and(|word| (word & mask) != 0)
    }

    /// Returns true if any bit in `elems` is set.
    #[inline]
    pub fn contains_any(&self, elems: Range<T>) -> bool {
        elems.start.index() < self.bit_set.domain_size
            && self.bit_set.contains_any(
                elems.start..T::from_usize(elems.end.index().min(self.bit_set.domain_size)),
            )
    }

    /// Iterates over the indices of set bits in sorted order.
    #[inline]
    pub fn iter(&self) -> BitIter<'_, T> {
        self.bit_set.iter()
    }

    /// Counts the number of set bits.
    #[inline]
    pub fn len(&self) -> usize {
        self.bit_set.count()
    }

    bit_relations_inherent_impls! {}
}

impl<T: Idx> From<DenseBitSet<T>> for GrowableBitSet<T> {
    fn from(bit_set: DenseBitSet<T>) -> Self {
        Self { bit_set }
    }
}

impl<T: Idx> BitRelations<Self> for GrowableBitSet<T> {
    #[inline]
    fn union(&mut self, other: &Self) -> bool {
        self.ensure(other.bit_set.domain_size);
        update_words(&mut self.bit_set.words, &other.bit_set.words, |a, b| a | b)
    }

    #[inline]
    fn subtract(&mut self, other: &Self) -> bool {
        let len = self.bit_set.words.len().min(other.bit_set.words.len());
        update_words(&mut self.bit_set.words[..len], &other.bit_set.words[..len], |a, b| a & !b)
    }

    #[inline]
    fn intersect(&mut self, other: &Self) -> bool {
        let len = self.bit_set.words.len().min(other.bit_set.words.len());
        let changed =
            update_words(&mut self.bit_set.words[..len], &other.bit_set.words[..len], |a, b| a & b);
        let truncated = self.bit_set.words[len..].iter().any(|word| *word != 0);
        self.bit_set.words[len..].fill(0);
        changed || truncated
    }
}

#[inline]
fn update_words<Op>(lhs: &mut [Word], rhs: &[Word], op: Op) -> bool
where
    Op: Fn(Word, Word) -> Word,
{
    assert_eq!(lhs.len(), rhs.len());
    let mut changed = 0;
    for (lhs_slot, &rhs_val) in iter::zip(lhs, rhs) {
        let old_val = *lhs_slot;
        let new_val = op(old_val, rhs_val);
        *lhs_slot = new_val;
        changed |= old_val ^ new_val;
    }
    changed != 0
}

#[inline]
fn num_words(domain_size: usize) -> usize {
    domain_size.div_ceil(WORD_BITS)
}

#[inline]
fn word_index_and_mask<T: Idx>(elem: T) -> (usize, Word) {
    word_index_and_mask_usize(elem.index())
}

#[inline]
fn word_index_and_mask_usize(elem: usize) -> (usize, Word) {
    let word_index = elem / WORD_BITS;
    let mask = 1 << (elem % WORD_BITS);
    (word_index, mask)
}

fn clear_excess_bits_in_final_word(domain_size: usize, words: &mut [Word]) {
    let num_bits_in_final_word = domain_size % WORD_BITS;
    if num_bits_in_final_word > 0 {
        let mask = (1 << num_bits_in_final_word) - 1;
        words[words.len() - 1] &= mask;
    }
}

#[inline]
fn max_bit(word: Word) -> usize {
    WORD_BITS - 1 - word.leading_zeros() as usize
}

#[inline]
fn count_ones(words: &[Word]) -> usize {
    words.iter().map(|word| word.count_ones() as usize).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::newtype_index;

    newtype_index! {
        struct TestIdx;
    }

    #[test]
    fn dense_insert_remove_iter() {
        let _ = TestIdx::MAX;
        let mut set = DenseBitSet::<TestIdx>::new_empty(130);
        for idx in [0, 1, 63, 64, 129] {
            assert!(set.insert(TestIdx::from_usize(idx)));
        }
        assert!(!set.insert(TestIdx::from_usize(64)));
        assert!(set.contains(TestIdx::from_usize(129)));
        assert_eq!(set.count(), 5);

        assert!(set.remove(TestIdx::from_usize(1)));
        assert!(!set.contains(TestIdx::from_usize(1)));

        let values: Vec<_> = set.iter().map(Idx::index).collect();
        assert_eq!(values, [0, 63, 64, 129]);
    }

    #[test]
    fn dense_relations() {
        let mut left = DenseBitSet::<TestIdx>::new_empty(100);
        let mut right = DenseBitSet::<TestIdx>::new_empty(100);
        left.insert(TestIdx::from_usize(1));
        left.insert(TestIdx::from_usize(3));
        right.insert(TestIdx::from_usize(3));
        right.insert(TestIdx::from_usize(5));

        assert!(left.union(&right));
        assert!(!left.union(&right));
        assert_eq!(left.iter().map(Idx::index).collect::<Vec<_>>(), [1, 3, 5]);

        assert!(left.subtract(&right));
        assert_eq!(left.iter().map(Idx::index).collect::<Vec<_>>(), [1]);
    }

    #[test]
    fn growable_extends_on_insert() {
        let mut set = GrowableBitSet::<TestIdx>::new_empty();
        assert!(set.insert(TestIdx::from_usize(250)));
        assert!(set.contains(TestIdx::from_usize(250)));
        assert!(!set.contains(TestIdx::from_usize(249)));
        assert_eq!(set.count(), 1);
    }
}

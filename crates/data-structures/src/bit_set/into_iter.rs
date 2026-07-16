use super::{BitIter, DenseBitSet, GrowableBitSet, MixedBitIter, MixedBitSet};
use crate::index::Idx;

impl<'a, T: Idx> IntoIterator for &'a DenseBitSet<T> {
    type Item = T;
    type IntoIter = BitIter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T: Idx> IntoIterator for &'a MixedBitSet<T> {
    type Item = T;
    type IntoIter = MixedBitIter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T: Idx> IntoIterator for &'a GrowableBitSet<T> {
    type Item = T;
    type IntoIter = BitIter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

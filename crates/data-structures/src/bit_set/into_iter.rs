use super::{
    BitIter, BitSetIndex, ChunkedBitIter, ChunkedBitSet, DenseBitSet, GrowableBitSet, MixedBitIter,
    MixedBitSet,
};

impl<'a, T: BitSetIndex> IntoIterator for &'a DenseBitSet<T> {
    type Item = T;
    type IntoIter = BitIter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T: BitSetIndex> IntoIterator for &'a ChunkedBitSet<T> {
    type Item = T;
    type IntoIter = ChunkedBitIter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T: BitSetIndex> IntoIterator for &'a MixedBitSet<T> {
    type Item = T;
    type IntoIter = MixedBitIter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T: BitSetIndex> IntoIterator for &'a GrowableBitSet<T> {
    type Item = T;
    type IntoIter = BitIter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

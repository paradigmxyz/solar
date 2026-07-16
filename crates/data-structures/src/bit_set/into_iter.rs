use super::{
    BitIter, BitSetIndex, CHUNK_BITS, Chunk, ChunkedBitIter, ChunkedBitSet, DenseBitSet,
    GrowableBitSet, MixedBitIter, MixedBitSet, WORD_BITS, Word, num_words,
};
use std::{marker::PhantomData, ops::Range, rc::Rc, vec};

/// A consuming iterator over a [`DenseBitSet`].
pub struct DenseBitIntoIter<T> {
    word: Word,
    offset: usize,
    iter: vec::IntoIter<Word>,
    marker: PhantomData<T>,
}

impl<T: BitSetIndex> DenseBitIntoIter<T> {
    fn new(words: Vec<Word>) -> Self {
        Self {
            word: 0,
            offset: usize::MAX - (WORD_BITS - 1),
            iter: words.into_iter(),
            marker: PhantomData,
        }
    }
}

impl<T: BitSetIndex> Iterator for DenseBitIntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.word != 0 {
                let bit_pos = self.word.trailing_zeros() as usize;
                self.word ^= 1 << bit_pos;
                return Some(T::from_usize(bit_pos + self.offset));
            }
            self.word = self.iter.next()?;
            self.offset = self.offset.wrapping_add(WORD_BITS);
        }
    }
}

/// A consuming iterator over a [`ChunkedBitSet`].
pub struct ChunkedBitIntoIter<T> {
    chunks: vec::IntoIter<Chunk>,
    chunk_offset: usize,
    next_chunk_offset: usize,
    chunk_iter: ChunkIntoIter,
    marker: PhantomData<T>,
}

impl<T: BitSetIndex> ChunkedBitIntoIter<T> {
    fn new(chunks: Box<[Chunk]>) -> Self {
        Self {
            chunks: chunks.into_vec().into_iter(),
            chunk_offset: 0,
            next_chunk_offset: 0,
            chunk_iter: ChunkIntoIter::Zeros,
            marker: PhantomData,
        }
    }
}

impl<T: BitSetIndex> Iterator for ChunkedBitIntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(index) = self.chunk_iter.next() {
                return Some(T::from_usize(self.chunk_offset + index));
            }
            let chunk = self.chunks.next()?;
            self.chunk_offset = self.next_chunk_offset;
            self.next_chunk_offset += CHUNK_BITS;
            self.chunk_iter = ChunkIntoIter::new(chunk);
        }
    }
}

enum ChunkIntoIter {
    Zeros,
    Ones(Range<usize>),
    Mixed {
        words: Rc<[Word; super::CHUNK_WORDS]>,
        num_words: usize,
        word_index: usize,
        word: Word,
        offset: usize,
    },
}

impl ChunkIntoIter {
    fn new(chunk: Chunk) -> Self {
        match chunk {
            Chunk::Zeros { .. } => Self::Zeros,
            Chunk::Ones { chunk_domain_size } => Self::Ones(0..usize::from(chunk_domain_size)),
            Chunk::Mixed { chunk_domain_size, words, .. } => Self::Mixed {
                words,
                num_words: num_words(usize::from(chunk_domain_size)),
                word_index: 0,
                word: 0,
                offset: 0,
            },
        }
    }
}

impl Iterator for ChunkIntoIter {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Zeros => None,
            Self::Ones(iter) => iter.next(),
            Self::Mixed { words, num_words, word_index, word, offset } => loop {
                if *word != 0 {
                    let bit_pos = word.trailing_zeros() as usize;
                    *word ^= 1 << bit_pos;
                    return Some(*offset + bit_pos);
                }
                if *word_index == *num_words {
                    return None;
                }
                *offset = *word_index * WORD_BITS;
                *word = words[*word_index];
                *word_index += 1;
            },
        }
    }
}

/// A consuming iterator over a [`MixedBitSet`].
pub enum MixedBitIntoIter<T: BitSetIndex> {
    /// Iteration over the dense representation.
    Small(DenseBitIntoIter<T>),
    /// Iteration over the chunked representation.
    Large(ChunkedBitIntoIter<T>),
}

impl<T: BitSetIndex> Iterator for MixedBitIntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Small(iter) => iter.next(),
            Self::Large(iter) => iter.next(),
        }
    }
}

impl<'a, T: BitSetIndex> IntoIterator for &'a DenseBitSet<T> {
    type Item = T;
    type IntoIter = BitIter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T: BitSetIndex> IntoIterator for DenseBitSet<T> {
    type Item = T;
    type IntoIter = DenseBitIntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        DenseBitIntoIter::new(self.words)
    }
}

impl<'a, T: BitSetIndex> IntoIterator for &'a ChunkedBitSet<T> {
    type Item = T;
    type IntoIter = ChunkedBitIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T: BitSetIndex> IntoIterator for ChunkedBitSet<T> {
    type Item = T;
    type IntoIter = ChunkedBitIntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        ChunkedBitIntoIter::new(self.chunks)
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

impl<T: BitSetIndex> IntoIterator for MixedBitSet<T> {
    type Item = T;
    type IntoIter = MixedBitIntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            Self::Small(set) => MixedBitIntoIter::Small(set.into_iter()),
            Self::Large(set) => MixedBitIntoIter::Large(set.into_iter()),
        }
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

impl<T: BitSetIndex> IntoIterator for GrowableBitSet<T> {
    type Item = T;
    type IntoIter = DenseBitIntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.bit_set.into_iter()
    }
}

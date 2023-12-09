use crate::BytePos;
use std::cmp;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    lo: BytePos,
    hi: BytePos,
}

impl Default for Span {
    #[inline(always)]
    fn default() -> Self {
        Self::DUMMY
    }
}

impl Span {
    pub const DUMMY: Self = Self { lo: BytePos(0), hi: BytePos(0) };

    #[inline]
    pub fn new(mut lo: BytePos, mut hi: BytePos) -> Self {
        if lo > hi {
            std::mem::swap(&mut lo, &mut hi);
        }
        Self { lo, hi }
    }

    #[inline(always)]
    pub fn lo(self) -> BytePos {
        self.lo
    }

    #[inline]
    pub fn with_lo(self, lo: BytePos) -> Self {
        Self { lo, hi: self.hi }
    }

    #[inline(always)]
    pub fn hi(self) -> BytePos {
        self.hi
    }

    #[inline]
    pub fn with_hi(self, hi: BytePos) -> Self {
        Self { lo: self.lo(), hi }
    }

    /// Returns a new span representing an empty span at the beginning of this span.
    #[inline]
    pub fn shrink_to_lo(self) -> Self {
        Self { lo: self.lo(), hi: self.lo() }
    }

    /// Returns a new span representing an empty span at the end of this span.
    #[inline]
    pub fn shrink_to_hi(self) -> Self {
        Self { lo: self.hi(), hi: self.hi() }
    }

    /// Returns `true` if this is a dummy span.
    #[inline]
    pub fn is_dummy(self) -> bool {
        self.lo().0 == 0 && self.hi().0 == 0
    }

    /// Returns `true` if `self` fully encloses `other`.
    #[inline]
    pub fn contains(self, other: Self) -> bool {
        self.lo() <= other.lo() && other.hi() <= self.hi()
    }

    /// Returns `true` if `self` touches `other`.
    #[inline]
    pub fn overlaps(self, other: Self) -> bool {
        self.lo() < other.hi() && other.lo() < self.hi()
    }

    /// Returns `true` if `self` and `other` are equal.
    #[inline]
    pub fn is_empty(self, other: Self) -> bool {
        self.lo() == other.lo() && self.hi() == other.hi()
    }

    /// Splits a span into two composite spans around a certain position.
    #[inline]
    pub fn split_at(self, pos: u32) -> (Self, Self) {
        let len = self.hi().0 - self.lo().0;
        debug_assert!(pos <= len);

        let split_pos = BytePos(self.lo().0 + pos);
        (Self::new(self.lo(), split_pos), Self::new(split_pos, self.hi()))
    }

    /// Returns a `Span` that would enclose both `self` and `end`.
    ///
    /// Note that this can also be used to extend the span "backwards":
    /// `start.to(end)` and `end.to(start)` return the same `Span`.
    ///
    /// ```text
    ///     ____             ___
    ///     self lorem ipsum end
    ///     ^^^^^^^^^^^^^^^^^^^^
    /// ```
    pub fn to(self, end: Self) -> Self {
        Self::new(
            cmp::min(self.lo(), end.lo()),
            cmp::max(self.hi(), end.hi()),
            // if span_data.ctxt == SyntaxContext::root() { end_data.ctxt } else { span_data.ctxt
            // }, if span_data.parent == end_data.parent { span_data.parent } else {
            // None },
        )
    }

    /// Returns a `Span` between the end of `self` to the beginning of `end`.
    ///
    /// ```text
    ///     ____             ___
    ///     self lorem ipsum end
    ///         ^^^^^^^^^^^^^
    /// ```
    pub fn between(self, end: Self) -> Self {
        Self::new(
            self.hi(),
            end.lo(),
            // if end.ctxt == SyntaxContext::root() { end.ctxt } else { span.ctxt },
            // if span.parent == end.parent { span.parent } else { None },
        )
    }

    /// Returns a `Span` from the beginning of `self` until the beginning of `end`.
    ///
    /// ```text
    ///     ____             ___
    ///     self lorem ipsum end
    ///     ^^^^^^^^^^^^^^^^^
    /// ```
    pub fn until(self, end: Self) -> Self {
        Self::new(
            self.lo(),
            end.lo(),
            // if end_data.ctxt == SyntaxContext::root() { end_data.ctxt } else { span_data.ctxt },
            // if span_data.parent == end_data.parent { span_data.parent } else { None },
        )
    }
}

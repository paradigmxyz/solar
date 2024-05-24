use crate::{BytePos, SessionGlobals};
use std::{cmp, fmt};

/// A source code location.
///
/// Essentially a `lo..hi` range into a `SourceMap` file's source code.
///
/// Both `lo` and `hi` are offset by the file's starting position.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

impl Default for &Span {
    #[inline(always)]
    fn default() -> Self {
        &Span::DUMMY
    }
}

impl fmt::Debug for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Use the global `SourceMap` to print the span. If that's not
        // available, fall back to printing the raw values.

        fn fallback(span: Span, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "Span({lo}..{hi})", lo = span.lo().0, hi = span.hi().0)
        }

        if SessionGlobals::is_set() {
            SessionGlobals::with(|g| {
                if let Some(source_map) = &*g.source_map.lock() {
                    f.write_str(&source_map.span_to_diagnostic_string(*self))
                } else {
                    fallback(*self, f)
                }
            })
        } else {
            fallback(*self, f)
        }
    }
}

impl Span {
    /// A dummy span.
    pub const DUMMY: Self = Self { lo: BytePos(0), hi: BytePos(0) };

    /// Creates a new span from two byte positions.
    #[inline]
    pub fn new(mut lo: BytePos, mut hi: BytePos) -> Self {
        if lo > hi {
            std::mem::swap(&mut lo, &mut hi);
        }
        Self { lo, hi }
    }

    /// Returns the span's start position.
    #[inline(always)]
    pub fn lo(self) -> BytePos {
        self.lo
    }

    /// Creates a new span with the same hi position as this span and the given lo position.
    #[inline]
    pub fn with_lo(self, lo: BytePos) -> Self {
        Self::new(lo, self.hi())
    }

    /// Returns the span's end position.
    #[inline(always)]
    pub fn hi(self) -> BytePos {
        self.hi
    }

    /// Creates a new span with the same lo position as this span and the given hi position.
    #[inline]
    pub fn with_hi(self, hi: BytePos) -> Self {
        Self::new(self.lo(), hi)
    }

    /// Creates a new span representing an empty span at the beginning of this span.
    #[inline]
    pub fn shrink_to_lo(self) -> Self {
        Self::new(self.lo(), self.lo())
    }

    /// Creates a new span representing an empty span at the end of this span.
    #[inline]
    pub fn shrink_to_hi(self) -> Self {
        Self::new(self.hi(), self.hi())
    }

    /// Returns `true` if this is a dummy span.
    #[inline]
    pub fn is_dummy(self) -> bool {
        self == Self::DUMMY
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
    #[inline]
    pub fn to(self, end: Self) -> Self {
        Self::new(cmp::min(self.lo(), end.lo()), cmp::max(self.hi(), end.hi()))
    }

    /// Returns a `Span` between the end of `self` to the beginning of `end`.
    ///
    /// ```text
    ///     ____             ___
    ///     self lorem ipsum end
    ///         ^^^^^^^^^^^^^
    /// ```
    #[inline]
    pub fn between(self, end: Self) -> Self {
        Self::new(self.hi(), end.lo())
    }

    /// Returns a `Span` from the beginning of `self` until the beginning of `end`.
    ///
    /// ```text
    ///     ____             ___
    ///     self lorem ipsum end
    ///     ^^^^^^^^^^^^^^^^^
    /// ```
    #[inline]
    pub fn until(self, end: Self) -> Self {
        Self::new(self.lo(), end.lo())
    }
}

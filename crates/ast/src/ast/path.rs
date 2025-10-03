use crate::BoxSlice;
use bumpalo::Bump;
use solar_data_structures::{BumpExt, smallvec::SmallVec};
use solar_interface::{Ident, Span, Symbol};
use std::{cmp::Ordering, fmt};

/// A boxed [`PathSlice`].
#[derive(Debug)]
pub struct AstPath<'ast>(BoxSlice<'ast, Ident>);

impl std::ops::Deref for AstPath<'_> {
    type Target = PathSlice;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { PathSlice::from_slice_unchecked(self.0) }
    }
}

impl std::ops::DerefMut for AstPath<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { PathSlice::from_mut_slice_unchecked(self.0) }
    }
}

impl PartialEq for AstPath<'_> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for AstPath<'_> {}

impl PartialOrd for AstPath<'_> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AstPath<'_> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_slice().cmp(other.as_slice())
    }
}

impl std::hash::Hash for AstPath<'_> {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state)
    }
}

impl<'ast> AstPath<'ast> {
    /// Creates a new path from a slice of segments by allocating them on the given arena.
    #[inline]
    pub fn new_in(arena: &'ast Bump, segments: &[Ident]) -> Self {
        Self(arena.alloc_thin_slice_copy((), segments))
    }

    /// Returns the path as a slice.
    #[inline]
    pub fn as_slice(&self) -> &PathSlice {
        self
    }

    /// Returns the path as a mutable slice.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut PathSlice {
        self
    }
}

/// A qualified identifier: `foo.bar.baz`.
///
/// This is a list of identifiers, and is never empty.
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct PathSlice([Ident]);

impl ToOwned for PathSlice {
    type Owned = Path;

    #[inline]
    fn to_owned(&self) -> Self::Owned {
        Path::new(&self.0)
    }
}

impl fmt::Debug for PathSlice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for PathSlice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, ident) in self.segments().iter().enumerate() {
            if i != 0 {
                f.write_str(".")?;
            }
            write!(f, "{ident}")?;
        }
        Ok(())
    }
}

impl PartialEq<Ident> for PathSlice {
    fn eq(&self, other: &Ident) -> bool {
        match self.get_ident() {
            Some(ident) => ident == other,
            None => false,
        }
    }
}

impl PartialEq<Symbol> for PathSlice {
    fn eq(&self, other: &Symbol) -> bool {
        match self.get_ident() {
            Some(ident) => ident.name == *other,
            None => false,
        }
    }
}

impl PathSlice {
    /// Creates a new path from a single ident.
    #[inline]
    pub fn from_ref(segment: &Ident) -> &Self {
        unsafe { Self::from_slice_unchecked(std::slice::from_ref(segment)) }
    }

    /// Creates a new path from a single ident.
    #[inline]
    pub fn from_mut(segment: &mut Ident) -> &mut Self {
        unsafe { Self::from_mut_slice_unchecked(std::slice::from_mut(segment)) }
    }

    /// Creates a new path from a slice of segments.
    ///
    /// # Panics
    ///
    /// Panics if `segments` is empty.
    #[inline]
    pub fn from_slice(segments: &[Ident]) -> &Self {
        if segments.is_empty() {
            panic_empty_path();
        }
        // SAFETY: `segments` is not empty.
        unsafe { Self::from_slice_unchecked(segments) }
    }

    /// Creates a new path from a slice of segments.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `segments` is not empty.
    #[inline]
    pub unsafe fn from_slice_unchecked(segments: &[Ident]) -> &Self {
        debug_assert!(!segments.is_empty());
        // SAFETY: We're just a wrapper around a slice `[Ident]`.
        unsafe { &*(segments as *const _ as *const Self) }
    }

    /// Creates a new path from a mutable slice of segments.
    ///
    /// # Panics
    ///
    /// Panics if `segments` is empty.
    #[inline]
    pub fn from_mut_slice(segments: &mut [Ident]) -> &mut Self {
        assert!(!segments.is_empty());
        // SAFETY: `segments` is not empty.
        unsafe { Self::from_mut_slice_unchecked(segments) }
    }

    /// Creates a new path from a mutable slice of segments.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `segments` is not empty.
    #[inline]
    pub unsafe fn from_mut_slice_unchecked(segments: &mut [Ident]) -> &mut Self {
        // SAFETY: We're just a wrapper around a slice `[Ident]`.
        unsafe { &mut *(segments as *mut _ as *mut Self) }
    }

    /// Returns the path's span.
    #[inline]
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn span(&self) -> Span {
        match self.segments() {
            [] => panic_empty_path(),
            [ident] => ident.span,
            [first, .., last] => first.span.with_hi(last.span.hi()),
        }
    }

    /// Returns the path's segments.
    #[inline]
    pub fn segments(&self) -> &[Ident] {
        &self.0
    }

    /// Returns the path's segments.
    #[inline]
    pub fn segments_mut(&mut self) -> &mut [Ident] {
        &mut self.0
    }

    /// If this path consists of a single ident, returns the ident.
    #[inline]
    pub fn get_ident(&self) -> Option<&Ident> {
        match self.segments() {
            [ident] => Some(ident),
            _ => None,
        }
    }

    /// If this path consists of a single ident, returns the ident.
    #[inline]
    pub fn get_ident_mut(&mut self) -> Option<&mut Ident> {
        match self.segments_mut() {
            [ident] => Some(ident),
            _ => None,
        }
    }

    /// Returns the first segment of the path.
    #[inline]
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn first(&self) -> &Ident {
        self.segments().first().unwrap_or_else(|| panic_empty_path())
    }

    /// Returns the first segment of the path.
    #[inline]
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn first_mut(&mut self) -> &mut Ident {
        self.segments_mut().first_mut().unwrap_or_else(|| panic_empty_path())
    }

    /// Returns the last segment of the path.
    #[inline]
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn last(&self) -> &Ident {
        self.segments().last().unwrap_or_else(|| panic_empty_path())
    }

    /// Returns the last segment of the path.
    #[inline]
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn last_mut(&mut self) -> &mut Ident {
        self.segments_mut().last_mut().unwrap_or_else(|| panic_empty_path())
    }
}

#[inline(never)]
#[cold]
#[cfg_attr(debug_assertions, track_caller)]
fn panic_empty_path() -> ! {
    panic!("paths cannot be empty");
}

/// A qualified identifier: `foo.bar.baz`.
///
/// This is a list of identifiers, and is never empty.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Path(SmallVec<[Ident; 1]>);

impl fmt::Debug for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_slice(), f)
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_slice(), f)
    }
}

impl PartialEq<Ident> for Path {
    fn eq(&self, other: &Ident) -> bool {
        PartialEq::eq(self.as_slice(), other)
    }
}

impl PartialEq<Symbol> for Path {
    fn eq(&self, other: &Symbol) -> bool {
        PartialEq::eq(self.as_slice(), other)
    }
}

impl FromIterator<Ident> for Path {
    /// Creates a path from an iterator of idents.
    ///
    /// # Panics
    ///
    /// Panics if the iterator is empty.
    fn from_iter<T: IntoIterator<Item = Ident>>(iter: T) -> Self {
        let mut iter = iter.into_iter();
        let first = iter.next().expect("paths cannot be empty");
        match iter.next() {
            Some(second) => Self(SmallVec::from_iter([first, second].into_iter().chain(iter))),
            None => Self::new_single(first),
        }
    }
}

impl std::ops::Deref for Path {
    type Target = PathSlice;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl std::ops::DerefMut for Path {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl std::borrow::Borrow<PathSlice> for Path {
    #[inline]
    fn borrow(&self) -> &PathSlice {
        self.as_slice()
    }
}

impl Path {
    /// Creates a new path from a slice of segments.
    ///
    /// # Panics
    ///
    /// Panics if `segments` is empty.
    #[inline]
    pub fn new(segments: &[Ident]) -> Self {
        assert!(!segments.is_empty());
        Self(SmallVec::from_slice(segments))
    }

    /// Creates a new path from a list of segments.
    ///
    /// # Panics
    ///
    /// Panics if `segments` is empty.
    #[inline]
    pub fn from_vec(segments: Vec<Ident>) -> Self {
        assert!(!segments.is_empty());
        Self(SmallVec::from_vec(segments))
    }

    /// Creates a new path from a list of segments.
    ///
    /// # Panics
    ///
    /// Panics if `segments` is empty.
    #[inline]
    pub fn from_ast(ast: AstPath<'_>) -> Self {
        Self::new(ast.segments())
    }

    /// Creates a new path from a single ident.
    #[inline]
    pub fn new_single(ident: Ident) -> Self {
        Self(SmallVec::from_buf_and_len([ident], 1))
    }

    /// Converts the path into a slice.
    #[inline]
    pub fn as_slice(&self) -> &PathSlice {
        unsafe { PathSlice::from_slice_unchecked(&self.0) }
    }

    /// Converts the path into a mutable slice.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut PathSlice {
        unsafe { PathSlice::from_mut_slice_unchecked(&mut self.0) }
    }
}

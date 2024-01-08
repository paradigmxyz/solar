//! Solidity AST.

use sulk_data_structures::smallvec::SmallVec;
use sulk_interface::{Ident, Span, Symbol};

mod expr;
pub use expr::*;

mod item;
pub use item::*;

mod lit;
pub use lit::*;

mod semver;
pub use semver::*;

mod stmt;
pub use stmt::*;

mod ty;
pub use ty::*;

pub mod yul;

/// A qualified identifier: `foo.bar.baz`.
///
/// This is a list of identifiers, and is never empty.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Path(SmallVec<[Ident; 1]>);

impl PartialEq<Ident> for Path {
    fn eq(&self, other: &Ident) -> bool {
        match self.get_ident() {
            Some(ident) => ident == other,
            None => false,
        }
    }
}

impl PartialEq<Symbol> for Path {
    fn eq(&self, other: &Symbol) -> bool {
        match self.get_ident() {
            Some(ident) => ident.name == *other,
            None => false,
        }
    }
}

impl Path {
    /// Creates a new path from a list of segments.
    ///
    /// # Panics
    ///
    /// Panics if `segments` is empty.
    #[inline]
    pub fn new(segments: Vec<Ident>) -> Self {
        assert!(!segments.is_empty());
        Self(SmallVec::from(segments))
    }

    /// Creates a new path from a single ident.
    #[inline]
    pub fn new_single(ident: Ident) -> Self {
        Self(SmallVec::from_buf_and_len([ident], 1))
    }

    /// Returns the path's span.
    #[inline]
    pub fn span(&self) -> Span {
        match self.segments() {
            [] => unreachable!(),
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
    pub fn first(&self) -> &Ident {
        self.segments().first().expect("paths cannot be empty")
    }

    /// Returns the first segment of the path.
    #[inline]
    pub fn first_mut(&mut self) -> &mut Ident {
        self.segments_mut().first_mut().expect("paths cannot be empty")
    }

    /// Returns the last segment of the path.
    #[inline]
    pub fn last(&self) -> &Ident {
        self.segments().last().expect("paths cannot be empty")
    }

    /// Returns the last segment of the path.
    #[inline]
    pub fn last_mut(&mut self) -> &mut Ident {
        self.segments_mut().last_mut().expect("paths cannot be empty")
    }
}

/// A Solidity source file.
#[derive(Clone, Debug)]
pub struct SourceUnit {
    /// The source unit's items.
    pub items: Vec<Item>,
}

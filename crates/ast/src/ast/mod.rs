//! Solidity AST.

use sulk_data_structures::smallvec::SmallVec;
use sulk_interface::{Ident, Span, Symbol};

mod expr;
pub use expr::*;

mod item;
pub use item::*;

mod stmt;
pub use stmt::*;

mod ty;
pub use ty::*;

/// A qualified identifier: `foo.bar.baz`.
#[derive(Clone, Debug)]
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
    /// Creates a new path from a single ident.
    pub fn single(ident: Ident) -> Self {
        Self(SmallVec::from_buf_and_len([ident], 1))
    }

    /// Creates a new path from a list of segments.
    ///
    /// # Panics
    ///
    /// Panics if `segments` is empty.
    pub fn new(segments: Vec<Ident>) -> Self {
        assert!(!segments.is_empty());
        Self(SmallVec::from(segments))
    }

    /// Returns the path's span.
    pub fn span(&self) -> Span {
        match self.segments() {
            [] => unreachable!(),
            [ident] => ident.span,
            [first, .., last] => first.span.with_hi(last.span.hi()),
        }
    }

    /// If this path consists of a single ident, returns the ident.
    pub fn get_ident(&self) -> Option<&Ident> {
        match self.segments() {
            [ident] => Some(ident),
            _ => None,
        }
    }

    /// Returns the path's segments.
    pub fn segments(&self) -> &[Ident] {
        &self.0
    }
}

/// A Solidity source file.
#[derive(Clone, Debug)]
pub struct SourceUnit {
    /// The source unit's items.
    pub items: Vec<Item>,
}

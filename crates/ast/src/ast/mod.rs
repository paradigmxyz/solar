//! Solidity AST.

use solar_data_structures::{index::IndexSlice, newtype_index, BumpExt};
use std::fmt;

pub use crate::token::CommentKind;
pub use solar_interface::{Ident, Span, Symbol};

mod expr;
pub use expr::*;

mod item;
pub use item::*;

mod lit;
pub use lit::*;

mod path;
pub use path::*;

mod semver;
pub use semver::*;

mod stmt;
pub use stmt::*;

mod ty;
pub use ty::*;

pub mod yul;

pub type Box<'ast, T> = &'ast mut T;

/// AST arena allocator.
pub struct Arena {
    pub bump: bumpalo::Bump,
    pub literals: typed_arena::Arena<Lit>,
}

impl Arena {
    /// Creates a new AST arena.
    pub fn new() -> Self {
        Self { bump: bumpalo::Bump::new(), literals: typed_arena::Arena::new() }
    }

    pub fn allocated_bytes(&self) -> usize {
        self.bump.allocated_bytes()
            + (self.literals.len() + self.literals.uninitialized_array().len())
                * std::mem::size_of::<Lit>()
    }

    pub fn used_bytes(&self) -> usize {
        self.bump.used_bytes() + self.literals.len() * std::mem::size_of::<Lit>()
    }
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for Arena {
    type Target = bumpalo::Bump;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.bump
    }
}

/// A list of doc-comments.
pub type DocComments<'ast> = Box<'ast, [DocComment]>;

/// A single doc-comment: `/// foo`, `/** bar */`.
#[derive(Clone, Debug)]
pub struct DocComment {
    /// The comment kind.
    pub kind: CommentKind,
    /// The comment's span including its "quotes" (`//`, `/**`).
    pub span: Span,
    /// The comment's contents excluding its "quotes" (`//`, `/**`)
    /// similarly to symbols in string literal tokens.
    pub symbol: Symbol,
}

/// A Solidity source file.
pub struct SourceUnit<'ast> {
    /// The source unit's items.
    pub items: Box<'ast, IndexSlice<ItemId, [Item<'ast>]>>,
}

impl fmt::Debug for SourceUnit<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SourceUnit ")?;
        self.items.fmt(f)
    }
}

impl<'ast> SourceUnit<'ast> {
    /// Creates a new source unit from the given items.
    pub fn new(items: Box<'ast, [Item<'ast>]>) -> Self {
        Self { items: IndexSlice::from_slice_mut(items) }
    }
}

newtype_index! {
    /// A [source unit item](Item) ID. Only used in [`SourceUnit`].
    pub struct ItemId;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_drop() {
        #[track_caller]
        fn assert_no_drop<T>() {
            assert!(!std::mem::needs_drop::<T>(), "{}", std::any::type_name::<T>());
        }
        assert_no_drop::<Type<'_>>();
        assert_no_drop::<Expr<'_>>();
        assert_no_drop::<Stmt<'_>>();
        assert_no_drop::<Item<'_>>();
        assert_no_drop::<SourceUnit<'_>>();
    }
}

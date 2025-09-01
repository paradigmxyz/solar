//! Solidity AST.

use solar_data_structures::{BumpExt, index::IndexSlice, newtype_index};
use std::fmt;

pub use crate::token::CommentKind;
pub use either::{self, Either};
pub use solar_interface::{Ident, Span, Spanned, Symbol};

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
    bump: bumpalo::Bump,
}

impl Arena {
    /// Creates a new AST arena.
    pub fn new() -> Self {
        Self { bump: bumpalo::Bump::new() }
    }

    /// Returns a reference to the arena's bump allocator.
    pub fn bump(&self) -> &bumpalo::Bump {
        &self.bump
    }

    /// Returns a mutable reference to the arena's bump allocator.
    pub fn bump_mut(&mut self) -> &mut bumpalo::Bump {
        &mut self.bump
    }

    /// Calculates the number of bytes currently allocated in the entire arena.
    pub fn allocated_bytes(&self) -> usize {
        self.bump.allocated_bytes()
    }

    /// Returns the number of bytes currently in use.
    pub fn used_bytes(&self) -> usize {
        self.bump.used_bytes()
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
#[derive(Default)]
pub struct DocComments<'ast> {
    /// The raw doc comments.
    pub comments: Box<'ast, [DocComment]>,
    /// The parsed Natspec, if it exists.
    pub natspec: Option<NatSpec<'ast>>,
}

impl<'ast> std::ops::Deref for DocComments<'ast> {
    type Target = Box<'ast, [DocComment]>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.comments
    }
}

impl std::ops::DerefMut for DocComments<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.comments
    }
}

impl fmt::Debug for DocComments<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DocComments")?;
        self.comments.fmt(f)?;
        f.write_str("\nNatSpec")?;
        self.natspec.fmt(f)
    }
}

impl DocComments<'_> {
    /// Returns the span containing all doc-comments.
    pub fn span(&self) -> Span {
        Span::join_first_last(self.iter().map(|d| d.span))
    }
}

/// A Natspec documentation block.
#[derive(Debug, Default)]
pub struct NatSpec<'ast> {
    pub span: Span,
    pub items: Box<'ast, [NatSpecItem]>,
}

impl<'ast> std::ops::Deref for NatSpec<'ast> {
    type Target = Box<'ast, [NatSpecItem]>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

impl std::ops::DerefMut for NatSpec<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.items
    }
}

/// A single item within a Natspec comment block.
#[derive(Clone, Copy, Debug)]
pub struct NatSpecItem {
    pub span: Span,
    pub kind: NatSpecKind,
    pub content: Symbol,
}

/// The kind of a `NatSpec` item.
///
/// Reference: <https://docs.soliditylang.org/en/latest/natspec-format.html#tags>
#[derive(Clone, Copy, Debug)]
pub enum NatSpecKind {
    Title,
    Author,
    Notice,
    Dev,
    Param { tag: Ident },
    Return { tag: Ident },
    Inheritdoc { tag: Ident },
    Custom { tag: Ident },
}

/// A single doc-comment: `/// foo`, `/** bar */`.
#[derive(Clone, Copy, Debug)]
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
        f.write_str("SourceUnit")?;
        self.items.fmt(f)
    }
}

impl<'ast> SourceUnit<'ast> {
    /// Creates a new source unit from the given items.
    pub fn new(items: Box<'ast, [Item<'ast>]>) -> Self {
        Self { items: IndexSlice::from_slice_mut(items) }
    }

    /// Counts the number of contracts in the source unit.
    pub fn count_contracts(&self) -> usize {
        self.items.iter().filter(|item| matches!(item.kind, ItemKind::Contract(_))).count()
    }

    /// Returns an iterator over the source unit's imports.
    pub fn imports(&self) -> impl Iterator<Item = (Span, &ImportDirective<'ast>)> {
        self.items.iter().filter_map(|item| match &item.kind {
            ItemKind::Import(import) => Some((item.span, import)),
            _ => None,
        })
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

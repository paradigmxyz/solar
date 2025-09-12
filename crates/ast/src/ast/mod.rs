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
pub struct DocComments<'ast>(pub Box<'ast, [DocComment]>);

impl<'ast> std::ops::Deref for DocComments<'ast> {
    type Target = Box<'ast, [DocComment]>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for DocComments<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'ast> From<Box<'ast, [DocComment]>> for DocComments<'ast> {
    fn from(comments: Box<'ast, [DocComment]>) -> Self {
        Self(comments)
    }
}

impl fmt::Debug for DocComments<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DocComments")?;
        self.0.fmt(f)
    }
}

impl DocComments<'_> {
    /// Returns the span containing all doc-comments.
    pub fn span(&self) -> Span {
        Span::join_first_last(self.iter().map(|d| d.span))
    }
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

    // Ensure that we track the size of individual AST nodes.
    #[test]
    #[cfg_attr(not(target_pointer_width = "64"), ignore = "64-bit only")]
    #[cfg_attr(feature = "nightly", ignore = "stable only")]
    fn sizes() {
        use snapbox::{assert_data_eq, str};
        #[track_caller]
        fn assert_size<T>(size: impl snapbox::IntoData) {
            assert_size_(std::mem::size_of::<T>(), size.into_data());
        }
        #[track_caller]
        fn assert_size_(actual: usize, expected: snapbox::Data) {
            assert_data_eq!(actual.to_string(), expected);
        }

        assert_size::<Span>(str!["8"]);
        assert_size::<DocComments<'_>>(str!["16"]);

        assert_size::<SourceUnit<'_>>(str!["16"]);

        assert_size::<PragmaDirective<'_>>(str!["40"]);
        assert_size::<ImportDirective<'_>>(str!["40"]);
        assert_size::<UsingDirective<'_>>(str!["64"]);
        assert_size::<ItemContract<'_>>(str!["64"]);
        assert_size::<ItemFunction<'_>>(str!["184"]);
        assert_size::<VariableDefinition<'_>>(str!["88"]);
        assert_size::<ItemStruct<'_>>(str!["32"]);
        assert_size::<ItemEnum<'_>>(str!["32"]);
        assert_size::<ItemUdvt<'_>>(str!["48"]);
        assert_size::<ItemError<'_>>(str!["40"]);
        assert_size::<ItemEvent<'_>>(str!["40"]);
        assert_size::<ItemKind<'_>>(str!["184"]);
        assert_size::<Item<'_>>(str!["208"]);

        assert_size::<FunctionHeader<'_>>(str!["144"]);
        assert_size::<ParameterList<'_>>(str!["24"]);

        assert_size::<ElementaryType>(str!["3"]);
        assert_size::<TypeKind<'_>>(str!["24"]);
        assert_size::<Type<'_>>(str!["32"]);

        assert_size::<ExprKind<'_>>(str!["40"]);
        assert_size::<Expr<'_>>(str!["48"]);

        assert_size::<StmtKind<'_>>(str!["64"]);
        assert_size::<Stmt<'_>>(str!["88"]);
        assert_size::<Block<'_>>(str!["24"]);

        assert_size::<yul::ExprCall<'_>>(str!["32"]);
        assert_size::<yul::ExprKind<'_>>(str!["40"]);
        assert_size::<yul::Expr<'_>>(str!["48"]);

        assert_size::<yul::StmtKind<'_>>(str!["120"]);
        assert_size::<yul::Stmt<'_>>(str!["144"]);
        assert_size::<yul::Block<'_>>(str!["24"]);
    }
}

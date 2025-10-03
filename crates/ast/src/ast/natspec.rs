use super::BoxSlice;
use crate::token::CommentKind;
use solar_interface::{Ident, Span, Symbol};

/// A single item within a Natspec comment block.
#[derive(Clone, Copy, Debug)]
pub struct NatSpecItem {
    /// The tag identifier of the item.
    pub kind: NatSpecKind,
    /// Span of the tag. '@' is not included.
    pub span: Span,
}

/// The kind of a [`NatSpecItem`].
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

    // Special tags reserved for internal purposes.
    Internal { tag: Ident },
}

/// Internal natspec tags.
pub const NATSPEC_INTERNAL_TAGS: &[&str] = &["solidity", "src", "use-src", "ast-id"];

/// Raw doc-comment data without parsed NatSpec.
/// Used for temporary storage before parsing.
#[derive(Debug, Clone, Copy)]
pub struct RawDocComment {
    /// The comment kind.
    pub kind: CommentKind,
    /// The comment's span including its "quotes" (`//`, `/**`).
    pub span: Span,
    /// The comment's contents excluding its "quotes" (`//`, `/**`)
    /// similarly to symbols in string literal tokens.
    pub symbol: Symbol,
}

/// A single doc-comment: `/// foo`, `/** bar */`.
#[derive(Debug)]
pub struct DocComment<'ast> {
    /// The comment kind.
    pub kind: CommentKind,
    /// The comment's span including its "quotes" (`//`, `/**`).
    pub span: Span,
    /// The comment's contents excluding its "quotes" (`//`, `/**`)
    /// similarly to symbols in string literal tokens.
    pub symbol: Symbol,
    /// The comment's natspec items
    pub natspec: BoxSlice<'ast, NatSpecItem>,
}

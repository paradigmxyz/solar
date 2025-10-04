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
    /// `@title`
    ///
    /// A title that describes the contract.
    Title,
    /// `@author`
    ///
    /// The name of the author.
    Author,
    /// `@notice`
    ///
    /// An annotation for end-users.
    Notice,
    /// `@dev`
    ///
    /// A technical annotation for developers.
    Dev,
    /// `@param <name>`
    ///
    /// Documents a parameter. The `tag` field contains the parameter name.
    Param { tag: Ident },
    /// `@return <name>`
    ///
    /// Documents a return variable. The `tag` field contains the return variable name.
    Return { tag: Ident },
    /// `@inheritdoc <contract>`
    ///
    /// Copies all missing tags from the base function. The `tag` field contains the contract name.
    Inheritdoc { tag: Ident },
    /// `@custom:<tag>`
    ///
    /// Custom tag with application-defined semantics. The `tag` field contains the custom tag name.
    Custom { tag: Ident },

    /// `@<tag>`
    ///
    /// Internal tags reserved for compiler purposes. The `tag` field contains the tag name.
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

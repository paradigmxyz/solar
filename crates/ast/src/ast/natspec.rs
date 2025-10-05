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
    /// Documents a parameter. The `name` field contains the parameter name.
    Param { name: Ident },
    /// `@return <name>`
    ///
    /// Documents a return variable. The `name` field contains the return variable name.
    Return { name: Ident },
    /// `@inheritdoc <contract>`
    ///
    /// Copies all tags from the base function. The `contract` field contains the contract name.
    Inheritdoc { contract: Ident },
    /// `@custom:<tag>`
    ///
    /// Custom tag with user-defined semantics. The `name` field contains the custom tag name.
    Custom { name: Ident },

    /// `@<tag>`
    ///
    /// Internal tags reserved for compiler purposes. The `tag` field contains the tag name.
    Internal { tag: Ident },
}

/// Internal natspec tags.
pub const NATSPEC_INTERNAL_TAGS: &[&str] = &["solidity", "src", "use-src", "ast-id"];

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

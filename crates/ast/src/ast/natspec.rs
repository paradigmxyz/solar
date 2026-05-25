use super::BoxSlice;
use crate::token::CommentKind;
use solar_interface::{Ident, Span, Symbol};
use std::fmt;

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

impl<'ast> DocComment<'ast> {
    /// Returns the content of a natspec excluding its tag.
    pub fn natspec_content<'a>(&self, item: &'a NatSpecItem) -> &'a str {
        item.content()
    }
}

/// A single item within a Natspec comment block.
#[derive(Clone, Copy, Debug)]
pub struct NatSpecItem {
    /// The tag identifier of the item.
    pub kind: NatSpecKind,
    /// Span of the tag. '@' is not included.
    pub span: Span,
    /// The symbol containing this tag's content.
    pub symbol: Symbol,
    /// Byte offset into the doc comment's symbol where this tag's content starts.
    pub content_start: u32,
    /// Byte offset into the doc comment's symbol where this tag's content ends.
    pub content_end: u32,
}

impl NatSpecItem {
    /// Returns the byte range of this item's content within the doc comment's symbol.
    pub fn content_range(&self) -> std::ops::Range<usize> {
        self.content_start as usize..self.content_end as usize
    }

    /// Returns the text content of this natspec item.
    ///
    /// The content is extracted from the symbol using the byte offsets.
    pub fn content(&self) -> &str {
        &self.symbol.as_str()[self.content_range()]
    }
}

impl fmt::Display for NatSpecItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let content = self.content();
        match self.kind {
            NatSpecKind::Title => write!(f, "@title {content}"),
            NatSpecKind::Author => write!(f, "@author {content}"),
            NatSpecKind::Notice => write!(f, "@notice {content}"),
            NatSpecKind::Dev => write!(f, "@dev {content}"),
            NatSpecKind::Param { name } => write!(f, "@param {} {content}", name.name),
            NatSpecKind::Return { name: Some(name) } => {
                write!(f, "@return {} {content}", name.name)
            }
            NatSpecKind::Return { name: None } => write!(f, "@return {content}"),
            NatSpecKind::Inheritdoc { contract } => {
                write!(f, "@inheritdoc {} {content}", contract.name)
            }
            NatSpecKind::Custom { name } => write!(f, "@custom:{} {content}", name.name),
            NatSpecKind::Internal { tag } => write!(f, "@{} {content}", tag.name),
        }
    }
}

/// The kind of a [`NatSpecItem`].
///
/// Reference: <https://docs.soliditylang.org/en/latest/natspec-format.html#tags>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// Documents a return variable. The optional `name` field contains the return variable name.
    Return { name: Option<Ident> },
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

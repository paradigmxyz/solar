use solar_ast as ast;
use solar_interface::{Ident, Span, Symbol};

/// Resolved natspec item after `@inheritdoc` expansion.
///
/// Identical to [`ast::NatSpecItem`] but uses [`NatSpecKind`] which excludes `@inheritdoc`.
#[derive(Clone, Copy, Debug)]
pub struct NatSpecItem {
    /// The tag identifier of the item.
    pub kind: NatSpecKind,
    /// Span of the tag. '@' is not included.
    pub span: Span,
    /// The symbol containing this tag's content.
    ///
    /// NOTE: Not stored at `Doc` level, because `@inheritdoc` items reference other source docs.
    pub(crate) symbol: Symbol,
    /// Byte offset into the doc comment's symbol where this tag's content starts.
    pub(crate) content_start: u32,
    /// Byte offset into the doc comment's symbol where this tag's content ends.
    pub(crate) content_end: u32,
}

/// Identical to [`ast::NatSpecKind`] but excludes `@inheritdoc` tags as those are resolved when
/// lowering.
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
    /// `@custom:<tag>`
    ///
    /// Custom tag with user-defined semantics. The `name` field contains the custom tag name.
    Custom { name: Ident },

    /// `@<tag>`
    ///
    /// Internal tags reserved for compiler purposes. The `tag` field contains the tag name.
    Internal { tag: Ident },
}

impl NatSpecItem {
    /// Converts an AST natspec item to HIR, returning `None` for `@inheritdoc` tags.
    ///
    /// The `symbol` parameter should be the symbol from the parent `ast::DocComment`.
    #[inline]
    pub fn from_ast(item: ast::NatSpecItem, symbol: Symbol) -> Option<Self> {
        use NatSpecKind as HirKind;
        use ast::NatSpecKind as AstKind;

        // Skip @inheritdoc - will be replaced by inherited tags during resolution
        let kind = match &item.kind {
            AstKind::Inheritdoc { .. } => return None,
            AstKind::Notice => HirKind::Notice,
            AstKind::Dev => HirKind::Dev,
            AstKind::Title => HirKind::Title,
            AstKind::Author => HirKind::Author,
            AstKind::Param { name } => HirKind::Param { name: *name },
            AstKind::Return { name } => HirKind::Return { name: *name },
            AstKind::Custom { name } => HirKind::Custom { name: *name },
            AstKind::Internal { tag } => HirKind::Internal { tag: *tag },
        };

        Some(Self {
            kind,
            span: item.span,
            symbol,
            content_start: item.content_start,
            content_end: item.content_end,
        })
    }

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

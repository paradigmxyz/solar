use solar_ast as ast;
use solar_interface::{Span, Symbol};

pub use ast::NatSpecKind;

/// Resolved natspec item after `@inheritdoc` expansion.
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

impl NatSpecItem {
    /// Converts an AST natspec item to HIR.
    ///
    /// The `symbol` parameter should be the symbol from the parent `ast::DocComment`.
    ///
    /// SAFETY: The caller must ensure the item is not an `@inheritdoc` tag.
    #[inline]
    pub fn from_ast(item: ast::NatSpecItem, symbol: Symbol) -> Self {
        debug_assert!(!matches!(item.kind, NatSpecKind::Inheritdoc { .. }));

        Self {
            kind: item.kind,
            span: item.span,
            symbol,
            content_start: item.content_start,
            content_end: item.content_end,
        }
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

use crate::hir;
use solar_ast as ast;
use solar_data_structures::BumpExt;

impl<'gcx> super::super::LoweringContext<'gcx> {
    /// Lowers documentation comments from AST to HIR.
    ///
    /// Validation happens after parameters are lowered.
    pub(super) fn lower_item_docs(
        &mut self,
        item: &'gcx ast::Item<'gcx>,
        item_id: hir::ItemId,
    ) -> hir::DocId {
        if item.docs.is_empty() {
            return hir::DocId::EMPTY;
        }
        let docs = self.copy_doc_comments(&item.docs);
        self.lower_docs(docs, item_id)
    }

    fn copy_doc_comments(&self, docs: &ast::DocComments<'_>) -> ast::DocComments<'gcx> {
        let docs = docs.iter().map(|doc| ast::DocComment {
            kind: doc.kind,
            span: doc.span,
            symbol: doc.symbol,
            natspec: self.arena.bump().alloc_thin_slice_copy((), doc.natspec),
        });
        self.arena.bump().alloc_from_iter_thin((), docs).into()
    }

    fn lower_docs(&mut self, docs: ast::DocComments<'gcx>, item_id: hir::ItemId) -> hir::DocId {
        if docs.is_empty() {
            return hir::DocId::EMPTY;
        }

        self.hir.docs.push(hir::Doc {
            source: self.current_source_id,
            item: item_id,
            ast_comments: docs,
        })
    }
}

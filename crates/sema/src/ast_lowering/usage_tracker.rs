use crate::{
    hir::{self, Visit},
    ParsedSources,
};
use solar_ast::{self as ast, Ident, Span};
use solar_data_structures::map::FxIndexSet;
use solar_interface::{Session, Symbol};

pub(crate) fn check_unused(sess: &Session, sources: &ParsedSources<'_>, hir: &hir::Hir<'_>) {
    let mut usage_tracker = UsageTracker::new(sources, hir);
    for source_id in hir.source_ids() {
        usage_tracker.clear();
        let _ = usage_tracker.visit_nested_source(source_id);
        usage_tracker.check_unused_imports(sess);
    }
}

/// Tracks usage of declarations for unused item detection.
struct UsageTracker<'a, 'b> {
    sources: &'a ParsedSources<'b>,
    hir: &'a hir::Hir<'a>,
    current_source: hir::SourceId,
    used_symbols: FxIndexSet<Symbol>,
    used_namespaces: FxIndexSet<hir::SourceId>,
}

impl<'a, 'b> UsageTracker<'a, 'b> {
    fn new(sources: &'a ParsedSources<'b>, hir: &'a hir::Hir<'a>) -> Self {
        Self {
            sources,
            hir,
            current_source: hir::SourceId::new(0),
            used_symbols: Default::default(),
            used_namespaces: Default::default(),
        }
    }

    fn clear(&mut self) {
        self.used_symbols.clear();
        self.used_namespaces.clear();
    }

    /// Mark a symbol as used in a source.
    fn mark_symbol_used(&mut self, symbol: Symbol) {
        self.used_symbols.insert(symbol);
    }

    /// Mark a namespace as used.
    fn mark_namespace_used(&mut self, source: hir::SourceId) {
        self.used_namespaces.insert(source);
    }

    /// Check for unused imports and emit warnings.
    fn check_unused_imports(&self, sess: &Session) {
        for (span, import, imported_source_id) in self.imports() {
            match &import.items {
                ast::ImportItems::Plain(_) | ast::ImportItems::Glob(_) => {
                    if let Some(alias) = import.source_alias() {
                        let namespace_used = self.used_namespaces.contains(&imported_source_id);
                        if !namespace_used && !self.used_symbols.contains(&alias.name) {
                            self.unused_import(sess, span);
                        }
                    }
                }
                ast::ImportItems::Aliases(symbols) => {
                    for &(orig, alias) in symbols.iter() {
                        let name = alias.unwrap_or(orig);
                        if !self.used_symbols.contains(&name.name) {
                            self.unused_import(sess, orig.span.to(name.span));
                        }
                    }
                }
            }
        }
    }

    fn unused_import(&self, sess: &Session, span: Span) {
        sess.dcx.warn("unused import").span(span).emit();
    }

    /// Find the import symbol that brought in an item from another source.
    /// Returns the local name used to import the item, if any.
    fn find_import_alias(&self, item_name: Symbol) -> Option<Ident> {
        for (_, import, _) in self.imports() {
            if let ast::ImportItems::Aliases(symbols) = &import.items {
                // Check if any of the named imports match this item
                for &(orig, alias) in symbols.iter() {
                    let name = alias.unwrap_or(orig);
                    if orig.name == item_name {
                        return Some(name);
                    }
                }
            }
        }
        None
    }

    /// Check for unused declarations (variables, functions, types, etc.).
    #[cfg(false)]
    fn check_unused_items(&self, sess: &Session, hir: &hir::Hir<'_>) {
        // Check all items
        for item_id in hir.item_ids() {
            if self.used_items.contains(&item_id) {
                continue;
            }

            let item = hir.item(item_id);

            // Skip items that should not be warned about
            // Public/external items are part of the contract's interface
            if item.is_public() {
                continue;
            }

            // State variables with explicit getters are implicitly used
            if let hir::Item::Variable(v) = item {
                if v.getter.is_some() {
                    continue;
                }
            }

            // Issue warning
            if let Some(name) = item.name() {
                sess.dcx
                    .warn(format!("unused {}: `{}`", item.description(), name.name))
                    .span(item.span())
                    .emit();
            }
        }
    }

    fn imports(&self) -> impl Iterator<Item = (Span, &'a ast::ImportDirective<'b>, hir::SourceId)> {
        let source = &self.sources[self.current_source];
        let ast = source.ast.as_ref().map(|ast| &ast.items[..]).unwrap_or_default();
        source.imports.iter().map(|(import_item_id, import_source_id)| {
            let import_item = &ast[*import_item_id];
            let ast::ItemKind::Import(import) = &import_item.kind else { unreachable!() };
            (import_item.span, import, *import_source_id)
        })
    }

    fn visit_res(&mut self, res: hir::Res) {
        match res {
            hir::Res::Item(item_id) => {
                // Mark the item as used
                // self.mark_item_used(item_id);

                // Check if this item is from another source (i.e., imported).
                let item = self.hir.item(item_id);
                let item_source = item.source();
                if item_source != self.current_source {
                    if let Some(name) = item.name() {
                        let symbol = self.find_import_alias(name.name).unwrap_or(name);
                        self.mark_symbol_used(symbol.name);
                    }
                }
            }
            hir::Res::Namespace(source_id) => {
                self.mark_namespace_used(source_id);
            }
            hir::Res::Builtin(_) | hir::Res::Err(_) => {}
        }
    }
}

impl<'a, 'b> hir::Visit<'a> for UsageTracker<'a, 'b> {
    type BreakValue = solar_data_structures::Never;

    fn hir(&self) -> &'a hir::Hir<'a> {
        self.hir
    }

    fn visit_nested_source(
        &mut self,
        id: hir::SourceId,
    ) -> std::ops::ControlFlow<Self::BreakValue> {
        self.current_source = id;
        self.walk_nested_source(id)
    }

    fn visit_expr(&mut self, expr: &'a hir::Expr<'a>) -> std::ops::ControlFlow<Self::BreakValue> {
        if let hir::ExprKind::Ident(resolutions) = expr.kind {
            if let &[res] = resolutions {
                self.visit_res(res);
            }
        }

        self.walk_expr(expr)
    }

    fn visit_ty(&mut self, ty: &'a hir::Type<'a>) -> std::ops::ControlFlow<Self::BreakValue> {
        if let hir::TypeKind::Custom(item_id) = &ty.kind {
            self.visit_res(hir::Res::Item(*item_id));
        }

        self.walk_ty(ty)
    }
}

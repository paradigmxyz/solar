use crate::{hir, ParsedSource, ParsedSources};
use solar_ast::{self as ast, Span, Visit};
use solar_data_structures::map::FxIndexSet;
use solar_interface::{Session, Symbol};
use std::ops::ControlFlow;

pub(crate) fn check_unused(sess: &Session, sources: &ParsedSources<'_>, hir: &hir::Hir<'_>) {
    let _ = hir;
    if sources.is_empty() {
        return;
    }

    let mut checker = UnusedChecker::new(sources.first().unwrap());
    for source in sources.iter() {
        if let Some(ast) = &source.ast {
            if !source.imports.is_empty() {
                checker.clear();
                checker.source = source;
                let _ = checker.visit_source_unit(ast);
                checker.check_unused_imports(sess);
            }
        }
    }
}

struct UnusedChecker<'a, 'b> {
    source: &'a ParsedSource<'b>,
    used_symbols: FxIndexSet<Symbol>,
}

impl<'a, 'b> UnusedChecker<'a, 'b> {
    fn new(source: &'a ParsedSource<'b>) -> Self {
        Self { source, used_symbols: Default::default() }
    }

    fn clear(&mut self) {
        self.used_symbols.clear();
    }

    /// Mark a symbol as used in a source.
    fn mark_symbol_used(&mut self, symbol: Symbol) {
        self.used_symbols.insert(symbol);
    }

    /// Check for unused imports and emit warnings.
    fn check_unused_imports(&self, sess: &Session) {
        for (span, import) in self.imports() {
            match &import.items {
                ast::ImportItems::Plain(_) | ast::ImportItems::Glob(_) => {
                    if let Some(alias) = import.source_alias() {
                        if !self.used_symbols.contains(&alias.name) {
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

    fn imports(&self) -> impl Iterator<Item = (Span, &'a ast::ImportDirective<'b>)> {
        let ast = self.source.ast.as_ref().map(|ast| &ast.items[..]).unwrap_or_default();
        self.source.imports.iter().map(|(import_item_id, _import_source_id)| {
            let import_item = &ast[*import_item_id];
            let ast::ItemKind::Import(import) = &import_item.kind else { unreachable!() };
            (import_item.span, import)
        })
    }
}

impl<'a> ast::Visit<'a> for UnusedChecker<'_, 'a> {
    type BreakValue = solar_data_structures::Never;

    fn visit_item(&mut self, item: &'a ast::Item<'a>) -> ControlFlow<Self::BreakValue> {
        if let ast::ItemKind::Import(_) = &item.kind {
            return ControlFlow::Continue(());
        }

        self.walk_item(item)
    }

    fn visit_using_directive(
        &mut self,
        using: &'a ast::UsingDirective<'a>,
    ) -> ControlFlow<Self::BreakValue> {
        match &using.list {
            ast::UsingList::Single(path) => {
                self.mark_symbol_used(path.first().name);
            }
            ast::UsingList::Multiple(items) => {
                for (path, _) in items.iter() {
                    self.mark_symbol_used(path.first().name);
                }
            }
        }

        self.walk_using_directive(using)
    }

    fn visit_modifier(&mut self, modifier: &'a ast::Modifier<'a>) -> ControlFlow<Self::BreakValue> {
        self.mark_symbol_used(modifier.name.first().name);

        self.walk_modifier(modifier)
    }

    fn visit_expr(&mut self, expr: &'a ast::Expr<'a>) -> ControlFlow<Self::BreakValue> {
        if let ast::ExprKind::Ident(id) = expr.kind {
            self.mark_symbol_used(id.name);
        }

        self.walk_expr(expr)
    }

    fn visit_ty(&mut self, ty: &'a ast::Type<'a>) -> ControlFlow<Self::BreakValue> {
        if let ast::TypeKind::Custom(path) = &ty.kind {
            self.mark_symbol_used(path.first().name);
        }

        self.walk_ty(ty)
    }
}

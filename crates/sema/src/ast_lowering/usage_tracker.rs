use crate::hir;
use solar_ast as ast;
use solar_data_structures::{index::IndexVec, map::FxIndexSet};
use solar_interface::{Session, Span, Symbol};

/// Tracks usage of declarations for unused item detection.
#[derive(Debug, Default)]
pub(crate) struct UsageTracker {
    /// Imports in each source file.
    /// source_id -> Vec<ImportEntry>
    imports: IndexVec<hir::SourceId, Vec<ImportEntry>>,

    /// Set of used declarations.
    /// (source_id, symbol_name) -> is_used
    used_symbols: FxIndexSet<(hir::SourceId, Symbol)>,

    /// Set of used items (by their HIR ID).
    used_items: FxIndexSet<hir::ItemId>,

    /// Set of used namespaces (using_source, namespace_source)
    /// E.g., if test.sol uses Lib2 namespace which points to Library2.sol,
    /// this contains (test.sol, Library2.sol)
    used_namespaces: FxIndexSet<(hir::SourceId, hir::SourceId)>,
}

#[derive(Debug, Clone)]
struct ImportEntry {
    /// AST item ID of the import statement.
    _item_id: ast::ItemId,
    /// Span of the import statement.
    span: Span,
    /// Type of import.
    kind: ImportKind,
    /// For namespace imports, the source ID that this import creates a namespace for
    namespace_target: Option<hir::SourceId>,
}

#[derive(Debug, Clone)]
enum ImportKind {
    /// import "file.sol";
    Plain,
    /// import "file.sol" as Alias;
    PlainAliased { alias: Symbol },
    /// import * as Alias from "file.sol";
    Glob { alias: Symbol },
    /// import {A, B as C} from "file.sol";
    Named { symbols: Vec<ImportedSymbol> },
}

#[derive(Debug, Clone)]
struct ImportedSymbol {
    /// Original name in the imported file.
    name: Symbol,
    /// Local alias if renamed.
    alias: Option<Symbol>,
}

impl ImportedSymbol {
    /// Get the local name (alias if present, otherwise original name).
    fn local_name(&self) -> Symbol {
        self.alias.unwrap_or(self.name)
    }
}

impl UsageTracker {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Initialize tracking for a source.
    pub(crate) fn init_source(&mut self, source_id: hir::SourceId) {
        if self.imports.len() <= source_id.index() {
            self.imports.resize(source_id.index() + 1, Vec::new());
        }
    }

    /// Track an import statement.
    pub(crate) fn track_import(
        &mut self,
        source_id: hir::SourceId,
        item_id: ast::ItemId,
        span: Span,
        import_items: &ast::ImportItems<'_>,
        namespace_target: Option<hir::SourceId>,
    ) {
        self.init_source(source_id);

        let kind = match import_items {
            ast::ImportItems::Plain(alias) => {
                if let Some(alias) = alias {
                    ImportKind::PlainAliased { alias: alias.name }
                } else {
                    ImportKind::Plain
                }
            }
            ast::ImportItems::Glob(alias) => ImportKind::Glob { alias: alias.name },
            ast::ImportItems::Aliases(aliases) => {
                let symbols = aliases
                    .iter()
                    .map(|&(name, alias)| ImportedSymbol {
                        name: name.name,
                        alias: alias.map(|a| a.name),
                    })
                    .collect();
                ImportKind::Named { symbols }
            }
        };

        self.imports[source_id].push(ImportEntry {
            _item_id: item_id,
            span,
            kind,
            namespace_target,
        });
    }

    /// Mark a symbol as used in a source.
    pub(crate) fn mark_symbol_used(&mut self, source_id: hir::SourceId, symbol: Symbol) {
        self.used_symbols.insert((source_id, symbol));
    }

    /// Mark an item as used.
    pub(crate) fn mark_item_used(&mut self, item_id: hir::ItemId) {
        self.used_items.insert(item_id);
    }

    /// Mark a namespace as used.
    pub(crate) fn mark_namespace_used(
        &mut self,
        using_source: hir::SourceId,
        namespace_source: hir::SourceId,
    ) {
        self.used_namespaces.insert((using_source, namespace_source));
    }

    /// Check for unused imports and emit warnings.
    pub(crate) fn check_unused_imports(&self, sess: &Session) {
        for (source_id, imports) in self.imports.iter_enumerated() {
            for import in imports {
                match &import.kind {
                    ImportKind::Plain => {
                        // Plain imports import everything, always considered used
                    }
                    ImportKind::PlainAliased { alias } => {
                        // Check if this namespace is used
                        let namespace_used = if let Some(target) = import.namespace_target {
                            self.used_namespaces.contains(&(source_id, target))
                        } else {
                            false
                        };

                        if !namespace_used && !self.used_symbols.contains(&(source_id, *alias)) {
                            sess.dcx.warn("unused import").span(import.span).emit();
                        }
                    }
                    ImportKind::Glob { alias } => {
                        // Check if this namespace is used
                        let namespace_used = if let Some(target) = import.namespace_target {
                            self.used_namespaces.contains(&(source_id, target))
                        } else {
                            false
                        };

                        if !namespace_used && !self.used_symbols.contains(&(source_id, *alias)) {
                            sess.dcx.warn("unused import").span(import.span).emit();
                        }
                    }
                    ImportKind::Named { symbols } => {
                        let unused: Vec<_> = symbols
                            .iter()
                            .filter(|s| !self.used_symbols.contains(&(source_id, s.local_name())))
                            .collect();

                        if !unused.is_empty() {
                            // Only warn if all symbols are unused
                            // TODO: Once we have individual spans, warn per symbol
                            if unused.len() == symbols.len() {
                                sess.dcx.warn("unused import").span(import.span).emit();
                            }
                        }
                    }
                }
            }
        }
    }

    /// Find the import symbol that brought in an item from another source.
    /// Returns the local name used to import the item, if any.
    pub(crate) fn find_import_alias(
        &self,
        source_id: hir::SourceId,
        item_name: Symbol,
    ) -> Option<Symbol> {
        if source_id.index() >= self.imports.len() {
            return None;
        }

        for import in &self.imports[source_id] {
            if let ImportKind::Named { symbols } = &import.kind {
                // Check if any of the named imports match this item
                for symbol in symbols {
                    if symbol.name == item_name {
                        return Some(symbol.local_name());
                    }
                }
            }
        }
        None
    }

    /// Check for unused declarations (variables, functions, types, etc.).
    pub(crate) fn check_unused_items(&self, sess: &Session, hir: &hir::Hir<'_>) {
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
}

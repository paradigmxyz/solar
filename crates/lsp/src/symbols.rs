use std::collections::HashMap;

use lsp_types::{Location, Range, Url};
use solar_interface::Span;
use solar_sema::{Gcx, hir::ItemId};

use crate::proto;

#[derive(Clone, Debug, Default)]
pub(crate) struct SymbolTables {
    declarations: Vec<DeclarationSymbol>,
    files: HashMap<Url, Vec<SymbolId>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct SymbolId(usize);

impl SymbolId {
    pub(crate) fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DeclarationSymbol {
    pub(crate) id: SymbolId,
    #[allow(dead_code, reason = "Scaffolded for LSP symbol query handlers")]
    pub(crate) name: String,
    #[allow(dead_code, reason = "Scaffolded for LSP symbol query handlers")]
    pub(crate) kind: DeclarationKind,
    pub(crate) location: Location,
    #[allow(dead_code, reason = "Scaffolded for LSP symbol query handlers")]
    pub(crate) name_range: Range,
    pub(crate) parent: Option<SymbolId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeclarationKind {
    Contract,
    Function,
    Variable,
    Struct,
    Enum,
    UserDefinedValueType,
    Error,
    Event,
    EnumVariant,
}

impl SymbolTables {
    /// Builds the LSP-owned declaration table from the compiler HIR.
    ///
    /// The compiler's resolver data is scoped to one analysis run. This table copies out the
    /// source-level declarations that LSP requests can query after that run has finished.
    pub(crate) fn build(gcx: Gcx<'_>) -> Self {
        let mut tables = Self::default();
        let mut item_symbols = HashMap::new();

        // First collect HIR items that correspond to source declarations. Parent links are
        // resolved in a second pass because HIR item iteration is grouped by item kind, so a
        // child can be visited before its parent declaration has a SymbolId.
        for item_id in gcx.hir.item_ids() {
            if is_generated_item(gcx, item_id) {
                continue;
            }

            let item = gcx.hir.item(item_id);
            let Some((name, name_span)) = declaration_name(gcx, item_id) else {
                continue;
            };
            let Some(location) = proto::span_to_location(gcx.sess.source_map(), item.span()) else {
                continue;
            };
            let Some(name_location) = proto::span_to_location(gcx.sess.source_map(), name_span)
            else {
                continue;
            };

            let symbol_id = tables.push_declaration(DeclarationSymbol {
                id: SymbolId(tables.declarations.len()),
                name,
                kind: declaration_kind(item_id),
                location,
                name_range: name_location.range,
                parent: None,
            });
            item_symbols.insert(item_id, symbol_id);
        }

        // Convert HIR ownership (`contract`, `parent`) into SymbolId links. These links are the
        // minimal scope structure needed by document symbols, completion, and cursor lookups.
        for (&item_id, &symbol_id) in &item_symbols {
            tables.declarations[symbol_id.index()].parent =
                parent_item(gcx, item_id).and_then(|parent| item_symbols.get(&parent).copied());
        }

        // Enum variants are declarations, but they are not HIR ItemIds. Add them explicitly and
        // attach them to their enum so callers can still traverse the declaration scope tree.
        for enum_id in gcx.hir.enumm_ids() {
            let enumm = gcx.hir.enumm(enum_id);
            let parent = item_symbols.get(&ItemId::Enum(enum_id)).copied();

            for variant in enumm.variants {
                let Some(location) = proto::span_to_location(gcx.sess.source_map(), variant.span)
                else {
                    continue;
                };
                tables.push_declaration(DeclarationSymbol {
                    id: SymbolId(tables.declarations.len()),
                    name: variant.to_string(),
                    kind: DeclarationKind::EnumVariant,
                    name_range: location.range,
                    location,
                    parent,
                });
            }
        }

        tables
    }

    #[allow(dead_code, reason = "Scaffolded for LSP symbol query handlers")]
    pub(crate) fn declarations(&self) -> &[DeclarationSymbol] {
        &self.declarations
    }

    #[allow(dead_code, reason = "Scaffolded for LSP symbol query handlers")]
    pub(crate) fn file_declarations<'a>(
        &'a self,
        uri: &'a Url,
    ) -> impl Iterator<Item = &'a DeclarationSymbol> + 'a {
        self.files.get(uri).into_iter().flat_map(|symbols| {
            symbols.iter().map(|symbol_id| &self.declarations[symbol_id.index()])
        })
    }

    pub(crate) fn extend(&mut self, mut other: Self) {
        let remapped_ids = other
            .declarations
            .iter()
            .enumerate()
            .map(|(index, declaration)| (declaration.id, SymbolId(self.declarations.len() + index)))
            .collect::<HashMap<_, _>>();

        for declaration in &mut other.declarations {
            declaration.id = remapped_ids[&declaration.id];
            declaration.parent = declaration.parent.map(|parent| remapped_ids[&parent]);
        }

        for (uri, symbols) in other.files {
            self.files
                .entry(uri)
                .or_default()
                .extend(symbols.into_iter().map(|symbol_id| remapped_ids[&symbol_id]));
        }

        self.declarations.extend(other.declarations);
    }

    fn push_declaration(&mut self, declaration: DeclarationSymbol) -> SymbolId {
        let id = declaration.id;
        self.files.entry(declaration.location.uri.clone()).or_default().push(id);
        self.declarations.push(declaration);
        id
    }
}

fn declaration_name(gcx: Gcx<'_>, item_id: ItemId) -> Option<(String, Span)> {
    let item = gcx.hir.item(item_id);
    if let Some(name) = item.name() {
        return Some((name.to_string(), name.span));
    }

    let function = gcx.hir.function(item_id.as_function()?);
    Some((function.kind.to_string(), function.keyword_span()))
}

/// Maps a HIR item discriminant to the declaration category stored in the LSP table.
///
/// This keeps the LSP-facing kind independent from the compiler's HIR enum while preserving the
/// source declaration category needed by document symbols, workspace symbols, and completion.
fn declaration_kind(item_id: ItemId) -> DeclarationKind {
    match item_id {
        ItemId::Contract(_) => DeclarationKind::Contract,
        ItemId::Function(_) => DeclarationKind::Function,
        ItemId::Variable(_) => DeclarationKind::Variable,
        ItemId::Struct(_) => DeclarationKind::Struct,
        ItemId::Enum(_) => DeclarationKind::Enum,
        ItemId::Udvt(_) => DeclarationKind::UserDefinedValueType,
        ItemId::Error(_) => DeclarationKind::Error,
        ItemId::Event(_) => DeclarationKind::Event,
    }
}

/// Returns whether a HIR item should be excluded from the source-level symbol table.
///
/// Generated items are useful to the compiler, but LSP queries should describe declarations the
/// user can see and navigate in source. For example, a public state variable already has a variable
/// declaration, so its compiler-generated getter function and getter-local variables would be
/// duplicate editor symbols.
fn is_generated_item(gcx: Gcx<'_>, item_id: ItemId) -> bool {
    match item_id {
        ItemId::Function(id) => gcx.hir.function(id).is_getter(),
        ItemId::Variable(id) => {
            let variable = gcx.hir.variable(id);
            matches!(variable.parent, Some(ItemId::Function(function)) if gcx.hir.function(function).is_getter())
        }
        _ => false,
    }
}

/// Returns the HIR item that owns this declaration's lexical scope, if any.
///
/// Most declarations are owned by their enclosing contract. Variables are more specific because HIR
/// records parameters, return variables, local variables, struct fields, and event/error parameters
/// with their immediate parent item. The returned item is later translated into a SymbolId parent
/// link so LSP features can traverse the declaration scope tree without holding HIR references.
fn parent_item(gcx: Gcx<'_>, item_id: ItemId) -> Option<ItemId> {
    match item_id {
        ItemId::Contract(_) => None,
        ItemId::Function(id) => gcx.hir.function(id).contract.map(ItemId::Contract),
        ItemId::Variable(id) => {
            let variable = gcx.hir.variable(id);
            variable.parent.or_else(|| variable.contract.map(ItemId::Contract))
        }
        ItemId::Struct(id) => gcx.hir.strukt(id).contract.map(ItemId::Contract),
        ItemId::Enum(id) => gcx.hir.enumm(id).contract.map(ItemId::Contract),
        ItemId::Udvt(id) => gcx.hir.udvt(id).contract.map(ItemId::Contract),
        ItemId::Error(id) => gcx.hir.error(id).contract.map(ItemId::Contract),
        ItemId::Event(id) => gcx.hir.event(id).contract.map(ItemId::Contract),
    }
}

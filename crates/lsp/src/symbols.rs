use lsp_types::{
    DocumentSymbol, Location, OneOf, Range, SymbolInformation, SymbolKind, Url, WorkspaceSymbol,
};
use solar_interface::{
    Span,
    data_structures::{index::IndexVec, map::FxHashMap, newtype_index},
};
use solar_sema::{
    Gcx,
    hir::{self, ContractKind, FunctionKind, ItemId, VarKind},
};

use crate::proto;

#[derive(Clone, Debug, Default)]
pub(crate) struct SymbolTables {
    declarations: IndexVec<SymbolId, DeclarationSymbol>,
    files: FxHashMap<Url, Vec<SymbolId>>,
    workspace_symbol_ids: Vec<SymbolId>,
}

newtype_index! {
    /// A declaration symbol ID in the LSP symbol table.
    pub(crate) struct SymbolId;
}

#[derive(Clone, Debug)]
pub(crate) struct DeclarationSymbol {
    pub(crate) id: SymbolId,
    pub(crate) name: String,
    search_name: String,
    pub(crate) kind: SymbolKind,
    pub(crate) location: Location,
    pub(crate) name_range: Range,
    pub(crate) parent: Option<SymbolId>,
}

impl SymbolTables {
    /// Builds the LSP-owned declaration table from the compiler HIR.
    ///
    /// The compiler's resolver data is scoped to one analysis run. This table copies out the
    /// source-level declarations that LSP requests can query after that run has finished.
    pub(crate) fn build(gcx: Gcx<'_>) -> Self {
        let mut tables = Self::default();
        let item_ids = gcx.hir.item_ids();
        let mut item_symbols =
            FxHashMap::with_capacity_and_hasher(item_ids.size_hint().0, Default::default());

        // First collect HIR items that correspond to source declarations. Parent links are
        // resolved in a second pass because HIR item iteration is grouped by item kind, so a
        // child can be visited before its parent declaration has a SymbolId.
        for item_id in item_ids {
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
                id: tables.declarations.next_idx(),
                search_name: search_name(&name),
                name,
                kind: item_symbol_kind(gcx, item_id),
                location,
                name_range: name_location.range,
                parent: None,
            });
            item_symbols.insert(item_id, symbol_id);
        }

        // Convert HIR ownership (`contract`, `parent`) into SymbolId links. These links are the
        // minimal scope structure needed by document symbols, completion, and cursor lookups.
        for (&item_id, &symbol_id) in &item_symbols {
            tables.declarations[symbol_id].parent =
                item_id.parent(&gcx.hir).and_then(|parent| item_symbols.get(&parent).copied());
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
                let name = variant.to_string();
                tables.push_declaration(DeclarationSymbol {
                    id: tables.declarations.next_idx(),
                    search_name: search_name(&name),
                    name,
                    kind: SymbolKind::ENUM_MEMBER,
                    name_range: location.range,
                    location,
                    parent,
                });
            }
        }

        tables.rebuild_indexes();
        tables
    }

    #[cfg(test)]
    pub(crate) fn declarations(&self) -> &[DeclarationSymbol] {
        self.declarations.as_raw_slice()
    }

    #[cfg(test)]
    pub(crate) fn file_declarations<'a>(
        &'a self,
        uri: &'a Url,
    ) -> impl Iterator<Item = &'a DeclarationSymbol> + 'a {
        self.files
            .get(uri)
            .into_iter()
            .flat_map(|symbols| symbols.iter().map(|&symbol_id| &self.declarations[symbol_id]))
    }

    pub(crate) fn extend(&mut self, mut other: Self) {
        if other.declarations.is_empty() {
            return;
        }

        let offset = self.declarations.len();
        for declaration in &mut other.declarations {
            declaration.id = remap_symbol_id(declaration.id, offset);
            declaration.parent = declaration.parent.map(|parent| remap_symbol_id(parent, offset));
        }

        for (uri, symbols) in other.files {
            self.files
                .entry(uri)
                .or_default()
                .extend(symbols.into_iter().map(|symbol_id| remap_symbol_id(symbol_id, offset)));
        }

        self.declarations.extend(other.declarations);
        self.rebuild_indexes();
    }

    pub(crate) fn document_symbols(&self, uri: &Url) -> Vec<DocumentSymbol> {
        let Some(file_symbol_ids) = self.files.get(uri) else {
            return Vec::new();
        };

        let mut child_symbols = FxHashMap::<SymbolId, Vec<SymbolId>>::with_capacity_and_hasher(
            file_symbol_ids.len(),
            Default::default(),
        );
        for &symbol_id in file_symbol_ids {
            if let Some(parent) = self.declarations[symbol_id].parent
                && self.declarations[parent].location.uri == *uri
            {
                child_symbols.entry(parent).or_default().push(symbol_id);
            }
        }

        file_symbol_ids
            .iter()
            .copied()
            .filter(|symbol_id| {
                self.declarations[*symbol_id]
                    .parent
                    .is_none_or(|parent| self.declarations[parent].location.uri != *uri)
            })
            .map(|symbol_id| self.document_symbol(symbol_id, &child_symbols))
            .collect()
    }

    pub(crate) fn flat_document_symbols(&self, uri: &Url) -> Vec<SymbolInformation> {
        let Some(file_symbol_ids) = self.files.get(uri) else {
            return Vec::new();
        };

        file_symbol_ids
            .iter()
            .copied()
            .map(|symbol_id| {
                let symbol = &self.declarations[symbol_id];
                SymbolInformation {
                    name: symbol.name.clone(),
                    kind: symbol.kind,
                    tags: None,
                    #[allow(deprecated)]
                    deprecated: None,
                    location: symbol.location.clone(),
                    container_name: self.container_name(symbol),
                }
            })
            .collect()
    }

    pub(crate) fn workspace_symbols(&self, query: &str) -> Vec<WorkspaceSymbol> {
        let query = (!query.is_empty()).then(|| search_name(query));
        let mut symbols =
            Vec::with_capacity(query.as_ref().map_or(self.workspace_symbol_ids.len(), |_| 0));

        for &symbol_id in &self.workspace_symbol_ids {
            let symbol = &self.declarations[symbol_id];
            if let Some(query) = &query
                && !symbol.search_name.contains(query)
            {
                continue;
            }

            symbols.push(WorkspaceSymbol {
                name: symbol.name.clone(),
                kind: symbol.kind,
                tags: None,
                container_name: self.container_name(symbol),
                location: OneOf::Left(symbol.location.clone()),
                data: None,
            });
        }

        symbols
    }

    fn push_declaration(&mut self, declaration: DeclarationSymbol) -> SymbolId {
        let id = declaration.id;
        self.files.entry(declaration.location.uri.clone()).or_default().push(id);
        let pushed_id = self.declarations.push(declaration);
        debug_assert_eq!(id, pushed_id);
        id
    }

    #[cfg(test)]
    pub(crate) fn push_for_test(
        &mut self,
        uri: &Url,
        name: &str,
        kind: SymbolKind,
        location: Range,
        name_range: Range,
        parent: Option<SymbolId>,
    ) -> SymbolId {
        let symbol_id = self.declarations.next_idx();
        let pushed_id = self.push_declaration(DeclarationSymbol {
            id: symbol_id,
            name: name.into(),
            search_name: search_name(name),
            kind,
            location: Location { uri: uri.clone(), range: location },
            name_range,
            parent,
        });
        self.rebuild_indexes();
        pushed_id
    }

    fn document_symbol(
        &self,
        symbol_id: SymbolId,
        child_symbols: &FxHashMap<SymbolId, Vec<SymbolId>>,
    ) -> DocumentSymbol {
        let symbol = &self.declarations[symbol_id];
        let children = child_symbols.get(&symbol_id).map(|children| {
            children.iter().map(|&child| self.document_symbol(child, child_symbols)).collect()
        });

        DocumentSymbol {
            name: symbol.name.clone(),
            detail: None,
            kind: symbol.kind,
            tags: None,
            #[allow(deprecated)]
            deprecated: None,
            range: symbol.location.range,
            selection_range: symbol.name_range,
            children,
        }
    }

    fn container_name(&self, symbol: &DeclarationSymbol) -> Option<String> {
        let parent = symbol.parent?;
        Some(self.declarations[parent].name.clone())
    }

    fn rebuild_indexes(&mut self) {
        for symbols in self.files.values_mut() {
            sort_symbol_ids(&self.declarations, symbols);
        }

        self.workspace_symbol_ids.clear();
        self.workspace_symbol_ids.reserve(self.declarations.len());
        self.workspace_symbol_ids.extend(self.declarations.indices());
        sort_symbol_ids(&self.declarations, &mut self.workspace_symbol_ids);
    }
}

fn remap_symbol_id(symbol_id: SymbolId, offset: usize) -> SymbolId {
    SymbolId::from_usize(symbol_id.index() + offset)
}

fn sort_symbol_ids(
    declarations: &IndexVec<SymbolId, DeclarationSymbol>,
    symbol_ids: &mut [SymbolId],
) {
    symbol_ids.sort_by_key(|symbol_id| {
        let location = &declarations[*symbol_id].location;
        (
            location.uri.as_str(),
            location.range.start.line,
            location.range.start.character,
            symbol_id.index(),
        )
    });
}

fn search_name(name: &str) -> String {
    name.to_lowercase()
}

#[cfg(test)]
pub(crate) fn push_symbol_for_test(
    tables: &mut SymbolTables,
    uri: &Url,
    name: &str,
    kind: SymbolKind,
    line: u32,
    character: u32,
    parent: Option<SymbolId>,
) -> SymbolId {
    let range = |start_line, start_col, end_line, end_col| Range {
        start: lsp_types::Position { line: start_line, character: start_col },
        end: lsp_types::Position { line: end_line, character: end_col },
    };
    tables.push_for_test(
        uri,
        name,
        kind,
        range(line, character, line, character + 10),
        range(line, character, line, character + name.len() as u32),
        parent,
    )
}

fn declaration_name(gcx: Gcx<'_>, item_id: ItemId) -> Option<(String, Span)> {
    let item = gcx.hir.item(item_id);
    if let Some(name) = item.name() {
        return Some((name.to_string(), name.span));
    }

    let function = gcx.hir.function(item_id.as_function()?);
    Some((function.kind.to_string(), function.keyword_span()))
}

fn item_symbol_kind(gcx: Gcx<'_>, item_id: ItemId) -> SymbolKind {
    match item_id {
        ItemId::Contract(id) => match gcx.hir.contract(id).kind {
            ContractKind::Contract | ContractKind::AbstractContract => SymbolKind::CLASS,
            ContractKind::Interface => SymbolKind::INTERFACE,
            ContractKind::Library => SymbolKind::MODULE,
        },
        ItemId::Function(id) => function_symbol_kind(gcx.hir.function(id)),
        ItemId::Variable(id) => variable_symbol_kind(gcx.hir.variable(id)),
        ItemId::Struct(_) => SymbolKind::STRUCT,
        ItemId::Enum(_) => SymbolKind::ENUM,
        ItemId::Udvt(_) => SymbolKind::TYPE_PARAMETER,
        ItemId::Error(_) | ItemId::Event(_) => SymbolKind::EVENT,
    }
}

fn function_symbol_kind(function: &hir::Function<'_>) -> SymbolKind {
    match function.kind {
        FunctionKind::Constructor => SymbolKind::CONSTRUCTOR,
        FunctionKind::Function if function.is_yul => SymbolKind::FUNCTION,
        FunctionKind::Function if function.contract.is_some() => SymbolKind::METHOD,
        FunctionKind::Function
        | FunctionKind::Fallback
        | FunctionKind::Receive
        | FunctionKind::Modifier => SymbolKind::FUNCTION,
    }
}

fn variable_symbol_kind(variable: &hir::Variable<'_>) -> SymbolKind {
    if variable.is_constant() {
        return SymbolKind::CONSTANT;
    }

    match variable.kind {
        VarKind::State | VarKind::Struct => SymbolKind::PROPERTY,
        VarKind::Global
        | VarKind::Event
        | VarKind::Error
        | VarKind::FunctionParam
        | VarKind::FunctionReturn
        | VarKind::FunctionTyParam
        | VarKind::FunctionTyReturn
        | VarKind::Statement
        | VarKind::TryCatch => SymbolKind::VARIABLE,
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

#[cfg(test)]
mod tests {
    use super::{push_symbol_for_test as push, *};
    use lsp_types::Position;

    #[test]
    fn document_symbols_are_nested_by_parent_and_ordered_by_source() {
        let uri = parse_uri("file:///workspace/src/Contract.sol");
        let tables = sample_tables(&uri, &parse_uri("file:///workspace/src/Other.sol"));

        let symbols = tables.document_symbols(&uri);

        assert_eq!(symbols.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(), ["C"]);
        assert_eq!(symbols[0].kind, SymbolKind::CLASS);
        assert_eq!(symbols[0].selection_range, range(0, 0, 0, 1));

        let contract_children = symbols[0].children.as_ref().unwrap();
        assert_eq!(
            contract_children.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(),
            ["x", "S", "constructor", "f"]
        );
        assert_eq!(contract_children[0].kind, SymbolKind::PROPERTY);
        assert_eq!(contract_children[1].kind, SymbolKind::STRUCT);
        assert_eq!(contract_children[2].kind, SymbolKind::CONSTRUCTOR);
        assert_eq!(contract_children[3].kind, SymbolKind::METHOD);

        let struct_children = contract_children[1].children.as_ref().unwrap();
        assert_eq!(
            struct_children.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(),
            ["field"]
        );
        assert_eq!(struct_children[0].kind, SymbolKind::PROPERTY);

        let function_children = contract_children[3].children.as_ref().unwrap();
        assert_eq!(
            function_children.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(),
            ["arg", "local"]
        );
        assert!(function_children.iter().all(|symbol| symbol.kind == SymbolKind::VARIABLE));
    }

    #[test]
    fn workspace_symbols_filter_by_query_and_include_container_names() {
        let uri = parse_uri("file:///workspace/src/Contract.sol");
        let other_uri = parse_uri("file:///workspace/src/Other.sol");
        let tables = sample_tables(&uri, &other_uri);

        let symbols = tables.workspace_symbols("f");

        assert_eq!(
            symbols.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(),
            ["field", "f", "OtherFunction"]
        );
        assert_eq!(symbols[0].container_name.as_deref(), Some("S"));
        assert_eq!(symbols[0].kind, SymbolKind::PROPERTY);
        assert_eq!(symbols[1].container_name.as_deref(), Some("C"));
        assert_eq!(symbols[1].kind, SymbolKind::METHOD);
        assert_eq!(symbols[2].container_name, None);
        assert_eq!(symbols[2].kind, SymbolKind::FUNCTION);

        let symbols = tables.workspace_symbols("OTHER");
        assert_eq!(
            symbols.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(),
            ["OtherFunction"]
        );
    }

    #[test]
    fn workspace_symbols_preserve_solidity_contract_categories() {
        let uri = parse_uri("file:///workspace/src/Contract.sol");
        let mut tables = SymbolTables::default();
        push(&mut tables, &uri, "Regular", SymbolKind::CLASS, 0, 0, None);
        push(&mut tables, &uri, "Iface", SymbolKind::INTERFACE, 1, 0, None);
        push(&mut tables, &uri, "Lib", SymbolKind::MODULE, 2, 0, None);

        let symbols = tables.workspace_symbols("");

        assert_eq!(
            symbols.iter().map(|symbol| (symbol.name.as_str(), symbol.kind)).collect::<Vec<_>>(),
            [
                ("Regular", SymbolKind::CLASS),
                ("Iface", SymbolKind::INTERFACE),
                ("Lib", SymbolKind::MODULE)
            ]
        );
    }

    fn sample_tables(uri: &Url, other_uri: &Url) -> SymbolTables {
        let mut tables = SymbolTables::default();

        let contract = push(&mut tables, uri, "C", SymbolKind::CLASS, 0, 0, None);
        push(&mut tables, uri, "x", SymbolKind::PROPERTY, 1, 4, Some(contract));
        let strukt = push(&mut tables, uri, "S", SymbolKind::STRUCT, 2, 4, Some(contract));
        push(&mut tables, uri, "field", SymbolKind::PROPERTY, 2, 15, Some(strukt));
        push(&mut tables, uri, "constructor", SymbolKind::CONSTRUCTOR, 3, 4, Some(contract));
        let function = push(&mut tables, uri, "f", SymbolKind::METHOD, 4, 4, Some(contract));
        push(&mut tables, uri, "arg", SymbolKind::VARIABLE, 4, 15, Some(function));
        push(&mut tables, uri, "local", SymbolKind::VARIABLE, 5, 8, Some(function));
        push(&mut tables, other_uri, "OtherFunction", SymbolKind::FUNCTION, 0, 0, None);

        tables
    }

    fn parse_uri(uri: &str) -> Url {
        Url::parse(uri).unwrap()
    }

    fn range(start_line: u32, start_col: u32, end_line: u32, end_col: u32) -> Range {
        Range {
            start: Position { line: start_line, character: start_col },
            end: Position { line: end_line, character: end_col },
        }
    }
}

use std::collections::HashMap;

use lsp_types::{DocumentSymbol, Location, OneOf, Range, SymbolKind, Url, WorkspaceSymbol};
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
    pub(crate) name: String,
    pub(crate) kind: DeclarationKind,
    pub(crate) location: Location,
    pub(crate) name_range: Range,
    pub(crate) parent: Option<SymbolId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeclarationKind {
    Contract,
    Interface,
    Library,
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
                kind: declaration_kind(gcx, item_id),
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

    #[cfg(test)]
    pub(crate) fn declarations(&self) -> &[DeclarationSymbol] {
        &self.declarations
    }

    #[cfg(test)]
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

    pub(crate) fn document_symbols(&self, uri: &Url) -> Vec<DocumentSymbol> {
        let mut file_symbol_ids = self.files.get(uri).cloned().unwrap_or_default();
        self.sort_symbol_ids(&mut file_symbol_ids);

        let mut child_symbols = HashMap::<SymbolId, Vec<SymbolId>>::new();
        for &symbol_id in &file_symbol_ids {
            if let Some(parent) = self.declarations[symbol_id.index()].parent
                && self.declarations[parent.index()].location.uri == *uri
            {
                child_symbols.entry(parent).or_default().push(symbol_id);
            }
        }

        for children in child_symbols.values_mut() {
            self.sort_symbol_ids(children);
        }

        file_symbol_ids
            .into_iter()
            .filter(|symbol_id| {
                self.declarations[symbol_id.index()]
                    .parent
                    .is_none_or(|parent| self.declarations[parent.index()].location.uri != *uri)
            })
            .map(|symbol_id| self.document_symbol(symbol_id, &child_symbols))
            .collect()
    }

    pub(crate) fn workspace_symbols(&self, query: &str) -> Vec<WorkspaceSymbol> {
        let query = query.to_lowercase();
        let mut symbol_ids = (0..self.declarations.len()).map(SymbolId).collect::<Vec<_>>();
        self.sort_symbol_ids(&mut symbol_ids);

        symbol_ids
            .into_iter()
            .filter_map(|symbol_id| {
                let symbol = &self.declarations[symbol_id.index()];
                if !query.is_empty() && !symbol.name.to_lowercase().contains(&query) {
                    return None;
                }

                Some(WorkspaceSymbol {
                    name: symbol.name.clone(),
                    kind: self.symbol_kind(symbol),
                    tags: None,
                    container_name: self.container_name(symbol),
                    location: OneOf::Left(symbol.location.clone()),
                    data: None,
                })
            })
            .collect()
    }

    fn push_declaration(&mut self, declaration: DeclarationSymbol) -> SymbolId {
        let id = declaration.id;
        self.files.entry(declaration.location.uri.clone()).or_default().push(id);
        self.declarations.push(declaration);
        id
    }

    #[cfg(test)]
    pub(crate) fn push_for_test(
        &mut self,
        uri: &Url,
        name: &str,
        kind: DeclarationKind,
        location: Range,
        name_range: Range,
        parent: Option<SymbolId>,
    ) -> SymbolId {
        let symbol_id = SymbolId(self.declarations.len());
        self.push_declaration(DeclarationSymbol {
            id: symbol_id,
            name: name.into(),
            kind,
            location: Location { uri: uri.clone(), range: location },
            name_range,
            parent,
        })
    }

    fn document_symbol(
        &self,
        symbol_id: SymbolId,
        child_symbols: &HashMap<SymbolId, Vec<SymbolId>>,
    ) -> DocumentSymbol {
        let symbol = &self.declarations[symbol_id.index()];
        let children = child_symbols.get(&symbol_id).into_iter().flat_map(|children| {
            children.iter().map(|&child| self.document_symbol(child, child_symbols))
        });
        let children = children.collect::<Vec<_>>();

        DocumentSymbol {
            name: symbol.name.clone(),
            detail: None,
            kind: self.symbol_kind(symbol),
            tags: None,
            #[allow(deprecated)]
            deprecated: None,
            range: symbol.location.range,
            selection_range: symbol.name_range,
            children: (!children.is_empty()).then_some(children),
        }
    }

    fn container_name(&self, symbol: &DeclarationSymbol) -> Option<String> {
        let parent = symbol.parent?;
        Some(self.declarations[parent.index()].name.clone())
    }

    fn sort_symbol_ids(&self, symbol_ids: &mut [SymbolId]) {
        symbol_ids.sort_by_key(|symbol_id| {
            let location = &self.declarations[symbol_id.index()].location;
            (
                location.uri.as_str(),
                location.range.start.line,
                location.range.start.character,
                symbol_id.index(),
            )
        });
    }

    fn symbol_kind(&self, symbol: &DeclarationSymbol) -> SymbolKind {
        match symbol.kind {
            DeclarationKind::Contract => SymbolKind::CLASS,
            DeclarationKind::Interface => SymbolKind::INTERFACE,
            DeclarationKind::Library => SymbolKind::MODULE,
            DeclarationKind::Function if symbol.name == "constructor" => SymbolKind::CONSTRUCTOR,
            DeclarationKind::Function
                if matches!(
                    self.parent_kind(symbol),
                    Some(
                        DeclarationKind::Contract
                            | DeclarationKind::Interface
                            | DeclarationKind::Library
                    )
                ) =>
            {
                SymbolKind::METHOD
            }
            DeclarationKind::Function => SymbolKind::FUNCTION,
            DeclarationKind::Variable
                if matches!(
                    self.parent_kind(symbol),
                    Some(
                        DeclarationKind::Contract
                            | DeclarationKind::Library
                            | DeclarationKind::Struct
                    )
                ) =>
            {
                SymbolKind::FIELD
            }
            DeclarationKind::Variable => SymbolKind::VARIABLE,
            DeclarationKind::Struct => SymbolKind::STRUCT,
            DeclarationKind::Enum => SymbolKind::ENUM,
            DeclarationKind::UserDefinedValueType => SymbolKind::STRUCT,
            DeclarationKind::Error => SymbolKind::FUNCTION,
            DeclarationKind::Event => SymbolKind::EVENT,
            DeclarationKind::EnumVariant => SymbolKind::ENUM_MEMBER,
        }
    }

    fn parent_kind(&self, symbol: &DeclarationSymbol) -> Option<DeclarationKind> {
        Some(self.declarations[symbol.parent?.index()].kind)
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

/// Maps a HIR item to the declaration category stored in the LSP table.
///
/// This keeps the LSP-facing kind independent from the compiler's HIR enum while preserving the
/// source declaration category needed by document symbols, workspace symbols, and completion.
fn declaration_kind(gcx: Gcx<'_>, item_id: ItemId) -> DeclarationKind {
    match item_id {
        ItemId::Contract(id) => {
            let contract = gcx.hir.contract(id);
            if contract.kind.is_interface() {
                DeclarationKind::Interface
            } else if contract.kind.is_library() {
                DeclarationKind::Library
            } else {
                DeclarationKind::Contract
            }
        }
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

#[cfg(test)]
mod tests {
    use lsp_types::{Position, Range};

    use super::*;

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
        assert_eq!(contract_children[0].kind, SymbolKind::FIELD);
        assert_eq!(contract_children[1].kind, SymbolKind::STRUCT);
        assert_eq!(contract_children[2].kind, SymbolKind::CONSTRUCTOR);
        assert_eq!(contract_children[3].kind, SymbolKind::METHOD);

        let struct_children = contract_children[1].children.as_ref().unwrap();
        assert_eq!(
            struct_children.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(),
            ["field"]
        );
        assert_eq!(struct_children[0].kind, SymbolKind::FIELD);

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
        assert_eq!(symbols[0].kind, SymbolKind::FIELD);
        assert_eq!(symbols[1].container_name.as_deref(), Some("C"));
        assert_eq!(symbols[1].kind, SymbolKind::METHOD);
        assert_eq!(symbols[2].container_name, None);
        assert_eq!(symbols[2].kind, SymbolKind::FUNCTION);
    }

    #[test]
    fn workspace_symbols_preserve_solidity_contract_categories() {
        let uri = parse_uri("file:///workspace/src/Contract.sol");
        let mut tables = SymbolTables::default();
        tables.push(&uri, "Regular", DeclarationKind::Contract, 0, 0, None);
        tables.push(&uri, "Iface", DeclarationKind::Interface, 1, 0, None);
        tables.push(&uri, "Lib", DeclarationKind::Library, 2, 0, None);

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

        let contract = tables.push(uri, "C", DeclarationKind::Contract, 0, 0, None);
        tables.push(uri, "x", DeclarationKind::Variable, 1, 4, Some(contract));
        let strukt = tables.push(uri, "S", DeclarationKind::Struct, 2, 4, Some(contract));
        tables.push(uri, "field", DeclarationKind::Variable, 2, 15, Some(strukt));
        tables.push(uri, "constructor", DeclarationKind::Function, 3, 4, Some(contract));
        let function = tables.push(uri, "f", DeclarationKind::Function, 4, 4, Some(contract));
        tables.push(uri, "arg", DeclarationKind::Variable, 4, 15, Some(function));
        tables.push(uri, "local", DeclarationKind::Variable, 5, 8, Some(function));
        tables.push(other_uri, "OtherFunction", DeclarationKind::Function, 0, 0, None);

        tables
    }

    trait SymbolTablesTestExt {
        fn push(
            &mut self,
            uri: &Url,
            name: &str,
            kind: DeclarationKind,
            line: u32,
            character: u32,
            parent: Option<SymbolId>,
        ) -> SymbolId;
    }

    impl SymbolTablesTestExt for SymbolTables {
        fn push(
            &mut self,
            uri: &Url,
            name: &str,
            kind: DeclarationKind,
            line: u32,
            character: u32,
            parent: Option<SymbolId>,
        ) -> SymbolId {
            self.push_for_test(
                uri,
                name,
                kind,
                range(line, character, line, character + 10),
                range(line, character, line, character + name.len() as u32),
                parent,
            )
        }
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

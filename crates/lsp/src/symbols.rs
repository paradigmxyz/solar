use lsp_types::{
    CompletionItem, CompletionItemKind, DocumentSymbol, GotoDefinitionResponse, Location, OneOf,
    Position, Range, SymbolInformation, SymbolKind, Url, WorkspaceSymbol,
};
use solar_interface::{
    Span, Symbol,
    data_structures::{Never, index::IndexVec, map::FxHashMap, newtype_index},
};
use solar_sema::{
    Gcx,
    hir::{
        self, ContractKind, EnumId, FunctionKind, ItemId, Res, StmtKind, TypeKind, UsingEntryKind,
        VarKind, Visit,
    },
    ty::{MemberCompletion, ResolvedMember, ScopeDeclaration},
};
use std::ops::ControlFlow;

use crate::proto;

#[derive(Clone, Debug, Default)]
pub(crate) struct SymbolTables {
    declarations: IndexVec<SymbolId, DeclarationSymbol>,
    completion_entries: IndexVec<CompletionEntryId, CompletionEntry>,
    declaration_completion_ids: IndexVec<SymbolId, CompletionEntryId>,
    files: FxHashMap<Url, Vec<SymbolId>>,
    workspace_symbol_ids: Vec<SymbolId>,
    symbols_by_key: FxHashMap<SymbolKey, SymbolId>,
    scopes: IndexVec<ScopeId, Scope>,
    member_completion_sets: IndexVec<MemberCompletionSetId, MemberCompletionSet>,
    member_completions: Vec<MemberCompletionScope>,
    file_scopes: FxHashMap<Url, Vec<ScopeId>>,
    references: Vec<SymbolReference>,
    file_references: FxHashMap<Url, Vec<usize>>,
    symbol_references: FxHashMap<SymbolId, Vec<Location>>,
}

newtype_index! {
    /// A declaration symbol ID in the LSP symbol table.
    pub(crate) struct SymbolId;

    /// A completion entry ID in the LSP symbol table.
    pub(crate) struct CompletionEntryId;

    /// A lexical scope ID in the LSP symbol table.
    pub(crate) struct ScopeId;

    /// A typed receiver member completion set in the LSP symbol table.
    pub(crate) struct MemberCompletionSetId;
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
    has_definition: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SymbolKey {
    Item(ItemId),
    EnumVariant(EnumId, usize),
}

#[derive(Clone, Debug)]
struct Scope {
    parent: Option<ScopeId>,
    uri: Url,
    range: Range,
    declarations: Vec<ScopedDeclaration>,
}

#[derive(Clone, Copy, Debug)]
struct ScopedDeclaration {
    completion_id: CompletionEntryId,
    member_completion_ids: Option<MemberCompletionSetId>,
    available_from: Option<Position>,
}

#[derive(Clone, Debug)]
struct CompletionEntry {
    label: String,
    kind: CompletionItemKind,
    detail: Option<String>,
}

#[derive(Clone, Debug)]
struct MemberCompletionScope {
    uri: Url,
    range: Range,
    entries: Vec<CompletionEntryId>,
}

#[derive(Clone, Debug)]
struct MemberCompletionSet {
    entries: Vec<CompletionEntryId>,
}

#[derive(Clone, Debug)]
struct SymbolReference {
    location: Location,
    targets: Vec<SymbolId>,
}

#[derive(Clone, Copy, Debug)]
struct CompletionContext {
    source: Option<hir::SourceId>,
    contract: Option<hir::ContractId>,
}

#[derive(Clone, Copy, Debug)]
enum NavigationTarget {
    Declaration,
    Definition,
}

impl NavigationTarget {
    fn includes(self, tables: &SymbolTables, symbol_id: SymbolId) -> bool {
        match self {
            Self::Declaration => true,
            Self::Definition => tables.declarations[symbol_id].has_definition,
        }
    }
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

            let symbol_id = tables.push_declaration(
                SymbolKey::Item(item_id),
                DeclarationSymbol {
                    id: tables.declarations.next_idx(),
                    search_name: search_name(&name),
                    name,
                    kind: item_symbol_kind(gcx, item_id),
                    location,
                    name_range: name_location.range,
                    parent: None,
                    has_definition: item_has_definition(gcx, item_id),
                },
            );
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

            for (variant_index, variant) in enumm.variants.iter().enumerate() {
                let Some(location) = proto::span_to_location(gcx.sess.source_map(), variant.span)
                else {
                    continue;
                };
                let name = variant.to_string();
                tables.push_declaration(
                    SymbolKey::EnumVariant(enum_id, variant_index),
                    DeclarationSymbol {
                        id: tables.declarations.next_idx(),
                        search_name: search_name(&name),
                        name,
                        kind: SymbolKind::ENUM_MEMBER,
                        name_range: location.range,
                        location,
                        parent,
                        has_definition: true,
                    },
                );
            }
        }

        tables.build_scopes(gcx);
        tables.build_member_completions(gcx);
        tables.build_references(gcx);
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
        if other.declarations.is_empty() && other.completion_entries.is_empty() {
            return;
        }

        let symbol_offset = self.declarations.len();
        let completion_offset = self.completion_entries.len();
        let scope_offset = self.scopes.len();
        let member_completion_set_offset = self.member_completion_sets.len();
        for declaration in &mut other.declarations {
            declaration.id = remap_symbol_id(declaration.id, symbol_offset);
            declaration.parent =
                declaration.parent.map(|parent| remap_symbol_id(parent, symbol_offset));
        }
        for completion_id in &mut other.declaration_completion_ids {
            *completion_id = remap_completion_entry_id(*completion_id, completion_offset);
        }
        for scope in &mut other.scopes {
            scope.parent = scope.parent.map(|parent| remap_scope_id(parent, scope_offset));
            for declaration in &mut scope.declarations {
                declaration.completion_id =
                    remap_completion_entry_id(declaration.completion_id, completion_offset);
                declaration.member_completion_ids = declaration
                    .member_completion_ids
                    .map(|id| remap_member_completion_set_id(id, member_completion_set_offset));
            }
        }
        for completion_set in &mut other.member_completion_sets {
            for entry in &mut completion_set.entries {
                *entry = remap_completion_entry_id(*entry, completion_offset);
            }
        }
        for completion in &mut other.member_completions {
            for entry in &mut completion.entries {
                *entry = remap_completion_entry_id(*entry, completion_offset);
            }
        }
        for reference in &mut other.references {
            for target in &mut reference.targets {
                *target = remap_symbol_id(*target, symbol_offset);
            }
        }

        for (uri, symbols) in other.files {
            self.files.entry(uri).or_default().extend(
                symbols.into_iter().map(|symbol_id| remap_symbol_id(symbol_id, symbol_offset)),
            );
        }
        self.declarations.extend(other.declarations);
        self.completion_entries.extend(other.completion_entries);
        self.declaration_completion_ids.extend(other.declaration_completion_ids);
        self.scopes.extend(other.scopes);
        self.member_completion_sets.extend(other.member_completion_sets);
        self.member_completions.extend(other.member_completions);
        self.references.extend(other.references);
        // HIR IDs are scoped to one compiler run, so this build-time map is not meaningful after
        // merging symbol tables from separate analysis batches.
        self.symbols_by_key.clear();
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

    pub(crate) fn goto_definition(
        &self,
        uri: &Url,
        position: Position,
    ) -> Option<GotoDefinitionResponse> {
        let locations = self.locations_for_position(uri, position, NavigationTarget::Definition)?;
        Some(GotoDefinitionResponse::Array(locations))
    }

    pub(crate) fn goto_declaration(
        &self,
        uri: &Url,
        position: Position,
    ) -> Option<GotoDefinitionResponse> {
        let locations =
            self.locations_for_position(uri, position, NavigationTarget::Declaration)?;
        Some(GotoDefinitionResponse::Array(locations))
    }

    pub(crate) fn references(
        &self,
        uri: &Url,
        position: Position,
        include_declaration: bool,
    ) -> Option<Vec<Location>> {
        let target = self.symbol_ids_at_position(uri, position)?;
        let mut locations = Vec::new();

        if include_declaration {
            locations.extend(target.iter().map(|&symbol_id| self.selection_location(symbol_id)));
        }

        for symbol_id in target {
            if let Some(references) = self.symbol_references.get(&symbol_id) {
                locations.extend(references.iter().cloned());
            }
        }

        sort_locations(&mut locations);
        locations.dedup_by(|a, b| a.uri == b.uri && a.range == b.range);
        Some(locations)
    }

    pub(crate) fn completion_items(
        &self,
        uri: &Url,
        position: Position,
        line: Option<&str>,
    ) -> Vec<CompletionItem> {
        if let Some(items) = self.member_completion_items(uri, position) {
            return items;
        }
        if let Some(line) = line
            && let Some((receiver, _prefix)) = member_access_prefix(line, position.character)
            && let Some(items) = self.member_completion_items_for_receiver(uri, position, receiver)
        {
            return items;
        }
        self.lexical_completion_items(uri, position)
    }

    fn lexical_completion_items(&self, uri: &Url, position: Position) -> Vec<CompletionItem> {
        let Some(scope_id) = self.scope_at_position(uri, position) else {
            return Vec::new();
        };

        let mut seen = FxHashMap::<&str, CompletionEntryId>::default();
        let mut scope = Some(scope_id);
        while let Some(scope_id) = scope {
            let current = &self.scopes[scope_id];
            for declaration in &current.declarations {
                if declaration
                    .available_from
                    .is_some_and(|available_from| available_from > position)
                {
                    continue;
                }
                let entry = &self.completion_entries[declaration.completion_id];
                seen.entry(entry.label.as_str()).or_insert(declaration.completion_id);
            }
            scope = current.parent;
        }

        let mut items =
            seen.into_values().map(|entry_id| self.completion_item(entry_id)).collect::<Vec<_>>();
        sort_completion_items(&mut items);
        items
    }

    fn push_declaration(&mut self, key: SymbolKey, declaration: DeclarationSymbol) -> SymbolId {
        let id = declaration.id;
        self.files.entry(declaration.location.uri.clone()).or_default().push(id);
        self.symbols_by_key.insert(key, id);
        let pushed_id = self.declarations.push(declaration);
        debug_assert_eq!(id, pushed_id);
        self.push_declaration_completion_entry(id);
        id
    }

    #[cfg(test)]
    fn push_test_declaration(&mut self, declaration: DeclarationSymbol) -> SymbolId {
        let id = declaration.id;
        self.files.entry(declaration.location.uri.clone()).or_default().push(id);
        let pushed_id = self.declarations.push(declaration);
        debug_assert_eq!(id, pushed_id);
        self.push_declaration_completion_entry(id);
        id
    }

    fn push_declaration_completion_entry(&mut self, symbol_id: SymbolId) -> CompletionEntryId {
        let symbol = &self.declarations[symbol_id];
        let completion_id = self.push_completion_entry(CompletionEntry {
            label: symbol.name.clone(),
            kind: completion_item_kind(symbol.kind),
            detail: self.container_name(symbol),
        });
        let pushed_id = self.declaration_completion_ids.push(completion_id);
        debug_assert_eq!(symbol_id, pushed_id);
        completion_id
    }

    fn push_completion_entry(&mut self, entry: CompletionEntry) -> CompletionEntryId {
        self.completion_entries.push(entry)
    }

    fn push_res_completion_entry(
        &mut self,
        gcx: Gcx<'_>,
        name: Symbol,
        res: Res,
    ) -> CompletionEntryId {
        self.push_completion_entry(CompletionEntry {
            label: name.to_string(),
            kind: completion_item_kind(res_symbol_kind(gcx, res)),
            detail: completion_detail(gcx, res),
        })
    }

    fn push_alias_completion_entry(
        &mut self,
        name: Symbol,
        symbol_id: SymbolId,
    ) -> CompletionEntryId {
        let symbol = &self.declarations[symbol_id];
        self.push_completion_entry(CompletionEntry {
            label: name.to_string(),
            kind: completion_item_kind(symbol.kind),
            detail: Some(symbol.name.clone()),
        })
    }

    fn build_scopes(&mut self, gcx: Gcx<'_>) {
        let mut builder = ScopeBuilder {
            tables: self,
            gcx,
            scope: None,
            context: CompletionContext { source: None, contract: None },
        };
        for source_id in gcx.hir.source_ids() {
            builder.visit_source_scope(source_id);
        }
    }

    fn build_member_completions(&mut self, gcx: Gcx<'_>) {
        let mut collector =
            MemberCompletionCollector { tables: self, gcx, source: None, contract: None };
        for source_id in gcx.hir.source_ids() {
            collector.source = Some(source_id);
            collector.contract = None;
            let _ = collector.visit_nested_source(source_id);
            collector.source = None;
        }
    }

    fn scope_for_span(&mut self, gcx: Gcx<'_>, span: Span, parent: ScopeId) -> Option<ScopeId> {
        let location = proto::span_to_location(gcx.sess.source_map(), span)?;
        Some(self.push_scope(location.uri, location.range, Some(parent)))
    }

    fn push_scope(&mut self, uri: Url, range: Range, parent: Option<ScopeId>) -> ScopeId {
        self.scopes.push(Scope { parent, uri, range, declarations: Vec::new() })
    }

    fn add_scope_declaration(
        &mut self,
        gcx: Gcx<'_>,
        scope: ScopeId,
        item_id: ItemId,
        context: CompletionContext,
    ) {
        if let Some(&symbol_id) = self.symbols_by_key.get(&SymbolKey::Item(item_id)) {
            let completion_id = self.declaration_completion_id(symbol_id);
            let member_completion_ids =
                self.member_completion_set_for_res(gcx, Res::Item(item_id), context);
            self.add_completion_entry_to_scope(scope, completion_id, member_completion_ids);
        }
    }

    fn add_resolved_scope_declarations(
        &mut self,
        gcx: Gcx<'_>,
        scope: ScopeId,
        context: CompletionContext,
        declarations: impl IntoIterator<Item = ScopeDeclaration>,
    ) {
        for declaration in declarations {
            self.add_resolved_scope_declaration(gcx, scope, context, declaration);
        }
    }

    fn add_resolved_scope_declaration(
        &mut self,
        gcx: Gcx<'_>,
        scope: ScopeId,
        context: CompletionContext,
        declaration: ScopeDeclaration,
    ) {
        let completion_id = match declaration.res {
            Res::Item(item_id) => {
                if let Some(&symbol_id) = self.symbols_by_key.get(&SymbolKey::Item(item_id)) {
                    let symbol = &self.declarations[symbol_id];
                    if symbol.name == declaration.name.as_str() {
                        self.declaration_completion_id(symbol_id)
                    } else {
                        self.push_alias_completion_entry(declaration.name, symbol_id)
                    }
                } else {
                    self.push_res_completion_entry(gcx, declaration.name, declaration.res)
                }
            }
            Res::Builtin(_) | Res::Namespace(_) => {
                self.push_res_completion_entry(gcx, declaration.name, declaration.res)
            }
            Res::Err(_) => return,
        };
        let member_completion_ids =
            self.member_completion_set_for_res(gcx, declaration.res, context);
        self.add_completion_entry_to_scope(scope, completion_id, member_completion_ids);
    }

    fn add_local_scope_declaration(
        &mut self,
        gcx: Gcx<'_>,
        scope: ScopeId,
        item_id: ItemId,
        span: Span,
        context: CompletionContext,
    ) {
        if let Some(&symbol_id) = self.symbols_by_key.get(&SymbolKey::Item(item_id)) {
            self.add_local_symbol_to_scope(gcx, scope, item_id, symbol_id, span, context);
        }
    }

    fn add_symbol_to_scope(&mut self, scope: ScopeId, symbol_id: SymbolId) {
        let completion_id = self.declaration_completion_id(symbol_id);
        self.add_completion_entry_to_scope(scope, completion_id, None);
    }

    fn add_local_symbol_to_scope(
        &mut self,
        gcx: Gcx<'_>,
        scope: ScopeId,
        item_id: ItemId,
        symbol_id: SymbolId,
        span: Span,
        context: CompletionContext,
    ) {
        let available_from = proto::span_to_location(gcx.sess.source_map(), span)
            .map(|location| location.range.end)
            .unwrap_or(self.declarations[symbol_id].location.range.end);
        let completion_id = self.declaration_completion_id(symbol_id);
        let member_completion_ids =
            self.member_completion_set_for_res(gcx, Res::Item(item_id), context);
        self.scopes[scope].declarations.push(ScopedDeclaration {
            completion_id,
            member_completion_ids,
            available_from: Some(available_from),
        });
    }

    fn add_completion_entry_to_scope(
        &mut self,
        scope: ScopeId,
        completion_id: CompletionEntryId,
        member_completion_ids: Option<MemberCompletionSetId>,
    ) {
        self.scopes[scope].declarations.push(ScopedDeclaration {
            completion_id,
            member_completion_ids,
            available_from: None,
        });
    }

    fn member_completion_set_for_res(
        &mut self,
        gcx: Gcx<'_>,
        res: Res,
        context: CompletionContext,
    ) -> Option<MemberCompletionSetId> {
        let source = context.source?;
        let has_members = match res {
            Res::Item(_) | Res::Namespace(_) => true,
            Res::Builtin(builtin) => builtin.members().is_some(),
            Res::Err(_) => false,
        };
        if !has_members {
            return None;
        }
        let entries = gcx
            .member_completions_of(gcx.type_of_res(res), source, context.contract)
            .into_iter()
            .map(|member| self.member_completion_entry(gcx, member))
            .collect::<Vec<_>>();
        if entries.is_empty() {
            return None;
        }
        Some(self.member_completion_sets.push(MemberCompletionSet { entries }))
    }

    fn declaration_completion_id(&self, symbol_id: SymbolId) -> CompletionEntryId {
        self.declaration_completion_ids[symbol_id]
    }

    fn build_references(&mut self, gcx: Gcx<'_>) {
        let mut collector = ReferenceCollector { tables: self, gcx, source: None, contract: None };
        for source_id in gcx.hir.source_ids() {
            collector.source = Some(source_id);
            collector.contract = None;
            for using in gcx.hir.source(source_id).usings {
                collector.visit_using_directive(using);
            }
            let _ = collector.visit_nested_source(source_id);
            collector.source = None;
        }
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
        let pushed_id = self.push_test_declaration(DeclarationSymbol {
            id: symbol_id,
            name: name.into(),
            search_name: search_name(name),
            kind,
            location: Location { uri: uri.clone(), range: location },
            name_range,
            parent,
            has_definition: true,
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

    fn locations_for_position(
        &self,
        uri: &Url,
        position: Position,
        target: NavigationTarget,
    ) -> Option<Vec<Location>> {
        let symbol_ids = self.symbol_ids_at_position(uri, position)?;
        let mut locations = symbol_ids
            .into_iter()
            .filter(|&symbol_id| target.includes(self, symbol_id))
            .map(|symbol_id| self.selection_location(symbol_id))
            .collect::<Vec<_>>();
        if locations.is_empty() {
            return None;
        }
        sort_locations(&mut locations);
        locations.dedup_by(|a, b| a.uri == b.uri && a.range == b.range);
        Some(locations)
    }

    fn symbol_ids_at_position(&self, uri: &Url, position: Position) -> Option<Vec<SymbolId>> {
        if let Some(reference) = self.reference_at_position(uri, position) {
            return Some(reference.targets.clone());
        }

        let symbol_id = self.declaration_at_position(uri, position)?;
        Some(vec![symbol_id])
    }

    fn reference_at_position(&self, uri: &Url, position: Position) -> Option<&SymbolReference> {
        self.file_references
            .get(uri)?
            .iter()
            .filter_map(|&index| {
                let reference = &self.references[index];
                range_contains(reference.location.range, position).then_some(reference)
            })
            .min_by_key(|reference| range_size_key(reference.location.range))
    }

    fn declaration_at_position(&self, uri: &Url, position: Position) -> Option<SymbolId> {
        self.files
            .get(uri)?
            .iter()
            .copied()
            .filter(|&symbol_id| range_contains(self.declarations[symbol_id].name_range, position))
            .min_by_key(|&symbol_id| range_size_key(self.declarations[symbol_id].name_range))
    }

    fn scope_at_position(&self, uri: &Url, position: Position) -> Option<ScopeId> {
        self.file_scopes
            .get(uri)?
            .iter()
            .copied()
            .filter(|&scope_id| range_contains(self.scopes[scope_id].range, position))
            .min_by_key(|&scope_id| {
                let (lines, chars) = range_size_key(self.scopes[scope_id].range);
                (lines, chars, u32::MAX - self.scope_depth(scope_id))
            })
    }

    fn scope_depth(&self, mut scope_id: ScopeId) -> u32 {
        let mut depth = 0;
        while let Some(parent) = self.scopes[scope_id].parent {
            depth += 1;
            scope_id = parent;
        }
        depth
    }

    fn member_completion_items(
        &self,
        uri: &Url,
        position: Position,
    ) -> Option<Vec<CompletionItem>> {
        let completion = self
            .member_completions
            .iter()
            .filter(|completion| {
                completion.uri == *uri && range_contains(completion.range, position)
            })
            .min_by_key(|completion| range_size_key(completion.range))?;
        self.completion_items_from_entries(&completion.entries)
    }

    fn member_completion_items_for_receiver(
        &self,
        uri: &Url,
        position: Position,
        receiver: &str,
    ) -> Option<Vec<CompletionItem>> {
        let mut scope = Some(self.scope_at_position(uri, position)?);
        while let Some(scope_id) = scope {
            let current = &self.scopes[scope_id];
            for declaration in &current.declarations {
                if declaration
                    .available_from
                    .is_some_and(|available_from| available_from > position)
                {
                    continue;
                }
                let entry = &self.completion_entries[declaration.completion_id];
                if entry.label != receiver {
                    continue;
                }
                let completion_set = declaration.member_completion_ids?;
                return self.completion_items_from_entries(
                    &self.member_completion_sets[completion_set].entries,
                );
            }
            scope = current.parent;
        }
        None
    }

    fn completion_items_from_entries(
        &self,
        entries: &[CompletionEntryId],
    ) -> Option<Vec<CompletionItem>> {
        let mut seen = FxHashMap::<&str, CompletionEntryId>::default();
        for &entry_id in entries {
            let entry = &self.completion_entries[entry_id];
            seen.entry(entry.label.as_str()).or_insert(entry_id);
        }
        if seen.is_empty() {
            return None;
        }
        let mut items =
            seen.into_values().map(|entry_id| self.completion_item(entry_id)).collect::<Vec<_>>();
        sort_completion_items(&mut items);
        Some(items)
    }

    fn member_completion_entry(
        &mut self,
        gcx: Gcx<'_>,
        member: MemberCompletion<'_>,
    ) -> CompletionEntryId {
        if let Some(symbol_id) = self.symbol_id_for_member_completion(gcx, member) {
            return self.declaration_completion_id(symbol_id);
        }
        self.push_completion_entry(CompletionEntry {
            label: member.member.name.to_string(),
            kind: member_completion_item_kind(member),
            detail: member.member.attached.then_some("using for".to_string()),
        })
    }

    fn symbol_id_for_member_completion(
        &self,
        gcx: Gcx<'_>,
        member: MemberCompletion<'_>,
    ) -> Option<SymbolId> {
        match member.resolved {
            Some(ResolvedMember::Res(res)) => self.symbol_id_for_res(res),
            Some(ResolvedMember::StructField { struct_id, field_index }) => {
                let field_id = gcx.hir.strukt(struct_id).fields.get(field_index).copied()?;
                self.symbols_by_key.get(&SymbolKey::Item(ItemId::Variable(field_id))).copied()
            }
            Some(ResolvedMember::EnumVariant { enum_id, variant_index }) => {
                self.symbols_by_key.get(&SymbolKey::EnumVariant(enum_id, variant_index)).copied()
            }
            None => member.member.res.and_then(|res| self.symbol_id_for_res(res)),
        }
    }

    fn symbol_id_for_res(&self, res: Res) -> Option<SymbolId> {
        match res {
            Res::Item(item_id) => self.symbols_by_key.get(&SymbolKey::Item(item_id)).copied(),
            Res::Namespace(_) | Res::Builtin(_) | Res::Err(_) => None,
        }
    }

    fn completion_item(&self, entry_id: CompletionEntryId) -> CompletionItem {
        let entry = &self.completion_entries[entry_id];
        CompletionItem {
            label: entry.label.clone(),
            kind: Some(entry.kind),
            detail: entry.detail.clone(),
            ..Default::default()
        }
    }

    fn selection_location(&self, symbol_id: SymbolId) -> Location {
        let symbol = &self.declarations[symbol_id];
        Location { uri: symbol.location.uri.clone(), range: symbol.name_range }
    }

    fn rebuild_indexes(&mut self) {
        for symbols in self.files.values_mut() {
            sort_symbol_ids(&self.declarations, symbols);
        }

        self.workspace_symbol_ids.clear();
        self.workspace_symbol_ids.reserve(self.declarations.len());
        self.workspace_symbol_ids.extend(self.declarations.indices());
        sort_symbol_ids(&self.declarations, &mut self.workspace_symbol_ids);

        self.file_scopes.clear();
        for scope_id in self.scopes.indices() {
            let uri = self.scopes[scope_id].uri.clone();
            self.file_scopes.entry(uri).or_default().push(scope_id);
        }
        for scopes in self.file_scopes.values_mut() {
            scopes.sort_by_key(|&scope_id| {
                let range = self.scopes[scope_id].range;
                (range.start.line, range.start.character, range.end.line, range.end.character)
            });
        }

        self.file_references.clear();
        self.symbol_references.clear();
        for (index, reference) in self.references.iter().enumerate() {
            self.file_references.entry(reference.location.uri.clone()).or_default().push(index);
            for &target in &reference.targets {
                self.symbol_references.entry(target).or_default().push(reference.location.clone());
            }
        }
        for references in self.file_references.values_mut() {
            references.sort_by_key(|&index| {
                let location = &self.references[index].location;
                (
                    location.range.start.line,
                    location.range.start.character,
                    location.range.end.line,
                    location.range.end.character,
                    index,
                )
            });
        }
        for locations in self.symbol_references.values_mut() {
            sort_locations(locations);
            locations.dedup_by(|a, b| a.uri == b.uri && a.range == b.range);
        }
    }
}

fn remap_symbol_id(symbol_id: SymbolId, offset: usize) -> SymbolId {
    SymbolId::from_usize(symbol_id.index() + offset)
}

fn remap_completion_entry_id(completion_id: CompletionEntryId, offset: usize) -> CompletionEntryId {
    CompletionEntryId::from_usize(completion_id.index() + offset)
}

fn remap_scope_id(scope_id: ScopeId, offset: usize) -> ScopeId {
    ScopeId::from_usize(scope_id.index() + offset)
}

fn remap_member_completion_set_id(
    id: MemberCompletionSetId,
    offset: usize,
) -> MemberCompletionSetId {
    MemberCompletionSetId::from_usize(id.index() + offset)
}

struct ScopeBuilder<'a, 'gcx> {
    tables: &'a mut SymbolTables,
    gcx: Gcx<'gcx>,
    scope: Option<ScopeId>,
    context: CompletionContext,
}

impl<'gcx> ScopeBuilder<'_, 'gcx> {
    fn visit_source_scope(&mut self, source_id: hir::SourceId) {
        let source = self.gcx.hir.source(source_id);
        let Some(path) = source.file.name.as_real() else {
            return;
        };
        let Some(uri) = Url::from_file_path(path).ok() else {
            return;
        };
        let Some(range) = proto::span_to_location(
            self.gcx.sess.source_map(),
            Span::new(source.file.start_pos, source.file.end_position()),
        )
        .map(|location| location.range) else {
            return;
        };

        let root = self.tables.push_scope(uri, range, None);
        let previous_context = self.context;
        self.context = CompletionContext { source: Some(source_id), contract: None };
        self.tables.add_resolved_scope_declarations(
            self.gcx,
            root,
            self.context,
            self.gcx.source_scope_declarations(source_id),
        );
        self.tables.add_resolved_scope_declarations(
            self.gcx,
            root,
            self.context,
            self.gcx.global_scope_declarations(),
        );
        self.with_scope(root, |this| {
            for &item_id in source.items {
                let _ = this.visit_nested_item(item_id);
            }
        });
        self.context = previous_context;
    }

    fn push_child_scope(&mut self, span: Span) -> Option<ScopeId> {
        let parent = self.scope?;
        self.tables.scope_for_span(self.gcx, span, parent)
    }

    fn with_scope(&mut self, scope: ScopeId, f: impl FnOnce(&mut Self)) {
        let previous = self.scope.replace(scope);
        f(self);
        self.scope = previous;
    }

    fn visit_block_scope(&mut self, block: hir::Block<'gcx>) {
        let Some(scope) = self.push_child_scope(block.span) else {
            return;
        };
        self.with_scope(scope, |this| {
            for stmt in block.stmts {
                let _ = this.visit_stmt(stmt);
            }
        });
    }

    fn visit_statement_child_scope(&mut self, stmt: &'gcx hir::Stmt<'gcx>) {
        match stmt.kind {
            StmtKind::Block(block)
            | StmtKind::UncheckedBlock(block)
            | StmtKind::AssemblyBlock(block)
            | StmtKind::Loop(block, _) => self.visit_block_scope(block),
            _ => {
                let _ = self.visit_stmt(stmt);
            }
        }
    }
}

impl<'gcx> hir::Visit<'gcx> for ScopeBuilder<'_, 'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_nested_contract(&mut self, id: hir::ContractId) -> ControlFlow<Self::BreakValue> {
        let contract = self.hir().contract(id);
        let Some(scope) = self.push_child_scope(contract.span) else {
            return ControlFlow::Continue(());
        };
        let previous_context = self.context;
        self.context.contract = Some(id);
        self.tables.add_resolved_scope_declarations(
            self.gcx,
            scope,
            self.context,
            self.gcx.contract_scope_declarations(id),
        );
        self.with_scope(scope, |this| {
            for &item_id in contract.items {
                let _ = this.visit_nested_item(item_id);
            }
        });
        self.context = previous_context;
        ControlFlow::Continue(())
    }

    fn visit_function(
        &mut self,
        function: &'gcx hir::Function<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        let Some(scope) = self.push_child_scope(function.body_span) else {
            return ControlFlow::Continue(());
        };
        self.with_scope(scope, |this| {
            for &param in function.parameters {
                this.tables.add_scope_declaration(
                    this.gcx,
                    scope,
                    ItemId::Variable(param),
                    this.context,
                );
            }
            for &ret in function.returns {
                this.tables.add_scope_declaration(
                    this.gcx,
                    scope,
                    ItemId::Variable(ret),
                    this.context,
                );
            }
            if let Some(body) = function.body {
                this.visit_block_scope(body);
            }
        });
        ControlFlow::Continue(())
    }

    fn visit_struct(&mut self, strukt: &'gcx hir::Struct<'gcx>) -> ControlFlow<Self::BreakValue> {
        let Some(scope) = self.push_child_scope(strukt.span) else {
            return ControlFlow::Continue(());
        };
        for &field in strukt.fields {
            self.tables.add_scope_declaration(
                self.gcx,
                scope,
                ItemId::Variable(field),
                self.context,
            );
        }
        ControlFlow::Continue(())
    }

    fn visit_nested_enum(&mut self, id: hir::EnumId) -> ControlFlow<Self::BreakValue> {
        let enumm = self.hir().enumm(id);
        let Some(scope) = self.push_child_scope(enumm.span) else {
            return ControlFlow::Continue(());
        };
        for variant_index in 0..enumm.variants.len() {
            if let Some(symbol_id) =
                self.tables.symbols_by_key.get(&SymbolKey::EnumVariant(id, variant_index))
            {
                self.tables.add_symbol_to_scope(scope, *symbol_id);
            }
        }
        ControlFlow::Continue(())
    }

    fn visit_error(&mut self, error: &'gcx hir::Error<'gcx>) -> ControlFlow<Self::BreakValue> {
        let Some(scope) = self.push_child_scope(error.span) else {
            return ControlFlow::Continue(());
        };
        for &param in error.parameters {
            self.tables.add_scope_declaration(
                self.gcx,
                scope,
                ItemId::Variable(param),
                self.context,
            );
        }
        ControlFlow::Continue(())
    }

    fn visit_event(&mut self, event: &'gcx hir::Event<'gcx>) -> ControlFlow<Self::BreakValue> {
        let Some(scope) = self.push_child_scope(event.span) else {
            return ControlFlow::Continue(());
        };
        for &param in event.parameters {
            self.tables.add_scope_declaration(
                self.gcx,
                scope,
                ItemId::Variable(param),
                self.context,
            );
        }
        ControlFlow::Continue(())
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
        let Some(scope) = self.scope else {
            return ControlFlow::Continue(());
        };
        match stmt.kind {
            StmtKind::DeclSingle(var) => {
                self.tables.add_local_scope_declaration(
                    self.gcx,
                    scope,
                    ItemId::Variable(var),
                    stmt.span,
                    self.context,
                );
            }
            StmtKind::DeclMulti(vars, _) => {
                for var in vars.iter().copied().flatten() {
                    self.tables.add_local_scope_declaration(
                        self.gcx,
                        scope,
                        ItemId::Variable(var),
                        stmt.span,
                        self.context,
                    );
                }
            }
            StmtKind::Block(block)
            | StmtKind::UncheckedBlock(block)
            | StmtKind::AssemblyBlock(block)
            | StmtKind::Loop(block, _) => self.visit_block_scope(block),
            StmtKind::If(_, true_, false_) => {
                self.visit_statement_child_scope(true_);
                if let Some(false_) = false_ {
                    self.visit_statement_child_scope(false_);
                }
            }
            StmtKind::Switch(switch) => {
                for case in switch.cases {
                    self.visit_block_scope(case.body);
                }
            }
            StmtKind::Try(try_) => {
                for clause in try_.clauses {
                    let Some(clause_scope) = self.push_child_scope(clause.span) else {
                        continue;
                    };
                    self.with_scope(clause_scope, |this| {
                        for &arg in clause.args {
                            this.tables.add_scope_declaration(
                                this.gcx,
                                clause_scope,
                                ItemId::Variable(arg),
                                this.context,
                            );
                        }
                        this.visit_block_scope(clause.block);
                    });
                }
            }
            StmtKind::Emit(_)
            | StmtKind::Revert(_)
            | StmtKind::Return(_)
            | StmtKind::Break
            | StmtKind::Continue
            | StmtKind::Expr(_)
            | StmtKind::Placeholder
            | StmtKind::Err(_) => {}
        }
        ControlFlow::Continue(())
    }
}

struct MemberCompletionCollector<'a, 'gcx> {
    tables: &'a mut SymbolTables,
    gcx: Gcx<'gcx>,
    source: Option<hir::SourceId>,
    contract: Option<hir::ContractId>,
}

impl<'gcx> MemberCompletionCollector<'_, 'gcx> {
    fn push_member_completions(
        &mut self,
        receiver: &'gcx hir::Expr<'gcx>,
        member: solar_interface::Ident,
    ) {
        let Some(source) = self.source else {
            return;
        };
        let Some(receiver_ty) = self.gcx.type_of_expr(receiver.id) else {
            return;
        };
        let Some(location) = proto::span_to_location(self.gcx.sess.source_map(), member.span)
        else {
            return;
        };
        let entries = self
            .gcx
            .member_completions_of(receiver_ty, source, self.contract)
            .into_iter()
            .map(|member| self.tables.member_completion_entry(self.gcx, member))
            .collect::<Vec<_>>();
        if entries.is_empty() {
            return;
        }
        self.tables.member_completions.push(MemberCompletionScope {
            uri: location.uri,
            range: location.range,
            entries,
        });
    }
}

impl<'gcx> hir::Visit<'gcx> for MemberCompletionCollector<'_, 'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_nested_contract(&mut self, id: hir::ContractId) -> ControlFlow<Self::BreakValue> {
        let previous_contract = self.contract.replace(id);
        let result = self.visit_contract(self.hir().contract(id));
        self.contract = previous_contract;
        result
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        match expr.kind {
            hir::ExprKind::Member(receiver, member)
            | hir::ExprKind::YulMember(receiver, member) => {
                self.visit_expr(receiver)?;
                self.push_member_completions(receiver, member);
            }
            _ => {
                hir::Visit::walk_expr(self, expr)?;
            }
        }
        ControlFlow::Continue(())
    }
}

struct ReferenceCollector<'a, 'gcx> {
    tables: &'a mut SymbolTables,
    gcx: Gcx<'gcx>,
    source: Option<hir::SourceId>,
    contract: Option<hir::ContractId>,
}

impl<'gcx> ReferenceCollector<'_, 'gcx> {
    fn push_reference(&mut self, span: Span, targets: Vec<SymbolId>) {
        if targets.is_empty() {
            return;
        }
        let Some(location) = proto::span_to_location(self.gcx.sess.source_map(), span) else {
            return;
        };
        self.tables.references.push(SymbolReference { location, targets });
    }

    fn symbol_ids_for_res(&self, res: impl IntoIterator<Item = Res>) -> Vec<SymbolId> {
        res.into_iter()
            .filter_map(|res| match res {
                Res::Item(item_id) => {
                    self.tables.symbols_by_key.get(&SymbolKey::Item(item_id)).copied()
                }
                Res::Namespace(_) | Res::Builtin(_) | Res::Err(_) => None,
            })
            .collect()
    }

    fn symbol_id_for_member(&self, member: ResolvedMember) -> Option<SymbolId> {
        match member {
            ResolvedMember::Res(Res::Item(item_id)) => {
                self.tables.symbols_by_key.get(&SymbolKey::Item(item_id)).copied()
            }
            ResolvedMember::StructField { struct_id, field_index } => {
                let field_id = self.gcx.hir.strukt(struct_id).fields.get(field_index).copied()?;
                self.tables
                    .symbols_by_key
                    .get(&SymbolKey::Item(ItemId::Variable(field_id)))
                    .copied()
            }
            ResolvedMember::EnumVariant { enum_id, variant_index } => self
                .tables
                .symbols_by_key
                .get(&SymbolKey::EnumVariant(enum_id, variant_index))
                .copied(),
            ResolvedMember::Res(Res::Namespace(_) | Res::Builtin(_) | Res::Err(_)) => None,
        }
    }

    fn symbol_ids_for_member_expr(&self, expr: &hir::Expr<'gcx>) -> Vec<SymbolId> {
        if let Some(member) = self.gcx.resolved_member(expr.id)
            && let Some(symbol_id) = self.symbol_id_for_member(member)
        {
            return vec![symbol_id];
        }

        if let Some(callee) = self.gcx.resolved_callee(expr.id) {
            let targets = self.symbol_ids_for_res([callee.res]);
            if !targets.is_empty() {
                return targets;
            }
        }

        Vec::new()
    }

    fn push_type_reference(&mut self, ty: &hir::Type<'gcx>) {
        if let TypeKind::Custom(item_id) = ty.kind
            && let Some(symbol_id) =
                self.tables.symbols_by_key.get(&SymbolKey::Item(item_id)).copied()
        {
            self.push_reference(ty.span, vec![symbol_id]);
        }
    }

    fn visit_using_directive(&mut self, using: &'gcx hir::UsingDirective<'gcx>) {
        for entry in using.entries {
            let targets = match entry.kind {
                UsingEntryKind::Library(contract_id) => {
                    self.symbol_ids_for_res([Res::Item(ItemId::Contract(contract_id))])
                }
                UsingEntryKind::Functions(functions) => self.symbol_ids_for_res(
                    functions
                        .iter()
                        .copied()
                        .map(|function_id| Res::Item(ItemId::Function(function_id))),
                ),
                UsingEntryKind::Err(_) => Vec::new(),
            };
            self.push_reference(entry.span, targets);
        }
        if let Some(ty) = &using.ty {
            self.visit_ty(ty);
        }
    }
}

impl<'gcx> hir::Visit<'gcx> for ReferenceCollector<'_, 'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_modifier(
        &mut self,
        modifier: &'gcx hir::Modifier<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        if let Some(symbol_id) =
            self.tables.symbols_by_key.get(&SymbolKey::Item(modifier.id)).copied()
        {
            self.push_reference(modifier.span.with_hi(modifier.args.span.lo()), vec![symbol_id]);
        }
        self.visit_call_args(&modifier.args)
    }

    fn visit_nested_contract(&mut self, id: hir::ContractId) -> ControlFlow<Self::BreakValue> {
        let previous_contract = self.contract.replace(id);
        let result = self.visit_contract(self.hir().contract(id));
        self.contract = previous_contract;
        result
    }

    fn visit_contract(
        &mut self,
        contract: &'gcx hir::Contract<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        for base in contract.bases_args {
            self.visit_modifier(base)?;
        }
        for using in contract.usings {
            self.visit_using_directive(using);
        }
        for &item in contract.items {
            if is_generated_item(self.gcx, item) {
                continue;
            }
            self.visit_nested_item(item)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        match expr.kind {
            hir::ExprKind::Ident(res) => {
                let targets = if let Some(callee) = self.gcx.resolved_callee(expr.id) {
                    self.symbol_ids_for_res([callee.res])
                } else {
                    self.symbol_ids_for_res(res.iter().copied())
                };
                self.push_reference(expr.span, targets);
            }
            hir::ExprKind::Member(receiver, ident) | hir::ExprKind::YulMember(receiver, ident) => {
                self.visit_expr(receiver)?;
                let targets = self.symbol_ids_for_member_expr(expr);
                self.push_reference(ident.span, targets);
            }
            _ => {
                hir::Visit::walk_expr(self, expr)?;
            }
        }
        ControlFlow::Continue(())
    }

    fn visit_ty(&mut self, ty: &'gcx hir::Type<'gcx>) -> ControlFlow<Self::BreakValue> {
        self.push_type_reference(ty);
        match ty.kind {
            TypeKind::Elementary(_) | TypeKind::Custom(_) | TypeKind::Err(_) => {}
            TypeKind::Array(array) => {
                self.visit_ty(&array.element)?;
                if let Some(size) = array.size {
                    self.visit_expr(size)?;
                }
            }
            TypeKind::Function(function) => {
                for &param in function.parameters {
                    self.visit_nested_var(param)?;
                }
                for &ret in function.returns {
                    self.visit_nested_var(ret)?;
                }
            }
            TypeKind::Mapping(mapping) => {
                self.visit_ty(&mapping.key)?;
                self.visit_ty(&mapping.value)?;
            }
        }
        ControlFlow::Continue(())
    }
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

fn sort_locations(locations: &mut [Location]) {
    locations.sort_by(|a, b| {
        a.uri.as_str().cmp(b.uri.as_str()).then_with(|| {
            (a.range.start.line, a.range.start.character, a.range.end.line, a.range.end.character)
                .cmp(&(
                    b.range.start.line,
                    b.range.start.character,
                    b.range.end.line,
                    b.range.end.character,
                ))
        })
    });
}

fn sort_completion_items(items: &mut [CompletionItem]) {
    items.sort_by(|a, b| a.label.cmp(&b.label));
}

fn range_contains(range: Range, position: Position) -> bool {
    if range.start == range.end {
        return position == range.start;
    }
    position >= range.start && position < range.end
}

fn range_size_key(range: Range) -> (u32, u32) {
    (
        range.end.line.saturating_sub(range.start.line),
        range.end.character.saturating_sub(range.start.character),
    )
}

fn member_access_prefix(line: &str, character: u32) -> Option<(&str, &str)> {
    let line_prefix = utf16_prefix(line, character)?;
    let trimmed = line_prefix.trim_end();
    let member_end = trimmed.len();
    let member_start = trimmed[..member_end]
        .char_indices()
        .rev()
        .find_map(|(index, ch)| (!is_identifier_continue(ch)).then_some(index + ch.len_utf8()))
        .unwrap_or(0);
    let member_prefix = &trimmed[member_start..member_end];
    if !member_prefix.is_empty() && !member_prefix.chars().next().is_some_and(is_identifier_start) {
        return None;
    }

    let before_dot = trimmed[..member_start].strip_suffix('.')?.trim_end();
    let receiver_end = before_dot.len();
    let receiver_start = before_dot[..receiver_end]
        .char_indices()
        .rev()
        .find_map(|(index, ch)| (!is_identifier_continue(ch)).then_some(index + ch.len_utf8()))
        .unwrap_or(0);
    let receiver = &before_dot[receiver_start..receiver_end];
    (!receiver.is_empty() && receiver.chars().next().is_some_and(is_identifier_start))
        .then_some((receiver, member_prefix))
}

fn utf16_prefix(line: &str, character: u32) -> Option<&str> {
    if character == 0 {
        return Some("");
    }

    let mut utf16_len = 0;
    for (index, ch) in line.char_indices() {
        if utf16_len == character {
            return Some(&line[..index]);
        }
        utf16_len += ch.len_utf16() as u32;
        if utf16_len > character {
            return None;
        }
    }
    (utf16_len == character).then_some(line)
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphabetic()
}

fn is_identifier_continue(ch: char) -> bool {
    is_identifier_start(ch) || ch.is_ascii_digit()
}

fn completion_item_kind(kind: SymbolKind) -> CompletionItemKind {
    match kind {
        SymbolKind::FILE => CompletionItemKind::FILE,
        SymbolKind::MODULE => CompletionItemKind::MODULE,
        SymbolKind::NAMESPACE | SymbolKind::PACKAGE => CompletionItemKind::MODULE,
        SymbolKind::CLASS => CompletionItemKind::CLASS,
        SymbolKind::METHOD => CompletionItemKind::METHOD,
        SymbolKind::PROPERTY => CompletionItemKind::PROPERTY,
        SymbolKind::FIELD => CompletionItemKind::FIELD,
        SymbolKind::CONSTRUCTOR => CompletionItemKind::CONSTRUCTOR,
        SymbolKind::ENUM => CompletionItemKind::ENUM,
        SymbolKind::INTERFACE => CompletionItemKind::INTERFACE,
        SymbolKind::FUNCTION => CompletionItemKind::FUNCTION,
        SymbolKind::VARIABLE => CompletionItemKind::VARIABLE,
        SymbolKind::CONSTANT => CompletionItemKind::CONSTANT,
        SymbolKind::STRING => CompletionItemKind::TEXT,
        SymbolKind::NUMBER => CompletionItemKind::VALUE,
        SymbolKind::BOOLEAN => CompletionItemKind::VALUE,
        SymbolKind::ARRAY => CompletionItemKind::VALUE,
        SymbolKind::OBJECT => CompletionItemKind::VALUE,
        SymbolKind::KEY => CompletionItemKind::VALUE,
        SymbolKind::NULL => CompletionItemKind::VALUE,
        SymbolKind::ENUM_MEMBER => CompletionItemKind::ENUM_MEMBER,
        SymbolKind::STRUCT => CompletionItemKind::STRUCT,
        SymbolKind::EVENT => CompletionItemKind::EVENT,
        SymbolKind::OPERATOR => CompletionItemKind::OPERATOR,
        SymbolKind::TYPE_PARAMETER => CompletionItemKind::TYPE_PARAMETER,
        _ => CompletionItemKind::TEXT,
    }
}

fn res_symbol_kind(gcx: Gcx<'_>, res: Res) -> SymbolKind {
    match res {
        Res::Item(item_id) => item_symbol_kind(gcx, item_id),
        Res::Namespace(_) => SymbolKind::MODULE,
        Res::Builtin(builtin) => {
            if builtin.members().is_some() {
                SymbolKind::MODULE
            } else {
                SymbolKind::FUNCTION
            }
        }
        Res::Err(_) => SymbolKind::NULL,
    }
}

fn completion_detail(gcx: Gcx<'_>, res: Res) -> Option<String> {
    match res {
        Res::Item(item_id) => Some(gcx.hir.item(item_id).description().to_string()),
        Res::Namespace(_) => Some("import namespace".to_string()),
        Res::Builtin(_) => Some("builtin".to_string()),
        Res::Err(_) => None,
    }
}

fn member_completion_item_kind(member: MemberCompletion<'_>) -> CompletionItemKind {
    if matches!(member.resolved, Some(ResolvedMember::EnumVariant { .. })) {
        return CompletionItemKind::ENUM_MEMBER;
    }
    if matches!(member.resolved, Some(ResolvedMember::StructField { .. })) {
        return CompletionItemKind::FIELD;
    }

    match member.member.res {
        Some(Res::Item(ItemId::Function(_))) => CompletionItemKind::METHOD,
        Some(Res::Item(ItemId::Variable(_))) | None => CompletionItemKind::FIELD,
        Some(Res::Item(ItemId::Contract(_))) | Some(Res::Namespace(_)) => {
            CompletionItemKind::MODULE
        }
        Some(Res::Item(ItemId::Struct(_))) => CompletionItemKind::STRUCT,
        Some(Res::Item(ItemId::Enum(_))) => CompletionItemKind::ENUM,
        Some(Res::Item(ItemId::Udvt(_))) => CompletionItemKind::TYPE_PARAMETER,
        Some(Res::Item(ItemId::Error(_) | ItemId::Event(_))) => CompletionItemKind::EVENT,
        Some(Res::Builtin(_)) => CompletionItemKind::METHOD,
        Some(Res::Err(_)) => CompletionItemKind::TEXT,
    }
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

fn item_has_definition(gcx: Gcx<'_>, item_id: ItemId) -> bool {
    match item_id {
        ItemId::Function(id) => gcx.hir.function(id).body.is_some(),
        _ => true,
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

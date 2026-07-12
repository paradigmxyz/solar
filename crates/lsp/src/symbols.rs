use lsp_types::{
    CompletionItem, CompletionItemKind, DocumentSymbol, GotoDefinitionResponse, InlayHint,
    Location, OneOf, Position, Range, SymbolInformation, SymbolKind, Url, WorkspaceSymbol,
};
use solar_interface::{
    Span,
    data_structures::{Never, index::IndexVec, map::FxHashMap, newtype_index},
};
use solar_sema::{
    Gcx,
    builtins::Builtin,
    hir::{
        self, ContractKind, EnumId, FunctionKind, ItemId, Res, StmtKind, TypeKind, UsingEntryKind,
        VarKind, Visit,
    },
    ty::{MemberCompletion, ResolvedMember, Ty},
};
use std::ops::ControlFlow;

use crate::{inlay_hints::InlayHintIndex, proto, signature_help::SignatureHelpIndex};

#[derive(Clone, Debug, Default)]
pub(crate) struct SymbolTables {
    declarations: IndexVec<SymbolId, DeclarationSymbol>,
    files: FxHashMap<Url, Vec<SymbolId>>,
    workspace_symbol_ids: Vec<SymbolId>,
    symbols_by_key: FxHashMap<SymbolKey, SymbolId>,
    scopes: IndexVec<ScopeId, Scope>,
    global_completions: Vec<CompletionItem>,
    builtin_member_completions: FxHashMap<String, Vec<CompletionItem>>,
    receiver_member_completions: FxHashMap<SymbolId, Vec<CompletionItem>>,
    member_completions: Vec<MemberCompletionScope>,
    file_member_completions: FxHashMap<Url, Vec<usize>>,
    file_scopes: FxHashMap<Url, Vec<ScopeId>>,
    references: Vec<SymbolReference>,
    file_references: FxHashMap<Url, Vec<usize>>,
    symbol_references: FxHashMap<SymbolId, Vec<Location>>,
    inlay_hints: InlayHintIndex,
    signature_help: SignatureHelpIndex,
}

newtype_index! {
    /// A declaration symbol ID in the LSP symbol table.
    pub(crate) struct SymbolId;

    /// A lexical scope ID in the LSP symbol table.
    pub(crate) struct ScopeId;
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
    symbol_id: SymbolId,
    available_from: Option<Position>,
}

#[derive(Clone, Debug)]
struct MemberCompletionScope {
    uri: Url,
    range: Range,
    items: Vec<CompletionItem>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CompletionContext<'a> {
    pub(crate) prefix: &'a str,
    pub(crate) member_receiver: Option<&'a str>,
}

impl<'a> CompletionContext<'a> {
    pub(crate) fn new(prefix: &'a str, member_receiver: Option<&'a str>) -> Self {
        Self { prefix, member_receiver }
    }
}

#[derive(Clone, Debug)]
struct SymbolReference {
    location: Location,
    targets: Vec<SymbolId>,
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
        tables.build_builtin_completions();
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
        tables.build_receiver_member_completions(gcx);
        tables.build_member_completions(gcx);
        tables.build_references(gcx);
        tables.inlay_hints = InlayHintIndex::build(gcx);
        tables.signature_help = SignatureHelpIndex::build(gcx);
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
        if self.global_completions.is_empty() {
            self.global_completions = std::mem::take(&mut other.global_completions);
        }
        if self.builtin_member_completions.is_empty() {
            self.builtin_member_completions = std::mem::take(&mut other.builtin_member_completions);
        }
        self.inlay_hints.extend(other.inlay_hints);
        self.signature_help.extend(other.signature_help);

        if other.declarations.is_empty() {
            return;
        }

        let symbol_offset = self.declarations.len();
        let scope_offset = self.scopes.len();
        for declaration in &mut other.declarations {
            declaration.id = remap_symbol_id(declaration.id, symbol_offset);
            declaration.parent =
                declaration.parent.map(|parent| remap_symbol_id(parent, symbol_offset));
        }
        for scope in &mut other.scopes {
            scope.parent = scope.parent.map(|parent| remap_scope_id(parent, scope_offset));
            for declaration in &mut scope.declarations {
                declaration.symbol_id = remap_symbol_id(declaration.symbol_id, symbol_offset);
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
        self.scopes.extend(other.scopes);
        self.receiver_member_completions.extend(
            other
                .receiver_member_completions
                .into_iter()
                .map(|(symbol_id, items)| (remap_symbol_id(symbol_id, symbol_offset), items)),
        );
        self.member_completions.extend(other.member_completions);
        self.references.extend(other.references);
        // HIR IDs are scoped to one compiler run, so this build-time map is not meaningful after
        // merging symbol tables from separate analysis batches.
        self.symbols_by_key.clear();
        self.rebuild_indexes();
    }

    pub(crate) fn inlay_hints(&self, uri: &Url, range: Range) -> Vec<InlayHint> {
        self.inlay_hints.hints(uri, range)
    }

    pub(crate) fn signature_help(
        &self,
        uri: &Url,
        position: Position,
        contents: &crop::Rope,
        label_offsets: bool,
        markdown_documentation: bool,
        signature_active_parameter: bool,
    ) -> Option<lsp_types::SignatureHelp> {
        self.signature_help.signature_help(
            uri,
            position,
            contents,
            label_offsets,
            markdown_documentation,
            signature_active_parameter,
        )
    }

    pub(crate) fn retain_signature_help_for_failed_files(&mut self, previous: &Self, uris: &[Url]) {
        self.signature_help.retain_failed_files(&previous.signature_help, uris);
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
        context: CompletionContext<'_>,
    ) -> Vec<CompletionItem> {
        if let Some(items) = self.member_completion_items(uri, position) {
            return filtered_completion_items(items, context.prefix);
        }
        if let Some(items) = self.builtin_member_completion_items(context.member_receiver) {
            return filtered_completion_items(items, context.prefix);
        }
        if let Some(items) =
            self.receiver_member_completion_items(uri, position, context.member_receiver)
        {
            return filtered_completion_items(items, context.prefix);
        }

        let Some(scope_id) = self.scope_at_position(uri, position) else {
            return Vec::new();
        };

        let mut seen = FxHashMap::<&str, SymbolId>::default();
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
                let symbol = &self.declarations[declaration.symbol_id];
                seen.entry(symbol.name.as_str()).or_insert(declaration.symbol_id);
            }
            scope = current.parent;
        }

        let mut items =
            seen.into_values().map(|symbol_id| self.completion_item(symbol_id)).collect::<Vec<_>>();
        items.extend(self.global_completions.iter().cloned());
        items.sort_by(|a, b| a.label.cmp(&b.label));
        items.dedup_by(|a, b| a.label == b.label);
        filter_completion_items(items, context.prefix)
    }

    fn build_builtin_completions(&mut self) {
        self.global_completions = Builtin::global().map(completion_item_for_builtin).collect();
        self.builtin_member_completions = Builtin::global()
            .filter_map(|builtin| {
                let mut items =
                    builtin.members()?.map(completion_item_for_builtin).collect::<Vec<_>>();
                sort_completion_items(&mut items);
                Some((builtin.name().to_string(), items))
            })
            .collect();
    }

    fn build_receiver_member_completions(&mut self, gcx: Gcx<'_>) {
        for variable_id in gcx.hir.variable_ids() {
            let Some(&symbol_id) =
                self.symbols_by_key.get(&SymbolKey::Item(ItemId::Variable(variable_id)))
            else {
                continue;
            };
            let variable = gcx.hir.variable(variable_id);
            let ty = gcx.type_of_item(ItemId::Variable(variable_id));
            let items =
                self.member_completion_items_for_ty(gcx, ty, variable.source, variable.contract);
            if !items.is_empty() {
                self.receiver_member_completions.insert(symbol_id, items);
            }
        }
    }

    fn member_completion_items_for_ty<'gcx>(
        &self,
        gcx: Gcx<'gcx>,
        ty: Ty<'gcx>,
        source: hir::SourceId,
        contract: Option<hir::ContractId>,
    ) -> Vec<CompletionItem> {
        let mut items = gcx
            .member_completions_of(ty, source, contract)
            .map(|member| self.completion_item_for_member(gcx, member))
            .collect::<Vec<_>>();
        sort_completion_items(&mut items);
        items.dedup_by(|a, b| a.label == b.label);
        items
    }

    fn push_declaration(&mut self, key: SymbolKey, declaration: DeclarationSymbol) -> SymbolId {
        let id = declaration.id;
        self.files.entry(declaration.location.uri.clone()).or_default().push(id);
        self.symbols_by_key.insert(key, id);
        let pushed_id = self.declarations.push(declaration);
        debug_assert_eq!(id, pushed_id);
        id
    }

    #[cfg(test)]
    fn push_test_declaration(&mut self, declaration: DeclarationSymbol) -> SymbolId {
        let id = declaration.id;
        self.files.entry(declaration.location.uri.clone()).or_default().push(id);
        let pushed_id = self.declarations.push(declaration);
        debug_assert_eq!(id, pushed_id);
        id
    }

    fn build_scopes(&mut self, gcx: Gcx<'_>) {
        let mut builder = ScopeBuilder { tables: self, gcx, scope: None };
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

    fn add_scope_declaration(&mut self, scope: ScopeId, item_id: ItemId) {
        if let Some(&symbol_id) = self.symbols_by_key.get(&SymbolKey::Item(item_id)) {
            self.add_symbol_to_scope(scope, symbol_id);
        }
    }

    fn add_local_scope_declaration(
        &mut self,
        gcx: Gcx<'_>,
        scope: ScopeId,
        item_id: ItemId,
        span: Span,
    ) {
        if let Some(&symbol_id) = self.symbols_by_key.get(&SymbolKey::Item(item_id)) {
            self.add_local_symbol_to_scope(gcx, scope, symbol_id, span);
        }
    }

    fn add_symbol_to_scope(&mut self, scope: ScopeId, symbol_id: SymbolId) {
        self.scopes[scope].declarations.push(ScopedDeclaration { symbol_id, available_from: None });
    }

    fn add_local_symbol_to_scope(
        &mut self,
        gcx: Gcx<'_>,
        scope: ScopeId,
        symbol_id: SymbolId,
        span: Span,
    ) {
        let available_from = proto::span_to_location(gcx.sess.source_map(), span)
            .map(|location| location.range.end)
            .unwrap_or(self.declarations[symbol_id].location.range.end);
        self.scopes[scope]
            .declarations
            .push(ScopedDeclaration { symbol_id, available_from: Some(available_from) });
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

    fn member_completion_items(&self, uri: &Url, position: Position) -> Option<&[CompletionItem]> {
        let completion = self
            .file_member_completions
            .get(uri)?
            .iter()
            .filter_map(|&index| {
                let completion = &self.member_completions[index];
                completion_range_contains(completion.range, position).then_some(completion)
            })
            .min_by_key(|completion| range_size_key(completion.range))?;
        Some(&completion.items)
    }

    fn builtin_member_completion_items(&self, receiver: Option<&str>) -> Option<&[CompletionItem]> {
        self.builtin_member_completions.get(receiver?).map(Vec::as_slice)
    }

    fn receiver_member_completion_items(
        &self,
        uri: &Url,
        position: Position,
        receiver: Option<&str>,
    ) -> Option<&[CompletionItem]> {
        let receiver = receiver?;
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
                let symbol_id = declaration.symbol_id;
                if self.declarations[symbol_id].name == receiver {
                    return self
                        .receiver_member_completions
                        .get(&symbol_id)
                        .map(Vec::as_slice)
                        .or(Some(&[]));
                }
            }
            scope = current.parent;
        }
        None
    }

    fn completion_item(&self, symbol_id: SymbolId) -> CompletionItem {
        let symbol = &self.declarations[symbol_id];
        CompletionItem {
            label: symbol.name.clone(),
            kind: Some(completion_item_kind(symbol.kind)),
            detail: self.container_name(symbol),
            ..Default::default()
        }
    }

    fn completion_item_for_member(
        &self,
        gcx: Gcx<'_>,
        member: MemberCompletion<'_>,
    ) -> CompletionItem {
        if let Some(symbol_id) = self.symbol_id_for_member_completion(gcx, member) {
            return self.completion_item(symbol_id);
        }

        CompletionItem {
            label: member.member.name.to_string(),
            kind: Some(member_completion_item_kind(gcx, member)),
            detail: member.member.attached.then_some("using for".to_string()),
            ..Default::default()
        }
    }

    fn selection_location(&self, symbol_id: SymbolId) -> Location {
        let symbol = &self.declarations[symbol_id];
        Location { uri: symbol.location.uri.clone(), range: symbol.name_range }
    }

    fn symbol_id_for_member_completion(
        &self,
        gcx: Gcx<'_>,
        member: MemberCompletion<'_>,
    ) -> Option<SymbolId> {
        if let Some(resolved) = member.resolved {
            return self.symbol_id_for_resolved_member(gcx, resolved);
        }
        member.member.res.and_then(|res| self.symbol_id_for_res(res))
    }

    fn symbol_id_for_res(&self, res: Res) -> Option<SymbolId> {
        match res {
            Res::Item(item_id) => self.symbols_by_key.get(&SymbolKey::Item(item_id)).copied(),
            Res::Namespace(_) | Res::Builtin(_) | Res::Err(_) => None,
        }
    }

    fn symbol_id_for_resolved_member(
        &self,
        gcx: Gcx<'_>,
        member: ResolvedMember,
    ) -> Option<SymbolId> {
        match member {
            ResolvedMember::Res(res) => self.symbol_id_for_res(res),
            ResolvedMember::StructField { struct_id, field_index } => {
                let field_id = gcx.hir.strukt(struct_id).fields.get(field_index).copied()?;
                self.symbols_by_key.get(&SymbolKey::Item(ItemId::Variable(field_id))).copied()
            }
            ResolvedMember::EnumVariant { enum_id, variant_index } => {
                self.symbols_by_key.get(&SymbolKey::EnumVariant(enum_id, variant_index)).copied()
            }
        }
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

        self.file_member_completions.clear();
        for (index, completion) in self.member_completions.iter().enumerate() {
            self.file_member_completions.entry(completion.uri.clone()).or_default().push(index);
        }
        for completions in self.file_member_completions.values_mut() {
            completions.sort_by_key(|&index| {
                let range = self.member_completions[index].range;
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

fn remap_scope_id(scope_id: ScopeId, offset: usize) -> ScopeId {
    ScopeId::from_usize(scope_id.index() + offset)
}

struct ScopeBuilder<'a, 'gcx> {
    tables: &'a mut SymbolTables,
    gcx: Gcx<'gcx>,
    scope: Option<ScopeId>,
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
        self.with_scope(root, |this| {
            for &item_id in source.items {
                this.tables.add_scope_declaration(root, item_id);
                let _ = this.visit_nested_item(item_id);
            }
        });
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
        self.with_scope(scope, |this| {
            for &item_id in contract.items {
                this.tables.add_scope_declaration(scope, item_id);
                let _ = this.visit_nested_item(item_id);
            }
        });
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
                this.tables.add_scope_declaration(scope, ItemId::Variable(param));
            }
            for &ret in function.returns {
                this.tables.add_scope_declaration(scope, ItemId::Variable(ret));
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
            self.tables.add_scope_declaration(scope, ItemId::Variable(field));
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
            self.tables.add_scope_declaration(scope, ItemId::Variable(param));
        }
        ControlFlow::Continue(())
    }

    fn visit_event(&mut self, event: &'gcx hir::Event<'gcx>) -> ControlFlow<Self::BreakValue> {
        let Some(scope) = self.push_child_scope(event.span) else {
            return ControlFlow::Continue(());
        };
        for &param in event.parameters {
            self.tables.add_scope_declaration(scope, ItemId::Variable(param));
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
                );
            }
            StmtKind::DeclMulti(vars, _) => {
                for var in vars.iter().copied().flatten() {
                    self.tables.add_local_scope_declaration(
                        self.gcx,
                        scope,
                        ItemId::Variable(var),
                        stmt.span,
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
                            this.tables.add_scope_declaration(clause_scope, ItemId::Variable(arg));
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

        let items = self.tables.member_completion_items_for_ty(
            self.gcx,
            receiver_ty,
            source,
            self.contract,
        );
        if items.is_empty() {
            return;
        }

        self.tables.member_completions.push(MemberCompletionScope {
            uri: location.uri,
            range: location.range,
            items,
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
            hir::ExprKind::Member(receiver, member) => {
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
        res.into_iter().filter_map(|res| self.tables.symbol_id_for_res(res)).collect()
    }

    fn symbol_ids_for_member_expr(&self, expr: &hir::Expr<'gcx>) -> Vec<SymbolId> {
        if let Some(member) = self.gcx.resolved_member(expr.id)
            && let Some(symbol_id) = self.tables.symbol_id_for_resolved_member(self.gcx, member)
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

fn completion_range_contains(range: Range, position: Position) -> bool {
    if range.start == range.end {
        return position == range.start;
    }
    position >= range.start && position <= range.end
}

fn range_size_key(range: Range) -> (u32, u32) {
    (
        range.end.line.saturating_sub(range.start.line),
        range.end.character.saturating_sub(range.start.character),
    )
}

fn member_completion_item_kind(gcx: Gcx<'_>, member: MemberCompletion<'_>) -> CompletionItemKind {
    match member.resolved {
        Some(ResolvedMember::EnumVariant { .. }) => CompletionItemKind::ENUM_MEMBER,
        Some(ResolvedMember::StructField { .. }) => CompletionItemKind::FIELD,
        Some(ResolvedMember::Res(_)) | None => match member.member.res {
            Some(Res::Item(item_id)) => completion_item_kind(item_symbol_kind(gcx, item_id)),
            Some(Res::Namespace(_)) => CompletionItemKind::MODULE,
            Some(Res::Builtin(_)) => CompletionItemKind::METHOD,
            Some(Res::Err(_)) | None => CompletionItemKind::FIELD,
        },
    }
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

fn completion_item_for_builtin(builtin: Builtin) -> CompletionItem {
    CompletionItem {
        label: builtin.name().to_string(),
        kind: Some(if builtin.members().is_some() {
            CompletionItemKind::MODULE
        } else {
            CompletionItemKind::FUNCTION
        }),
        ..Default::default()
    }
}

fn filter_completion_items(mut items: Vec<CompletionItem>, prefix: &str) -> Vec<CompletionItem> {
    let Some(prefix) = completion_filter_prefix(prefix) else { return items };
    items.retain(|item| fuzzy_completion_match(&prefix, &item.label));
    items
}

fn filtered_completion_items(items: &[CompletionItem], prefix: &str) -> Vec<CompletionItem> {
    let Some(prefix) = completion_filter_prefix(prefix) else { return items.to_vec() };
    items.iter().filter(|item| fuzzy_completion_match(&prefix, &item.label)).cloned().collect()
}

fn completion_filter_prefix(prefix: &str) -> Option<String> {
    (!prefix.is_empty()).then(|| prefix.to_lowercase())
}

fn fuzzy_completion_match(prefix: &str, label: &str) -> bool {
    let mut label_chars = label.chars().flat_map(char::to_lowercase);
    prefix
        .chars()
        .all(|prefix_char| label_chars.by_ref().any(|label_char| label_char == prefix_char))
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

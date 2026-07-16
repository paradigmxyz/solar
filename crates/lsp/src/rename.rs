use crate::{
    proto,
    symbols::{DeclarationSymbol, SymbolId},
};
use lsp_types::{Location, Position, Range, Url};
use solar_interface::{
    Ident, Span, Symbol,
    data_structures::{
        Never,
        index::IndexVec,
        map::{FxHashMap, FxHashSet},
        newtype_index,
    },
};
use solar_parse::{
    Lexer,
    ast::{self, ItemKind, visit::Visit},
};
use solar_sema::{
    Gcx,
    hir::{self, ItemId, VariableId},
};
use std::{collections::hash_map::Entry, sync::Arc};

newtype_index! {
    /// A file-local import alias in the rename index.
    struct ImportAliasId;

    /// A named mapping key or value in the rename index.
    struct MappingNameId;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum RenameTarget {
    Symbol(SymbolId),
    ImportAlias(ImportAliasId),
    MappingName(MappingNameId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ImportAlias {
    name: String,
    location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MappingName {
    name: String,
    location: Location,
}

#[derive(Clone, Debug)]
struct RenameOccurrence {
    location: Location,
    targets: Vec<RenameTarget>,
}

#[derive(Clone, Debug)]
pub(crate) struct RenameCandidate {
    pub(crate) old_name: String,
    pub(crate) range: Range,
    pub(crate) locations: Vec<Location>,
    pub(crate) analyzed_contents: FxHashMap<Url, Arc<String>>,
    pub(crate) conflicting_contents: bool,
    pub(crate) requires_yul_validation: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RenameIndex {
    aliases: IndexVec<ImportAliasId, ImportAlias>,
    mapping_names: IndexVec<MappingNameId, MappingName>,
    analyzed_contents: FxHashMap<Url, Arc<String>>,
    conflicting_contents: FxHashSet<Url>,
    symbol_targets: FxHashSet<SymbolId>,
    override_edges: Vec<(SymbolId, SymbolId)>,
    symbol_families: FxHashMap<SymbolId, usize>,
    yul_symbol_targets: FxHashSet<SymbolId>,
    occurrences: Vec<RenameOccurrence>,
    file_occurrences: FxHashMap<Url, Vec<usize>>,
    target_occurrences: FxHashMap<RenameTarget, Vec<Location>>,
    ambiguous_targets: FxHashSet<RenameTarget>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ImportBindingResolution {
    Symbol(SymbolId),
    Namespace(hir::SourceId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ImportBindingKey {
    source: hir::SourceId,
    resolution: ImportBindingResolution,
    name: Symbol,
}

#[derive(Default)]
pub(crate) struct ImportBindings {
    aliases: FxHashMap<ImportBindingKey, ImportAliasId>,
    symbol_sources: FxHashMap<SymbolId, hir::SourceId>,
}

#[derive(Default)]
pub(crate) struct MappingBindings {
    params: FxHashMap<VariableId, MappingNameId>,
}

#[derive(Clone, Copy)]
pub(crate) struct RenameReferenceContext<'a> {
    pub(crate) bindings: &'a ImportBindings,
    pub(crate) source: hir::SourceId,
    pub(crate) contract: Option<hir::ContractId>,
    pub(crate) item_symbols: &'a FxHashMap<ItemId, SymbolId>,
    pub(crate) declarations: &'a IndexVec<SymbolId, DeclarationSymbol>,
}

impl RenameIndex {
    pub(crate) fn record_source_contents(&mut self, gcx: Gcx<'_>) {
        for file in gcx.sess.source_map().files().iter() {
            let Some(path) = file.name.as_real() else { continue };
            let Ok(uri) = Url::from_file_path(path) else { continue };
            self.analyzed_contents.insert(uri, file.src.clone());
        }
    }

    pub(crate) fn add_symbol_declaration(&mut self, symbol_id: SymbolId, location: Location) {
        self.symbol_targets.insert(symbol_id);
        self.push_occurrence(location, vec![RenameTarget::Symbol(symbol_id)]);
    }

    pub(crate) fn build_imports(
        &mut self,
        gcx: Gcx<'_>,
        item_symbols: &FxHashMap<ItemId, SymbolId>,
    ) -> ImportBindings {
        let mut bindings = ImportBindings::default();
        for (&item_id, &symbol_id) in item_symbols {
            if self.symbol_targets.contains(&symbol_id) {
                bindings.symbol_sources.insert(symbol_id, gcx.hir.item(item_id).source());
            }
        }

        for (index, source) in gcx.hir.sources().enumerate() {
            let source_id = hir::SourceId::from_usize(index);
            let Some(ast) = gcx.sources[source_id].ast.as_ref() else { continue };
            for &(item_id, imported_source_id) in source.imports {
                let ItemKind::Import(import) = &ast.items[item_id].kind else { continue };
                match &import.items {
                    ast::ImportItems::Plain(alias) => {
                        if let Some(alias) = alias {
                            self.add_namespace_alias(
                                gcx,
                                &mut bindings,
                                source_id,
                                imported_source_id,
                                *alias,
                            );
                        }
                    }
                    ast::ImportItems::Glob(alias) => self.add_namespace_alias(
                        gcx,
                        &mut bindings,
                        source_id,
                        imported_source_id,
                        *alias,
                    ),
                    ast::ImportItems::Aliases(aliases) => {
                        for &(imported, alias) in aliases.iter() {
                            let symbols = imported_symbols(
                                gcx,
                                imported_source_id,
                                imported.name,
                                item_symbols,
                            )
                            .into_iter()
                            .filter(|symbol_id| self.symbol_targets.contains(symbol_id))
                            .collect::<Vec<_>>();
                            if symbols.is_empty() {
                                continue;
                            }

                            self.push_symbol_occurrence(gcx, imported.span, &symbols);
                            if let Some(alias) = alias
                                && let Some(alias_id) = self.add_alias(gcx, alias)
                            {
                                for symbol_id in symbols {
                                    bindings.aliases.insert(
                                        ImportBindingKey {
                                            source: source_id,
                                            resolution: ImportBindingResolution::Symbol(symbol_id),
                                            name: alias.name,
                                        },
                                        alias_id,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        bindings
    }

    pub(crate) fn build_mapping_names(&mut self, gcx: Gcx<'_>) -> MappingBindings {
        let mut bindings = MappingBindings::default();
        for variable_id in gcx.hir.variable_ids() {
            let variable = gcx.hir.variable(variable_id);
            let Some(getter_id) = variable.getter else { continue };
            let mut ty = &variable.ty;
            let mut params = gcx.hir.function(getter_id).parameters.iter().copied();
            let mut return_name = None;

            for param in &mut params {
                match ty.kind {
                    hir::TypeKind::Mapping(mapping) => {
                        if let Some(name) = mapping.key_name
                            && let Some(name_id) = self.add_mapping_name(gcx, name)
                        {
                            bindings.params.insert(param, name_id);
                        }
                        ty = &mapping.value;
                        return_name = mapping.value_name;
                    }
                    hir::TypeKind::Array(array) => ty = &array.element,
                    _ => break,
                }
            }

            if !matches!(ty.kind, hir::TypeKind::Custom(ItemId::Struct(_)))
                && let Some(name) = return_name
            {
                let _ = self.add_mapping_name(gcx, name);
            }
        }
        bindings
    }

    pub(crate) fn build_natspec(
        &mut self,
        gcx: Gcx<'_>,
        bindings: &ImportBindings,
        item_symbols: &FxHashMap<ItemId, SymbolId>,
        declarations: &IndexVec<SymbolId, DeclarationSymbol>,
    ) {
        for item_id in gcx.hir.item_ids() {
            let item = gcx.hir.item(item_id);
            let doc_id = item.doc();
            if doc_id.is_empty() {
                continue;
            }
            let ast_comments = gcx.hir.doc(doc_id).ast_comments();

            for natspec in gcx.natspec_doc_comments(doc_id) {
                if !ast_comments
                    .iter()
                    .flat_map(|comment| comment.natspec.iter())
                    .any(|local| local.span == natspec.span)
                {
                    continue;
                }
                match natspec.kind {
                    hir::NatSpecKind::Param { name } => {
                        let Some(parameters) = item.parameters() else { continue };
                        self.push_natspec_variable_reference(gcx, parameters, name, item_symbols);
                    }
                    hir::NatSpecKind::Return { name: Some(name) } => {
                        let Some(function_id) = item_id.as_function() else { continue };
                        self.push_natspec_variable_reference(
                            gcx,
                            gcx.hir.function(function_id).returns,
                            name,
                            item_symbols,
                        );
                    }
                    hir::NatSpecKind::Inheritdoc { contract } => {
                        let Some(contract_id) = gcx.natspec_contract(contract.name, item.source())
                        else {
                            continue;
                        };
                        let Some(&symbol_id) = item_symbols.get(&ItemId::Contract(contract_id))
                        else {
                            continue;
                        };
                        self.push_path_occurrences(
                            gcx,
                            RenameReferenceContext {
                                bindings,
                                source: item.source(),
                                contract: item.contract(),
                                item_symbols,
                                declarations,
                            },
                            contract.span,
                            &[symbol_id],
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    fn push_natspec_variable_reference(
        &mut self,
        gcx: Gcx<'_>,
        variables: &[VariableId],
        name: Ident,
        item_symbols: &FxHashMap<ItemId, SymbolId>,
    ) {
        let Some(variable_id) = variables.iter().copied().find(|&variable_id| {
            gcx.hir.variable(variable_id).name.is_some_and(|variable| variable.name == name.name)
        }) else {
            return;
        };
        let Some(&symbol_id) = item_symbols.get(&ItemId::Variable(variable_id)) else { return };
        self.push_symbol_occurrence(gcx, name.span, &[symbol_id]);
    }

    pub(crate) fn push_mapping_reference(
        &mut self,
        gcx: Gcx<'_>,
        bindings: &MappingBindings,
        param: VariableId,
        span: Span,
    ) -> bool {
        let Some(&name_id) = bindings.params.get(&param) else { return false };
        self.push_span_occurrence(gcx, span, vec![RenameTarget::MappingName(name_id)]);
        true
    }

    pub(crate) fn push_symbol_reference(
        &mut self,
        gcx: Gcx<'_>,
        context: RenameReferenceContext<'_>,
        span: Span,
        symbols: &[SymbolId],
    ) {
        let symbols = symbols
            .iter()
            .copied()
            .filter(|symbol_id| self.symbol_targets.contains(symbol_id))
            .collect::<Vec<_>>();
        if symbols.is_empty() {
            return;
        }
        self.push_path_occurrences(gcx, context, span, &symbols);
    }

    pub(crate) fn mark_yul_symbols(&mut self, symbols: &[SymbolId]) {
        self.yul_symbol_targets
            .extend(symbols.iter().copied().filter(|symbol| self.symbol_targets.contains(symbol)));
    }

    pub(crate) fn build_overrides(
        &mut self,
        gcx: Gcx<'_>,
        bindings: &ImportBindings,
        item_symbols: &FxHashMap<ItemId, SymbolId>,
        declarations: &IndexVec<SymbolId, DeclarationSymbol>,
    ) {
        let mut paths = OverridePathCollector::default();
        for source in gcx.sources.asts() {
            let _ = paths.visit_source_unit(source);
        }

        for function_id in gcx.hir.function_ids() {
            let function = gcx.hir.function(function_id);
            self.add_override_edges(gcx, function_id.into(), item_symbols);
            let key = function.name.map_or_else(|| function.keyword_span(), |name| name.span);
            let Some(function_paths) = paths.paths.get(&key) else { continue };
            for (path, &contract_id) in function_paths.iter().zip(function.overrides) {
                let Some(&symbol_id) = item_symbols.get(&ItemId::Contract(contract_id)) else {
                    continue;
                };
                self.push_path_occurrences(
                    gcx,
                    RenameReferenceContext {
                        bindings,
                        source: function.source,
                        contract: function.contract,
                        item_symbols,
                        declarations,
                    },
                    path_span(path),
                    &[symbol_id],
                );
            }
        }

        for variable_id in gcx.hir.variable_ids() {
            let variable = gcx.hir.variable(variable_id);
            if variable.getter.is_none() {
                continue;
            }
            self.add_override_edges(gcx, variable_id.into(), item_symbols);
            let Some(name) = variable.name else { continue };
            let Some(variable_paths) = paths.paths.get(&name.span) else { continue };
            for (path, &contract_id) in variable_paths.iter().zip(variable.overrides) {
                let Some(&symbol_id) = item_symbols.get(&ItemId::Contract(contract_id)) else {
                    continue;
                };
                self.push_path_occurrences(
                    gcx,
                    RenameReferenceContext {
                        bindings,
                        source: variable.source,
                        contract: variable.contract,
                        item_symbols,
                        declarations,
                    },
                    path_span(path),
                    &[symbol_id],
                );
            }
        }
    }

    fn add_override_edges(
        &mut self,
        gcx: Gcx<'_>,
        item: ItemId,
        item_symbols: &FxHashMap<ItemId, SymbolId>,
    ) {
        let Some(&symbol_id) = item_symbols.get(&item) else { return };
        self.override_edges.extend(
            gcx.base_override_items(item)
                .iter()
                .filter_map(|base| item_symbols.get(base).map(|&base| (symbol_id, base))),
        );
    }

    pub(crate) fn push_namespace_reference(
        &mut self,
        gcx: Gcx<'_>,
        bindings: &ImportBindings,
        source: hir::SourceId,
        span: Span,
        namespaces: impl IntoIterator<Item = hir::SourceId>,
    ) {
        let Some(ident) = identifiers_in_span(gcx, span).pop() else { return };
        let mut targets = namespaces
            .into_iter()
            .filter_map(|namespace| {
                bindings.aliases.get(&ImportBindingKey {
                    source,
                    resolution: ImportBindingResolution::Namespace(namespace),
                    name: ident.name,
                })
            })
            .copied()
            .map(RenameTarget::ImportAlias)
            .collect::<Vec<_>>();
        targets.sort_unstable();
        targets.dedup();
        self.push_span_occurrence(gcx, ident.span, targets);
    }

    pub(crate) fn candidate(
        &self,
        uri: &Url,
        position: Position,
        declarations: &IndexVec<SymbolId, DeclarationSymbol>,
    ) -> Option<RenameCandidate> {
        let occurrence = self.occurrence_at(uri, position)?;
        let &target = occurrence.targets.first()?;
        let targets = match target {
            RenameTarget::Symbol(symbol_id) => {
                let family = self.symbol_families.get(&symbol_id)?;
                self.symbol_targets
                    .iter()
                    .copied()
                    .filter(|candidate| self.symbol_families.get(candidate) == Some(family))
                    .map(RenameTarget::Symbol)
                    .collect::<Vec<_>>()
            }
            RenameTarget::ImportAlias(alias_id) => self
                .aliases
                .indices()
                .filter(|&candidate| self.aliases[alias_id] == self.aliases[candidate])
                .map(RenameTarget::ImportAlias)
                .collect(),
            RenameTarget::MappingName(name_id) => self
                .mapping_names
                .indices()
                .filter(|&candidate| self.mapping_names[name_id] == self.mapping_names[candidate])
                .map(RenameTarget::MappingName)
                .collect(),
        };
        if targets.iter().any(|target| self.ambiguous_targets.contains(target)) {
            return None;
        }

        let old_name = match target {
            RenameTarget::Symbol(symbol_id) => declarations[symbol_id].name.clone(),
            RenameTarget::ImportAlias(alias_id) => self.aliases[alias_id].name.clone(),
            RenameTarget::MappingName(name_id) => self.mapping_names[name_id].name.clone(),
        };
        let mut locations = targets
            .iter()
            .filter_map(|target| self.target_occurrences.get(target))
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        sort_locations(&mut locations);
        locations.dedup_by(|a, b| a.uri == b.uri && a.range == b.range);
        let mut analyzed_contents = FxHashMap::default();
        for location in &locations {
            if let Entry::Vacant(entry) = analyzed_contents.entry(location.uri.clone()) {
                let contents = self.analyzed_contents.get(&location.uri)?.clone();
                entry.insert(contents);
            }
        }
        let conflicting_contents =
            locations.iter().any(|location| self.conflicting_contents.contains(&location.uri));
        Some(RenameCandidate {
            old_name,
            range: occurrence.location.range,
            locations,
            analyzed_contents,
            conflicting_contents,
            requires_yul_validation: targets.iter().any(|target| match *target {
                RenameTarget::Symbol(symbol_id) => self.yul_symbol_targets.contains(&symbol_id),
                RenameTarget::ImportAlias(_) | RenameTarget::MappingName(_) => false,
            }),
        })
    }

    pub(crate) fn extend(&mut self, mut other: Self, symbol_offset: usize) {
        let alias_offset = self.aliases.len();
        let mapping_name_offset = self.mapping_names.len();
        self.symbol_targets.extend(
            other.symbol_targets.drain().map(|symbol_id| remap_symbol_id(symbol_id, symbol_offset)),
        );
        self.yul_symbol_targets.extend(
            other
                .yul_symbol_targets
                .drain()
                .map(|symbol_id| remap_symbol_id(symbol_id, symbol_offset)),
        );
        for occurrence in &mut other.occurrences {
            for target in &mut occurrence.targets {
                *target = match *target {
                    RenameTarget::Symbol(symbol_id) => {
                        RenameTarget::Symbol(remap_symbol_id(symbol_id, symbol_offset))
                    }
                    RenameTarget::ImportAlias(alias_id) => {
                        RenameTarget::ImportAlias(remap_alias_id(alias_id, alias_offset))
                    }
                    RenameTarget::MappingName(name_id) => RenameTarget::MappingName(
                        remap_mapping_name_id(name_id, mapping_name_offset),
                    ),
                };
            }
        }
        for (derived, base) in &mut other.override_edges {
            *derived = remap_symbol_id(*derived, symbol_offset);
            *base = remap_symbol_id(*base, symbol_offset);
        }
        self.conflicting_contents.extend(other.conflicting_contents);
        for (uri, contents) in other.analyzed_contents {
            if self.analyzed_contents.get(&uri).is_some_and(|existing| existing != &contents) {
                self.conflicting_contents.insert(uri.clone());
            }
            self.analyzed_contents.insert(uri, contents);
        }
        self.aliases.extend(other.aliases);
        self.mapping_names.extend(other.mapping_names);
        self.override_edges.extend(other.override_edges);
        self.occurrences.extend(other.occurrences);
    }

    pub(crate) fn rebuild(&mut self, declarations: &IndexVec<SymbolId, DeclarationSymbol>) {
        self.normalize_occurrences();
        self.rebuild_symbol_families(declarations);
        self.file_occurrences.clear();
        self.target_occurrences.clear();
        self.ambiguous_targets.clear();

        for (index, occurrence) in self.occurrences.iter().enumerate() {
            self.file_occurrences.entry(occurrence.location.uri.clone()).or_default().push(index);
            if occurrence.targets.len() > 1
                && !same_rename_targets(
                    &self.aliases,
                    &self.mapping_names,
                    &self.symbol_families,
                    &occurrence.targets,
                )
            {
                self.ambiguous_targets.extend(occurrence.targets.iter().copied());
            }
            for &target in &occurrence.targets {
                self.target_occurrences
                    .entry(target)
                    .or_default()
                    .push(occurrence.location.clone());
            }
        }

        for locations in self.target_occurrences.values_mut() {
            sort_locations(locations);
            locations.dedup_by(|a, b| a.uri == b.uri && a.range == b.range);
        }
    }

    fn rebuild_symbol_families(&mut self, declarations: &IndexVec<SymbolId, DeclarationSymbol>) {
        self.symbol_families.clear();
        let mut parents = (0..declarations.len()).collect::<Vec<_>>();
        let mut declarations_by_location = FxHashMap::default();
        for &symbol_id in &self.symbol_targets {
            let declaration = &declarations[symbol_id];
            let range = declaration.name_range;
            let key = (
                declaration.name.as_str(),
                declaration.location.uri.as_str(),
                range.start.line,
                range.start.character,
                range.end.line,
                range.end.character,
            );
            if let Some(&other) = declarations_by_location.get(&key) {
                union(&mut parents, symbol_id.index(), other);
            } else {
                declarations_by_location.insert(key, symbol_id.index());
            }
        }
        for &(derived, base) in &self.override_edges {
            union(&mut parents, derived.index(), base.index());
        }
        for &symbol_id in &self.symbol_targets {
            self.symbol_families.insert(symbol_id, find(&mut parents, symbol_id.index()));
        }
    }

    fn add_namespace_alias(
        &mut self,
        gcx: Gcx<'_>,
        bindings: &mut ImportBindings,
        source: hir::SourceId,
        imported_source: hir::SourceId,
        alias: Ident,
    ) {
        let Some(alias_id) = self.add_alias(gcx, alias) else { return };
        bindings.aliases.insert(
            ImportBindingKey {
                source,
                resolution: ImportBindingResolution::Namespace(imported_source),
                name: alias.name,
            },
            alias_id,
        );
    }

    fn add_alias(&mut self, gcx: Gcx<'_>, alias: Ident) -> Option<ImportAliasId> {
        let location = proto::span_to_location(gcx.sess.source_map(), alias.span)?;
        let alias_id =
            self.aliases.push(ImportAlias { name: alias.to_string(), location: location.clone() });
        self.push_occurrence(location, vec![RenameTarget::ImportAlias(alias_id)]);
        Some(alias_id)
    }

    fn add_mapping_name(&mut self, gcx: Gcx<'_>, name: Ident) -> Option<MappingNameId> {
        let location = proto::span_to_location(gcx.sess.source_map(), name.span)?;
        let name_id = self
            .mapping_names
            .push(MappingName { name: name.to_string(), location: location.clone() });
        self.push_occurrence(location, vec![RenameTarget::MappingName(name_id)]);
        Some(name_id)
    }

    fn push_symbol_occurrence(&mut self, gcx: Gcx<'_>, span: Span, symbols: &[SymbolId]) {
        self.push_span_occurrence(
            gcx,
            span,
            symbols.iter().copied().map(RenameTarget::Symbol).collect(),
        );
    }

    fn push_path_occurrences(
        &mut self,
        gcx: Gcx<'_>,
        context: RenameReferenceContext<'_>,
        span: Span,
        symbols: &[SymbolId],
    ) {
        let identifiers = identifiers_in_span(gcx, span);
        let Some((&final_ident, qualifiers)) = identifiers.split_last() else { return };
        let final_targets = symbols
            .iter()
            .filter_map(|&symbol_id| {
                target_for_ident(
                    context.bindings,
                    context.source,
                    symbol_id,
                    final_ident,
                    context.declarations,
                )
            })
            .collect::<Vec<_>>();
        if final_targets.is_empty() {
            return;
        }
        self.push_span_occurrence(gcx, final_ident.span, final_targets);
        if qualifiers.is_empty() {
            return;
        }

        let Some(resolutions) =
            gcx.source_path_resolutions(&identifiers, context.source, context.contract)
        else {
            return;
        };
        for (&ident, resolutions) in qualifiers.iter().zip(resolutions) {
            let targets = resolutions
                .into_iter()
                .filter_map(|resolution| match resolution {
                    hir::Res::Item(item_id) => context
                        .item_symbols
                        .get(&item_id)
                        .copied()
                        .filter(|symbol_id| self.symbol_targets.contains(symbol_id))
                        .and_then(|symbol_id| {
                            target_for_ident(
                                context.bindings,
                                context.source,
                                symbol_id,
                                ident,
                                context.declarations,
                            )
                        }),
                    hir::Res::Namespace(namespace) => context
                        .bindings
                        .aliases
                        .get(&ImportBindingKey {
                            source: context.source,
                            resolution: ImportBindingResolution::Namespace(namespace),
                            name: ident.name,
                        })
                        .copied()
                        .map(RenameTarget::ImportAlias),
                    hir::Res::Builtin(_) | hir::Res::Err(_) => None,
                })
                .collect();
            self.push_span_occurrence(gcx, ident.span, targets);
        }
    }

    fn push_span_occurrence(&mut self, gcx: Gcx<'_>, span: Span, mut targets: Vec<RenameTarget>) {
        targets.sort_unstable();
        targets.dedup();
        if targets.is_empty() {
            return;
        }
        let Some(location) = proto::span_to_location(gcx.sess.source_map(), span) else { return };
        self.push_occurrence(location, targets);
    }

    fn push_occurrence(&mut self, location: Location, targets: Vec<RenameTarget>) {
        self.occurrences.push(RenameOccurrence { location, targets });
    }

    fn occurrence_at(&self, uri: &Url, position: Position) -> Option<&RenameOccurrence> {
        self.file_occurrences
            .get(uri)?
            .iter()
            .filter_map(|&index| {
                let occurrence = &self.occurrences[index];
                range_contains(occurrence.location.range, position).then_some(occurrence)
            })
            .min_by_key(|occurrence| range_size_key(occurrence.location.range))
    }

    fn normalize_occurrences(&mut self) {
        self.occurrences.sort_by(|a, b| compare_locations(&a.location, &b.location));
        let mut normalized = Vec::<RenameOccurrence>::with_capacity(self.occurrences.len());
        for occurrence in self.occurrences.drain(..) {
            if let Some(previous) = normalized.last_mut()
                && previous.location == occurrence.location
            {
                previous.targets.extend(occurrence.targets);
                previous.targets.sort_unstable();
                previous.targets.dedup();
            } else {
                normalized.push(occurrence);
            }
        }
        self.occurrences = normalized;
    }
}

fn same_rename_targets(
    aliases: &IndexVec<ImportAliasId, ImportAlias>,
    mapping_names: &IndexVec<MappingNameId, MappingName>,
    symbol_families: &FxHashMap<SymbolId, usize>,
    targets: &[RenameTarget],
) -> bool {
    let Some(first) = targets.first().copied() else { return false };
    match first {
        RenameTarget::Symbol(first) => {
            let Some(family) = symbol_families.get(&first) else { return false };
            targets.iter().all(|target| {
                let RenameTarget::Symbol(symbol_id) = *target else { return false };
                symbol_families.get(&symbol_id) == Some(family)
            })
        }
        RenameTarget::MappingName(first) => targets.iter().all(|target| {
            let RenameTarget::MappingName(name_id) = *target else { return false };
            mapping_names[first] == mapping_names[name_id]
        }),
        RenameTarget::ImportAlias(first) => targets.iter().all(|target| {
            let RenameTarget::ImportAlias(alias_id) = *target else { return false };
            aliases[first] == aliases[alias_id]
        }),
    }
}

fn find(parents: &mut [usize], index: usize) -> usize {
    let parent = parents[index];
    if parent == index {
        index
    } else {
        let root = find(parents, parent);
        parents[index] = root;
        root
    }
}

fn union(parents: &mut [usize], a: usize, b: usize) {
    let a = find(parents, a);
    let b = find(parents, b);
    parents[a] = b;
}

fn imported_symbols(
    gcx: Gcx<'_>,
    source: hir::SourceId,
    name: Symbol,
    item_symbols: &FxHashMap<ItemId, SymbolId>,
) -> Vec<SymbolId> {
    gcx.hir
        .source(source)
        .items
        .iter()
        .filter_map(|&item_id| {
            let item = gcx.hir.item(item_id);
            (item.name()?.name == name).then(|| item_symbols.get(&item_id).copied()).flatten()
        })
        .collect()
}

#[derive(Default)]
struct OverridePathCollector {
    paths: FxHashMap<Span, Vec<Vec<Ident>>>,
}

impl<'ast> ast::visit::Visit<'ast> for OverridePathCollector {
    type BreakValue = Never;

    fn visit_item_function(
        &mut self,
        function: &'ast ast::ItemFunction<'ast>,
    ) -> std::ops::ControlFlow<Self::BreakValue> {
        if let Some(override_) = &function.header.override_ {
            let key = function.header.name.map_or_else(
                || {
                    function
                        .header
                        .span
                        .with_hi(function.header.span.lo() + function.kind.to_str().len() as u32)
                },
                |name| name.span,
            );
            self.paths
                .insert(key, override_.paths.iter().map(|path| path.segments().to_vec()).collect());
        }
        self.walk_item_function(function)
    }

    fn visit_variable_definition(
        &mut self,
        variable: &'ast ast::VariableDefinition<'ast>,
    ) -> std::ops::ControlFlow<Self::BreakValue> {
        if let Some(override_) = &variable.override_
            && let Some(name) = variable.name
        {
            self.paths.insert(
                name.span,
                override_.paths.iter().map(|path| path.segments().to_vec()).collect(),
            );
        }
        self.walk_variable_definition(variable)
    }
}

fn path_span(path: &[Ident]) -> Span {
    match path {
        [ident] => ident.span,
        [first, .., last] => first.span.with_hi(last.span.hi()),
        [] => Span::DUMMY,
    }
}

fn target_for_ident(
    bindings: &ImportBindings,
    source: hir::SourceId,
    symbol_id: SymbolId,
    ident: Ident,
    declarations: &IndexVec<SymbolId, DeclarationSymbol>,
) -> Option<RenameTarget> {
    if let Some(&alias_id) = bindings.aliases.get(&ImportBindingKey {
        source,
        resolution: ImportBindingResolution::Symbol(symbol_id),
        name: ident.name,
    }) {
        return Some(RenameTarget::ImportAlias(alias_id));
    }
    (declarations[symbol_id].name == ident.to_string()).then_some(RenameTarget::Symbol(symbol_id))
}

fn identifiers_in_span(gcx: Gcx<'_>, span: Span) -> Vec<Ident> {
    let Ok(source) = gcx.sess.source_map().span_to_snippet(span) else { return Vec::new() };
    Lexer::with_start_pos(gcx.sess, &source, span.lo()).filter_map(|token| token.ident()).collect()
}

fn remap_symbol_id(symbol_id: SymbolId, offset: usize) -> SymbolId {
    SymbolId::from_usize(symbol_id.index() + offset)
}

fn remap_alias_id(alias_id: ImportAliasId, offset: usize) -> ImportAliasId {
    ImportAliasId::from_usize(alias_id.index() + offset)
}

fn remap_mapping_name_id(name_id: MappingNameId, offset: usize) -> MappingNameId {
    MappingNameId::from_usize(name_id.index() + offset)
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

fn compare_locations(a: &Location, b: &Location) -> std::cmp::Ordering {
    a.uri.as_str().cmp(b.uri.as_str()).then_with(|| {
        (a.range.start.line, a.range.start.character, a.range.end.line, a.range.end.character).cmp(
            &(b.range.start.line, b.range.start.character, b.range.end.line, b.range.end.character),
        )
    })
}

fn sort_locations(locations: &mut [Location]) {
    locations.sort_by(compare_locations);
}

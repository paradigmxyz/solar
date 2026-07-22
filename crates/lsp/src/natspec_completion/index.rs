use lsp_types::Url;
use solar_interface::{
    Ident, Span,
    data_structures::map::{FxHashMap, FxHashSet},
    source_map::SourceFile,
};
use solar_parse::{
    Cursor,
    ast::{self, ItemKind},
};
use solar_sema::{
    Gcx,
    hir::{self, ItemId},
};
use std::{
    fmt::Write,
    path::PathBuf,
    sync::{Arc, OnceLock},
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum DeclarationPath {
    Source { item_ordinal: usize },
    Contract { contract_ordinal: usize, contract_name: Box<str>, item_ordinal: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TargetKind {
    Contract(ast::ContractKind),
    Function(ast::FunctionKind),
    Variable,
    Struct,
    Enum,
    Event,
    Error,
}

impl TargetKind {
    pub(crate) fn from_ast(item: &ast::Item<'_>) -> Option<Self> {
        Some(match &item.kind {
            ItemKind::Contract(contract) => Self::Contract(contract.kind),
            ItemKind::Function(function) => Self::Function(function.kind),
            ItemKind::Variable(_) => Self::Variable,
            ItemKind::Struct(_) => Self::Struct,
            ItemKind::Enum(_) => Self::Enum,
            ItemKind::Event(_) => Self::Event,
            ItemKind::Error(_) => Self::Error,
            ItemKind::Pragma(_) | ItemKind::Import(_) | ItemKind::Using(_) | ItemKind::Udvt(_) => {
                return None;
            }
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DeclarationKey {
    pub(crate) path: DeclarationPath,
    pub(crate) kind: TargetKind,
    pub(crate) name: Option<Box<str>>,
    pub(crate) header_fingerprint: Box<str>,
}

impl DeclarationKey {
    /// Creates the stable source identity shared by analysis-time and request-time parsing.
    pub(crate) fn from_ast(
        file: &SourceFile,
        path: DeclarationPath,
        item: &ast::Item<'_>,
    ) -> Option<Self> {
        let kind = TargetKind::from_ast(item)?;
        let span = match &item.kind {
            ItemKind::Function(function) => function.header.span,
            _ => item.span,
        };
        let source = source_for_span(file, span)?;
        Some(Self {
            path,
            kind,
            name: item.name().map(|name| Box::<str>::from(name.as_str())),
            header_fingerprint: syntax_fingerprint(source),
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NatSpecTargetSemantics {
    pub(crate) getter_returns: Vec<Option<String>>,
    pub(crate) inheritdoc_contracts: Vec<String>,
}

impl NatSpecTargetSemantics {
    fn is_empty(&self) -> bool {
        self.getter_returns.is_empty() && self.inheritdoc_contracts.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum IndexedSemantics {
    Unique(NatSpecTargetSemantics),
    Ambiguous,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct NatSpecCompletionIndex {
    // The source fingerprint ignores all trivia, including NatSpec comments. Keep indexed values
    // independent of documentation text so trivia-only edits can safely reuse this data.
    by_file: FxHashMap<PathBuf, IndexedFile>,
}

#[derive(Clone, Debug, Default)]
struct IndexedFile {
    source: Option<Arc<String>>,
    syntax_fingerprint: OnceLock<Box<str>>,
    entries: FxHashMap<DeclarationKey, IndexedSemantics>,
}

impl IndexedFile {
    fn new(source: Arc<String>) -> Self {
        Self { source: Some(source), ..Self::default() }
    }

    fn syntax_fingerprint(&self) -> Option<&str> {
        let source = self.source.as_deref()?;
        Some(self.syntax_fingerprint.get_or_init(|| syntax_fingerprint(source)))
    }

    fn has_same_syntax(&self, other: &Self) -> bool {
        match (&self.source, &other.source) {
            (Some(current), Some(incoming)) => {
                Arc::ptr_eq(current, incoming)
                    || current == incoming
                    || self.syntax_fingerprint() == other.syntax_fingerprint()
            }
            (None, None) => true,
            (Some(_), None) | (None, Some(_)) => false,
        }
    }

    fn mark_source_ambiguous(&mut self) {
        self.source = None;
        self.syntax_fingerprint = OnceLock::new();
    }
}

struct SemanticItem {
    item_id: ItemId,
    semantics: NatSpecTargetSemantics,
}

impl NatSpecCompletionIndex {
    pub(crate) fn build(gcx: Gcx<'_>) -> Self {
        let mut index = Self::default();
        let mut semantic_items_by_source =
            FxHashMap::<hir::SourceId, FxHashMap<Span, Vec<SemanticItem>>>::default();

        for source in gcx.hir.sources() {
            for &item_id in source.items {
                if let ItemId::Contract(contract_id) = item_id {
                    for &contract_item_id in gcx.hir.contract(contract_id).items {
                        collect_semantic_item(gcx, &mut semantic_items_by_source, contract_item_id);
                    }
                } else {
                    collect_semantic_item(gcx, &mut semantic_items_by_source, item_id);
                }
            }
        }

        for (source_id, source) in gcx.sources.iter_enumerated() {
            let Some(path) = source.file.name.as_real() else { continue };
            let Some(ast) = &source.ast else { continue };
            let mut indexed_file = IndexedFile::new(source.file.src.clone());
            if let Some(semantic_items) = semantic_items_by_source.get_mut(&source_id) {
                let mut remaining = semantic_items.values().map(Vec::len).sum::<usize>();

                'source_items: for (item_ordinal, item) in ast.items.iter().enumerate() {
                    let path = DeclarationPath::Source { item_ordinal };
                    remaining -= usize::from(Self::add_ast_item(
                        &mut indexed_file.entries,
                        gcx,
                        &source.file,
                        path,
                        item,
                        semantic_items,
                    ));
                    if remaining == 0 {
                        break;
                    }

                    let ItemKind::Contract(contract) = &item.kind else { continue };
                    for (contract_item_ordinal, contract_item) in contract.body.iter().enumerate() {
                        if !semantic_items.contains_key(&contract_item.span) {
                            continue;
                        }
                        let path = DeclarationPath::Contract {
                            contract_ordinal: item_ordinal,
                            contract_name: Box::from(contract.name.as_str()),
                            item_ordinal: contract_item_ordinal,
                        };
                        remaining -= usize::from(Self::add_ast_item(
                            &mut indexed_file.entries,
                            gcx,
                            &source.file,
                            path,
                            contract_item,
                            semantic_items,
                        ));
                        if remaining == 0 {
                            break 'source_items;
                        }
                    }
                }
            }
            index.by_file.insert(path.to_path_buf(), indexed_file);
        }
        index
    }

    pub(crate) fn extend(&mut self, other: Self) {
        use std::collections::hash_map::Entry;
        for (uri, mut incoming) in other.by_file {
            match self.by_file.entry(uri) {
                Entry::Vacant(entry) => {
                    entry.insert(incoming);
                }
                Entry::Occupied(mut entry) => {
                    let current = entry.get_mut();
                    if !current.has_same_syntax(&incoming) {
                        current.mark_source_ambiguous();
                    }
                    let current_keys = current.entries.keys().cloned().collect::<FxHashSet<_>>();
                    let incoming_keys = incoming.entries.keys().cloned().collect::<FxHashSet<_>>();
                    for key in current_keys.symmetric_difference(&incoming_keys) {
                        current.entries.insert(key.clone(), IndexedSemantics::Ambiguous);
                    }
                    for (key, semantics) in incoming.entries.drain() {
                        merge_entry(&mut current.entries, key, semantics);
                    }
                }
            }
        }
    }

    pub(crate) fn get(
        &self,
        uri: &Url,
        source_fingerprint: &str,
        key: &DeclarationKey,
    ) -> Option<&NatSpecTargetSemantics> {
        let path = uri.to_file_path().ok()?;
        let file = self.by_file.get(&path)?;
        let IndexedSemantics::Unique(semantics) = file.entries.get(key)? else { return None };
        if file.syntax_fingerprint() != Some(source_fingerprint) {
            return None;
        }
        Some(semantics)
    }

    pub(crate) fn source_fingerprint(&self, uri: &Url) -> Option<&str> {
        let path = uri.to_file_path().ok()?;
        self.by_file.get(&path)?.syntax_fingerprint()
    }

    fn add_ast_item(
        entries: &mut FxHashMap<DeclarationKey, IndexedSemantics>,
        gcx: Gcx<'_>,
        file: &SourceFile,
        path: DeclarationPath,
        ast_item: &ast::Item<'_>,
        semantic_items: &mut FxHashMap<Span, Vec<SemanticItem>>,
    ) -> bool {
        let Some(items) = semantic_items.get_mut(&ast_item.span) else { return false };
        let Some(index) = find_hir_item(gcx, items, ast_item) else { return false };
        let Some(key) = DeclarationKey::from_ast(file, path, ast_item) else {
            return false;
        };
        let semantic_item = items.swap_remove(index);
        merge_entry(entries, key, IndexedSemantics::Unique(semantic_item.semantics));
        true
    }
}

fn collect_semantic_item(
    gcx: Gcx<'_>,
    semantic_items_by_source: &mut FxHashMap<hir::SourceId, FxHashMap<Span, Vec<SemanticItem>>>,
    item_id: ItemId,
) {
    let eligible = match item_id {
        ItemId::Function(id) => {
            let function = gcx.hir.function(id);
            function.contract.is_some_and(|contract| !gcx.hir.contract(contract).bases.is_empty())
                && !function.is_yul
                && !function.is_getter()
                && !gcx.base_override_items(item_id).is_empty()
        }
        ItemId::Variable(id) => {
            let variable = gcx.hir.variable(id);
            variable.parent.is_none() && variable.getter.is_some()
        }
        _ => false,
    };
    if !eligible {
        return;
    }

    let semantics = target_semantics(gcx, item_id);
    if semantics.is_empty() {
        return;
    }
    let item = gcx.hir.item(item_id);
    semantic_items_by_source
        .entry(item.source())
        .or_default()
        .entry(item.span())
        .or_default()
        .push(SemanticItem { item_id, semantics });
}

fn find_hir_item(gcx: Gcx<'_>, items: &[SemanticItem], ast_item: &ast::Item<'_>) -> Option<usize> {
    let target_kind = TargetKind::from_ast(ast_item)?;
    let target_name = ast_item.name().map(|name| name.name);
    items.iter().position(|item| {
        let item_id = item.item_id;
        if hir_target_kind(gcx, item_id) != Some(target_kind)
            || gcx.hir.item(item_id).name().map(|name| name.name) != target_name
        {
            return false;
        }
        !matches!(item_id, ItemId::Variable(id) if gcx.hir.variable(id).parent.is_some())
    })
}

fn hir_target_kind(gcx: Gcx<'_>, item_id: ItemId) -> Option<TargetKind> {
    Some(match item_id {
        ItemId::Contract(id) => TargetKind::Contract(gcx.hir.contract(id).kind),
        ItemId::Function(id) => TargetKind::Function(gcx.hir.function(id).kind),
        ItemId::Variable(_) => TargetKind::Variable,
        ItemId::Struct(_) => TargetKind::Struct,
        ItemId::Enum(_) => TargetKind::Enum,
        ItemId::Event(_) => TargetKind::Event,
        ItemId::Error(_) => TargetKind::Error,
        ItemId::Udvt(_) => return None,
    })
}

fn target_semantics(gcx: Gcx<'_>, item_id: ItemId) -> NatSpecTargetSemantics {
    let getter_returns = match item_id {
        ItemId::Variable(id) => gcx.hir.variable(id).getter.map_or_else(Vec::new, |getter| {
            gcx.hir
                .function(getter)
                .returns
                .iter()
                .map(|&return_id| gcx.hir.variable(return_id).name.map(|name| name.to_string()))
                .collect()
        }),
        _ => Vec::new(),
    };

    let mut inheritdoc_contracts = inherited_contract_names(gcx, item_id);
    inheritdoc_contracts.sort_unstable();
    inheritdoc_contracts.dedup();
    NatSpecTargetSemantics { getter_returns, inheritdoc_contracts }
}

fn inherited_contract_names(gcx: Gcx<'_>, item_id: ItemId) -> Vec<String> {
    let source_id = gcx.hir.item(item_id).source();
    let mut pending = gcx.base_override_items(item_id).to_vec();
    let mut seen = FxHashSet::default();
    let mut names = Vec::new();
    while let Some(base_item) = pending.pop() {
        if !seen.insert(base_item) {
            continue;
        }
        if let Some(contract_id) = gcx.hir.item(base_item).contract()
            && let Some(name) = source_visible_contract_name(gcx, source_id, contract_id)
        {
            names.push(name);
        }
        pending.extend_from_slice(gcx.base_override_items(base_item));
    }
    names
}

fn source_visible_contract_name(
    gcx: Gcx<'_>,
    source_id: hir::SourceId,
    contract_id: hir::ContractId,
) -> Option<String> {
    let mut visiting = FxHashSet::default();
    let mut candidates =
        visible_contract_names_in_source(gcx, source_id, contract_id, &mut visiting);
    candidates.sort_unstable_by(|lhs, rhs| lhs.as_str().cmp(rhs.as_str()));
    candidates.dedup_by_key(|candidate| candidate.name);
    candidates
        .into_iter()
        .find_map(|candidate| visible_contract_name(gcx, source_id, contract_id, candidate))
}

fn visible_contract_names_in_source(
    gcx: Gcx<'_>,
    source_id: hir::SourceId,
    contract_id: hir::ContractId,
    visiting: &mut FxHashSet<hir::SourceId>,
) -> Vec<Ident> {
    if !visiting.insert(source_id) {
        return Vec::new();
    }

    let contract = gcx.hir.contract(contract_id);
    let mut names = if contract.source == source_id { vec![contract.name] } else { Vec::new() };
    if let Some(source) = gcx.sources.get(source_id)
        && let Some(ast) = &source.ast
    {
        for &(import_item_id, imported_source_id) in &source.imports {
            let ItemKind::Import(import) = &ast.items[import_item_id].kind else { continue };
            let imported_names =
                visible_contract_names_in_source(gcx, imported_source_id, contract_id, visiting);
            match &import.items {
                ast::ImportItems::Plain(None) => names.extend(imported_names),
                ast::ImportItems::Aliases(aliases) => {
                    for imported_name in imported_names {
                        names.extend(
                            aliases
                                .iter()
                                .filter(|(name, _)| name.name == imported_name.name)
                                .map(|(name, alias)| alias.unwrap_or(*name)),
                        );
                    }
                }
                ast::ImportItems::Plain(Some(_)) | ast::ImportItems::Glob(_) => {}
            }
        }
    }
    visiting.remove(&source_id);
    names
}

fn visible_contract_name(
    gcx: Gcx<'_>,
    source_id: hir::SourceId,
    contract_id: hir::ContractId,
    candidate: solar_interface::Ident,
) -> Option<String> {
    (gcx.natspec_contract(candidate.name, source_id) == Some(contract_id))
        .then(|| candidate.to_string())
}

fn merge_entry(
    entries: &mut FxHashMap<DeclarationKey, IndexedSemantics>,
    key: DeclarationKey,
    incoming: IndexedSemantics,
) {
    use std::collections::hash_map::Entry;
    match entries.entry(key) {
        Entry::Vacant(entry) => {
            entry.insert(incoming);
        }
        Entry::Occupied(mut entry) => {
            let same = matches!(
                (entry.get(), &incoming),
                (IndexedSemantics::Unique(current), IndexedSemantics::Unique(other))
                    if current == other
            );
            if !same {
                entry.insert(IndexedSemantics::Ambiguous);
            }
        }
    }
}

fn source_for_span(file: &SourceFile, span: Span) -> Option<&str> {
    let lo = span.lo().0.checked_sub(file.start_pos.0)? as usize;
    let hi = span.hi().0.checked_sub(file.start_pos.0)? as usize;
    file.src.get(lo..hi)
}

pub(crate) fn syntax_fingerprint(source: &str) -> Box<str> {
    let mut normalized = String::with_capacity(source.len());
    for (position, token) in Cursor::new(source).with_position() {
        if token.kind.is_trivial() {
            continue;
        }
        let end = position + token.len as usize;
        let Some(token_source) = source.get(position..end) else { continue };
        let _ = write!(normalized, "{}:{token_source}", token_source.len());
    }
    normalized.into_boxed_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri_path(uri: &Url) -> PathBuf {
        uri.to_file_path().unwrap()
    }

    #[test]
    fn token_fingerprint_ignores_trivia_but_preserves_literal_contents() {
        let compact =
            syntax_fingerprint(r#"function f(uint256 x) external returns (string memory)"#);
        let spaced = syntax_fingerprint(
            "function /* before name */ f ( uint256 x )\n// before visibility\nexternal returns(string memory)",
        );
        assert_eq!(compact, spaced);

        let comment_literal = syntax_fingerprint(r#"string constant X = \"/* not trivia */\";"#);
        let empty_literal = syntax_fingerprint(r#"string constant X = \"\";"#);
        assert_ne!(comment_literal, empty_literal);
    }

    #[test]
    fn conflicting_semantics_become_ambiguous() {
        let key = DeclarationKey {
            path: DeclarationPath::Source { item_ordinal: 0 },
            kind: TargetKind::Variable,
            name: Some(Box::from("value")),
            header_fingerprint: Box::from("fingerprint"),
        };
        let mut entries = FxHashMap::default();
        merge_entry(
            &mut entries,
            key.clone(),
            IndexedSemantics::Unique(NatSpecTargetSemantics {
                getter_returns: vec![Some("first".into())],
                inheritdoc_contracts: Vec::new(),
            }),
        );
        merge_entry(
            &mut entries,
            key.clone(),
            IndexedSemantics::Unique(NatSpecTargetSemantics {
                getter_returns: vec![Some("second".into())],
                inheritdoc_contracts: Vec::new(),
            }),
        );

        assert_eq!(entries.get(&key), Some(&IndexedSemantics::Ambiguous));
    }

    #[test]
    fn extending_empty_and_nonempty_semantics_becomes_ambiguous() {
        let uri = Url::parse("file:///Completion.sol").unwrap();
        let key = DeclarationKey {
            path: DeclarationPath::Source { item_ordinal: 0 },
            kind: TargetKind::Variable,
            name: Some(Box::from("value")),
            header_fingerprint: Box::from("header"),
        };
        let semantics = NatSpecTargetSemantics {
            getter_returns: vec![Some("result".into())],
            inheritdoc_contracts: Vec::new(),
        };
        let empty_file = IndexedFile::new(Arc::new("source".into()));
        let populated_file = IndexedFile {
            source: Some(Arc::new("source".into())),
            entries: [(key.clone(), IndexedSemantics::Unique(semantics))].into_iter().collect(),
            ..IndexedFile::default()
        };
        let path = uri_path(&uri);
        let mut index =
            NatSpecCompletionIndex { by_file: [(path.clone(), empty_file)].into_iter().collect() };

        index.extend(NatSpecCompletionIndex {
            by_file: [(path, populated_file)].into_iter().collect(),
        });

        assert_eq!(index.get(&uri, &syntax_fingerprint("source"), &key), None);
    }

    #[test]
    fn extending_with_a_new_file_preserves_its_syntax_fingerprint() {
        let uri = Url::parse("file:///Completion.sol").unwrap();
        let key = DeclarationKey {
            path: DeclarationPath::Source { item_ordinal: 0 },
            kind: TargetKind::Variable,
            name: Some(Box::from("value")),
            header_fingerprint: Box::from("header"),
        };
        let semantics =
            NatSpecTargetSemantics { getter_returns: vec![None], inheritdoc_contracts: Vec::new() };
        let mut incoming = NatSpecCompletionIndex::default();
        incoming.by_file.insert(
            uri_path(&uri),
            IndexedFile {
                source: Some(Arc::new("source".into())),
                entries: [(key.clone(), IndexedSemantics::Unique(semantics.clone()))]
                    .into_iter()
                    .collect(),
                ..IndexedFile::default()
            },
        );

        let mut index = NatSpecCompletionIndex::default();
        index.extend(incoming);

        assert_eq!(index.get(&uri, &syntax_fingerprint("source"), &key), Some(&semantics));
        assert_eq!(index.get(&uri, "stale", &key), None);
    }

    #[test]
    fn equivalent_file_uri_retrieves_semantics() {
        let uri = Url::from_file_path(std::env::temp_dir().join("Completion.sol")).unwrap();
        let equivalent_uri =
            Url::parse(&uri.as_str().replacen("Completion.sol", "%43ompletion.sol", 1)).unwrap();
        let key = DeclarationKey {
            path: DeclarationPath::Source { item_ordinal: 0 },
            kind: TargetKind::Variable,
            name: Some(Box::from("value")),
            header_fingerprint: Box::from("header"),
        };
        let semantics =
            NatSpecTargetSemantics { getter_returns: vec![None], inheritdoc_contracts: Vec::new() };
        let index = NatSpecCompletionIndex {
            by_file: [(
                uri_path(&uri),
                IndexedFile {
                    source: Some(Arc::new("source".into())),
                    entries: [(key.clone(), IndexedSemantics::Unique(semantics.clone()))]
                        .into_iter()
                        .collect(),
                    ..IndexedFile::default()
                },
            )]
            .into_iter()
            .collect(),
        };

        assert_ne!(uri, equivalent_uri);
        assert_eq!(uri.to_file_path(), equivalent_uri.to_file_path());
        assert_eq!(
            index.get(&equivalent_uri, &syntax_fingerprint("source"), &key),
            Some(&semantics)
        );
    }

    #[test]
    fn extending_identical_sources_keeps_fingerprint_lazy() {
        let uri = Url::parse("file:///Completion.sol").unwrap();
        let path = uri_path(&uri);
        let mut index = NatSpecCompletionIndex {
            by_file: [(path.clone(), IndexedFile::new(Arc::new("contract C {}".into())))]
                .into_iter()
                .collect(),
        };

        index.extend(NatSpecCompletionIndex {
            by_file: [(path.clone(), IndexedFile::new(Arc::new("contract C {}".into())))]
                .into_iter()
                .collect(),
        });

        assert!(index.by_file[&path].syntax_fingerprint.get().is_none());
    }

    #[test]
    fn source_fingerprint_is_computed_lazily() {
        let file = IndexedFile::new(Arc::new("contract C {}".into()));

        assert!(file.syntax_fingerprint.get().is_none());
        assert_eq!(file.syntax_fingerprint(), Some(syntax_fingerprint("contract C {}").as_ref()));
        assert!(file.syntax_fingerprint.get().is_some());
    }

    #[test]
    fn missing_semantics_do_not_compute_source_fingerprint() {
        let uri = Url::parse("file:///Completion.sol").unwrap();
        let path = uri_path(&uri);
        let key = DeclarationKey {
            path: DeclarationPath::Source { item_ordinal: 0 },
            kind: TargetKind::Variable,
            name: Some(Box::from("value")),
            header_fingerprint: Box::from("header"),
        };
        let index = NatSpecCompletionIndex {
            by_file: [(path.clone(), IndexedFile::new(Arc::new("contract C {}".into())))]
                .into_iter()
                .collect(),
        };

        assert_eq!(index.get(&uri, "unused", &key), None);
        assert!(index.by_file[&path].syntax_fingerprint.get().is_none());
    }
}

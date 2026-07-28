use lsp_types::{DocumentLink, Range, TextEdit, Url};
use solar_interface::{
    data_structures::map::{FxHashMap, FxHashSet},
    source_map::FileResolver,
};
use solar_parse::lexer::unescape::{StrKind, try_parse_string_literal};
use solar_sema::{Gcx, ast};
use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use crate::proto;

#[derive(Clone, Debug, Default)]
pub(crate) struct DocumentLinkIndex {
    by_file: FxHashMap<PathBuf, Vec<StoredDocumentLink>>,
    source_contents: FxHashMap<PathBuf, Arc<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileMove {
    pub(crate) old_path: PathBuf,
    pub(crate) new_path: PathBuf,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ImportEditPlan {
    pub(crate) changes: HashMap<Url, Vec<TextEdit>>,
    pub(crate) analyzed_contents: HashMap<Url, Arc<String>>,
}

#[derive(Clone, Debug)]
struct StoredDocumentLink {
    range: Range,
    directive_range: Range,
    import_path: String,
    import_style: ImportPathStyle,
    target: PathBuf,
}

#[derive(Clone, Debug)]
enum ImportPathStyle {
    Relative,
    Anchored { prefix: String, target_root: PathBuf },
    Opaque,
}

impl DocumentLinkIndex {
    pub(crate) fn build(gcx: Gcx<'_>, source_paths: &FxHashSet<PathBuf>) -> Self {
        let mut index = Self::default();
        let mut file_resolver = FileResolver::new(gcx.sess.source_map());
        file_resolver.configure_from_sess(gcx.sess);
        for source in gcx.sources.iter() {
            let Some(source_path) = source.file.name.as_real() else { continue };
            if !source_paths.contains(source_path) {
                continue;
            }
            let source_path = source_path.to_path_buf();
            let Some(ast) = &source.ast else { continue };
            for &(item_id, target_source_id) in &source.imports {
                let ast::ItemKind::Import(import) = &ast.items[item_id].kind else { continue };
                let Some(location) =
                    proto::span_to_location(gcx.sess.source_map(), import.path.span)
                else {
                    continue;
                };
                let Some(directive_location) =
                    proto::span_to_location(gcx.sess.source_map(), ast.items[item_id].span)
                else {
                    continue;
                };
                let Some(target) = gcx.sources.get(target_source_id) else { continue };
                let Some(target_path) = target.file.name.as_real() else { continue };
                let mut invalid_escape = false;
                let import_path =
                    try_parse_string_literal(import.path.value.as_str(), StrKind::Str, |_, _| {
                        invalid_escape = true
                    });
                if invalid_escape {
                    continue;
                }
                let Ok(import_path) = std::str::from_utf8(&import_path).map(str::to_owned) else {
                    continue;
                };
                index.source_contents.insert(source_path.clone(), source.file.src.clone());
                index.push(
                    source_path.clone(),
                    StoredDocumentLink {
                        range: location.range,
                        directive_range: directive_location.range,
                        import_style: ImportPathStyle::new(
                            &file_resolver,
                            &source_path,
                            target_path,
                            &import_path,
                        ),
                        import_path,
                        target: target_path.to_path_buf(),
                    },
                );
            }
        }
        index.sort();
        index
    }

    fn push(&mut self, source: PathBuf, link: StoredDocumentLink) {
        self.by_file.entry(source).or_default().push(link);
    }

    #[cfg(test)]
    pub(crate) fn insert_for_test(&mut self, source: PathBuf, range: Range, target: Url) {
        self.source_contents.insert(source.clone(), Arc::new(String::new()));
        self.push(
            source,
            StoredDocumentLink {
                range,
                directive_range: range,
                import_path: String::new(),
                import_style: ImportPathStyle::Opaque,
                target: target.to_file_path().unwrap(),
            },
        );
    }

    pub(crate) fn extend(&mut self, other: Self) {
        debug_assert!(other.by_file.keys().all(|path| !self.by_file.contains_key(path)));
        debug_assert!(
            other.source_contents.keys().all(|path| !self.source_contents.contains_key(path))
        );
        self.by_file.extend(other.by_file);
        self.source_contents.extend(other.source_contents);
    }

    pub(crate) fn links(&self, path: &Path) -> Vec<DocumentLink> {
        let Some(links) = self.by_file.get(path) else { return Vec::new() };
        links.iter().filter_map(StoredDocumentLink::to_lsp).collect()
    }

    pub(crate) fn rename_edits(&self, moves: &[FileMove]) -> ImportEditPlan {
        let mut changes = HashMap::new();
        for (source, imports) in &self.by_file {
            let moved_source = moved_path(source, moves);
            for import in imports {
                let moved_target = moved_path(&import.target, moves);
                if moved_source == *source && moved_target == import.target {
                    continue;
                }
                let Some(new_path) =
                    import.rewritten_path(source, &moved_source, &moved_target, moves)
                else {
                    continue;
                };
                if new_path == import.import_path {
                    continue;
                }
                let Ok(uri) = Url::from_file_path(source) else { continue };
                changes
                    .entry(uri)
                    .or_insert_with(Vec::new)
                    .push(TextEdit::new(import.range, serde_json::to_string(&new_path).unwrap()));
            }
        }
        self.edit_plan(changes)
    }

    pub(crate) fn delete_edits(&self, deleted_paths: &[PathBuf]) -> ImportEditPlan {
        let mut changes = HashMap::new();
        for (source, imports) in &self.by_file {
            if deleted_paths.iter().any(|deleted| source.starts_with(deleted)) {
                continue;
            }
            let Ok(uri) = Url::from_file_path(source) else { continue };
            for import in imports {
                if deleted_paths.iter().any(|deleted| import.target.starts_with(deleted)) {
                    changes
                        .entry(uri.clone())
                        .or_insert_with(Vec::new)
                        .push(TextEdit::new(import.directive_range, String::new()));
                }
            }
        }
        self.edit_plan(changes)
    }

    fn edit_plan(&self, changes: HashMap<Url, Vec<TextEdit>>) -> ImportEditPlan {
        let analyzed_contents = changes
            .keys()
            .filter_map(|uri| {
                let path = uri.to_file_path().ok()?;
                Some((uri.clone(), self.source_contents.get(&path)?.clone()))
            })
            .collect();
        ImportEditPlan { changes, analyzed_contents }
    }

    fn sort(&mut self) {
        for links in self.by_file.values_mut() {
            links.sort_unstable_by_key(|link| (link.range.start, link.range.end));
        }
    }
}

impl StoredDocumentLink {
    fn to_lsp(&self) -> Option<DocumentLink> {
        Some(DocumentLink {
            range: self.range,
            target: Some(Url::from_file_path(&self.target).ok()?),
            tooltip: None,
            data: None,
        })
    }

    fn rewritten_path(
        &self,
        source: &Path,
        moved_source: &Path,
        moved_target: &Path,
        moves: &[FileMove],
    ) -> Option<String> {
        if moved_target == self.target {
            return match self.import_style {
                ImportPathStyle::Relative => relative_import_path(moved_source, moved_target),
                ImportPathStyle::Anchored { .. } | ImportPathStyle::Opaque
                    if moved_source == source =>
                {
                    Some(self.import_path.clone())
                }
                ImportPathStyle::Anchored { .. } | ImportPathStyle::Opaque => {
                    relative_import_path(moved_source, moved_target)
                }
            };
        }

        if let ImportPathStyle::Anchored { prefix, target_root } = &self.import_style {
            let moved_target_root = moved_path(target_root, moves);
            let source_move = matching_move(source, moves).map(|(index, _)| index);
            let target_root_move = matching_move(target_root, moves).map(|(index, _)| index);
            let anchor_is_stable = (moved_source == source && moved_target_root == *target_root)
                || (source_move.is_some() && source_move == target_root_move);
            if anchor_is_stable && let Ok(suffix) = moved_target.strip_prefix(moved_target_root) {
                return Some(join_import_path(prefix, suffix));
            }
        }
        relative_import_path(moved_source, moved_target)
    }
}

impl ImportPathStyle {
    fn new(
        file_resolver: &FileResolver<'_>,
        source: &Path,
        target: &Path,
        import_path: &str,
    ) -> Self {
        let original = Path::new(import_path);
        if original.starts_with("./") || original.starts_with("../") {
            return Self::Relative;
        }

        let parent = file_resolver
            .try_base_path()
            .and_then(|base_path| source.strip_prefix(base_path).ok())
            .unwrap_or(source);
        let remapped = file_resolver.remap_path(original, Some(parent));
        let original_components = original.components().collect::<Vec<_>>();
        let remapped_components = remapped.components().collect::<Vec<_>>();
        let common = original_components
            .iter()
            .rev()
            .zip(remapped_components.iter().rev())
            .take_while(|(original, remapped)| original == remapped)
            .count();
        if common == 0 {
            return Self::Opaque;
        }

        let target_components = target.components().collect::<Vec<_>>();
        let remapped_suffix = &remapped_components[remapped_components.len() - common..];
        if target_components.len() < common
            || target_components[target_components.len() - common..] != *remapped_suffix
        {
            return Self::Opaque;
        }

        let prefix =
            components_to_import_path(&original_components[..original_components.len() - common]);
        let Some(target_root) = target.ancestors().nth(common).map(Path::to_path_buf) else {
            return Self::Opaque;
        };
        Self::Anchored { prefix, target_root }
    }
}

fn moved_path(path: &Path, moves: &[FileMove]) -> PathBuf {
    matching_move(path, moves).map_or_else(|| path.to_path_buf(), |(_, path)| path)
}

fn matching_move(path: &Path, moves: &[FileMove]) -> Option<(usize, PathBuf)> {
    moves
        .iter()
        .enumerate()
        .filter_map(|file_move| {
            let (index, file_move) = file_move;
            let suffix = path.strip_prefix(&file_move.old_path).ok()?;
            Some((index, file_move.old_path.components().count(), file_move.new_path.join(suffix)))
        })
        .max_by_key(|(_, components, _)| *components)
        .map(|(index, _, path)| (index, path))
}

fn relative_import_path(source: &Path, target: &Path) -> Option<String> {
    let source_dir = source.parent()?;
    let source_components = source_dir.components().collect::<Vec<_>>();
    let target_components = target.components().collect::<Vec<_>>();
    let common = source_components
        .iter()
        .zip(&target_components)
        .take_while(|(source, target)| source == target)
        .count();
    if common == 0 && (source_dir.is_absolute() || target.is_absolute()) {
        return None;
    }

    let mut relative = PathBuf::new();
    for component in &source_components[common..] {
        match component {
            Component::Normal(_) | Component::ParentDir => relative.push(".."),
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir => return None,
        }
    }
    for component in &target_components[common..] {
        match component {
            Component::Normal(value) => relative.push(value),
            Component::ParentDir => relative.push(".."),
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir => return None,
        }
    }

    let path = components_to_import_path(&relative.components().collect::<Vec<_>>());
    Some(if path.starts_with("../") { path } else { format!("./{path}") })
}

fn components_to_import_path(components: &[Component<'_>]) -> String {
    let absolute = components.first().is_some_and(|component| *component == Component::RootDir);
    let path = components
        .iter()
        .filter(|component| !matches!(component, Component::RootDir | Component::CurDir))
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if absolute { format!("/{path}") } else { path }
}

fn join_import_path(prefix: &str, suffix: &Path) -> String {
    let suffix = components_to_import_path(&suffix.components().collect::<Vec<_>>());
    match (prefix.is_empty(), suffix.is_empty()) {
        (true, _) => suffix,
        (_, true) => prefix.to_owned(),
        (false, false) => format!("{}/{suffix}", prefix.trim_end_matches('/')),
    }
}

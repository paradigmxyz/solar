use lsp_types::{TextEdit, Url};
use normalize_path::NormalizePath;
use solar_config::ImportRemapping;
use solar_interface::source_map::apply_import_remappings;
use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
};

use super::{
    DocumentLinkIndex, ImportEditPlan, ImportPathStyle, StoredDocumentLink,
    components_to_import_path,
};
use crate::file_operations::FileMoveBatch;

impl DocumentLinkIndex {
    pub(crate) fn rename_edits(&self, moves: &FileMoveBatch) -> ImportEditPlan {
        let mut changes = HashMap::new();
        for (source, imports) in &self.by_file {
            let moved_source = moved_path(source, moves);
            for import in imports {
                let moved_target = moved_path(&import.target, moves);
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
}

impl StoredDocumentLink {
    fn rewritten_path(
        &self,
        source: &Path,
        moved_source: &Path,
        moved_target: &Path,
        moves: &FileMoveBatch,
    ) -> Option<String> {
        if moved_target == self.target {
            match &self.import_style {
                ImportPathStyle::Relative if moved_source == source => {
                    return Some(self.import_path.clone());
                }
                ImportPathStyle::Relative => {
                    return relative_import_path(moved_source, moved_target);
                }
                ImportPathStyle::Opaque { resolver_root }
                    if moved_source == source
                        && resolver_root
                            .as_deref()
                            .is_none_or(|root| moved_path(root, moves) == root) =>
                {
                    return Some(self.import_path.clone());
                }
                ImportPathStyle::Opaque { .. } => {
                    return relative_import_path(moved_source, moved_target);
                }
                ImportPathStyle::Anchored { .. } => {}
            }
        }

        if let ImportPathStyle::Anchored {
            prefix,
            target_root,
            resolver_root,
            configuration_root,
            remappings,
        } = &self.import_style
        {
            let moved_target_root = moved_path(target_root, moves);
            let source_move = moves.map_path(source).map(|(id, _)| id);
            let target_root_move = moves.map_path(target_root).map(|(id, _)| id);
            let configuration_root_move = configuration_root
                .as_deref()
                .and_then(|root| moves.map_path(root))
                .map(|(id, _)| id);
            let resolver_root_is_unchanged =
                resolver_root.as_deref().is_none_or(|root| moved_path(root, moves) == root);
            if moved_source == source && moved_target == self.target && resolver_root_is_unchanged {
                return Some(self.import_path.clone());
            }
            let anchor_is_stable = (moved_source == source
                && moved_target_root == *target_root
                && resolver_root_is_unchanged)
                || (source_move.is_some()
                    && source_move == target_root_move
                    && source_move == configuration_root_move);
            if anchor_is_stable && let Ok(suffix) = moved_target.strip_prefix(moved_target_root) {
                let new_path = join_import_path(prefix, suffix);
                if anchored_import_resolves_to_target(
                    &new_path,
                    moved_source,
                    moved_target,
                    configuration_root.as_deref(),
                    remappings,
                    moves,
                ) {
                    return Some(new_path);
                }
            }
        }
        relative_import_path(moved_source, moved_target)
    }
}

fn anchored_import_resolves_to_target(
    import_path: &str,
    source: &Path,
    target: &Path,
    configuration_root: Option<&Path>,
    remappings: &[ImportRemapping],
    moves: &FileMoveBatch,
) -> bool {
    let Some(configuration_root) = configuration_root else { return false };
    let configuration_root = moved_path(configuration_root, moves);
    let parent = source.strip_prefix(&configuration_root).unwrap_or(source);
    let remapped = apply_import_remappings(remappings, Path::new(import_path), Some(parent));
    let resolved = if remapped.is_absolute() {
        remapped.as_ref().normalize()
    } else {
        configuration_root.join(remapped.as_ref()).normalize()
    };
    resolved == target.normalize()
}

fn moved_path(path: &Path, moves: &FileMoveBatch) -> PathBuf {
    moves.map_path(path).map_or_else(|| path.to_path_buf(), |(_, path)| path)
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

fn join_import_path(prefix: &str, suffix: &Path) -> String {
    let suffix = components_to_import_path(&suffix.components().collect::<Vec<_>>());
    match (prefix.is_empty(), suffix.is_empty()) {
        (true, _) => suffix,
        (_, true) => prefix.to_owned(),
        (false, false) => format!("{}/{suffix}", prefix.trim_end_matches('/')),
    }
}

use lsp_types::{DocumentLink, Url};
use solar_config::ImportRemapping;
use solar_interface::{data_structures::map::FxHashSet, source_map::FileResolver};
use solar_parse::lexer::unescape::{StrKind, try_parse_string_literal};
use solar_sema::{Gcx, ast};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use super::{DocumentLinkIndex, ImportPathStyle, StoredDocumentLink, components_to_import_path};
use crate::proto;

impl DocumentLinkIndex {
    pub(crate) fn build(gcx: Gcx<'_>, source_paths: &FxHashSet<PathBuf>) -> Self {
        let mut index = Self::default();
        let mut file_resolver = FileResolver::new(gcx.sess.source_map());
        file_resolver.configure_from_sess(gcx.sess);
        let remappings = Arc::<[ImportRemapping]>::from(gcx.sess.opts.import_remappings.clone());
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
                            remappings.clone(),
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
    pub(crate) fn insert_for_test(
        &mut self,
        source: PathBuf,
        range: lsp_types::Range,
        target: Url,
    ) {
        self.source_contents.insert(source.clone(), Arc::new(String::new()));
        self.push(
            source,
            StoredDocumentLink {
                range,
                directive_range: range,
                import_path: String::new(),
                import_style: ImportPathStyle::Opaque { resolver_root: None },
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
}

impl ImportPathStyle {
    fn new(
        file_resolver: &FileResolver<'_>,
        source: &Path,
        target: &Path,
        import_path: &str,
        remappings: Arc<[ImportRemapping]>,
    ) -> Self {
        let original = Path::new(import_path);
        if original.starts_with("./") || original.starts_with("../") {
            return Self::Relative;
        }

        let resolver_root = file_resolver.try_base_path().map(Path::to_path_buf);
        let parent = resolver_root
            .as_deref()
            .and_then(|base_path| source.strip_prefix(base_path).ok())
            .unwrap_or(source);
        let remapped = file_resolver.remap_path(original, Some(parent));
        let configuration_root = resolver_root
            .as_deref()
            .filter(|base_path| {
                !remapped.is_absolute()
                    && file_resolver.normalize(&base_path.join(remapped.as_ref())).as_ref()
                        == target
            })
            .map(Path::to_path_buf);
        let original_components = original.components().collect::<Vec<_>>();
        let remapped_components = remapped.components().collect::<Vec<_>>();
        let common = original_components
            .iter()
            .rev()
            .zip(remapped_components.iter().rev())
            .take_while(|(original, remapped)| original == remapped)
            .count();
        if common == 0 {
            return Self::Opaque { resolver_root };
        }

        let target_components = target.components().collect::<Vec<_>>();
        let remapped_suffix = &remapped_components[remapped_components.len() - common..];
        if target_components.len() < common
            || target_components[target_components.len() - common..] != *remapped_suffix
        {
            return Self::Opaque { resolver_root };
        }

        let prefix =
            components_to_import_path(&original_components[..original_components.len() - common]);
        let Some(target_root) = target.ancestors().nth(common).map(Path::to_path_buf) else {
            return Self::Opaque { resolver_root };
        };
        Self::Anchored { prefix, target_root, resolver_root, configuration_root, remappings }
    }
}

use lsp_types::{DocumentLink, Range, Url};
use solar_interface::{
    Span,
    data_structures::map::{FxHashMap, FxHashSet},
};
use solar_sema::{Gcx, ast};
use std::path::{Path, PathBuf};

use crate::proto;

#[derive(Clone, Debug, Default)]
pub(crate) struct DocumentLinkIndex {
    by_file: FxHashMap<PathBuf, Vec<StoredDocumentLink>>,
}

#[derive(Clone, Debug)]
struct StoredDocumentLink {
    range: Range,
    target: Url,
}

impl DocumentLinkIndex {
    pub(crate) fn build(gcx: Gcx<'_>, source_paths: &FxHashSet<PathBuf>) -> Self {
        let mut index = Self::default();
        for source in gcx.sources.iter() {
            let Some(source_path) = source.file.name.as_real() else { continue };
            if !source_paths.contains(source_path) {
                continue;
            }
            let source_path = source_path.to_path_buf();
            let Some(ast) = &source.ast else { continue };
            for &(item_id, target_source_id) in &source.imports {
                let ast::ItemKind::Import(import) = &ast.items[item_id].kind else { continue };
                let span = import.path.span;
                if span.hi().to_u32().saturating_sub(span.lo().to_u32()) < 2 {
                    continue;
                }
                let span = Span::new(span.lo() + 1, span.hi() - 1);
                let Some(location) = proto::span_to_location(gcx.sess.source_map(), span) else {
                    continue;
                };
                let Some(target) = gcx.sources.get(target_source_id) else { continue };
                let Some(target_path) = target.file.name.as_real() else { continue };
                let Ok(target_uri) = Url::from_file_path(target_path) else { continue };
                index.push(
                    source_path.clone(),
                    StoredDocumentLink { range: location.range, target: target_uri },
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
        self.push(source, StoredDocumentLink { range, target });
    }

    pub(crate) fn extend(&mut self, other: Self) {
        debug_assert!(other.by_file.keys().all(|path| !self.by_file.contains_key(path)));
        self.by_file.extend(other.by_file);
    }

    pub(crate) fn links(&self, path: &Path) -> Vec<DocumentLink> {
        let Some(links) = self.by_file.get(path) else { return Vec::new() };
        links.iter().map(StoredDocumentLink::to_lsp).collect()
    }

    fn sort(&mut self) {
        for links in self.by_file.values_mut() {
            links.sort_unstable_by_key(|link| (link.range.start, link.range.end));
        }
    }
}

impl StoredDocumentLink {
    fn to_lsp(&self) -> DocumentLink {
        DocumentLink {
            range: self.range,
            target: Some(self.target.clone()),
            tooltip: None,
            data: None,
        }
    }
}

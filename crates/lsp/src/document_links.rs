use lsp_types::{DocumentLink, Range, Url};
use solar_interface::{Span, data_structures::map::FxHashMap};
use solar_sema::{Gcx, ast};

use crate::proto;

#[derive(Clone, Debug, Default)]
pub(crate) struct DocumentLinkIndex {
    by_file: FxHashMap<Url, FxHashMap<Range, LinkTarget>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LinkTarget {
    Resolved(Url),
    Ambiguous,
}

impl DocumentLinkIndex {
    pub(crate) fn build(gcx: Gcx<'_>) -> Self {
        let mut index = Self::default();
        for source in gcx.sources.iter() {
            if source.file.name.as_real().is_none() {
                continue;
            }
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
                index.insert(location.uri, location.range, target_uri);
            }
        }
        index
    }

    fn insert(&mut self, source: Url, range: Range, target: Url) {
        let links = self.by_file.entry(source).or_default();
        merge_target(links, range, LinkTarget::Resolved(target));
    }

    #[cfg(test)]
    pub(crate) fn insert_for_test(&mut self, source: Url, range: Range, target: Url) {
        self.insert(source, range, target);
    }

    pub(crate) fn extend(&mut self, other: Self) {
        for (source, links) in other.by_file {
            let destination = self.by_file.entry(source).or_default();
            for (range, target) in links {
                merge_target(destination, range, target);
            }
        }
    }

    pub(crate) fn links(&self, uri: &Url) -> Vec<DocumentLink> {
        let Some(links) = self.by_file.get(uri) else { return Vec::new() };
        let mut links = links
            .iter()
            .filter_map(|(&range, target)| {
                let LinkTarget::Resolved(target) = target else { return None };
                Some(DocumentLink {
                    range,
                    target: Some(target.clone()),
                    tooltip: None,
                    data: None,
                })
            })
            .collect::<Vec<_>>();
        links.sort_by_key(|link| {
            (
                link.range.start.line,
                link.range.start.character,
                link.range.end.line,
                link.range.end.character,
            )
        });
        links
    }
}

fn merge_target(links: &mut FxHashMap<Range, LinkTarget>, range: Range, target: LinkTarget) {
    match links.get_mut(&range) {
        None => {
            links.insert(range, target);
        }
        Some(existing) if *existing == target => {}
        Some(existing) => *existing = LinkTarget::Ambiguous,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{DocumentLink, Position, Range, Url};

    #[test]
    fn identical_links_are_deduplicated() {
        let source = uri("file:///workspace/src/Main.sol");
        let target = uri("file:///workspace/src/Dependency.sol");
        let range = Range::new(Position::new(1, 8), Position::new(1, 24));
        let mut index = DocumentLinkIndex::default();

        index.insert(source.clone(), range, target.clone());
        index.insert(source.clone(), range, target.clone());

        assert_eq!(index.links(&source), vec![link(range, target)]);
    }

    #[test]
    fn conflicting_targets_are_omitted() {
        let source = uri("file:///workspace/src/Main.sol");
        let range = Range::new(Position::new(1, 8), Position::new(1, 24));
        let mut index = DocumentLinkIndex::default();

        index.insert(source.clone(), range, uri("file:///workspace-a/src/Dependency.sol"));
        index.insert(source.clone(), range, uri("file:///workspace-b/src/Dependency.sol"));

        assert!(index.links(&source).is_empty());
    }

    #[test]
    fn extending_deduplicates_equal_links_and_omits_conflicts() {
        let source = uri("file:///workspace/src/Main.sol");
        let first_range = Range::new(Position::new(1, 8), Position::new(1, 24));
        let second_range = Range::new(Position::new(2, 8), Position::new(2, 24));
        let shared_target = uri("file:///workspace/src/Shared.sol");
        let mut index = DocumentLinkIndex::default();
        index.insert(source.clone(), first_range, shared_target.clone());
        index.insert(source.clone(), second_range, uri("file:///workspace-a/src/Dependency.sol"));
        let mut other = DocumentLinkIndex::default();
        other.insert(source.clone(), first_range, shared_target.clone());
        other.insert(source.clone(), second_range, uri("file:///workspace-b/src/Dependency.sol"));

        index.extend(other);

        assert_eq!(index.links(&source), vec![link(first_range, shared_target)]);
    }

    fn uri(value: &str) -> Url {
        Url::parse(value).unwrap()
    }

    fn link(range: Range, target: Url) -> DocumentLink {
        DocumentLink { range, target: Some(target), tooltip: None, data: None }
    }
}

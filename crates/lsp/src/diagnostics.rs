use lsp_types::{Diagnostic, Url};
use solar_interface::data_structures::map::{FxHashMap, FxHashSet};
use std::path::PathBuf;

pub(crate) type DiagnosticMap = FxHashMap<Url, Vec<Diagnostic>>;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum DiagnosticOwner {
    Compiler,
    Flycheck { id: String, workspace: PathBuf },
}

#[derive(Default)]
pub(crate) struct DiagnosticStore {
    diagnostics: FxHashMap<DiagnosticOwner, DiagnosticMap>,
    published_uris: FxHashSet<Url>,
}

impl DiagnosticStore {
    pub(crate) fn replace_and_publish_batches(
        &mut self,
        owner: DiagnosticOwner,
        diagnostics: DiagnosticMap,
    ) -> Vec<(Url, Vec<Diagnostic>)> {
        let affected_uris = self.replace(owner, diagnostics);
        self.publish_batches(affected_uris)
    }

    fn replace(&mut self, owner: DiagnosticOwner, diagnostics: DiagnosticMap) -> FxHashSet<Url> {
        let mut affected_uris =
            FxHashSet::with_capacity_and_hasher(diagnostics.len(), Default::default());
        affected_uris.extend(diagnostics.keys().cloned());

        let previous = if diagnostics.is_empty() {
            self.diagnostics.remove(&owner)
        } else {
            self.diagnostics.insert(owner, diagnostics)
        };

        if let Some(previous) = previous {
            affected_uris.extend(previous.into_keys());
        }

        affected_uris
    }

    fn publish_batches(&mut self, affected_uris: FxHashSet<Url>) -> Vec<(Url, Vec<Diagnostic>)> {
        let mut owners = self.diagnostics.iter().collect::<Vec<_>>();
        owners.sort_by_key(|(owner, _)| *owner);

        let mut uris = affected_uris.into_iter().collect::<Vec<_>>();
        uris.sort_by(|lhs, rhs| lhs.as_str().cmp(rhs.as_str()));

        uris.into_iter()
            .filter_map(|uri| {
                let was_published = self.published_uris.contains(&uri);
                let mut has_entry = false;
                let mut diagnostics = Vec::new();

                for (_, owner_diagnostics) in &owners {
                    if let Some(uri_diagnostics) = owner_diagnostics.get(&uri) {
                        has_entry = true;
                        diagnostics.extend(uri_diagnostics.iter().cloned());
                    }
                }

                if diagnostics.is_empty() {
                    self.published_uris.remove(&uri);
                } else {
                    self.published_uris.insert(uri.clone());
                }

                (has_entry || was_published).then_some((uri, diagnostics))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Position, Range};

    fn diagnostic(message: &str) -> Diagnostic {
        Diagnostic::new_simple(
            Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 0, character: 1 },
            },
            message.into(),
        )
    }

    fn uri(path: &str) -> Url {
        Url::parse(&format!("file:///workspace/{path}")).unwrap()
    }

    #[test]
    fn publish_batches_merges_owners_for_same_uri() {
        let file = uri("src/Test.sol");
        let mut store = DiagnosticStore::default();

        let batches = store.replace_and_publish_batches(
            DiagnosticOwner::Compiler,
            DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("compiler")])]),
        );
        assert_eq!(batches.len(), 1);

        let batches = store.replace_and_publish_batches(
            DiagnosticOwner::Flycheck {
                id: "forge-lint".into(),
                workspace: PathBuf::from("/workspace"),
            },
            DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("lint")])]),
        );

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].0, file);
        assert_eq!(
            batches[0].1.iter().map(|diagnostic| diagnostic.message.as_str()).collect::<Vec<_>>(),
            ["compiler", "lint"]
        );
    }

    #[test]
    fn owner_replacement_does_not_clear_other_owners() {
        let file = uri("src/Test.sol");
        let mut store = DiagnosticStore::default();

        store.replace_and_publish_batches(
            DiagnosticOwner::Compiler,
            DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("compiler")])]),
        );
        store.replace_and_publish_batches(
            DiagnosticOwner::Flycheck {
                id: "forge-lint".into(),
                workspace: PathBuf::from("/workspace"),
            },
            DiagnosticMap::from_iter([(file, vec![diagnostic("lint")])]),
        );
        let batches = store.replace_and_publish_batches(
            DiagnosticOwner::Flycheck {
                id: "forge-lint".into(),
                workspace: PathBuf::from("/workspace"),
            },
            DiagnosticMap::default(),
        );

        assert_eq!(batches.len(), 1);
        assert_eq!(
            batches[0].1.iter().map(|diagnostic| diagnostic.message.as_str()).collect::<Vec<_>>(),
            ["compiler"]
        );
    }

    #[test]
    fn publish_batches_clears_stale_uris() {
        let first = uri("src/First.sol");
        let second = uri("src/Second.sol");
        let mut store = DiagnosticStore::default();

        let initial = store.replace_and_publish_batches(
            DiagnosticOwner::Compiler,
            DiagnosticMap::from_iter([(first.clone(), vec![diagnostic("first")])]),
        );
        assert_eq!(initial, vec![(first.clone(), vec![diagnostic("first")])]);

        let batches = store.replace_and_publish_batches(
            DiagnosticOwner::Compiler,
            DiagnosticMap::from_iter([(second.clone(), vec![diagnostic("second")])]),
        );

        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0], (first, Vec::new()));
        assert_eq!(batches[1], (second, vec![diagnostic("second")]));
    }

    #[test]
    fn owner_replacement_only_publishes_affected_uris() {
        let first = uri("src/First.sol");
        let second = uri("src/Second.sol");
        let mut store = DiagnosticStore::default();

        store.replace_and_publish_batches(
            DiagnosticOwner::Compiler,
            DiagnosticMap::from_iter([
                (first.clone(), vec![diagnostic("first")]),
                (second, vec![diagnostic("second")]),
            ]),
        );

        let batches = store.replace_and_publish_batches(
            DiagnosticOwner::Flycheck {
                id: "forge-lint".into(),
                workspace: PathBuf::from("/workspace"),
            },
            DiagnosticMap::from_iter([(first.clone(), vec![diagnostic("lint")])]),
        );

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].0, first);
        assert_eq!(
            batches[0].1.iter().map(|diagnostic| diagnostic.message.as_str()).collect::<Vec<_>>(),
            ["first", "lint"]
        );
    }
}

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
    pub(crate) fn replace(&mut self, owner: DiagnosticOwner, diagnostics: DiagnosticMap) {
        if diagnostics.is_empty() {
            self.diagnostics.remove(&owner);
        } else {
            self.diagnostics.insert(owner, diagnostics);
        }
    }

    pub(crate) fn publish_batches(&mut self) -> Vec<(Url, Vec<Diagnostic>)> {
        let mut merged = DiagnosticMap::default();
        let mut owners = self.diagnostics.keys().collect::<Vec<_>>();
        owners.sort();

        for owner in owners {
            for (uri, diagnostics) in &self.diagnostics[owner] {
                merged.entry(uri.clone()).or_default().extend(diagnostics.iter().cloned());
            }
        }

        let mut uris = merged.keys().cloned().collect::<FxHashSet<_>>();
        uris.extend(self.published_uris.iter().cloned());
        let mut uris = uris.into_iter().collect::<Vec<_>>();
        uris.sort_by(|lhs, rhs| lhs.as_str().cmp(rhs.as_str()));

        let mut next_published_uris = FxHashSet::default();
        let batches = uris
            .into_iter()
            .map(|uri| {
                let diagnostics = merged.remove(&uri).unwrap_or_default();
                if !diagnostics.is_empty() {
                    next_published_uris.insert(uri.clone());
                }
                (uri, diagnostics)
            })
            .collect();

        self.published_uris = next_published_uris;
        batches
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

        store.replace(
            DiagnosticOwner::Compiler,
            DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("compiler")])]),
        );
        store.replace(
            DiagnosticOwner::Flycheck {
                id: "forge-lint".into(),
                workspace: PathBuf::from("/workspace"),
            },
            DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("lint")])]),
        );

        let batches = store.publish_batches();

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

        store.replace(
            DiagnosticOwner::Compiler,
            DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("compiler")])]),
        );
        store.replace(
            DiagnosticOwner::Flycheck {
                id: "forge-lint".into(),
                workspace: PathBuf::from("/workspace"),
            },
            DiagnosticMap::from_iter([(file, vec![diagnostic("lint")])]),
        );
        store.replace(
            DiagnosticOwner::Flycheck {
                id: "forge-lint".into(),
                workspace: PathBuf::from("/workspace"),
            },
            DiagnosticMap::default(),
        );

        let batches = store.publish_batches();

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

        store.replace(
            DiagnosticOwner::Compiler,
            DiagnosticMap::from_iter([(first.clone(), vec![diagnostic("first")])]),
        );
        let initial = store.publish_batches();
        assert_eq!(initial, vec![(first.clone(), vec![diagnostic("first")])]);

        store.replace(
            DiagnosticOwner::Compiler,
            DiagnosticMap::from_iter([(second.clone(), vec![diagnostic("second")])]),
        );
        let batches = store.publish_batches();

        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0], (first, Vec::new()));
        assert_eq!(batches[1], (second, vec![diagnostic("second")]));
    }
}

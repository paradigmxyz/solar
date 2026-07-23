use lsp_types::{Diagnostic, Url};
use solar_interface::data_structures::map::{FxHashMap, FxHashSet};
use std::path::PathBuf;

pub(crate) type DiagnosticMap = FxHashMap<Url, Vec<Diagnostic>>;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum DiagnosticOwner {
    Compiler,
    Flycheck { id: String, workspace: PathBuf },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PullReport {
    Full { result_id: String, diagnostics: Vec<Diagnostic> },
    Unchanged { result_id: String },
}

#[derive(Clone, Debug)]
struct CachedReport {
    result_id: String,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Default)]
pub(crate) struct DiagnosticStore {
    diagnostics: FxHashMap<DiagnosticOwner, DiagnosticMap>,
    published_uris: FxHashSet<Url>,
    reports: FxHashMap<Url, CachedReport>,
    next_result_id: u64,
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

    pub(crate) fn clear_uris_and_publish_batches(
        &mut self,
        uris: impl IntoIterator<Item = Url>,
    ) -> Vec<(Url, Vec<Diagnostic>)> {
        let affected_uris = uris.into_iter().collect::<FxHashSet<_>>();
        if affected_uris.is_empty() {
            return Vec::new();
        }

        self.diagnostics.retain(|_, owner_diagnostics| {
            owner_diagnostics.retain(|uri, _| !affected_uris.contains(uri));
            !owner_diagnostics.is_empty()
        });

        self.publish_batches(affected_uris)
    }

    pub(crate) fn pull_report(
        &mut self,
        uri: &Url,
        previous_result_id: Option<&str>,
    ) -> PullReport {
        if !self.reports.contains_key(uri) {
            let result_id = self.next_result_id();
            self.reports.insert(uri.clone(), CachedReport { result_id, diagnostics: Vec::new() });
        }

        let report = self.reports.get(uri).expect("diagnostic report was inserted");
        if previous_result_id == Some(report.result_id.as_str()) {
            PullReport::Unchanged { result_id: report.result_id.clone() }
        } else {
            PullReport::Full {
                result_id: report.result_id.clone(),
                diagnostics: report.diagnostics.clone(),
            }
        }
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

        let updates = uris
            .into_iter()
            .map(|uri| {
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

                (uri, diagnostics, has_entry || was_published)
            })
            .collect::<Vec<_>>();

        for (uri, diagnostics, _) in &updates {
            self.refresh_report(uri, diagnostics);
        }

        updates
            .into_iter()
            .filter_map(|(uri, diagnostics, should_publish)| {
                should_publish.then_some((uri, diagnostics))
            })
            .collect()
    }

    fn refresh_report(&mut self, uri: &Url, diagnostics: &[Diagnostic]) {
        if self.reports.get(uri).is_some_and(|report| report.diagnostics == diagnostics) {
            return;
        }

        let result_id = self.next_result_id();
        self.reports
            .insert(uri.clone(), CachedReport { result_id, diagnostics: diagnostics.to_vec() });
    }

    fn next_result_id(&mut self) -> String {
        self.next_result_id =
            self.next_result_id.checked_add(1).expect("diagnostic result ID counter exhausted");
        format!("solar-{}", self.next_result_id)
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

    #[test]
    fn clearing_uris_removes_diagnostics_from_all_owners() {
        let file = uri("src/Deleted.sol");
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
            DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("lint")])]),
        );

        let batches = store.clear_uris_and_publish_batches([file.clone()]);

        assert_eq!(batches, vec![(file, Vec::new())]);
    }

    #[test]
    fn pull_report_returns_stable_empty_report() {
        let file = uri("src/Empty.sol");
        let mut store = DiagnosticStore::default();

        let PullReport::Full { result_id, diagnostics } = store.pull_report(&file, None) else {
            panic!("first pull should return a full report");
        };
        assert!(diagnostics.is_empty());

        assert_eq!(
            store.pull_report(&file, Some(&result_id)),
            PullReport::Unchanged { result_id: result_id.clone() }
        );

        let PullReport::Full { result_id: next_id, diagnostics } =
            store.pull_report(&file, Some("stale"))
        else {
            panic!("an unknown result ID should return a full report");
        };
        assert_eq!(next_id, result_id);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn pull_report_changes_id_only_when_diagnostics_change() {
        let file = uri("src/Test.sol");
        let mut store = DiagnosticStore::default();
        let owner = DiagnosticOwner::Compiler;

        store.replace_and_publish_batches(
            owner.clone(),
            DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("first")])]),
        );
        let PullReport::Full { result_id, diagnostics } = store.pull_report(&file, None) else {
            panic!("first pull should return a full report");
        };
        assert_eq!(diagnostics, vec![diagnostic("first")]);

        store.replace_and_publish_batches(
            owner.clone(),
            DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("first")])]),
        );
        assert_eq!(
            store.pull_report(&file, Some(&result_id)),
            PullReport::Unchanged { result_id: result_id.clone() }
        );

        store.replace_and_publish_batches(
            owner,
            DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("second")])]),
        );
        let PullReport::Full { result_id: next_id, diagnostics } =
            store.pull_report(&file, Some(&result_id))
        else {
            panic!("changed diagnostics should return a full report");
        };
        assert_ne!(next_id, result_id);
        assert_eq!(diagnostics, vec![diagnostic("second")]);
    }

    #[test]
    fn clearing_diagnostics_updates_pull_report_to_empty() {
        let file = uri("src/Deleted.sol");
        let mut store = DiagnosticStore::default();

        store.replace_and_publish_batches(
            DiagnosticOwner::Compiler,
            DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("compiler")])]),
        );
        let PullReport::Full { result_id, .. } = store.pull_report(&file, None) else {
            panic!("first pull should return a full report");
        };

        store.clear_uris_and_publish_batches([file.clone()]);
        let PullReport::Full { result_id: empty_id, diagnostics } =
            store.pull_report(&file, Some(&result_id))
        else {
            panic!("clearing diagnostics should return a full report");
        };
        assert_ne!(empty_id, result_id);
        assert!(diagnostics.is_empty());
        assert_eq!(
            store.pull_report(&file, Some(&empty_id)),
            PullReport::Unchanged { result_id: empty_id }
        );
    }

    #[test]
    fn pull_report_ids_are_independent_per_uri() {
        let first = uri("src/First.sol");
        let second = uri("src/Second.sol");
        let mut store = DiagnosticStore::default();

        store.replace_and_publish_batches(
            DiagnosticOwner::Compiler,
            DiagnosticMap::from_iter([
                (first.clone(), vec![diagnostic("first")]),
                (second.clone(), vec![diagnostic("second")]),
            ]),
        );
        let PullReport::Full { result_id: first_id, .. } = store.pull_report(&first, None) else {
            panic!("first pull should return a full report");
        };
        let PullReport::Full { result_id: second_id, .. } = store.pull_report(&second, None) else {
            panic!("first pull should return a full report");
        };

        store.replace_and_publish_batches(
            DiagnosticOwner::Compiler,
            DiagnosticMap::from_iter([
                (first.clone(), vec![diagnostic("changed")]),
                (second.clone(), vec![diagnostic("second")]),
            ]),
        );
        assert_eq!(
            store.pull_report(&second, Some(&second_id)),
            PullReport::Unchanged { result_id: second_id }
        );
        assert!(matches!(store.pull_report(&first, Some(&first_id)), PullReport::Full { .. }));
    }
}

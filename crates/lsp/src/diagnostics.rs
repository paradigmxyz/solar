use crate::file_operations::file_path_from_url;
use lsp_types::{Diagnostic, PreviousResultId, Url};
use normalize_path::NormalizePath;
use solar_interface::data_structures::map::{FxHashMap, FxHashSet};
use std::{borrow::Cow, path::PathBuf};

pub(crate) type DiagnosticMap = FxHashMap<Url, Vec<Diagnostic>>;
pub(crate) type AnalyzedDocuments = FxHashMap<Url, Option<i64>>;

const EMPTY_RESULT_ID: &str = "solar-empty";

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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WorkspacePullReport {
    pub(crate) uri: Url,
    pub(crate) version: Option<i64>,
    pub(crate) report: PullReport,
    pub(crate) is_stale: bool,
}

#[derive(Clone, Debug)]
struct CachedReport {
    result_id: String,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Default)]
pub(crate) struct DiagnosticStore {
    diagnostics: FxHashMap<DiagnosticOwner, DiagnosticMap>,
    reports: FxHashMap<Url, CachedReport>,
    analyzed_documents: AnalyzedDocuments,
    next_result_id: u64,
}

#[derive(Debug, Default)]
pub(crate) struct DiagnosticUpdate {
    pub(crate) batches: Vec<(Url, Vec<Diagnostic>)>,
    pub(crate) pull_reports_changed: bool,
    pub(crate) workspace_documents_changed: bool,
}

impl DiagnosticStore {
    pub(crate) fn replace_compiler_snapshot_and_publish_batches(
        &mut self,
        diagnostics: DiagnosticMap,
        analyzed_documents: AnalyzedDocuments,
    ) -> DiagnosticUpdate {
        let workspace_documents_changed = self.analyzed_documents.len() != analyzed_documents.len()
            || self.analyzed_documents.keys().any(|uri| !analyzed_documents.contains_key(uri));
        self.analyzed_documents = analyzed_documents;
        let affected_uris = self.replace(DiagnosticOwner::Compiler, diagnostics);
        let mut update = self.publish_batches(affected_uris);
        update.workspace_documents_changed = workspace_documents_changed;
        update
    }

    pub(crate) fn replace_and_publish_batches(
        &mut self,
        owner: DiagnosticOwner,
        diagnostics: DiagnosticMap,
    ) -> DiagnosticUpdate {
        let affected_uris = self.replace(owner, diagnostics);
        self.publish_batches(affected_uris)
    }

    pub(crate) fn clear_file_path_prefixes_retaining_and_publish_batches(
        &mut self,
        prefixes: &[PathBuf],
        retained_prefixes: &[PathBuf],
    ) -> DiagnosticUpdate {
        if prefixes.is_empty() {
            return DiagnosticUpdate::default();
        }

        let prefixes = prefixes.iter().map(|prefix| prefix.normalize()).collect::<Vec<_>>();
        let retained_prefixes =
            retained_prefixes.iter().map(|prefix| prefix.normalize()).collect::<Vec<_>>();
        let matches_prefix = |uri: &Url| {
            file_path_from_url(uri).is_some_and(|path| {
                let path = path.normalize();
                prefixes.iter().any(|prefix| path.starts_with(prefix))
                    && !retained_prefixes.iter().any(|prefix| path.starts_with(prefix))
            })
        };
        let mut affected_uris = self
            .reports
            .keys()
            .filter(|uri| matches_prefix(uri))
            .cloned()
            .collect::<FxHashSet<_>>();
        let previous_document_count = self.analyzed_documents.len();
        self.analyzed_documents.retain(|uri, _| !matches_prefix(uri));
        let workspace_documents_changed = self.analyzed_documents.len() != previous_document_count;
        self.diagnostics.retain(|_, owner_diagnostics| {
            owner_diagnostics.retain(|uri, _| {
                let retain = !matches_prefix(uri);
                if !retain {
                    affected_uris.insert(uri.clone());
                }
                retain
            });
            !owner_diagnostics.is_empty()
        });

        let mut update = self.publish_batches(affected_uris);
        update.workspace_documents_changed = workspace_documents_changed;
        update
    }

    pub(crate) fn clear_owners_and_publish_batches(
        &mut self,
        owners: impl IntoIterator<Item = DiagnosticOwner>,
    ) -> DiagnosticUpdate {
        let mut affected_uris = FxHashSet::default();
        for owner in owners {
            if let Some(diagnostics) = self.diagnostics.remove(&owner) {
                affected_uris.extend(diagnostics.into_keys());
            }
        }
        self.publish_batches(affected_uris)
    }

    pub(crate) fn pull_report(&self, uri: &Url, previous_result_id: Option<&str>) -> PullReport {
        Self::make_pull_report(self.reports.get(uri), previous_result_id.map(Cow::Borrowed))
    }

    fn make_pull_report(
        report: Option<&CachedReport>,
        previous_result_id: Option<Cow<'_, str>>,
    ) -> PullReport {
        let result_id = report.map_or(EMPTY_RESULT_ID, |report| report.result_id.as_str());
        match previous_result_id {
            Some(previous_result_id) if previous_result_id == result_id => {
                PullReport::Unchanged { result_id: previous_result_id.into_owned() }
            }
            _ => PullReport::Full {
                result_id: result_id.to_owned(),
                diagnostics: report.map_or_else(Vec::new, |report| report.diagnostics.clone()),
            },
        }
    }

    pub(crate) fn workspace_pull_reports(
        &self,
        previous_result_ids: Vec<PreviousResultId>,
    ) -> Vec<WorkspacePullReport> {
        let capacity =
            previous_result_ids.len().max(self.analyzed_documents.len() + self.reports.len());
        let mut documents = FxHashMap::with_capacity_and_hasher(capacity, Default::default());
        for PreviousResultId { uri, value } in previous_result_ids {
            documents.insert(normalize_file_uri(uri), Some(value));
        }
        for uri in self.analyzed_documents.keys().chain(self.reports.keys()) {
            if !documents.contains_key(uri) {
                documents.insert(uri.clone(), None);
            }
        }

        let mut documents = documents.into_iter().collect::<Vec<_>>();
        documents.sort_unstable_by(|(lhs, _), (rhs, _)| lhs.as_str().cmp(rhs.as_str()));

        let report_capacity =
            documents.len().min(self.analyzed_documents.len() + self.reports.len());
        let mut reports = Vec::with_capacity(report_capacity);
        for (uri, previous_result_id) in documents {
            let version = self.analyzed_documents.get(&uri).copied();
            let cached_report = self.reports.get(&uri);
            let is_current = version.is_some() || cached_report.is_some();
            if !is_current
                && previous_result_id
                    .as_deref()
                    .is_none_or(|result_id| result_id.is_empty() || result_id == EMPTY_RESULT_ID)
            {
                continue;
            }
            reports.push(WorkspacePullReport {
                version: version.flatten(),
                report: Self::make_pull_report(cached_report, previous_result_id.map(Cow::Owned)),
                uri,
                is_stale: !is_current,
            });
        }
        reports
    }

    pub(crate) fn update_analyzed_document_version(&mut self, uri: Url, version: i64) {
        let uri = normalize_file_uri(uri);
        if let Some(current) = self.analyzed_documents.get_mut(&uri) {
            *current = Some(version);
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

    fn publish_batches(&mut self, affected_uris: FxHashSet<Url>) -> DiagnosticUpdate {
        if affected_uris.is_empty() {
            return DiagnosticUpdate::default();
        }

        let Self { diagnostics: all_diagnostics, reports, next_result_id, .. } = self;
        let mut owners = all_diagnostics.iter().collect::<Vec<_>>();
        owners.sort_by_key(|(owner, _)| *owner);

        let mut uris = affected_uris.into_iter().collect::<Vec<_>>();
        uris.sort_by(|lhs, rhs| lhs.as_str().cmp(rhs.as_str()));

        let mut pull_reports_changed = false;
        let batches = uris
            .into_iter()
            .filter_map(|uri| {
                let mut has_entry = false;
                let mut diagnostics = Vec::new();

                for (_, owner_diagnostics) in &owners {
                    if let Some(uri_diagnostics) = owner_diagnostics.get(&uri) {
                        has_entry = true;
                        diagnostics.extend(uri_diagnostics.iter().cloned());
                    }
                }

                let previous = reports.get(&uri);
                let was_published = previous.is_some();
                let report_changed = previous
                    .map_or(!diagnostics.is_empty(), |report| report.diagnostics != diagnostics);
                pull_reports_changed |= report_changed;
                if diagnostics.is_empty() {
                    if was_published {
                        reports.remove(&uri);
                    }
                } else if report_changed {
                    let result_id = Self::next_result_id(next_result_id);
                    reports.insert(
                        uri.clone(),
                        CachedReport { result_id, diagnostics: diagnostics.clone() },
                    );
                }

                (has_entry || was_published).then_some((uri, diagnostics))
            })
            .collect();
        DiagnosticUpdate { batches, pull_reports_changed, workspace_documents_changed: false }
    }

    fn next_result_id(next_result_id: &mut u64) -> String {
        *next_result_id =
            next_result_id.checked_add(1).expect("diagnostic result ID counter exhausted");
        format!("solar-{next_result_id}")
    }
}

pub(crate) fn normalize_file_uri(uri: Url) -> Url {
    if uri.scheme() != "file" {
        return uri;
    }

    let path = uri.path();
    let is_windows_drive_root = cfg!(windows)
        && path.len() == 4
        && path.as_bytes()[0] == b'/'
        && path.as_bytes()[1].is_ascii_alphabetic()
        && path.as_bytes()[2] == b':'
        && path.as_bytes()[3] == b'/';
    let has_lowercase_windows_drive = cfg!(windows)
        && path.len() >= 3
        && path.as_bytes()[0] == b'/'
        && path.as_bytes()[1].is_ascii_lowercase()
        && path.as_bytes()[2] == b':';
    if uri.host_str().is_none()
        && uri.query().is_none()
        && uri.fragment().is_none()
        && path.starts_with('/')
        && !path.as_bytes().contains(&b'%')
        && !path.as_bytes().windows(2).any(|bytes| bytes == b"//")
        && (!path.ends_with('/') || path == "/" || is_windows_drive_root)
        && !has_lowercase_windows_drive
    {
        return uri;
    }
    uri.to_file_path().ok().and_then(|path| Url::from_file_path(path).ok()).unwrap_or(uri)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Position, PreviousResultId, Range};

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
        Url::from_file_path(std::env::temp_dir().join("solar-lsp-diagnostics").join(path)).unwrap()
    }

    fn round_trip_file_uri(uri: Url) -> Url {
        uri.to_file_path().ok().and_then(|path| Url::from_file_path(path).ok()).unwrap_or(uri)
    }

    #[test]
    fn file_uri_fast_path_matches_round_trip_normalization() {
        let mut uris = vec![
            uri("src/Canonical.sol"),
            Url::parse("file:///").unwrap(),
            Url::parse("file:///tmp/Encoded%20Name.sol").unwrap(),
            Url::parse("file:///tmp//Repeated.sol").unwrap(),
            Url::parse("file:///tmp/directory/").unwrap(),
            Url::parse("file://localhost/tmp/Hosted.sol").unwrap(),
            Url::parse("file:///tmp/Query.sol?version=1").unwrap(),
            Url::parse("file:///tmp/Fragment.sol#source").unwrap(),
        ];
        if cfg!(windows) {
            uris.extend([
                Url::parse("file:///C:/tmp/Canonical.sol").unwrap(),
                Url::parse("file:///C:/").unwrap(),
                Url::parse("file:///tmp/NoDrive.sol").unwrap(),
                Url::parse("file:///C%3A/tmp/EncodedDrive.sol").unwrap(),
                Url::parse("file://server/share/Hosted.sol").unwrap(),
            ]);
        }

        for uri in uris {
            assert_eq!(normalize_file_uri(uri.clone()), round_trip_file_uri(uri.clone()), "{uri}");
        }
    }

    #[test]
    fn normalize_file_uri_preserves_non_file_uris() {
        let uri = Url::parse("untitled:/tmp/Virtual.sol").unwrap();

        assert_eq!(normalize_file_uri(uri.clone()), uri);
    }

    #[cfg(windows)]
    #[test]
    fn normalize_file_uri_canonicalizes_lowercase_windows_drive() {
        let lowercase = Url::parse("file:///c:/tmp/Contract.sol").unwrap();
        let uppercase = Url::parse("file:///C:/tmp/Contract.sol").unwrap();

        assert_eq!(normalize_file_uri(lowercase), uppercase);
    }

    #[test]
    fn workspace_reports_include_clean_documents_and_reuse_result_ids() {
        let clean = uri("src/Clean.sol");
        let broken = uri("src/Broken.sol");
        let mut store = DiagnosticStore::default();
        store.replace_compiler_snapshot_and_publish_batches(
            DiagnosticMap::from_iter([(broken.clone(), vec![diagnostic("broken")])]),
            AnalyzedDocuments::from_iter([(clean.clone(), None), (broken.clone(), Some(7))]),
        );

        let reports = store.workspace_pull_reports(Vec::new());

        assert_eq!(reports.iter().map(|report| &report.uri).collect::<Vec<_>>(), [&broken, &clean]);
        assert_eq!(reports[0].version, Some(7));
        assert_eq!(reports[1].version, None);
        assert!(matches!(
            &reports[0].report,
            PullReport::Full { diagnostics, .. } if diagnostics.as_slice() == [diagnostic("broken")]
        ));
        assert!(matches!(
            &reports[1].report,
            PullReport::Full { diagnostics, .. } if diagnostics.is_empty()
        ));

        let previous = reports
            .iter()
            .map(|report| PreviousResultId {
                uri: report.uri.clone(),
                value: match &report.report {
                    PullReport::Full { result_id, .. } | PullReport::Unchanged { result_id } => {
                        result_id.clone()
                    }
                },
            })
            .collect::<Vec<_>>();
        let reports = store.workspace_pull_reports(previous);

        assert!(reports.iter().all(|report| matches!(report.report, PullReport::Unchanged { .. })));
    }

    #[test]
    fn workspace_reports_clear_removed_diagnostics_once() {
        let canonical_uri = uri("src/Stale.sol");
        let encoded_uri =
            Url::parse(&canonical_uri.as_str().replacen("Stale.sol", "%53tale.sol", 1)).unwrap();
        let mut store = DiagnosticStore::default();
        store.replace_compiler_snapshot_and_publish_batches(
            DiagnosticMap::from_iter([(
                canonical_uri.clone(),
                vec![diagnostic("stale diagnostic")],
            )]),
            AnalyzedDocuments::from_iter([(canonical_uri.clone(), None)]),
        );
        let [initial] = store.workspace_pull_reports(Vec::new()).try_into().unwrap();
        let PullReport::Full { result_id: stale_result_id, .. } = initial.report else {
            panic!("initial report should be full");
        };
        store.replace_compiler_snapshot_and_publish_batches(
            DiagnosticMap::default(),
            AnalyzedDocuments::default(),
        );

        let [cleared] = store
            .workspace_pull_reports(vec![PreviousResultId {
                uri: encoded_uri,
                value: stale_result_id,
            }])
            .try_into()
            .unwrap();

        assert_eq!(cleared.uri, canonical_uri);
        let PullReport::Full { result_id: empty_result_id, diagnostics } = cleared.report else {
            panic!("removed diagnostics should be cleared with a full report");
        };
        assert!(diagnostics.is_empty());
        assert_eq!(empty_result_id, EMPTY_RESULT_ID);
        assert!(
            store
                .workspace_pull_reports(vec![PreviousResultId {
                    uri: canonical_uri,
                    value: empty_result_id,
                }])
                .is_empty()
        );
    }

    #[test]
    fn publish_batches_merges_owners_for_same_uri() {
        let file = uri("src/Test.sol");
        let mut store = DiagnosticStore::default();

        let batches = store
            .replace_and_publish_batches(
                DiagnosticOwner::Compiler,
                DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("compiler")])]),
            )
            .batches;
        assert_eq!(batches.len(), 1);

        let batches = store
            .replace_and_publish_batches(
                DiagnosticOwner::Flycheck {
                    id: "forge-lint".into(),
                    workspace: PathBuf::from("/workspace"),
                },
                DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("lint")])]),
            )
            .batches;

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
        let batches = store
            .replace_and_publish_batches(
                DiagnosticOwner::Flycheck {
                    id: "forge-lint".into(),
                    workspace: PathBuf::from("/workspace"),
                },
                DiagnosticMap::default(),
            )
            .batches;

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

        let initial = store
            .replace_and_publish_batches(
                DiagnosticOwner::Compiler,
                DiagnosticMap::from_iter([(first.clone(), vec![diagnostic("first")])]),
            )
            .batches;
        assert_eq!(initial, vec![(first.clone(), vec![diagnostic("first")])]);

        let batches = store
            .replace_and_publish_batches(
                DiagnosticOwner::Compiler,
                DiagnosticMap::from_iter([(second.clone(), vec![diagnostic("second")])]),
            )
            .batches;

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

        let batches = store
            .replace_and_publish_batches(
                DiagnosticOwner::Flycheck {
                    id: "forge-lint".into(),
                    workspace: PathBuf::from("/workspace"),
                },
                DiagnosticMap::from_iter([(first.clone(), vec![diagnostic("lint")])]),
            )
            .batches;

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].0, first);
        assert_eq!(
            batches[0].1.iter().map(|diagnostic| diagnostic.message.as_str()).collect::<Vec<_>>(),
            ["first", "lint"]
        );
    }

    #[test]
    fn clearing_file_path_prefixes_removes_descendant_diagnostics_from_all_owners() {
        let deleted = uri("pkg/Deleted.sol");
        let nested = uri("pkg/nested/Dependency.sol");
        let sibling = uri("pkg2/Keep.sol");
        let unrelated = uri("other/Keep.sol");
        let non_file = Url::parse("untitled:Keep.sol").unwrap();
        let file_uri = uri("pkg/Keep.sol");
        let hierarchical_non_file =
            Url::parse(&file_uri.as_str().replacen("file:", "untitled:", 1)).unwrap();
        let mut store = DiagnosticStore::default();

        store.replace_and_publish_batches(
            DiagnosticOwner::Compiler,
            DiagnosticMap::from_iter([
                (deleted.clone(), vec![diagnostic("deleted compiler")]),
                (nested.clone(), vec![diagnostic("nested compiler")]),
                (sibling.clone(), vec![diagnostic("sibling")]),
                (non_file.clone(), vec![diagnostic("non-file")]),
                (hierarchical_non_file.clone(), vec![diagnostic("hierarchical non-file")]),
            ]),
        );
        let flycheck = DiagnosticOwner::Flycheck {
            id: "forge-lint".into(),
            workspace: PathBuf::from("/workspace"),
        };
        store.replace_and_publish_batches(
            flycheck.clone(),
            DiagnosticMap::from_iter([
                (nested.clone(), vec![diagnostic("nested lint")]),
                (unrelated.clone(), vec![diagnostic("unrelated")]),
            ]),
        );

        let prefix = uri("pkg").to_file_path().unwrap();
        let batches =
            store.clear_file_path_prefixes_retaining_and_publish_batches(&[prefix], &[]).batches;

        assert_eq!(batches, vec![(deleted.clone(), Vec::new()), (nested.clone(), Vec::new())]);
        assert!(store.diagnostics.values().all(|diagnostics| {
            !diagnostics.contains_key(&deleted) && !diagnostics.contains_key(&nested)
        }));
        assert!(store.diagnostics[&DiagnosticOwner::Compiler].contains_key(&sibling));
        assert!(store.diagnostics[&DiagnosticOwner::Compiler].contains_key(&non_file));
        assert!(store.diagnostics[&DiagnosticOwner::Compiler].contains_key(&hierarchical_non_file));
        assert!(store.diagnostics[&flycheck].contains_key(&unrelated));
    }

    #[test]
    fn clearing_file_path_prefixes_normalizes_diagnostic_paths() {
        let prefix = uri("lib").to_file_path().unwrap();
        let path = prefix.parent().unwrap().join("src").join("..").join("lib/Dependency.sol");
        let file = Url::from_file_path(path).unwrap();
        let mut store = DiagnosticStore::default();
        store.replace_and_publish_batches(
            DiagnosticOwner::Compiler,
            DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("stale")])]),
        );

        let batches =
            store.clear_file_path_prefixes_retaining_and_publish_batches(&[prefix], &[]).batches;

        assert_eq!(batches, vec![(file, Vec::new())]);
    }

    #[test]
    fn clearing_owners_publishes_only_final_merged_batches() {
        let file = uri("src/Test.sol");
        let mut store = DiagnosticStore::default();
        let first = DiagnosticOwner::Flycheck {
            id: "first".into(),
            workspace: PathBuf::from("/workspace"),
        };
        let second = DiagnosticOwner::Flycheck {
            id: "second".into(),
            workspace: PathBuf::from("/workspace"),
        };
        store.replace_and_publish_batches(
            DiagnosticOwner::Compiler,
            DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("compiler")])]),
        );
        store.replace_and_publish_batches(
            first.clone(),
            DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("first")])]),
        );
        store.replace_and_publish_batches(
            second.clone(),
            DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("second")])]),
        );

        let update = store.clear_owners_and_publish_batches([first, second]);

        assert_eq!(update.batches, vec![(file, vec![diagnostic("compiler")])]);
        assert!(update.pull_reports_changed);
    }

    #[test]
    fn empty_entries_are_published_without_being_cached() {
        let file = uri("src/Empty.sol");
        let mut store = DiagnosticStore::default();
        let owner = DiagnosticOwner::Compiler;

        assert!(
            store
                .replace_and_publish_batches(owner.clone(), DiagnosticMap::default())
                .batches
                .is_empty()
        );

        let empty_diagnostics = || DiagnosticMap::from_iter([(file.clone(), Vec::new())]);
        assert_eq!(
            store.replace_and_publish_batches(owner.clone(), empty_diagnostics()).batches,
            vec![(file.clone(), Vec::new())]
        );
        assert!(store.reports.is_empty());

        assert_eq!(
            store.replace_and_publish_batches(owner.clone(), empty_diagnostics()).batches,
            vec![(file, Vec::new())]
        );
        assert!(store.reports.is_empty());

        assert!(
            store.replace_and_publish_batches(owner, DiagnosticMap::default()).batches.is_empty()
        );
        assert!(store.reports.is_empty());
    }

    #[test]
    fn pull_report_returns_stable_empty_report() {
        let file = uri("src/Empty.sol");
        let store = DiagnosticStore::default();

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
    fn empty_pull_reports_share_id_without_being_cached() {
        let first = uri("src/First.sol");
        let second = uri("src/Second.sol");
        let store = DiagnosticStore::default();

        let PullReport::Full { result_id: first_id, diagnostics } = store.pull_report(&first, None)
        else {
            panic!("first pull should return a full report");
        };
        assert!(diagnostics.is_empty());

        let PullReport::Full { result_id: second_id, diagnostics } =
            store.pull_report(&second, None)
        else {
            panic!("first pull should return a full report");
        };
        assert!(diagnostics.is_empty());
        assert_eq!(second_id, first_id);
        assert!(store.reports.is_empty());
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
    fn diagnostic_updates_report_actual_pull_report_changes() {
        let file = uri("src/Test.sol");
        let mut store = DiagnosticStore::default();
        let owner = DiagnosticOwner::Compiler;

        let update = store.replace_and_publish_batches(
            owner.clone(),
            DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("same")])]),
        );
        assert!(update.pull_reports_changed);

        let update = store.replace_and_publish_batches(
            owner,
            DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("same")])]),
        );
        assert!(!update.batches.is_empty());
        assert!(!update.pull_reports_changed);

        let path = file.to_file_path().unwrap();
        let update = store.clear_file_path_prefixes_retaining_and_publish_batches(&[path], &[]);
        assert!(update.pull_reports_changed);
    }

    #[test]
    fn empty_diagnostic_updates_do_not_change_pull_reports() {
        let file = uri("src/Empty.sol");
        let mut store = DiagnosticStore::default();

        let update = store.replace_and_publish_batches(
            DiagnosticOwner::Compiler,
            DiagnosticMap::from_iter([(file.clone(), Vec::new())]),
        );
        assert_eq!(update.batches, vec![(file.clone(), Vec::new())]);
        assert!(!update.pull_reports_changed);

        let path = file.to_file_path().unwrap();
        let update = store.clear_file_path_prefixes_retaining_and_publish_batches(&[path], &[]);
        assert!(!update.pull_reports_changed);
    }

    #[test]
    fn clearing_and_restoring_diagnostics_updates_pull_report() {
        let file = uri("src/Deleted.sol");
        let mut store = DiagnosticStore::default();

        store.replace_and_publish_batches(
            DiagnosticOwner::Compiler,
            DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("compiler")])]),
        );
        let PullReport::Full { result_id, .. } = store.pull_report(&file, None) else {
            panic!("first pull should return a full report");
        };

        let path = file.to_file_path().unwrap();
        store.clear_file_path_prefixes_retaining_and_publish_batches(&[path], &[]);
        assert!(store.reports.is_empty());
        let PullReport::Full { result_id: empty_id, diagnostics } =
            store.pull_report(&file, Some(&result_id))
        else {
            panic!("clearing diagnostics should return a full report");
        };
        assert_ne!(empty_id, result_id);
        assert!(diagnostics.is_empty());
        assert_eq!(
            store.pull_report(&file, Some(&empty_id)),
            PullReport::Unchanged { result_id: empty_id.clone() }
        );

        store.replace_and_publish_batches(
            DiagnosticOwner::Compiler,
            DiagnosticMap::from_iter([(file.clone(), vec![diagnostic("compiler")])]),
        );
        let PullReport::Full { result_id: restored_id, diagnostics } =
            store.pull_report(&file, Some(&empty_id))
        else {
            panic!("restored diagnostics should return a full report");
        };
        assert_ne!(restored_id, result_id);
        assert_eq!(diagnostics, vec![diagnostic("compiler")]);
        assert_eq!(store.reports.len(), 1);
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

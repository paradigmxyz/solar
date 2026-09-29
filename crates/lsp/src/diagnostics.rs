use crate::{file_operations::file_path_from_url, proto::normalize_file_uri};
use lsp_types::{Diagnostic, PreviousResultId, PublishDiagnosticsParams, Range, Url};
use normalize_path::NormalizePath;
use solar_interface::data_structures::map::{FxHashMap, FxHashSet};
use std::{borrow::Cow, path::PathBuf};

pub(crate) mod presentation;

pub(crate) type DiagnosticMap = FxHashMap<Url, Vec<Diagnostic>>;
pub(crate) type AnalyzedDocuments = FxHashMap<Url, Option<i64>>;

const EMPTY_RESULT_ID: &str = "solar-empty";
// Sorting the complete request is cheaper than building and probing a second URI index for
// small workspaces. Larger workspaces use the cached current-URI order and merge only stale IDs.
const SORTED_WORKSPACE_REPORT_THRESHOLD: usize = 512;

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
    /// Current report/document URIs in protocol order. Rebuilding this is tied to mutations so
    /// workspace pulls do not sort the full workspace on every request.
    workspace_uris: Vec<Url>,
    next_result_id: u64,
}

#[derive(Debug, Default)]
pub(crate) struct DiagnosticUpdate {
    pub(crate) batches: Vec<PublishDiagnosticsParams>,
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
        if affected_uris.is_empty() {
            return DiagnosticUpdate::default();
        }
        self.publish_batches(affected_uris)
    }

    pub(crate) fn pull_report(&self, uri: &Url, previous_result_id: Option<&str>) -> PullReport {
        Self::make_pull_report(self.reports.get(uri), previous_result_id.map(Cow::Borrowed))
    }

    /// Copies only diagnostics relevant to the requested quick-fix range.
    ///
    /// Cursor requests usually select a small part of the report. Filter before cloning the
    /// diagnostics and their suggestion data; exact source positions and edits are still
    /// validated against the current document when building actions.
    pub(crate) fn code_action_diagnostics(&self, uri: &Url, range: Range) -> Vec<Diagnostic> {
        let Some(report) = self.reports.get(uri) else { return Vec::new() };
        report
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                crate::code_actions::code_action_ranges_intersect(range, diagnostic.range)
            })
            .cloned()
            .collect()
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
        if self.workspace_uris.len().max(previous_result_ids.len())
            <= SORTED_WORKSPACE_REPORT_THRESHOLD
        {
            return self.workspace_pull_reports_sorted(previous_result_ids);
        }

        let capacity = previous_result_ids.len().max(self.workspace_uris.len());
        let mut previous = FxHashMap::with_capacity_and_hasher(capacity, Default::default());
        for PreviousResultId { uri, value } in previous_result_ids {
            previous.insert(normalize_file_uri(uri), value);
        }

        // The current URI list is already sorted by `publish_batches`; only stale client entries
        // need sorting. This keeps a large unchanged workspace pull linear in the workspace size.
        let mut documents = Vec::with_capacity(capacity);
        for uri in &self.workspace_uris {
            documents.push((uri.clone(), previous.remove(uri)));
        }
        if !previous.is_empty() {
            let mut stale =
                previous.into_iter().map(|(uri, value)| (uri, Some(value))).collect::<Vec<_>>();
            stale.sort_unstable_by(|(lhs, _), (rhs, _)| lhs.as_str().cmp(rhs.as_str()));
            let mut merged = Vec::with_capacity(documents.len() + stale.len());
            let mut current = documents.into_iter().peekable();
            let mut stale = stale.into_iter().peekable();
            loop {
                match (current.peek(), stale.peek()) {
                    (Some((current_uri, _)), Some((stale_uri, _)))
                        if current_uri.as_str() <= stale_uri.as_str() =>
                    {
                        merged.push(current.next().unwrap());
                    }
                    (Some(_), Some(_)) => merged.push(stale.next().unwrap()),
                    (Some(_), None) => merged.extend(&mut current),
                    (None, Some(_)) => merged.extend(&mut stale),
                    (None, None) => break,
                }
            }
            documents = merged;
        }

        self.make_workspace_reports(documents)
    }

    fn workspace_pull_reports_sorted(
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
        self.make_workspace_reports(documents)
    }

    fn make_workspace_reports(
        &self,
        documents: Vec<(Url, Option<String>)>,
    ) -> Vec<WorkspacePullReport> {
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
            self.rebuild_workspace_uris();
            return DiagnosticUpdate::default();
        }

        let Self {
            diagnostics: all_diagnostics, reports, analyzed_documents, next_result_id, ..
        } = self;
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
                        diagnostics.extend_from_slice(uri_diagnostics);
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

                // Carry the analyzed version with the report, even if the VFS changes before send.
                let version = analyzed_documents
                    .get(&uri)
                    .copied()
                    .flatten()
                    .and_then(|version| i32::try_from(version).ok());
                (has_entry || was_published).then_some(PublishDiagnosticsParams::new(
                    uri,
                    diagnostics,
                    version,
                ))
            })
            .collect();
        self.rebuild_workspace_uris();
        DiagnosticUpdate { batches, pull_reports_changed, workspace_documents_changed: false }
    }

    fn rebuild_workspace_uris(&mut self) {
        self.workspace_uris.clear();
        self.workspace_uris.extend(self.analyzed_documents.keys().cloned());
        self.workspace_uris.extend(
            self.reports.keys().filter(|uri| !self.analyzed_documents.contains_key(*uri)).cloned(),
        );
        self.workspace_uris.sort_unstable_by(|lhs, rhs| lhs.as_str().cmp(rhs.as_str()));
    }

    fn next_result_id(next_result_id: &mut u64) -> String {
        *next_result_id =
            next_result_id.checked_add(1).expect("diagnostic result ID counter exhausted");
        format!("solar-{next_result_id}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Position;

    fn diagnostic(message: &str) -> Diagnostic {
        Diagnostic::new_simple(Range::new(Position::new(0, 0), Position::new(0, 1)), message.into())
    }

    fn uri(path: &str) -> Url {
        Url::from_file_path(std::env::temp_dir().join("solar-lsp-diagnostics").join(path)).unwrap()
    }

    fn map(entries: &[(&Url, &[&str])]) -> DiagnosticMap {
        entries
            .iter()
            .map(|(uri, messages)| {
                ((*uri).clone(), messages.iter().map(|m| diagnostic(m)).collect())
            })
            .collect()
    }

    fn batch(uri: &Url, messages: &[&str], version: Option<i32>) -> PublishDiagnosticsParams {
        PublishDiagnosticsParams::new(
            uri.clone(),
            messages.iter().map(|message| diagnostic(message)).collect(),
            version,
        )
    }

    fn lint(id: &str) -> DiagnosticOwner {
        DiagnosticOwner::Flycheck { id: id.into(), workspace: PathBuf::from("/workspace") }
    }

    fn full(report: PullReport) -> (String, Vec<Diagnostic>) {
        let PullReport::Full { result_id, diagnostics } = report else {
            panic!("expected a full report, got {report:?}")
        };
        (result_id, diagnostics)
    }

    #[test]
    fn owner_updates_merge_and_publish_only_affected_uris_with_analyzed_versions() {
        let first = uri("src/First.sol");
        let second = uri("src/Second.sol");
        let mut store = DiagnosticStore::default();
        let compiler = store.replace_compiler_snapshot_and_publish_batches(
            map(&[(&first, &["first"]), (&second, &["second"])]),
            AnalyzedDocuments::from_iter([(first.clone(), Some(7))]),
        );
        assert_eq!(
            compiler.batches,
            [batch(&first, &["first"], Some(7)), batch(&second, &["second"], None)]
        );

        // Batches carry the analyzed version even when the document version changed later.
        store.update_analyzed_document_version(first.clone(), 8);
        let lint_update = store.replace_and_publish_batches(lint("a"), map(&[(&first, &["a"])]));
        assert_eq!(lint_update.batches, [batch(&first, &["first", "a"], Some(8))]);

        let lint_update = store.replace_and_publish_batches(lint("a"), DiagnosticMap::default());
        assert_eq!(lint_update.batches, [batch(&first, &["first"], Some(8))]);

        // Clearing several owners publishes one final merged batch per URI.
        store.replace_and_publish_batches(lint("a"), map(&[(&first, &["a"])]));
        store.replace_and_publish_batches(lint("b"), map(&[(&first, &["b"])]));
        let cleared = store.clear_owners_and_publish_batches([lint("a"), lint("b")]);
        assert_eq!(cleared.batches, [batch(&first, &["first"], Some(8))]);
        assert!(cleared.pull_reports_changed);

        let replaced = store
            .replace_and_publish_batches(DiagnosticOwner::Compiler, map(&[(&second, &["new"])]));
        assert_eq!(replaced.batches, [batch(&first, &[], Some(8)), batch(&second, &["new"], None)]);
    }

    #[test]
    fn clearing_file_path_prefixes_removes_normalized_descendant_diagnostics_from_all_owners() {
        let deleted = uri("pkg/Deleted.sol");
        let nested = uri("pkg/nested/Dependency.sol");
        let prefix = uri("pkg").to_file_path().unwrap();
        let dotted =
            Url::from_file_path(prefix.parent().unwrap().join("src/../pkg/Dotted.sol")).unwrap();
        let sibling = uri("pkg2/Keep.sol");
        let unrelated = uri("other/Keep.sol");
        let non_file = Url::parse("untitled:Keep.sol").unwrap();
        let hierarchical_non_file =
            Url::parse(&uri("pkg/Keep.sol").as_str().replacen("file:", "untitled:", 1)).unwrap();
        let mut store = DiagnosticStore::default();
        store.replace_and_publish_batches(
            DiagnosticOwner::Compiler,
            map(&[
                (&deleted, &["deleted compiler"]),
                (&nested, &["nested compiler"]),
                (&dotted, &["dotted"]),
                (&sibling, &["sibling"]),
                (&non_file, &["non-file"]),
                (&hierarchical_non_file, &["hierarchical non-file"]),
            ]),
        );
        store.replace_and_publish_batches(
            lint("a"),
            map(&[(&nested, &["nested lint"]), (&unrelated, &["unrelated"])]),
        );

        let batches =
            store.clear_file_path_prefixes_retaining_and_publish_batches(&[prefix], &[]).batches;

        let mut expected = [&deleted, &nested, &dotted].map(|uri| batch(uri, &[], None));
        expected.sort_by(|lhs, rhs| lhs.uri.as_str().cmp(rhs.uri.as_str()));
        assert_eq!(batches, expected);
        let remaining = |owner: &DiagnosticOwner| {
            let mut uris = store.diagnostics[owner].keys().cloned().collect::<Vec<_>>();
            uris.sort_by(|lhs, rhs| lhs.as_str().cmp(rhs.as_str()));
            uris
        };
        let mut expected = vec![sibling, non_file, hierarchical_non_file];
        expected.sort_by(|lhs, rhs| lhs.as_str().cmp(rhs.as_str()));
        assert_eq!(remaining(&DiagnosticOwner::Compiler), expected);
        assert_eq!(remaining(&lint("a")), [unrelated]);
    }

    #[test]
    fn clearing_empty_owner_keeps_workspace_uri_cache() {
        let file = uri("src/Test.sol");
        let mut store = DiagnosticStore::default();
        store.replace_compiler_snapshot_and_publish_batches(
            map(&[(&file, &["compiler"])]),
            AnalyzedDocuments::from_iter([(file.clone(), Some(7))]),
        );
        store.replace_and_publish_batches(lint("retained"), map(&[(&file, &["lint"])]));
        let [initial] = store.workspace_pull_reports(Vec::new()).try_into().unwrap();
        assert_eq!(initial.version, Some(7));
        let (initial_result_id, diagnostics) = full(initial.report);
        assert_eq!(diagnostics, [diagnostic("compiler"), diagnostic("lint")]);

        let previous =
            vec![PreviousResultId { uri: file.clone(), value: initial_result_id.clone() }];
        for _ in 0..2 {
            let update = store.clear_owners_and_publish_batches([lint("absent")]);
            assert!(update.batches.is_empty());
            assert!(!update.pull_reports_changed);
            let [unchanged] = store.workspace_pull_reports(previous.clone()).try_into().unwrap();
            assert_eq!(unchanged.uri, file);
            assert_eq!(unchanged.version, Some(7));
            let expected = PullReport::Unchanged { result_id: initial_result_id.clone() };
            assert_eq!(unchanged.report, expected);
        }

        let update = store.clear_owners_and_publish_batches([lint("retained")]);
        assert_eq!(update.batches, [batch(&file, &["compiler"], Some(7))]);
        assert!(update.pull_reports_changed);
        let [cleared] = store.workspace_pull_reports(previous).try_into().unwrap();
        assert_eq!(cleared.uri, file);
        assert_eq!(cleared.version, Some(7));
        let (result_id, diagnostics) = full(cleared.report);
        assert_ne!(result_id, initial_result_id);
        assert_eq!(diagnostics, [diagnostic("compiler")]);
    }

    #[test]
    fn empty_entries_are_published_without_being_cached_or_changing_pull_reports() {
        let file = uri("src/Empty.sol");
        let mut store = DiagnosticStore::default();
        let owner = DiagnosticOwner::Compiler;

        let update = store.replace_and_publish_batches(owner.clone(), DiagnosticMap::default());
        assert!(update.batches.is_empty());
        for _ in 0..2 {
            let update = store.replace_and_publish_batches(owner.clone(), map(&[(&file, &[])]));
            assert_eq!(update.batches, [batch(&file, &[], None)]);
            assert!(!update.pull_reports_changed);
            assert!(store.reports.is_empty());
        }

        let path = file.to_file_path().unwrap();
        let update = store.clear_file_path_prefixes_retaining_and_publish_batches(&[path], &[]);
        assert!(!update.pull_reports_changed);
        store.replace_and_publish_batches(owner.clone(), map(&[(&file, &[])]));
        assert!(
            store.replace_and_publish_batches(owner, DiagnosticMap::default()).batches.is_empty()
        );
        assert!(store.reports.is_empty());
    }

    #[test]
    fn empty_pull_reports_share_a_stable_id_without_being_cached() {
        let first = uri("src/First.sol");
        let store = DiagnosticStore::default();

        let (result_id, diagnostics) = full(store.pull_report(&first, None));
        assert!(diagnostics.is_empty());
        assert_eq!(
            full(store.pull_report(&uri("src/Second.sol"), None)),
            (result_id.clone(), Vec::new())
        );
        assert_eq!(full(store.pull_report(&first, Some("stale"))), (result_id.clone(), Vec::new()));
        assert_eq!(
            store.pull_report(&first, Some(&result_id)),
            PullReport::Unchanged { result_id: result_id.clone() }
        );
        assert!(store.reports.is_empty());
    }

    #[test]
    fn pull_report_ids_change_only_when_a_uri_changes() {
        let first = uri("src/First.sol");
        let second = uri("src/Second.sol");
        let mut store = DiagnosticStore::default();
        let owner = DiagnosticOwner::Compiler;
        let publish = |store: &mut DiagnosticStore, first_message| {
            store.replace_and_publish_batches(
                owner.clone(),
                map(&[(&first, &[first_message]), (&second, &["second"])]),
            )
        };

        assert!(publish(&mut store, "first").pull_reports_changed);
        let (first_id, diagnostics) = full(store.pull_report(&first, None));
        assert_eq!(diagnostics, [diagnostic("first")]);
        let (second_id, _) = full(store.pull_report(&second, None));

        let update = publish(&mut store, "first");
        assert!(!update.batches.is_empty());
        assert!(!update.pull_reports_changed);
        assert_eq!(
            store.pull_report(&first, Some(&first_id)),
            PullReport::Unchanged { result_id: first_id.clone() }
        );

        publish(&mut store, "changed");
        assert_eq!(
            store.pull_report(&second, Some(&second_id)),
            PullReport::Unchanged { result_id: second_id }
        );
        let (next_id, diagnostics) = full(store.pull_report(&first, Some(&first_id)));
        assert_ne!(next_id, first_id);
        assert_eq!(diagnostics, [diagnostic("changed")]);
    }

    #[test]
    fn clearing_and_restoring_diagnostics_updates_pull_report() {
        let file = uri("src/Deleted.sol");
        let mut store = DiagnosticStore::default();
        let publish = |store: &mut DiagnosticStore| {
            store.replace_and_publish_batches(
                DiagnosticOwner::Compiler,
                map(&[(&file, &["compiler"])]),
            )
        };

        publish(&mut store);
        let (result_id, _) = full(store.pull_report(&file, None));

        let path = file.to_file_path().unwrap();
        let update = store.clear_file_path_prefixes_retaining_and_publish_batches(&[path], &[]);
        assert!(update.pull_reports_changed);
        assert!(store.reports.is_empty());
        let (empty_id, diagnostics) = full(store.pull_report(&file, Some(&result_id)));
        assert_ne!(empty_id, result_id);
        assert!(diagnostics.is_empty());
        assert_eq!(
            store.pull_report(&file, Some(&empty_id)),
            PullReport::Unchanged { result_id: empty_id.clone() }
        );

        publish(&mut store);
        let (restored_id, diagnostics) = full(store.pull_report(&file, Some(&empty_id)));
        assert_ne!(restored_id, result_id);
        assert_eq!(diagnostics, [diagnostic("compiler")]);
        assert_eq!(store.reports.len(), 1);
    }
}

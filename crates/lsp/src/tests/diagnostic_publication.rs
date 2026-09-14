use super::*;
use lsp_types::{Position, Range};
use std::sync::Arc;

fn uri(path: &str) -> Url {
    Url::from_file_path(std::env::temp_dir().join("solar-lsp-publication").join(path)).unwrap()
}

fn diagnostic(message: &str) -> Diagnostic {
    Diagnostic::new_simple(Range::new(Position::new(0, 0), Position::new(0, 1)), message.into())
}

#[test]
fn push_and_pull_publication_preserve_reports_and_result_ids() {
    let file = uri("src/Warnings.sol");
    let mut initial = diagnostic("function state mutability can be restricted to pure");
    initial.data = Some(serde_json::json!({"sourceFingerprint": "before", "suggestions": []}));
    let mut changed = initial.clone();
    changed.data.as_mut().unwrap()["sourceFingerprint"] = "after".into();
    let original = Arc::new(DiagnosticMap::from_iter([(file.clone(), vec![initial])]));
    let changed = Arc::new(DiagnosticMap::from_iter([(file.clone(), vec![changed])]));
    let mut histories = Vec::new();

    for publish in [true, false] {
        let mut store = DiagnosticStore::default();
        let mut previous_result_id = None;
        let mut history = Vec::new();
        for (stage, snapshot) in [
            original.clone(),
            original.clone(),
            changed.clone(),
            Arc::new(DiagnosticMap::default()),
        ]
        .into_iter()
        .enumerate()
        {
            let expected = snapshot.get(&file).cloned().unwrap_or_default();
            let update = store.replace_compiler_snapshot_and_publish_batches(
                snapshot,
                AnalyzedDocuments::from_iter([(file.clone(), Some(stage as i64 + 1))]),
                publish,
            );
            assert_eq!(update.pull_reports_changed, stage != 1);
            assert_eq!(update.workspace_documents_changed, stage == 0);
            assert_eq!(
                update.batches,
                if publish { vec![(file.clone(), expected.clone())] } else { Vec::new() }
            );

            let full = store.pull_report(&file, None);
            let PullReport::Full { result_id, diagnostics } = &full else {
                panic!("a pull without a previous result ID must be full");
            };
            assert_eq!(*diagnostics, expected);
            if stage == 1 {
                assert_eq!(previous_result_id.as_ref(), Some(result_id));
                assert_eq!(
                    store.pull_report(&file, previous_result_id.as_deref()),
                    PullReport::Unchanged { result_id: result_id.clone() }
                );
            } else {
                assert_ne!(previous_result_id.as_ref(), Some(result_id));
                assert_eq!(store.pull_report(&file, previous_result_id.as_deref()), full);
            }
            if stage == 3 {
                assert_eq!(result_id, EMPTY_RESULT_ID);
            }
            previous_result_id = Some(result_id.clone());
            let reports = store.workspace_pull_reports(Vec::new());
            assert_eq!(
                reports,
                vec![WorkspacePullReport {
                    uri: file.clone(),
                    version: Some(stage as i64 + 1),
                    report: full,
                    is_stale: false,
                }]
            );
            history.push(reports);
        }
        histories.push(history);
    }

    assert_eq!(histories[0], histories[1]);
}

#[test]
fn clearing_paths_preserves_shared_snapshot_and_restores_owner_order() {
    let removed = uri("pkg/Removed.sol");
    let retained = uri("pkg/retained/Keep.sol");
    let original = DiagnosticMap::from_iter([
        (removed.clone(), vec![diagnostic("removed compiler")]),
        (retained.clone(), vec![diagnostic("retained compiler")]),
    ]);
    let shared = Arc::new(original.clone());
    let documents =
        AnalyzedDocuments::from_iter([(removed.clone(), None), (retained.clone(), None)]);
    let mut store = DiagnosticStore::default();
    store.replace_compiler_snapshot_and_publish_batches(shared.clone(), documents.clone(), false);
    for owner in ["z-lint", "a-lint"] {
        store.replace_and_publish_batches(
            DiagnosticOwner::Flycheck {
                id: owner.into(),
                workspace: uri("pkg").to_file_path().unwrap(),
            },
            DiagnosticMap::from_iter([
                (removed.clone(), vec![diagnostic(owner)]),
                (retained.clone(), vec![diagnostic(owner)]),
            ]),
        );
    }
    let before_removal = store.pull_report(&removed, None);
    let retained_report = store.pull_report(&retained, None);

    let update = store.clear_file_path_prefixes_retaining_and_publish_batches(
        &[uri("pkg").to_file_path().unwrap()],
        &[uri("pkg/retained").to_file_path().unwrap()],
    );
    assert_eq!(update.batches, vec![(removed.clone(), Vec::new())]);
    assert!(update.pull_reports_changed);
    assert!(update.workspace_documents_changed);
    assert_eq!(*shared, original);
    assert_eq!(store.pull_report(&retained, None), retained_report);
    assert_eq!(
        store.pull_report(&removed, None),
        PullReport::Full { result_id: EMPTY_RESULT_ID.into(), diagnostics: Vec::new() }
    );

    let update = store.replace_compiler_snapshot_and_publish_batches(shared, documents, false);
    assert!(update.batches.is_empty());
    assert!(update.pull_reports_changed);
    assert!(update.workspace_documents_changed);
    let restored = store.pull_report(&removed, None);
    let PullReport::Full { result_id: restored_id, diagnostics } = restored else {
        panic!("restoring a removed compiler diagnostic must return a full report");
    };
    assert_eq!(diagnostics, vec![diagnostic("removed compiler")]);
    let PullReport::Full { result_id: previous_id, .. } = before_removal else {
        panic!("the initial compiler and flycheck report must be full");
    };
    assert_ne!(restored_id, previous_id);
    assert_ne!(restored_id, EMPTY_RESULT_ID);
    assert_eq!(store.pull_report(&retained, None), retained_report);
    let PullReport::Full { diagnostics, .. } = retained_report else {
        panic!("retained compiler and flycheck diagnostics must have a full report");
    };
    assert_eq!(
        diagnostics,
        vec![diagnostic("retained compiler"), diagnostic("a-lint"), diagnostic("z-lint")]
    );
}

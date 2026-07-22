use super::{
    AnalysisBatch, GlobalState, SymbolTables, analyze, snapshot_with_config,
    support::RequestFixture,
};
use crate::test_support::TestProject;
use async_lsp::ClientSocket;
use lsp_types::{
    DocumentLinkParams, PartialResultParams, Position, Range, TextDocumentIdentifier, Url,
    WorkDoneProgressParams,
};
use snapbox::str;
use solar_config::CompileOpts;
use std::{
    future::Future,
    sync::atomic::Ordering,
    task::{Context, Waker},
};

#[test]
fn all_import_forms_use_full_literal_utf16_ranges() {
    let fixture = RequestFixture::new(
        r#"
        //- /Imports.sol
        /* 😀 */ import "./Plain.sol";
        import * as Glob from "./Glob.sol";
        import {Named as Alias} from "./Named.sol";

        //- /Plain.sol
        contract Plain {}

        //- /Glob.sol
        contract Glob {}

        //- /Named.sol
        contract Named {}
        "#,
        "/Imports.sol",
    );

    fixture.check_document_links(
        "/Imports.sol",
        str![[r#"
0:16..0:29 -> /Plain.sol
1:22..1:34 -> /Glob.sol
2:29..2:42 -> /Named.sol

"#]],
    );
}

#[test]
fn returns_only_successfully_resolved_imports() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Imports.sol
        import "./Valid.sol";
        import "./Missing.sol";

        //- /Valid.sol
        contract Valid {}
        "#,
        "/Imports.sol",
    );

    fixture.check_document_links(
        "/Imports.sol",
        str![[r#"
0:7..0:20 -> /Valid.sol

"#]],
    );
}

#[test]
fn equivalent_percent_encoded_uri_returns_document_links() {
    let project = TestProject::from_fixture(
        r#"
        //- /Imports.sol
        import "./Target.sol";

        //- /Target.sol
        contract Target {}
        "#,
    );
    let path = project.path("/Imports.sol");
    let tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path.clone(), project.read_file("/Imports.sol"))],
    ))
    .symbol_tables;
    let canonical_uri = Url::from_file_path(&path).unwrap();
    let encoded_uri =
        Url::parse(&canonical_uri.as_str().replacen("Imports.sol", "%49mports.sol", 1)).unwrap();

    assert_ne!(canonical_uri, encoded_uri);
    assert_eq!(canonical_uri.to_file_path(), encoded_uri.to_file_path());

    let params = DocumentLinkParams {
        text_document: TextDocumentIdentifier::new(encoded_uri),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let mut state = GlobalState::new(ClientSocket::new_closed());
    *state.symbol_tables.write() = tables;
    let mut request = std::pin::pin!(crate::handlers::document_links(&mut state, params));
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let std::task::Poll::Ready(response) = request.as_mut().poll(&mut context) else {
        panic!("document-link request should be ready");
    };

    let links = response.unwrap().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, Some(Url::from_file_path(project.path("/Target.sol")).unwrap()));
}

#[test]
fn overlapping_workspaces_prefer_vfs_document_links() {
    let mut project = TestProject::from_fixture(
        r#"
        //- /Root.sol open
        import "./nested/A.sol";

        //- /nested/A.sol
        import "./Disk.sol";
        import "./Old.sol";

        //- /nested/Disk.sol
        contract Disk {}

        //- /nested/Old.sol
        contract Old {}

        //- /nested/OverlayLonger.sol
        contract OverlayLonger {}

        //- /nested/New.sol
        contract New {}
        "#,
    );
    project.open_file("/nested/A.sol", "import \"./OverlayLonger.sol\";\nimport \"./New.sol\";");

    let config = project.config_with_roots(&["/", "/nested"]);
    let snapshot = snapshot_with_config(config, project.vfs());
    let mut tables = SymbolTables::default();

    for batch in snapshot.analysis_batches(Vec::new()) {
        if !batch.files.is_empty() {
            tables.extend(analyze(batch).symbol_tables);
        }
    }

    let path = project.path("/nested/A.sol");
    let links = tables
        .document_links(&path)
        .into_iter()
        .map(|link| (link.range, link.target.unwrap()))
        .collect::<Vec<_>>();

    assert_eq!(
        links,
        [
            (
                Range::new(Position::new(0, 7), Position::new(0, 28)),
                Url::from_file_path(project.path("/nested/OverlayLonger.sol")).unwrap(),
            ),
            (
                Range::new(Position::new(1, 7), Position::new(1, 18)),
                Url::from_file_path(project.path("/nested/New.sol")).unwrap(),
            ),
        ]
    );
}

#[test]
fn waits_for_current_analysis_before_returning_document_links() {
    let project = TestProject::from_fixture(
        r#"
        //- /Imports.sol
        import "./Old.sol";

        //- /Old.sol
        contract Old {}

        //- /New.sol
        contract New {}
        "#,
    );
    let path = project.path("/Imports.sol");
    let old_tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path.clone(), project.read_file("/Imports.sol"))],
    ))
    .symbol_tables;
    let new_tables = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path.clone(), "import \"./New.sol\";".into())],
    ))
    .symbol_tables;
    let uri = Url::from_file_path(path).unwrap();
    let params = DocumentLinkParams {
        text_document: TextDocumentIdentifier::new(uri),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let mut state = GlobalState::new(ClientSocket::new_closed());
    *state.symbol_tables.write() = old_tables;
    state.analysis_version.fetch_add(1, Ordering::AcqRel);

    let mut request = std::pin::pin!(crate::handlers::document_links(&mut state, params));
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);

    assert!(request.as_mut().poll(&mut context).is_pending());

    state.analysis_version.fetch_add(1, Ordering::AcqRel);
    let mut snapshot = state.snapshot();
    assert!(snapshot.publish_symbol_tables(2, new_tables));
    assert!(!snapshot.publish_symbol_tables(1, SymbolTables::default()));
    let std::task::Poll::Ready(response) = request.as_mut().poll(&mut context) else {
        panic!("document-link request should complete after analysis is published");
    };
    let links = response.unwrap().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, Some(Url::from_file_path(project.path("/New.sol")).unwrap()));
}

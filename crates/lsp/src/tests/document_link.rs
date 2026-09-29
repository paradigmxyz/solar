use super::*;
use snapbox::str;

#[test]
fn links_resolved_import_forms_with_full_literal_utf16_ranges() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Imports.sol
        /* 😀 */ import "./Plain.sol";
        import * as Glob from "./Glob.sol";
        import {Named as Alias} from "./Named.sol";
        import "./Missing.sol";

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
fn equivalent_file_uris_return_document_links() {
    let fixture = RequestFixture::new(
        r#"
        //- /Imports.sol
        import "./Target.sol";

        //- /Target.sol
        contract Target {}
        "#,
        "/Imports.sol",
    );
    let canonical_uri = fixture.project().uri("/Imports.sol");
    for spelling in ["%49mports.sol", "nested%2F..%2FImports.sol"] {
        let encoded_uri =
            Url::parse(&canonical_uri.as_str().replacen("Imports.sol", spelling, 1)).unwrap();

        assert_ne!(canonical_uri, encoded_uri);
        assert_eq!(crate::proto::vfs_path(&canonical_uri), crate::proto::vfs_path(&encoded_uri));
        fixture.check_document_links_at(encoded_uri, "0:7..0:21 -> /Target.sol\n");
    }
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
    let tables =
        analyze_workspace(&snapshot_with_config(config, project.vfs())).result.symbol_tables;

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
                project.uri("/nested/OverlayLonger.sol"),
            ),
            (Range::new(Position::new(1, 7), Position::new(1, 18)), project.uri("/nested/New.sol"),),
        ]
    );
}

#[test]
fn waits_for_requested_analysis_before_returning_document_links() {
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
    let old_tables = analyze_source(path.clone(), project.read_file("/Imports.sol")).symbol_tables;
    let new_tables = analyze_source(path, "import \"./New.sol\";").symbol_tables;
    let params = document_params(&project.uri("/Imports.sol"));
    let mut state = state_with(Config::default());
    state.symbol_tables.store(Arc::new(old_tables));
    state.analysis_version.fetch_add(1, Ordering::AcqRel);

    let mut request = std::pin::pin!(crate::handlers::document_links(&mut state, params));
    let mut context = Context::from_waker(Waker::noop());

    assert!(request.as_mut().poll(&mut context).is_pending());

    let mut snapshot = state.snapshot();
    assert!(snapshot.publish_symbol_tables(1, Arc::new(new_tables)));
    assert!(!snapshot.publish_symbol_tables(0, Default::default()));
    let Poll::Ready(response) = request.as_mut().poll(&mut context) else {
        panic!("document-link request should complete after analysis is published");
    };
    let links = response.unwrap().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, Some(project.uri("/New.sol")));
}

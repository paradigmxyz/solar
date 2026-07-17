use super::{SymbolTables, analyze, snapshot_with_config, support::RequestFixture};
use crate::test_support::TestProject;
use lsp_types::{Position, Range, Url};
use snapbox::str;

#[test]
fn all_import_forms_use_content_only_utf16_ranges() {
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
0:17..0:28 -> /Plain.sol
1:23..1:33 -> /Glob.sol
2:30..2:41 -> /Named.sol

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
0:8..0:19 -> /Valid.sol

"#]],
    );
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

    let uri = Url::from_file_path(project.path("/nested/A.sol")).unwrap();
    let links = tables
        .document_links(&uri)
        .into_iter()
        .map(|link| (link.range, link.target.unwrap()))
        .collect::<Vec<_>>();

    assert_eq!(
        links,
        [
            (
                Range::new(Position::new(0, 8), Position::new(0, 27)),
                Url::from_file_path(project.path("/nested/OverlayLonger.sol")).unwrap(),
            ),
            (
                Range::new(Position::new(1, 8), Position::new(1, 17)),
                Url::from_file_path(project.path("/nested/New.sol")).unwrap(),
            ),
        ]
    );
}

use super::analyze_project;
use crate::{file_operations::FileMoveBatch, test_support::TestProject};
use lsp_types::{Position, Range, TextEdit, Url};
use std::path::PathBuf;

fn move_batch(moves: impl IntoIterator<Item = (PathBuf, PathBuf)>) -> FileMoveBatch {
    FileMoveBatch::new(moves).unwrap()
}

#[test]
fn rename_file_rewrites_relative_import_target() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol
        import "./Target.sol";

        //- /src/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/src/Importer.sol");
    let moves = move_batch([(project.path("/src/Target.sol"), project.path("/src/Renamed.sol"))]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 21)),
                "\"./Renamed.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn unrelated_rename_preserves_noncanonical_relative_import() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol
        import "./nested/../Target.sol";

        //- /src/Target.sol
        contract Target {}

        //- /src/Other.sol
        contract Other {}
        "#,
    );
    let tables = analyze_project(&project);
    let moves =
        move_batch([(project.path("/src/Other.sol"), project.path("/src/RenamedOther.sol"))]);
    let edits = tables.import_rename_edits(&moves);

    assert!(edits.changes.is_empty());
}

#[test]
fn rename_file_preserves_foundry_remapping() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"
        remappings = ["@lib/=lib/"]

        //- /src/Importer.sol
        import "@lib/Target.sol";

        //- /lib/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/src/Importer.sol");
    let moves = move_batch([(project.path("/lib/Target.sol"), project.path("/lib/Renamed.sol"))]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 24)),
                "\"@lib/Renamed.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn rename_workspace_folder_keeps_foundry_remapping_unchanged() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/foundry.toml
        [profile.default]
        src = "src"
        remappings = ["@lib/=lib/"]

        //- /project/src/Importer.sol
        import "@lib/Target.sol";

        //- /project/lib/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let moves = move_batch([(project.path("/project"), project.path("/renamed"))]);
    let edits = tables.import_rename_edits(&moves);

    assert!(edits.changes.is_empty());
}

#[test]
fn rename_workspace_with_absolute_remapping_rewrites_import() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/src/Importer.sol
        import "@lib/Target.sol";

        //- /project/lib/Target.sol
        contract Target {}
        "#,
    );
    let remapping = project.path("/project/lib").to_string_lossy().replace('\\', "/");
    project.write_file(
        "/project/foundry.toml",
        &format!("[profile.default]\nsrc = \"src\"\nremappings = [\"@lib/={remapping}/\"]\n"),
    );
    let tables = analyze_project(&project);
    let importer = project.path("/project/src/Importer.sol");
    let moves = move_batch([(project.path("/project"), project.path("/renamed"))]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 24)),
                "\"../lib/Target.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn rename_workspace_with_absolute_include_path_rewrites_remapping() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/src/Importer.sol
        import "@lib/Target.sol";

        //- /project/vendor/pkg/Target.sol
        contract Target {}
        "#,
    );
    let include_path = project.path("/project/vendor").to_string_lossy().replace('\\', "/");
    project.write_file(
        "/project/foundry.toml",
        &format!(
            "[profile.default]\nsrc = \"src\"\nlibs = [\"{include_path}\"]\nremappings = [\"@lib/=pkg/\"]\n"
        ),
    );
    let tables = analyze_project(&project);
    let importer = project.path("/project/src/Importer.sol");
    let moves = move_batch([(project.path("/project"), project.path("/renamed"))]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 24)),
                "\"../vendor/pkg/Target.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn rename_configuration_root_rewrites_import_when_nested_moves_leave_files_put() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/foundry.toml
        [profile.default]
        src = "src"
        remappings = ["@lib/=lib/"]

        //- /project/src/Importer.sol
        import "@lib/Target.sol";

        //- /project/lib/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/project/src/Importer.sol");
    let moves = move_batch([
        (project.path("/project"), project.path("/renamed")),
        (project.path("/project/src"), project.path("/project/src")),
        (project.path("/project/lib"), project.path("/project/lib")),
    ]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 24)),
                "\"../lib/Target.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn rename_configuration_root_rewrites_include_path_import_when_files_stay_put() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/foundry.toml
        [profile.default]
        src = "src"
        libs = ["vendor"]
        remappings = ["@lib/=pkg/"]

        //- /project/src/Importer.sol
        import "@lib/Target.sol";

        //- /project/vendor/pkg/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/project/src/Importer.sol");
    let moves = move_batch([
        (project.path("/project"), project.path("/renamed")),
        (project.path("/project/src"), project.path("/project/src")),
        (project.path("/project/vendor"), project.path("/project/vendor")),
    ]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 24)),
                "\"../vendor/pkg/Target.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn rename_configuration_root_rewrites_opaque_import_when_files_stay_put() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/foundry.toml
        [profile.default]
        src = "src"
        remappings = ["Alias=lib/Target.sol"]

        //- /project/src/Importer.sol
        import "Alias";

        //- /project/lib/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/project/src/Importer.sol");
    let moves = move_batch([
        (project.path("/project"), project.path("/renamed")),
        (project.path("/project/src"), project.path("/project/src")),
        (project.path("/project/lib"), project.path("/project/lib")),
    ]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 14)),
                "\"../lib/Target.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn rename_target_falls_back_when_a_more_specific_remapping_would_capture_it() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/foundry.toml
        [profile.default]
        src = "src"
        remappings = ["@lib/=lib/", "@lib/special/=shadow/"]

        //- /project/src/Importer.sol
        import "@lib/Target.sol";

        //- /project/lib/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/project/src/Importer.sol");
    let moves = move_batch([
        (project.path("/project"), project.path("/renamed")),
        (project.path("/project/lib/Target.sol"), project.path("/renamed/lib/special/Target.sol")),
    ]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 24)),
                "\"../lib/special/Target.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn rename_target_falls_back_when_common_suffix_loses_the_remapping_prefix() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/foundry.toml
        [profile.default]
        src = "src"
        remappings = ["foo/=vendor/foo/"]

        //- /project/src/Importer.sol
        import "foo/Target.sol";

        //- /project/vendor/foo/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/project/src/Importer.sol");
    let moves = move_batch([(
        project.path("/project/vendor/foo/Target.sol"),
        project.path("/project/vendor/other/Target.sol"),
    )]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 23)),
                "\"../vendor/other/Target.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn rename_remapped_subtree_rewrites_import_when_manifest_stays_put() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"
        remappings = ["@lib/=src/pkg/lib/"]

        //- /src/pkg/contracts/Importer.sol
        import "@lib/Target.sol";

        //- /src/pkg/lib/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/src/pkg/contracts/Importer.sol");
    let moves = move_batch([(project.path("/src/pkg"), project.path("/src/renamed"))]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 24)),
                "\"../lib/Target.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn rename_importer_folder_recalculates_relative_import() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol
        import "../shared/Target.sol";

        //- /shared/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/src/Importer.sol");
    let moves = move_batch([(project.path("/src"), project.path("/contracts/nested"))]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 29)),
                "\"../../shared/Target.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn delete_file_removes_complete_import_directive() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol
        import {Target} from "./Target.sol";
        import "./Keep.sol";

        //- /src/Target.sol
        contract Target {}

        //- /src/Keep.sol
        contract Keep {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/src/Importer.sol");
    let edits = tables.import_delete_edits(&[project.path("/src/Target.sol")]);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 0), Position::new(0, 36)),
                String::new(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn delete_folder_removes_descendant_imports_but_not_deleted_importers() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol
        import "../package/Target.sol";

        //- /package/Target.sol
        import "./Nested.sol";
        contract Target {}

        //- /package/Nested.sol
        contract Nested {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/src/Importer.sol");
    let edits = tables.import_delete_edits(&[project.path("/package")]);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 0), Position::new(0, 31)),
                String::new(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

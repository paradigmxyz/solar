use super::analyze_project;
use crate::{
    document_links::DocumentLinkIndex, file_operations::FileMoveBatch, test_support::TestProject,
};
use lsp_types::{Position, Range, TextEdit, Url};
use solar_parse::lexer::unescape::{StrKind, try_parse_string_literal};
use std::{fs, path::PathBuf};

#[cfg(windows)]
use solar_config::ImportRemapping;
#[cfg(unix)]
use std::{ffi::OsString, os::unix::ffi::OsStringExt};

fn move_batch(moves: impl IntoIterator<Item = (PathBuf, PathBuf)>) -> FileMoveBatch {
    FileMoveBatch::new(moves).unwrap()
}

fn assert_import_literal_round_trips(literal: &str, expected: &[u8]) {
    let contents = literal.strip_prefix('"').unwrap().strip_suffix('"').unwrap();
    let mut errors = Vec::new();
    let actual = try_parse_string_literal(contents, StrKind::Str, |range, error| {
        errors.push((range, error));
    });
    assert!(errors.is_empty(), "invalid Solidity string literal: {errors:?}");
    assert_eq!(actual.as_ref(), expected);
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
        edits.changes(),
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
fn rename_file_escapes_unicode_import_bytes() {
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
    let target = project.path("/src/Target.sol");
    let renamed = project.path("/src/Renamed-中.sol");
    let moves = move_batch([(target.clone(), renamed.clone())]);
    let edits = tables.import_rename_edits(&moves);
    let edit = edits.first_edit().unwrap();

    assert_eq!(edit.new_text, r#""./Renamed-\xE4\xB8\xAD.sol""#);
    assert_import_literal_round_trips(&edit.new_text, "./Renamed-中.sol".as_bytes());

    fs::rename(target, &renamed).unwrap();
    fs::write(&importer, format!("import {};\n", edit.new_text)).unwrap();
    let tables = analyze_project(&project);
    let links = tables.document_links(&importer);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_ref().unwrap().to_file_path().unwrap(), renamed);
}

#[test]
fn rename_file_escapes_non_bmp_import_bytes() {
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
    let target = project.path("/src/Target.sol");
    let renamed = project.path("/src/Renamed-😀.sol");
    let moves = move_batch([(target.clone(), renamed.clone())]);
    let edits = tables.import_rename_edits(&moves);
    let edit = edits.first_edit().unwrap();

    assert_eq!(edit.new_text, r#""./Renamed-\xF0\x9F\x98\x80.sol""#);
    assert_import_literal_round_trips(&edit.new_text, "./Renamed-😀.sol".as_bytes());

    fs::rename(target, &renamed).unwrap();
    fs::write(&importer, format!("import {};\n", edit.new_text)).unwrap();
    let tables = analyze_project(&project);
    let links = tables.document_links(&importer);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_ref().unwrap().to_file_path().unwrap(), renamed);
}

#[test]
fn rename_file_escapes_quote_and_control_import_bytes() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol
        import "./Target.sol";

        //- /src/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let target = project.path("/src/Target.sol");
    let renamed = project.path("/src").join("Renamed-\"\u{8}\u{c}.sol");
    let moves = move_batch([(target.clone(), renamed.clone())]);
    let edits = tables.import_rename_edits(&moves);
    let edit = edits.first_edit().unwrap();

    assert_eq!(edit.new_text, r#""./Renamed-\x22\x08\x0C.sol""#);
    assert_import_literal_round_trips(&edit.new_text, b"./Renamed-\"\x08\x0c.sol");

    #[cfg(unix)]
    {
        let importer = project.path("/src/Importer.sol");
        fs::rename(target, &renamed).unwrap();
        fs::write(&importer, format!("import {};\n", edit.new_text)).unwrap();
        let tables = analyze_project(&project);
        let links = tables.document_links(&importer);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target.as_ref().unwrap().to_file_path().unwrap(), renamed);
    }
}

#[cfg(unix)]
#[test]
fn rename_file_escapes_backslash_import_byte() {
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
    let target = project.path("/src/Target.sol");
    let renamed = project.path("/src").join("Renamed-\\.sol");
    let moves = move_batch([(target.clone(), renamed.clone())]);
    let edits = tables.import_rename_edits(&moves);
    let edit = edits.first_edit().unwrap();

    assert_eq!(edit.new_text, r#""./Renamed-\x5C.sol""#);
    assert_import_literal_round_trips(&edit.new_text, b"./Renamed-\\.sol");

    fs::rename(target, &renamed).unwrap();
    fs::write(&importer, format!("import {};\n", edit.new_text)).unwrap();
    let tables = analyze_project(&project);
    let links = tables.document_links(&importer);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_ref().unwrap().to_file_path().unwrap(), renamed);
}

#[cfg(unix)]
#[test]
fn rename_file_preserves_non_utf8_import_bytes() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol
        import "./Target-\xFF.sol";
        "#,
    );
    let importer = project.path("/src/Importer.sol");
    let import_path = PathBuf::from(OsString::from_vec(b"./Target-\xff.sol".to_vec()));
    let target = project.path("/src").join(OsString::from_vec(b"Target-\xff.sol".to_vec()));
    let renamed = project.path("/src").join(OsString::from_vec(b"Renamed-\xfe.sol".to_vec()));
    let mut index = DocumentLinkIndex::default();
    index.insert_import_path_for_test(
        importer.clone(),
        Range::new(Position::new(0, 7), Position::new(0, 26)),
        import_path,
        target.clone(),
        None,
        Vec::new(),
    );
    let links = index.links(&importer);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_ref().unwrap().to_file_path().unwrap(), target);

    #[cfg(target_os = "linux")]
    let moves = move_batch([(target.clone(), renamed.clone())]);
    #[cfg(not(target_os = "linux"))]
    let moves = move_batch([(target, renamed)]);
    let edits = index.rename_edits(&moves);
    let edit = edits.first_edit().unwrap();

    assert_eq!(edit.new_text, r#""./Renamed-\xFE.sol""#);
    assert_import_literal_round_trips(&edit.new_text, b"./Renamed-\xfe.sol");

    #[cfg(target_os = "linux")]
    {
        fs::write(&target, "contract Target {}").unwrap();
        let tables = analyze_project(&project);
        let edits = tables.import_rename_edits(&moves);
        let edit = edits.first_edit().unwrap();
        assert_eq!(edit.new_text, r#""./Renamed-\xFE.sol""#);

        fs::rename(target, &renamed).unwrap();
        fs::write(project.path("/src/Importer.sol"), format!("import {};\n", edit.new_text))
            .unwrap();
        let tables = analyze_project(&project);
        let links = tables.document_links(&project.path("/src/Importer.sol"));
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target.as_ref().unwrap().to_file_path().unwrap(), renamed);
    }
}

#[cfg(windows)]
#[test]
fn rename_file_across_windows_drives_uses_absolute_import() {
    let importer = PathBuf::from(r"C:\src\Importer.sol");
    let target = PathBuf::from(r"C:\src\Target.sol");
    let renamed = PathBuf::from(r"D:\contracts\Renamed.sol");
    let mut index = DocumentLinkIndex::default();
    index.insert_import_path_for_test(
        importer,
        Range::new(Position::new(0, 7), Position::new(0, 21)),
        PathBuf::from("./Target.sol"),
        target.clone(),
        None,
        Vec::new(),
    );
    let edits = index.rename_edits(&move_batch([(target, renamed)]));
    let edit = edits.first_edit().unwrap();

    assert_eq!(edit.new_text, r#""D:/contracts/Renamed.sol""#);
    assert_import_literal_round_trips(&edit.new_text, b"D:/contracts/Renamed.sol");
}

#[cfg(windows)]
#[test]
fn rename_importer_across_windows_drives_uses_absolute_import() {
    let importer = PathBuf::from(r"C:\src\Importer.sol");
    let moved_importer = PathBuf::from(r"D:\contracts\Importer.sol");
    let target = PathBuf::from(r"C:\src\Target.sol");
    let mut index = DocumentLinkIndex::default();
    index.insert_import_path_for_test(
        importer.clone(),
        Range::new(Position::new(0, 7), Position::new(0, 21)),
        PathBuf::from("./Target.sol"),
        target,
        None,
        Vec::new(),
    );
    let edits = index.rename_edits(&move_batch([(importer, moved_importer)]));
    let edit = edits.first_edit().unwrap();

    assert_eq!(edit.new_text, r#""C:/src/Target.sol""#);
    assert_import_literal_round_trips(&edit.new_text, b"C:/src/Target.sol");
}

#[cfg(windows)]
#[test]
fn rename_file_across_unc_shares_uses_absolute_import() {
    let importer = PathBuf::from(r"\\server\source\src\Importer.sol");
    let target = PathBuf::from(r"\\server\source\src\Target.sol");
    let renamed = PathBuf::from(r"\\server\destination\contracts\Renamed.sol");
    let mut index = DocumentLinkIndex::default();
    index.insert_import_path_for_test(
        importer,
        Range::new(Position::new(0, 7), Position::new(0, 21)),
        PathBuf::from("./Target.sol"),
        target.clone(),
        None,
        Vec::new(),
    );
    let edits = index.rename_edits(&move_batch([(target, renamed)]));
    let edit = edits.first_edit().unwrap();

    assert_eq!(edit.new_text, r#""//server/destination/contracts/Renamed.sol""#);
    assert_import_literal_round_trips(
        &edit.new_text,
        b"//server/destination/contracts/Renamed.sol",
    );
}

#[cfg(windows)]
#[test]
fn rename_file_omits_absolute_import_captured_by_remapping() {
    let importer = PathBuf::from(r"C:\src\Importer.sol");
    let target = PathBuf::from(r"C:\src\Target.sol");
    let renamed = PathBuf::from(r"D:\contracts\Renamed.sol");
    let mut index = DocumentLinkIndex::default();
    index.insert_import_path_for_test(
        importer,
        Range::new(Position::new(0, 7), Position::new(0, 21)),
        PathBuf::from("./Target.sol"),
        target.clone(),
        Some(PathBuf::from(r"C:\")),
        vec![ImportRemapping {
            context: String::new(),
            prefix: "D:/contracts/".into(),
            path: "D:/shadow/".into(),
        }],
    );

    let edits = index.rename_edits(&move_batch([(target, renamed)]));

    assert!(edits.is_empty());
}

#[cfg(windows)]
#[test]
fn rename_file_to_verbatim_drive_preserves_absolute_import() {
    let importer = PathBuf::from(r"C:\src\Importer.sol");
    let target = PathBuf::from(r"C:\src\Target.sol");
    let renamed = PathBuf::from(r"\\?\D:\contracts\Renamed.sol");
    let mut index = DocumentLinkIndex::default();
    index.insert_import_path_for_test(
        importer,
        Range::new(Position::new(0, 7), Position::new(0, 21)),
        PathBuf::from("./Target.sol"),
        target.clone(),
        None,
        Vec::new(),
    );
    let edits = index.rename_edits(&move_batch([(target, renamed)]));
    let edit = edits.first_edit().unwrap();

    assert_eq!(edit.new_text, r#""\x5C\x5C?\x5CD:\x5Ccontracts\x5CRenamed.sol""#);
    assert_import_literal_round_trips(&edit.new_text, br"\\?\D:\contracts\Renamed.sol");
}

#[cfg(windows)]
#[test]
fn rename_file_to_verbatim_unc_preserves_absolute_import() {
    let importer = PathBuf::from(r"C:\src\Importer.sol");
    let target = PathBuf::from(r"C:\src\Target.sol");
    let renamed = PathBuf::from(r"\\?\UNC\server\destination\contracts\Renamed.sol");
    let mut index = DocumentLinkIndex::default();
    index.insert_import_path_for_test(
        importer,
        Range::new(Position::new(0, 7), Position::new(0, 21)),
        PathBuf::from("./Target.sol"),
        target.clone(),
        None,
        Vec::new(),
    );
    let edits = index.rename_edits(&move_batch([(target, renamed)]));
    let edit = edits.first_edit().unwrap();

    assert_eq!(
        edit.new_text,
        r#""\x5C\x5C?\x5CUNC\x5Cserver\x5Cdestination\x5Ccontracts\x5CRenamed.sol""#
    );
    assert_import_literal_round_trips(
        &edit.new_text,
        br"\\?\UNC\server\destination\contracts\Renamed.sol",
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

    assert!(edits.is_empty());
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
        edits.changes(),
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

    assert!(edits.is_empty());
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
        edits.changes(),
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
        edits.changes(),
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
        edits.changes(),
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
        edits.changes(),
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
        edits.changes(),
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
        edits.changes(),
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
        edits.changes(),
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
        edits.changes(),
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
        edits.changes(),
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
        edits.changes(),
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
        edits.changes(),
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

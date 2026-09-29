use super::analyze_project;
use crate::{
    document_links::{DocumentLinkIndex, ImportEditPlan},
    file_operations::FileMoveBatch,
    test_support::TestProject,
};
use lsp_types::{Position, Range, TextEdit, Url};
use solar_config::ImportRemapping;
use solar_parse::lexer::unescape::{StrKind, try_parse_string_literal};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::{ffi::OsString, os::unix::ffi::OsStringExt};

const IMPORTER_AND_TARGET: &str = r#"
//- /src/Importer.sol
import "./Target.sol";

//- /src/Target.sol
contract Target {}
"#;

const FOUNDRY_LIB: &str = r#"
//- /project/foundry.toml
[profile.default]
src = "src"
remappings = ["@lib/=lib/"]

//- /project/src/Importer.sol
import "@lib/Target.sol";

//- /project/lib/Target.sol
contract Target {}
"#;

fn move_batch(moves: impl IntoIterator<Item = (PathBuf, PathBuf)>) -> FileMoveBatch {
    FileMoveBatch::new(moves).unwrap()
}

/// Asserts that `plan` replaces line 0 of each importer between the given columns.
fn assert_edits(project: &TestProject, plan: ImportEditPlan, expected: &[(&str, u32, u32, &str)]) {
    let expected = expected
        .iter()
        .map(|&(importer, start, end, text)| {
            let range = Range::new(Position::new(0, start), Position::new(0, end));
            let uri = Url::from_file_path(project.path(importer)).unwrap();
            (uri, vec![TextEdit::new(range, text.into())])
        })
        .collect::<HashMap<_, _>>();
    assert_eq!(plan.changes(), expected);
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

/// Applies a rewritten import on disk and checks that it resolves to the moved target.
fn assert_moved_import_resolves(
    project: &TestProject,
    importer: &Path,
    new_text: &str,
    target: &Path,
    renamed: &Path,
) {
    fs::rename(target, renamed).unwrap();
    fs::write(importer, format!("import {new_text};\n")).unwrap();
    let links = analyze_project(project).document_links(importer);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_ref().unwrap().to_file_path().unwrap(), renamed);
}

fn single_import_index(
    importer: PathBuf,
    end: u32,
    import_path: impl Into<PathBuf>,
    target: PathBuf,
    resolver_root: Option<PathBuf>,
    remappings: Vec<ImportRemapping>,
) -> DocumentLinkIndex {
    let mut index = DocumentLinkIndex::default();
    index.insert_import_path_for_test(
        importer,
        Range::new(Position::new(0, 7), Position::new(0, end)),
        import_path.into(),
        target,
        resolver_root,
        remappings,
    );
    index
}

#[test]
fn rename_edits_rewrite_imports() {
    let cases: [(&str, &[(&str, &str)], &[(&str, u32, u32, &str)]); _] = [
        (
            IMPORTER_AND_TARGET,
            &[("/src/Target.sol", "/src/Renamed.sol")],
            &[("/src/Importer.sol", 7, 21, "\"./Renamed.sol\"")],
        ),
        // An unrelated rename preserves a noncanonical relative import.
        (
            r#"
            //- /src/Importer.sol
            import "./nested/../Target.sol";

            //- /src/Target.sol
            contract Target {}

            //- /src/Other.sol
            contract Other {}
            "#,
            &[("/src/Other.sol", "/src/RenamedOther.sol")],
            &[],
        ),
        (
            FOUNDRY_LIB,
            &[("/project/lib/Target.sol", "/project/lib/Renamed.sol")],
            &[("/project/src/Importer.sol", 7, 24, "\"@lib/Renamed.sol\"")],
        ),
        (FOUNDRY_LIB, &[("/project", "/renamed")], &[]),
        // The configuration root moves while nested moves leave the files in place.
        (
            FOUNDRY_LIB,
            &[
                ("/project", "/renamed"),
                ("/project/src", "/project/src"),
                ("/project/lib", "/project/lib"),
            ],
            &[("/project/src/Importer.sol", 7, 24, "\"../lib/Target.sol\"")],
        ),
        (
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
            &[
                ("/project", "/renamed"),
                ("/project/src", "/project/src"),
                ("/project/vendor", "/project/vendor"),
            ],
            &[("/project/src/Importer.sol", 7, 24, "\"../vendor/pkg/Target.sol\"")],
        ),
        (
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
            &[
                ("/project", "/renamed"),
                ("/project/src", "/project/src"),
                ("/project/lib", "/project/lib"),
            ],
            &[("/project/src/Importer.sol", 7, 14, "\"../lib/Target.sol\"")],
        ),
        // A more specific remapping would capture the anchored import.
        (
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
            &[
                ("/project", "/renamed"),
                ("/project/lib/Target.sol", "/renamed/lib/special/Target.sol"),
            ],
            &[("/project/src/Importer.sol", 7, 24, "\"../lib/special/Target.sol\"")],
        ),
        // The common suffix would lose the remapping prefix.
        (
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
            &[("/project/vendor/foo/Target.sol", "/project/vendor/other/Target.sol")],
            &[("/project/src/Importer.sol", 7, 23, "\"../vendor/other/Target.sol\"")],
        ),
        // A remapped subtree moves while the manifest stays put.
        (
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
            &[("/src/pkg", "/src/renamed")],
            &[("/src/pkg/contracts/Importer.sol", 7, 24, "\"../lib/Target.sol\"")],
        ),
        (
            r#"
            //- /src/Importer.sol
            import "../shared/Target.sol";

            //- /shared/Target.sol
            contract Target {}
            "#,
            &[("/src", "/contracts/nested")],
            &[("/src/Importer.sol", 7, 29, "\"../../shared/Target.sol\"")],
        ),
    ];

    for (fixture, moves, expected) in cases {
        let project = TestProject::from_fixture(fixture);
        let moves =
            move_batch(moves.iter().map(|(old, new)| (project.path(old), project.path(new))));
        assert_edits(&project, analyze_project(&project).import_rename_edits(&moves), expected);
    }
}

#[test]
fn rename_workspace_with_absolute_resolution_paths_rewrites_import() {
    for (manifest, target, expected) in [
        ("remappings = [\"@lib/=$ROOT/lib/\"]", "lib/Target.sol", "\"../lib/Target.sol\""),
        (
            "libs = [\"$ROOT/vendor\"]\nremappings = [\"@lib/=pkg/\"]",
            "vendor/pkg/Target.sol",
            "\"../vendor/pkg/Target.sol\"",
        ),
    ] {
        let project = TestProject::from_fixture(&format!(
            "//- /project/src/Importer.sol\nimport \"@lib/Target.sol\";\n//- /project/{target}\ncontract Target {{}}\n"
        ));
        let root = project.path("/project").to_string_lossy().replace('\\', "/");
        let manifest = manifest.replace("$ROOT", &root);
        project.write_file(
            "/project/foundry.toml",
            &format!("[profile.default]\nsrc = \"src\"\n{manifest}\n"),
        );

        let moves = move_batch([(project.path("/project"), project.path("/renamed"))]);
        let plan = analyze_project(&project).import_rename_edits(&moves);

        assert_edits(&project, plan, &[("/project/src/Importer.sol", 7, 24, expected)]);
    }
}

#[test]
fn rename_file_escapes_import_bytes() {
    // Quote and control bytes are not valid file names on Windows, and a backslash is a separator.
    let cases = [
        ("Renamed-中.sol", r#""./Renamed-\xE4\xB8\xAD.sol""#, true),
        ("Renamed-😀.sol", r#""./Renamed-\xF0\x9F\x98\x80.sol""#, true),
        ("Renamed-\"\n\r\t\u{8}\u{c}.sol", r#""./Renamed-\"\n\r\t\x08\x0C.sol""#, cfg!(unix)),
        #[cfg(unix)]
        ("Renamed-\\.sol", r#""./Renamed-\\.sol""#, true),
    ];

    for (name, literal, on_disk) in cases {
        let project = TestProject::from_fixture(IMPORTER_AND_TARGET);
        let target = project.path("/src/Target.sol");
        let renamed = project.path("/src").join(name);
        let moves = move_batch([(target.clone(), renamed.clone())]);
        let plan = analyze_project(&project).import_rename_edits(&moves);
        let edit = plan.first_edit().unwrap();

        assert_eq!(edit.new_text, literal);
        assert_import_literal_round_trips(&edit.new_text, format!("./{name}").as_bytes());
        if on_disk {
            let importer = project.path("/src/Importer.sol");
            assert_moved_import_resolves(&project, &importer, &edit.new_text, &target, &renamed);
        }
    }
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
    let os = |bytes: &[u8]| OsString::from_vec(bytes.to_vec());
    let target = project.path("/src").join(os(b"Target-\xff.sol"));
    let renamed = project.path("/src").join(os(b"Renamed-\xfe.sol"));
    let index = single_import_index(
        importer.clone(),
        26,
        os(b"./Target-\xff.sol"),
        target.clone(),
        None,
        Vec::new(),
    );
    let links = index.links(&importer);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_ref().unwrap().to_file_path().unwrap(), target);

    let moves = move_batch([(target.clone(), renamed.clone())]);
    let edits = index.rename_edits(&moves);
    let edit = edits.first_edit().unwrap();

    assert_eq!(edit.new_text, r#""./Renamed-\xFE.sol""#);
    assert_import_literal_round_trips(&edit.new_text, b"./Renamed-\xfe.sol");

    #[cfg(target_os = "linux")]
    {
        fs::write(&target, "contract Target {}").unwrap();
        let edits = analyze_project(&project).import_rename_edits(&moves);
        let edit = edits.first_edit().unwrap();
        assert_eq!(edit.new_text, r#""./Renamed-\xFE.sol""#);
        assert_moved_import_resolves(&project, &importer, &edit.new_text, &target, &renamed);
    }
}

#[cfg(windows)]
#[test]
fn rename_across_windows_roots_uses_absolute_imports() {
    let cases = [
        (r"C:\src", r"C:\src\Target.sol", r"D:\contracts\Renamed.sol", r"D:/contracts/Renamed.sol"),
        (r"C:\src", r"C:\src\Importer.sol", r"D:\contracts\Importer.sol", r"C:/src/Target.sol"),
        (
            r"\\server\source\src",
            r"\\server\source\src\Target.sol",
            r"\\server\destination\contracts\Renamed.sol",
            r"//server/destination/contracts/Renamed.sol",
        ),
        (
            r"C:\src",
            r"C:\src\Target.sol",
            r"\\?\D:\contracts\Renamed.sol",
            r"\\?\D:\contracts\Renamed.sol",
        ),
        (
            r"C:\src",
            r"C:\src\Target.sol",
            r"\\?\UNC\server\destination\contracts\Renamed.sol",
            r"\\?\UNC\server\destination\contracts\Renamed.sol",
        ),
    ];

    for (dir, old, new, import_path) in cases {
        let dir = Path::new(dir);
        let index = single_import_index(
            dir.join("Importer.sol"),
            21,
            "./Target.sol",
            dir.join("Target.sol"),
            None,
            Vec::new(),
        );
        let edits = index.rename_edits(&move_batch([(PathBuf::from(old), PathBuf::from(new))]));
        let edit = edits.first_edit().unwrap();

        assert_eq!(edit.new_text, format!("\"{}\"", import_path.replace('\\', r"\\")));
        assert_import_literal_round_trips(&edit.new_text, import_path.as_bytes());
    }
}

#[cfg(windows)]
#[test]
fn rename_file_omits_absolute_import_captured_by_remapping() {
    let target = PathBuf::from(r"C:\src\Target.sol");
    let remapping = ImportRemapping {
        context: String::new(),
        prefix: "D:/contracts/".into(),
        path: "D:/shadow/".into(),
    };
    let index = single_import_index(
        PathBuf::from(r"C:\src\Importer.sol"),
        21,
        "./Target.sol",
        target.clone(),
        Some(PathBuf::from(r"C:\")),
        vec![remapping],
    );

    let edits =
        index.rename_edits(&move_batch([(target, PathBuf::from(r"D:\contracts\Renamed.sol"))]));

    assert!(edits.is_empty());
}

#[test]
fn delete_edits_remove_complete_import_directives() {
    let cases = [
        (
            r#"
            //- /src/Importer.sol
            import {Target} from "./Target.sol";
            import "./Keep.sol";

            //- /src/Target.sol
            contract Target {}

            //- /src/Keep.sol
            contract Keep {}
            "#,
            "/src/Target.sol",
            36,
        ),
        // Deleted importers inside the folder are not edited.
        (
            r#"
            //- /src/Importer.sol
            import "../package/Target.sol";

            //- /package/Target.sol
            import "./Nested.sol";
            contract Target {}

            //- /package/Nested.sol
            contract Nested {}
            "#,
            "/package",
            31,
        ),
    ];

    for (fixture, deleted, end) in cases {
        let project = TestProject::from_fixture(fixture);
        let plan = analyze_project(&project).import_delete_edits(&[project.path(deleted)]);
        assert_edits(&project, plan, &[("/src/Importer.sol", 0, end, "")]);
    }
}

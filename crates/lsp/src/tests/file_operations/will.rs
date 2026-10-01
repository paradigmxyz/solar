use super::*;
use crate::{handlers, vfs::VfsPath};
use lsp_types::{CreateFilesParams, DeleteFilesParams, FileDelete, TextEdit, WorkspaceEdit};
use std::{collections::HashMap, fs};

mod scope;

type EditResult = Result<Option<WorkspaceEdit>, ResponseError>;

fn will_delete(state: &mut GlobalState, path: impl AsRef<Path>) -> EditResult {
    let files = vec![FileDelete { uri: uri(path) }];
    block_on(handlers::will_delete_files(state, DeleteFilesParams { files }))
}

fn will_rename(
    state: &mut GlobalState,
    old: impl AsRef<Path>,
    new: impl AsRef<Path>,
) -> EditResult {
    block_on(handlers::will_rename_files(state, rename_params([(old, new)])))
}

fn assert_one_edit_per_file(edit: EditResult, files: &[PathBuf]) {
    let changes = edit.unwrap().unwrap().changes.unwrap();
    let mut edited = changes
        .iter()
        .map(|(uri, edits)| (uri.to_file_path().unwrap(), edits.len()))
        .collect::<Vec<_>>();
    edited.sort();
    let mut expected = files.iter().map(|file| (file.clone(), 1)).collect::<Vec<_>>();
    expected.sort();
    assert_eq!(edited, expected);
}

#[test]
fn will_file_operations_return_import_edits_without_mutating_state() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol open
        import "./Target.sol";

        //- /src/Target.sol
        contract Target {}
        "#,
    );
    let importer = project.path("/src/Importer.sol");
    let importer_uri = Url::from_file_path(&importer).unwrap();
    let target = project.path("/src/Target.sol");
    let mut state = state(&project);

    // Creates never return speculative edits.
    let create = block_on(handlers::will_create_files(&mut state, CreateFilesParams::default()));
    assert!(create.unwrap().is_none());
    let delete = will_delete(&mut state, &target).unwrap().unwrap();
    let rename =
        will_rename(&mut state, &target, project.path("/src/Renamed.sol")).unwrap().unwrap();

    let range = |start, end| Range::new(Position::new(0, start), Position::new(0, end));
    assert_eq!(
        delete.changes,
        Some(HashMap::from([(
            importer_uri.clone(),
            vec![TextEdit::new(range(0, 22), String::new())]
        )]))
    );
    assert_eq!(
        rename.changes,
        Some(HashMap::from([(
            importer_uri,
            vec![TextEdit::new(range(7, 21), "\"./Renamed.sol\"".into())]
        )]))
    );
    assert!(rename.document_changes.is_none());
    assert_eq!(
        state.vfs.read().get_file_contents(&VfsPath::from(importer.clone())).unwrap().to_string(),
        "import \"./Target.sol\";"
    );
    assert_eq!(
        state.symbol_tables.load().document_links(&importer)[0].target,
        Some(Url::from_file_path(target).unwrap())
    );
}

#[test]
fn will_file_operations_edit_every_workspace_importer() {
    let default_foundry = r#"
        //- /foundry.toml
        [profile.default]
        //- /checks/Importer.sol
        import "../src/Target.sol";
        //- /src/Target.sol
        contract Target {}
        "#;
    let cases = [
        // Absent default flycheck roots must not suppress complete import edits.
        (
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"

            //- /src/Importer.sol open
            import "./Target.sol";

            //- /src/Target.sol
            contract Target {}
            "#,
            None,
            &["/src/Importer.sol"][..],
        ),
        // Project metadata and dependency roots must not suppress project import edits.
        (default_foundry, Some("/.git/config"), &["/checks/Importer.sol"]),
        (default_foundry, Some("/lib/Unused.sol"), &["/checks/Importer.sol"]),
        (default_foundry, Some("/out/Generated.sol"), &["/checks/Importer.sol"]),
        // Closed Foundry test and script importers.
        (
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"

            //- /src/Main.sol
            import "./Target.sol";

            //- /test/Importer.t.sol
            import "../src/Target.sol";

            //- /script/Importer.s.sol
            import "../src/Target.sol";

            //- /src/Target.sol
            contract Target {}
            "#,
            None,
            &["/src/Main.sol", "/test/Importer.t.sol", "/script/Importer.s.sol"],
        ),
    ];

    for (fixture, ignored, importers) in cases {
        let project = TestProject::from_fixture(fixture);
        if let Some(ignored) = ignored {
            project.write_file(ignored, "contract Ignored {}");
        }
        let importers = importers.iter().map(|path| project.path(path)).collect::<Vec<_>>();
        let target = project.path("/src/Target.sol");
        let mut state = state(&project);

        assert_one_edit_per_file(will_delete(&mut state, &target), &importers);
        let renamed = project.path("/src/Renamed.sol");
        assert_one_edit_per_file(will_rename(&mut state, &target, renamed), &importers);
    }
}

#[test]
fn will_file_operations_refuse_incomplete_import_edits() {
    let foundry_dependency = |main_import| {
        format!(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"
            remappings = ["@lib/=lib/"]

            //- /src/Main.sol
            import "{main_import}";

            //- /lib/Dependency.sol open
            import "./Target.sol";

            //- /lib/Target.sol
            contract Target {{}}
            "#
        )
    };
    let with_src_importer = |src: &str, importer: &str| {
        let dir = if src == "." { String::new() } else { format!("/{src}") };
        format!(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "{src}"

            //- {dir}/Main.sol
            import "./Target.sol";

            //- {importer}
            import "../Target.sol";

            //- {dir}/Target.sol
            contract Target {{}}
            "#
        )
    };
    // Each case lists an importer that source pruning must omit from analysis, if any.
    let cases = [
        // Partial edits that would also rewrite an open dependency.
        (foundry_dependency("@lib/Target.sol"), None, None, "/lib/Target.sol"),
        // Open dependencies are never edited.
        (foundry_dependency("@lib/Dependency.sol"), None, None, "/lib/Target.sol"),
        // Closed importers in a default-named source folder.
        (with_src_importer("src", "/src/lib/Library.sol"), None, None, "/src/Target.sol"),
        // Closed importers in an excluded output folder.
        (with_src_importer(".", "/out/Generated.sol"), None, None, "/Target.sol"),
        (
            with_src_importer("src", "/src/.hidden/Importer.sol"),
            None,
            Some("/src/.hidden/Importer.sol"),
            "/src/Target.sol",
        ),
        (
            with_src_importer("src", "/src/nested/Importer.sol")
                + "//- /src/nested/.git\ngitdir: elsewhere\n",
            None,
            Some("/src/nested/Importer.sol"),
            "/src/Target.sol",
        ),
        (
            with_src_importer("src", "/src/generated/Importer.sol"),
            Some(json!({ "indexing": { "exclude": ["src/generated/**"] } })),
            Some("/src/generated/Importer.sol"),
            "/src/Target.sol",
        ),
        (
            r#"
            //- /Main.sol
            import "./Target.sol";

            //- /node_modules/Importer.sol
            import "../Target.sol";

            //- /Target.sol
            contract Target {}
            "#
            .to_owned(),
            None,
            Some("/node_modules/Importer.sol"),
            "/Target.sol",
        ),
        (
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"

            //- /src/Main.sol
            import "./Target.sol";

            //- /test/node_modules/Importer.t.sol
            import "../../src/Target.sol";

            //- /src/Target.sol
            contract Target {}
            "#
            .to_owned(),
            None,
            Some("/test/node_modules/Importer.t.sol"),
            "/src/Target.sol",
        ),
    ];

    for (fixture, initialization_options, pruned, target) in cases {
        let project = TestProject::from_fixture(&fixture);
        let mut params = project.initialize_params();
        params.initialization_options = initialization_options;
        let mut state = state_with_config(&project, rediscovered_config(params));
        if let Some(importer) = pruned {
            let links = state.symbol_tables.load().document_links(&project.path(importer));
            assert!(links.is_empty(), "pruned importer was unexpectedly analyzed: {importer}");
        }
        let plan = state.symbol_tables.load().import_delete_edits(&[project.path(target)]);
        assert!(!plan.is_empty(), "{fixture}");
        let target = project.path(target);

        assert!(will_delete(&mut state, &target).unwrap().is_none(), "{fixture}");
        let renamed = target.with_file_name("Renamed.sol");
        assert!(will_rename(&mut state, &target, renamed).unwrap().is_none(), "{fixture}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn will_delete_refuses_import_edits_after_source_load_failure() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Main.sol
        import "./Target.sol";

        //- /src/Lost.sol
        import "./Target.sol";

        //- /src/Target.sol
        contract Target {}
        "#,
    );
    let mut state = project.state();
    fs::remove_file(project.path("/src/Lost.sol")).unwrap();
    state.recompute_for_file_changes(Vec::new(), Vec::new(), false);
    settle(&state).await;

    let files = vec![FileDelete { uri: uri(project.path("/src/Target.sol")) }];
    let edit = handlers::will_delete_files(&mut state, DeleteFilesParams { files }).await.unwrap();

    assert!(edit.is_none());
}

#[test]
fn will_rename_validates_closed_importer_on_disk_across_workspace_roots() {
    let project = TestProject::from_fixture(
        r#"
        //- /one/Importer.sol
        import "../two/Target.sol";

        //- /two/Target.sol
        contract Target {}
        "#,
    );
    let importer = project.path("/one/Importer.sol");
    let (old_target, new_target) =
        (project.path("/two/Target.sol"), project.path("/two/Renamed.sol"));
    let mut state = state(&project);
    state.config = Arc::new(project.config_with_roots(&["/one", "/two"]));

    let edit = will_rename(&mut state, &old_target, &new_target).unwrap().unwrap();

    let new_text = &edit.changes.unwrap()[&Url::from_file_path(&importer).unwrap()][0].new_text;
    assert_eq!(new_text, "\"../two/Renamed.sol\"");
    assert_moved_import_resolves(&project, [&importer; 2], new_text, [&old_target, &new_target]);

    fs::write(&importer, "import \"../two/Other.sol\";").unwrap();
    let error = will_rename(&mut state, &old_target, &new_target).unwrap_err();
    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
}

#[test]
fn will_rename_rewrites_independently_moved_importer_and_target() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol
        import "../deps/Target.sol";

        //- /deps/Target.sol
        contract Target {}
        "#,
    );
    let importer = project.path("/src/Importer.sol");
    let moved_importer = project.path("/contracts/nested/Importer.sol");
    let target = project.path("/deps/Target.sol");
    let moved_target = project.path("/vendor/pkg/Target.sol");
    let params = rename_params([(&importer, &moved_importer), (&target, &moved_target)]);
    let mut state = state(&project);

    let edit = block_on(handlers::will_rename_files(&mut state, params)).unwrap().unwrap();

    let new_text = &edit.changes.unwrap()[&Url::from_file_path(&importer).unwrap()][0].new_text;
    assert_eq!(new_text, "\"../../vendor/pkg/Target.sol\"");
    assert_moved_import_resolves(
        &project,
        [&importer, &moved_importer],
        new_text,
        [&target, &moved_target],
    );
}

#[test]
fn will_rename_preserves_valid_import_paths_when_importer_moves() {
    for (old, new, importer, moved_importer) in [
        ("/src/Importer.sol", "/src/Renamed.sol", "/src/Importer.sol", "/src/Renamed.sol"),
        (
            "/src/Importer.sol",
            "/src/nested/Importer.sol",
            "/src/Importer.sol",
            "/src/nested/Importer.sol",
        ),
        (
            "/src/consumer",
            "/src/renamed",
            "/src/consumer/Importer.sol",
            "/src/renamed/Importer.sol",
        ),
    ] {
        let project = TestProject::from_fixture(&format!(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"
            auto_detect_remappings = false
            remappings = ["@lib/=lib/"]

            //- {importer}
            import "@lib/Target.sol";
            import "src/Target.sol";

            //- /lib/Target.sol
            contract RemappedTarget {{}}

            //- /src/Target.sol
            contract DirectTarget {{}}
            "#,
        ));
        let mut state = state(&project);
        let (old, new) = (project.path(old), project.path(new));
        assert_eq!(will_rename(&mut state, &old, &new).unwrap(), None, "{old:?} -> {new:?}");

        fs::create_dir_all(new.parent().unwrap()).unwrap();
        fs::rename(old, new).unwrap();
        let links = analyze_project(&project).document_links(&project.path(moved_importer));
        let targets = links.into_iter().map(|link| link.target.unwrap()).collect::<Vec<_>>();
        let expected = ["/lib/Target.sol", "/src/Target.sol"]
            .map(|path| Url::from_file_path(project.path(path)).unwrap());
        assert_eq!(targets, expected);
    }
}

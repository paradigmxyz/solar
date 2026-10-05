//! File-operation edits must respect dependency ownership as well as index coverage.

use super::*;
use crate::workspace::WorkspaceEditError;

#[cfg(unix)]
use std::os::unix::fs::symlink;

/// Requests rename and delete edits for `target`.
fn will_edits(state: &mut GlobalState, target: &Path) -> [EditResult; 2] {
    [will_rename(state, target, target.with_file_name("Renamed.sol")), will_delete(state, target)]
}

fn assert_rejected(state: &mut GlobalState, target: &Path) {
    for edit in will_edits(state, target) {
        assert_eq!(edit.unwrap(), None, "dependency edits must reject the entire plan");
    }
}

#[test]
fn will_operations_reject_indexed_dependency_edits() {
    for (directory, settings, default_excludes) in [
        ("vendor/dep/src", r#"remappings = ["dep/=vendor/dep/src/"]"#, true),
        ("dependencies/dep-1.0/src", "", true),
        ("node_modules/dep", "", false),
        ("lib/dep/src", "libs = []", false),
    ] {
        for open in [false, true] {
            let mut project = TestProject::from_fixture(&format!(
                r#"
                //- /foundry.toml
                [profile.default]
                {settings}

                //- /src/Main.sol
                import "../{directory}/Target.sol";

                //- /{directory}/Importer.sol
                import "./Target.sol";

                //- /{directory}/Target.sol
                contract Target {{}}
                "#,
            ));
            let importer = format!("/{directory}/Importer.sol");
            if open {
                project.open_file(&importer, &project.read_file(&importer));
            }
            let options = json!({ "indexing": { "useDefaultExcludes": default_excludes } });
            let config = config_with_options(project.initialize_params(), options);
            let mut state = state_with_config(&project, config);
            assert!(!state.config.may_omit_source_files());
            for path in [&importer, "/src/Main.sol"] {
                assert!(state.config.tracks_source_file(&project.path(path)));
                assert_eq!(state.symbol_tables.load().document_links(&project.path(path)).len(), 1);
            }
            assert_rejected(&mut state, &project.path(&format!("/{directory}/Target.sol")));
        }
    }
}

#[test]
fn will_operations_allow_project_importers_of_dependency_targets() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        remappings = ["dep/=vendor/dep/src/"]

        //- /src/Importer.sol open
        import "dep/Target.sol";

        //- /vendor/dep/src/Target.sol
        contract Target {}
        "#,
    );
    let importer = Url::from_file_path(project.path("/src/Importer.sol")).unwrap();
    let target = project.path("/vendor/dep/src/Target.sol");
    for document_changes in [false, true] {
        let capabilities =
            json!({ "workspace": { "workspaceEdit": { "documentChanges": document_changes } } });
        let config =
            rediscovered_config(with_capabilities(project.initialize_params(), capabilities));
        let mut state = state_with_config(&project, config);
        assert!(matches!(
            state.config.workspace_edit_scope().check(&target),
            Err(WorkspaceEditError::Dependency)
        ));
        // Only the importer needs edit permission, not the target.
        let expected = [
            json!({ "range": Range::new(Position::new(0, 7), Position::new(0, 23)), "newText": "\"dep/Renamed.sol\"" }),
            json!({ "range": Range::new(Position::new(0, 0), Position::new(0, 24)), "newText": "" }),
        ];
        for (edit, expected) in will_edits(&mut state, &target).into_iter().zip(expected) {
            let expected = if document_changes {
                json!({ "documentChanges": [{
                    "textDocument": { "uri": importer, "version": 0 },
                    "edits": [expected],
                }] })
            } else {
                json!({ "changes": { importer.as_str(): [expected] } })
            };
            assert_eq!(edit.unwrap(), Some(from_json(expected)));
        }
    }
}

#[test]
fn will_operations_allow_explicit_dependency_directory_sources() {
    for explicit_workspace in [false, true] {
        let settings = if explicit_workspace { "" } else { r#"src = "dependencies/app/src""# };
        let project = TestProject::from_fixture(&format!(
            r#"
            //- /foundry.toml
            [profile.default]
            {settings}
            remappings = ["app/=dependencies/app/src/"]

            //- /dependencies/app/src/Importer.sol
            import "./Target.sol";

            //- /dependencies/app/src/Target.sol
            contract Target {{}}
            "#,
        ));
        let config = if explicit_workspace {
            project.config_with_roots(&["/", "/dependencies/app/src"])
        } else {
            project.config()
        };
        let mut state = state_with_config(&project, config);
        let importer = project.path("/dependencies/app/src/Importer.sol");
        for edit in will_edits(&mut state, &project.path("/dependencies/app/src/Target.sol")) {
            assert_one_edit_per_file(edit, std::slice::from_ref(&importer));
        }
    }
}

#[test]
fn will_operations_use_dependency_policy_published_with_analysis() {
    for published_protects in [false, true] {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            remappings = ["dep/=vendor/dep/src/"]

            //- /src/Main.sol
            import "dep/Target.sol";

            //- /vendor/dep/src/Importer.sol
            import "./Target.sol";

            //- /vendor/dep/src/Target.sol
            contract Target {}
            "#,
        );
        let protected = project.config();
        project.write_file(
            "/foundry.toml",
            "[profile.default]\nsrc = \"vendor/dep/src\"\nremappings = [\"dep/=vendor/dep/src/\"]\n",
        );
        let allowed = project.config();
        let (published, current) =
            if published_protects { (protected, allowed) } else { (allowed, protected) };
        let mut state = state_with_config(&project, published.clone());
        state.analysis_commit.lock().analysis_config = Some(Arc::new(published));
        state.config = Arc::new(current);
        let target = project.path("/vendor/dep/src/Target.sol");
        for edit in will_edits(&mut state, &target) {
            assert_eq!(edit.unwrap().is_none(), published_protects);
        }
        state.analysis_commit.lock().analysis_config = None;
        for edit in will_edits(&mut state, &target) {
            assert_eq!(edit.unwrap().is_some(), published_protects);
        }
    }
}

#[cfg(unix)]
const SYMLINK_FIXTURE: &str = r#"
//- /foundry.toml
[profile.default]
auto_detect_remappings = false

//- /src/Importer.sol open
import "../Target.sol";

//- /Target.sol
contract Target {}
"#;

/// Checks that each source is indexed and links to `target`.
#[cfg(unix)]
fn assert_links_to(state: &GlobalState, sources: &[&PathBuf], target: &Path) {
    assert!(!state.config.may_omit_source_files());
    for source in sources {
        assert!(state.config.tracks_source_file(source));
        let links = state.symbol_tables.load().document_links(source);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, Some(Url::from_file_path(target).unwrap()));
    }
}

#[cfg(unix)]
#[test]
fn will_file_operations_reject_source_symlinks_into_dependencies() {
    for directory in [false, true] {
        let project = TestProject::from_fixture(&format!(
            "{SYMLINK_FIXTURE}\n//- /lib/dep/src/Importer.sol\nimport \"../Target.sol\";\n"
        ));
        let importer = project.path("/src/Importer.sol");
        let target = project.path("/Target.sol");
        let mut state = state(&project);
        assert_links_to(&state, &[&importer], &target);
        for edit in will_edits(&mut state, &target) {
            assert_one_edit_per_file(edit, std::slice::from_ref(&importer));
        }

        if directory {
            fs::remove_dir_all(project.path("/src")).unwrap();
            symlink(project.path("/lib/dep/src"), project.path("/src")).unwrap();
        } else {
            fs::remove_file(&importer).unwrap();
            symlink(project.path("/lib/dep/src/Importer.sol"), &importer).unwrap();
        }
        // The lexical path stays editable, but its resolved target is a dependency.
        assert!(state.config.workspace_edit_scope().check(&importer).is_ok());
        assert_rejected(&mut state, &target);
    }
}

#[cfg(unix)]
#[test]
fn will_file_operations_refresh_retargeted_dependency_symlinks() {
    let project = TestProject::from_fixture(&format!(
        "{SYMLINK_FIXTURE}\n//- /vendor_old/Empty.sol\ncontract Empty {{}}\n\n\
         //- /vendor_new/Importer.sol\nimport \"../Target.sol\";\n"
    ));
    let library = project.path("/lib");
    let importer = project.path("/src/Importer.sol");
    let dependency = project.path("/vendor_new/Importer.sol");
    let target = project.path("/Target.sol");
    symlink(project.path("/vendor_old"), &library).unwrap();
    let mut state = state(&project);
    let config = Arc::clone(&state.config);
    assert_links_to(&state, &[&importer, &dependency], &target);
    fs::remove_file(&importer).unwrap();
    symlink(&dependency, &importer).unwrap();
    for edit in will_edits(&mut state, &target) {
        assert_one_edit_per_file(edit, &[importer.clone(), dependency.clone()]);
    }

    fs::remove_file(&library).unwrap();
    symlink(project.path("/vendor_new"), &library).unwrap();
    assert_rejected(&mut state, &target);
    assert!(Arc::ptr_eq(&state.config, &config));
}

#[cfg(unix)]
#[test]
fn will_file_operations_reject_dangling_links_but_allow_unsaved_importers() {
    for link in [false, true] {
        let project = TestProject::from_fixture(SYMLINK_FIXTURE);
        let importer = project.path("/src/Importer.sol");
        let target = project.path("/Target.sol");
        let mut state = state(&project);
        assert_links_to(&state, &[&importer], &target);
        fs::remove_file(&importer).unwrap();
        if link {
            symlink(project.path("/lib/dep/Missing.sol"), &importer).unwrap();
            assert_rejected(&mut state, &target);
        } else {
            for edit in will_edits(&mut state, &target) {
                assert_one_edit_per_file(edit, std::slice::from_ref(&importer));
            }
        }
    }
}

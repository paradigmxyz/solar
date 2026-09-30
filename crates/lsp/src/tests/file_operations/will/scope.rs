//! File-operation edits must respect dependency ownership as well as index coverage.

use super::*;
use crate::workspace::WorkspaceEditError;
use lsp_types::{
    DocumentChanges, WorkspaceClientCapabilities, WorkspaceEdit, WorkspaceEditClientCapabilities,
};
use std::path::Path;

#[cfg(unix)]
mod paths;

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
            let config = config_with_initialization_options(
                &project,
                Some(serde_json::json!({
                    "indexing": { "useDefaultExcludes": default_excludes },
                })),
            );
            let mut state = state_with_config(&project, config);
            assert!(!state.config.may_omit_source_files());
            for path in [&importer, "/src/Main.sol"] {
                assert!(state.config.tracks_source_file(&project.path(path)));
                assert_eq!(state.symbol_tables.load().document_links(&project.path(path)).len(), 1);
            }
            for edit in will_edits(&mut state, &project.path(&format!("/{directory}/Target.sol"))) {
                assert!(
                    edit.is_none(),
                    "dependency edits must reject the entire plan ({directory}, open={open}): {edit:?}",
                );
            }
        }
    }
}

#[test]
fn will_operations_allow_project_importers_of_dependency_targets() {
    for document_changes in [false, true] {
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
        let mut params = project.initialize_params();
        params.capabilities.workspace = Some(WorkspaceClientCapabilities {
            workspace_edit: Some(WorkspaceEditClientCapabilities {
                document_changes: Some(document_changes),
                ..Default::default()
            }),
            ..Default::default()
        });
        let (_, mut config) = negotiate_capabilities(params);
        config.rediscover_workspaces();
        let mut state = state_with_config(&project, config);
        let importer = Url::from_file_path(project.path("/src/Importer.sol")).unwrap();
        let target = project.path("/vendor/dep/src/Target.sol");
        assert!(matches!(
            state.config.workspace_edit_scope().check(&target),
            Err(WorkspaceEditError::Dependency)
        ));

        for (edit, expected) in will_edits(&mut state, &target).into_iter().zip([
            TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 23)),
                r#""dep/Renamed.sol""#.into(),
            ),
            TextEdit::new(Range::new(Position::new(0, 0), Position::new(0, 24)), String::new()),
        ]) {
            let edit = edit.expect("only the importer needs edit permission, not the target");
            if document_changes {
                assert!(edit.changes.is_none());
                let Some(DocumentChanges::Edits(edits)) = edit.document_changes else {
                    panic!("expected versioned document edits");
                };
                assert_eq!(edits.len(), 1);
                assert_eq!(edits[0].text_document.uri, importer);
                assert_eq!(edits[0].text_document.version, Some(0));
                assert_eq!(edits[0].edits, vec![lsp_types::OneOf::Left(expected)]);
            } else {
                assert!(edit.document_changes.is_none());
                assert_eq!(edit.changes.unwrap(), [(importer.clone(), vec![expected])].into());
            }
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
        let importer =
            Url::from_file_path(project.path("/dependencies/app/src/Importer.sol")).unwrap();
        for edit in will_edits(&mut state, &project.path("/dependencies/app/src/Target.sol")) {
            let changes =
                edit.expect("explicit project sources must remain editable").changes.unwrap();
            assert_eq!(changes.len(), 1);
            assert_eq!(changes[&importer].len(), 1);
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
            r#"[profile.default]
src = "vendor/dep/src"
remappings = ["dep/=vendor/dep/src/"]
"#,
        );
        let allowed = project.config();
        let (published, current) =
            if published_protects { (protected, allowed) } else { (allowed, protected) };
        let mut state = state_with_config(&project, published.clone());
        state.analysis_commit.lock().analysis_config = Some(Arc::new(published));
        state.config = Arc::new(current);
        let target = project.path("/vendor/dep/src/Target.sol");
        for edit in will_edits(&mut state, &target) {
            assert_eq!(edit.is_none(), published_protects);
        }
        state.analysis_commit.lock().analysis_config = None;
        for edit in will_edits(&mut state, &target) {
            assert_eq!(edit.is_some(), published_protects);
        }
    }
}

fn will_edits(state: &mut GlobalState, target: &Path) -> [Option<WorkspaceEdit>; 2] {
    [
        block_on(crate::handlers::will_rename_files(
            state,
            RenameFilesParams {
                files: vec![FileRename {
                    old_uri: Url::from_file_path(target).unwrap().to_string(),
                    new_uri: Url::from_file_path(target.with_file_name("Renamed.sol"))
                        .unwrap()
                        .to_string(),
                }],
            },
        ))
        .unwrap(),
        block_on(crate::handlers::will_delete_files(
            state,
            DeleteFilesParams {
                files: vec![FileDelete { uri: Url::from_file_path(target).unwrap().to_string() }],
            },
        ))
        .unwrap(),
    ]
}

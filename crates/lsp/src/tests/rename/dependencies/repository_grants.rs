use super::*;

#[tokio::test]
async fn discovered_repository_manifest_cannot_grant_other_projects_dependencies() {
    let fixture = RequestFixture::new(
        r#"
        //- /a/foundry.toml
        [profile.default]
        libs = ["vendor"]
        auto_detect_remappings = false
        //- /a/vendor/owned/Owned.sol
        contract $1Owned {}
        //- /a/src/Main.sol
        import "../vendor/owned/Owned.sol";
        contract Main is $2Owned {}
        //- /b/foundry.toml
        [profile.default]
        src = "../c/modules/dep/src"
        libs = []
        auto_detect_remappings = false
        //- /c/foundry.toml
        [profile.default]
        libs = []
        auto_detect_remappings = false
        //- /c/modules/dep/.git
        gitdir: elsewhere
        //- /c/modules/dep/src/foundry.toml
        [profile.default]
        src = "../../../../a/vendor/owned"
        libs = []
        auto_detect_remappings = false
        //- /c/modules/dep/src/Dep.sol
        contract Dep {}
        "#,
        "/a/src/Main.sol",
    );
    for explicit_client_root in [false, true] {
        for marker in ["$1", "$2"] {
            let (mut state, params) = fixture.rename_state_and_params(marker, "Renamed");
            if explicit_client_root {
                let initialize = InitializeParams {
                    workspace_folders: Some(
                        ["/", "/c/modules/dep/src"]
                            .into_iter()
                            .map(|path| WorkspaceFolder {
                                uri: Url::from_file_path(fixture.project_path(path)).unwrap(),
                                name: path.into(),
                            })
                            .collect(),
                    ),
                    ..Default::default()
                };
                let (_, mut config) = negotiate_capabilities(initialize);
                config.rediscover_workspaces();
                state.config = Arc::new(config);
            }

            // A source grant discovers the repository's manifest without making that
            // manifest independent authorization to edit another project's dependencies.
            assert_eq!(state.config.workspaces().len(), 4);
            let dependency = state
                .config
                .workspaces()
                .iter()
                .find(|workspace| {
                    workspace.compile_opts().base_path.as_ref()
                        == Some(&fixture.project_path("/c/modules/dep/src"))
                })
                .expect("the repository manifest is discovered through the explicit source root");
            assert!(
                dependency.import_source_roots().contains(&fixture.project_path("/a/vendor/owned"))
            );
            if !explicit_client_root {
                assert_dependency_rename_rejected(&mut state, params).await;
                continue;
            }

            assert!(
                handlers::prepare_rename(&mut state, params.text_document_position.clone())
                    .await
                    .unwrap()
                    .is_some()
            );
            let changes =
                handlers::rename(&mut state, params).await.unwrap().unwrap().changes.unwrap();
            assert_eq!(changes.len(), 2);
            for path in ["/a/vendor/owned/Owned.sol", "/a/src/Main.sol"] {
                let uri = Url::from_file_path(fixture.project_path(path)).unwrap();
                assert_eq!(changes[&uri].len(), 1);
                assert_eq!(changes[&uri][0].new_text, "Renamed");
            }
        }
    }
}

use super::*;

#[tokio::test]
async fn rejects_renaming_nested_repository_declarations_and_override_families() {
    for marker_file in [true, false] {
        let marker = if marker_file { ".git" } else { ".git/HEAD" };
        let mut fixture = RequestFixture::new(
            &format!(
                r#"
                //- /foundry.toml
                [profile.default]
                libs = []
                //- /modules/dep/{marker}
                gitdir: elsewhere
                //- /modules/dep/IDep.sol
                interface $1IDep {{
                    function $2ping() external returns (uint256);
                }}
                //- /modules/dep/Other.sol
                import {{IDep}} from "./IDep.sol";
                contract Other {{ IDep dep; }}
                //- /src/Impl.sol
                import {{IDep}} from "../modules/dep/IDep.sol";
                contract Impl is $3IDep {{
                    function $4ping() external pure override returns (uint256) {{ return 1; }}
                    function callDep(IDep dep) external returns (uint256) {{ return dep.$5ping(); }}
                }}
                "#,
            ),
            "/src/Impl.sol",
        );
        for open_dependency in [false, true] {
            if open_dependency {
                let contents = fixture.project_contents("/modules/dep/IDep.sol");
                fixture.set_open_file_contents("/modules/dep/IDep.sol", &contents);
            }
            for marker in ["$1", "$2", "$3", "$4", "$5"] {
                let (mut state, params) = fixture.rename_state_and_params(marker, "renamed");
                assert_dependency_rename_rejected(&mut state, params).await;
            }
        }
    }
}

#[tokio::test]
async fn allows_nested_repository_sources_explicitly_owned_by_a_project() {
    for independent in [false, true] {
        let manifest = if independent { "/owner/foundry.toml" } else { "/foundry.toml" };
        let source = if independent { "../modules/dep/src" } else { "modules/dep/src" };
        let root_manifest = if independent { "//- /foundry.toml" } else { "" };
        let fixture = RequestFixture::new(
            &format!(
                r#"
                {root_manifest}
                //- {manifest}
                [profile.default]
                src = "{source}"
                libs = []
                //- /modules/dep/.git
                gitdir: elsewhere
                //- /modules/dep/src/Owned.sol
                contract $1Owned {{}}
                //- /modules/dep/Other.sol
                contract $2Other {{}}
                //- /src/Main.sol
                import {{Owned}} from "../modules/dep/src/Owned.sol";
                import {{Other}} from "../modules/dep/Other.sol";
                contract Main is $3Owned, Other {{}}
                "#,
            ),
            "/src/Main.sol",
        );
        for marker in ["$1", "$3"] {
            let (mut state, params) = fixture.rename_state_and_params(marker, "Renamed");
            assert!(
                handlers::prepare_rename(&mut state, params.text_document_position.clone())
                    .await
                    .unwrap()
                    .is_some()
            );
            let changes =
                handlers::rename(&mut state, params).await.unwrap().unwrap().changes.unwrap();
            assert_eq!(changes.len(), 2);
            for (path, count) in [("/modules/dep/src/Owned.sol", 1), ("/src/Main.sol", 2)] {
                let uri = Url::from_file_path(fixture.project_path(path)).unwrap();
                assert_eq!(changes[&uri].len(), count);
                assert!(changes[&uri].iter().all(|edit| edit.new_text == "Renamed"));
            }
        }
        let (mut state, params) = fixture.rename_state_and_params("$2", "Renamed");
        assert_dependency_rename_rejected(&mut state, params).await;
    }
}

#[tokio::test]
async fn explicit_client_roots_do_not_authorize_deeper_nested_repositories() {
    let fixture = RequestFixture::new(
        r#"
        //- /foundry.toml
        [profile.default]
        libs = []
        //- /modules/dep/.git
        gitdir: elsewhere
        //- /modules/dep/foundry.toml
        [profile.default]
        libs = []
        //- /modules/dep/src/Owned.sol
        contract $1Owned {}
        //- /modules/dep/modules/inner/.git
        gitdir: elsewhere
        //- /modules/dep/modules/inner/Inner.sol
        contract $2Inner {}
        //- /src/Main.sol
        import {Owned} from "../modules/dep/src/Owned.sol";
        import {Inner} from "../modules/dep/modules/inner/Inner.sol";
        contract Main is $3Owned, Inner {}
        "#,
        "/src/Main.sol",
    );
    for roots in [["/", "/modules/dep"], ["/modules/dep", "/"]] {
        for marker in ["$1", "$2", "$3"] {
            let (mut state, params) = fixture.rename_state_and_params(marker, "Renamed");
            let initialize = InitializeParams {
                workspace_folders: Some(
                    roots
                        .iter()
                        .map(|path| WorkspaceFolder {
                            uri: Url::from_file_path(fixture.project_path(path)).unwrap(),
                            name: (*path).into(),
                        })
                        .collect(),
                ),
                ..Default::default()
            };
            let (_, mut config) = negotiate_capabilities(initialize);
            config.rediscover_workspaces();
            state.config = Arc::new(config);
            if marker == "$2" {
                assert_dependency_rename_rejected(&mut state, params).await;
            } else {
                assert!(
                    handlers::prepare_rename(&mut state, params.text_document_position.clone())
                        .await
                        .unwrap()
                        .is_some()
                );
                let changes =
                    handlers::rename(&mut state, params).await.unwrap().unwrap().changes.unwrap();
                assert_eq!(changes.len(), 2);
                for (path, count) in [("/modules/dep/src/Owned.sol", 1), ("/src/Main.sol", 2)] {
                    let uri = Url::from_file_path(fixture.project_path(path)).unwrap();
                    assert_eq!(changes[&uri].len(), count);
                    assert!(changes[&uri].iter().all(|edit| edit.new_text == "Renamed"));
                }
            }
        }
    }
}

#[tokio::test]
async fn ordinary_directories_and_disabled_repository_exclusion_remain_editable() {
    for marker_exists in [false, true] {
        let fixture = RequestFixture::new(
            r#"
            //- /foundry.toml
            [profile.default]
            libs = []
            //- /modules/dep/Owned.sol
            contract $1Owned {}
            //- /src/Main.sol
            import {Owned} from "../modules/dep/Owned.sol";
            contract Main is $2Owned {}
            "#,
            "/src/Main.sol",
        );
        if marker_exists {
            fixture.write_file("/modules/dep/.git", "gitdir: elsewhere");
        }
        let (mut state, params) = fixture.rename_state_and_params("$2", "Renamed");
        let initialize = InitializeParams {
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: Url::from_file_path(fixture.project_path("/")).unwrap(),
                name: "fixture".into(),
            }]),
            initialization_options: Some(serde_json::json!({
                "indexing": { "excludeNestedRepositories": !marker_exists },
            })),
            ..Default::default()
        };
        let (_, mut config) = negotiate_capabilities(initialize);
        config.rediscover_workspaces();
        state.config = Arc::new(config);
        assert!(
            handlers::prepare_rename(&mut state, params.text_document_position.clone())
                .await
                .unwrap()
                .is_some()
        );
        let changes = handlers::rename(&mut state, params).await.unwrap().unwrap().changes.unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(changes.values().map(Vec::len).sum::<usize>(), 3);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn rejects_source_symlinks_into_nested_repositories() {
    for directory in [false, true] {
        let fixture = RequestFixture::new(
            r#"
            //- /foundry.toml
            [profile.default]
            libs = []
            //- /modules/dep/.git
            gitdir: elsewhere
            //- /modules/dep/IDep.sol
            interface IDep {}
            //- /src/IDep.sol
            interface $1IDep {}
            "#,
            "/src/IDep.sol",
        );
        if directory {
            fs::remove_dir_all(fixture.project_path("/src")).unwrap();
            symlink(fixture.project_path("/modules/dep"), fixture.project_path("/src")).unwrap();
        } else {
            fs::remove_file(fixture.project_path("/src/IDep.sol")).unwrap();
            symlink(
                fixture.project_path("/modules/dep/IDep.sol"),
                fixture.project_path("/src/IDep.sol"),
            )
            .unwrap();
        }
        let (mut state, params) = fixture.rename_state_and_params("$1", "Renamed");
        assert_dependency_rename_rejected(&mut state, params).await;
    }
}

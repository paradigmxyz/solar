use super::*;

#[tokio::test]
async fn rejects_renaming_nested_repository_declarations_and_override_families() {
    for marker in [".git", ".git/HEAD"] {
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
                check_marker(&fixture, marker, &["/"], DEPENDENCY).await;
            }
        }
    }
}

#[tokio::test]
async fn allows_nested_repository_sources_explicitly_owned_by_a_project() {
    // The owning manifest is either the root project or an independent sibling project.
    for (root_manifest, manifest, source) in [
        ("", "/foundry.toml", "modules/dep/src"),
        ("//- /foundry.toml", "/owner/foundry.toml", "../modules/dep/src"),
    ] {
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
            check_marker(
                &fixture,
                marker,
                &["/"],
                str![[r#"
/modules/dep/src/Owned.sol:0:9-0:14 -> Renamed
/src/Main.sol:0:8-0:13 -> Renamed
/src/Main.sol:2:17-2:22 -> Renamed

"#]],
            )
            .await;
        }
        check_marker(&fixture, "$2", &["/"], DEPENDENCY).await;
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
        for marker in ["$1", "$3"] {
            check_marker(
                &fixture,
                marker,
                &roots,
                str![[r#"
/modules/dep/src/Owned.sol:0:9-0:14 -> Renamed
/src/Main.sol:0:8-0:13 -> Renamed
/src/Main.sol:2:17-2:22 -> Renamed

"#]],
            )
            .await;
        }
        check_marker(&fixture, "$2", &roots, DEPENDENCY).await;
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
        let options = json!({ "indexing": { "excludeNestedRepositories": !marker_exists } });
        state.config =
            Arc::new(config_with_options(fixture.project().initialize_params(), options));
        check_report(
            &fixture,
            &mut state,
            params,
            str![[r#"
/modules/dep/Owned.sol:0:9-0:14 -> Renamed
/src/Main.sol:0:8-0:13 -> Renamed
/src/Main.sol:1:17-1:22 -> Renamed

"#]],
        )
        .await;
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
        let (link, target) = if directory {
            ("/src", "/modules/dep")
        } else {
            ("/src/IDep.sol", "/modules/dep/IDep.sol")
        };
        let link = fixture.project_path(link);
        if directory { fs::remove_dir_all(&link) } else { fs::remove_file(&link) }.unwrap();
        symlink(fixture.project_path(target), link).unwrap();
        check_marker(&fixture, "$1", &["/"], DEPENDENCY).await;
    }
}

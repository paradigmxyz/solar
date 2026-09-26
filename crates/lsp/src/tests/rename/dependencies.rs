use super::*;
use crate::handlers;
use lsp_types::{RenameParams, WorkspaceFolder};

#[cfg(unix)]
use std::{fs, os::unix::fs::symlink};

async fn assert_dependency_rename_rejected(state: &mut GlobalState, params: RenameParams) {
    let prepared = handlers::prepare_rename(state, params.text_document_position.clone()).await;
    let renamed = handlers::rename(state, params).await;
    for error in [prepared.unwrap_err(), renamed.unwrap_err()] {
        assert_eq!(error.code, ErrorCode::REQUEST_FAILED);
        snapbox::assert_data_eq!(
            error.message,
            "cannot rename this symbol because it would modify dependency files",
        );
    }
}

#[tokio::test]
async fn rejects_renaming_dependency_declarations_and_override_families() {
    let mut fixture = RequestFixture::new(
        r#"
        //- /foundry.toml
        [profile.default]
        //- /lib/dep/src/IDep.sol
        interface $1IDep {
            function $2ping() external returns (uint256);
        }
        //- /src/Impl.sol
        import {IDep} from "../lib/dep/src/IDep.sol";
        contract Impl is $3IDep {
            function $4ping() external pure override returns (uint256) { return 1; }
            function callDep(IDep dep) external returns (uint256) { return dep.$5ping(); }
        }
        "#,
        "/src/Impl.sol",
    );
    for open_dependency in [false, true] {
        if open_dependency {
            let contents = fixture.project_contents("/lib/dep/src/IDep.sol");
            fixture.set_open_file_contents("/lib/dep/src/IDep.sol", &contents);
        }
        for marker in ["$1", "$2", "$3", "$4", "$5"] {
            let (mut state, params) = fixture.rename_state_and_params(marker, "renamed");
            assert_dependency_rename_rejected(&mut state, params).await;
        }
    }
}

#[tokio::test]
async fn protects_dependency_roots_and_remappings() {
    for (configuration, dependency_root, nested_manifest) in [
        ("", "lib/dep", false),
        ("[profile.default]\nlibs = [\"vendor\"]", "vendor/dep", false),
        ("[profile.default]\nlibs = []", "lib/dep", false),
        ("[profile.default]\nlibs = []", "node_modules/dep", false),
        ("[profile.default]\nlibs = []", "dependencies/dep", false),
        (
            "[profile.default]\nremappings = [\"dep/=dependencies/dep/src/\"]",
            "dependencies/dep",
            true,
        ),
        ("[profile.default]\nremappings = [\"dep/=vendor/dep/src/\"]", "vendor/dep", true),
    ] {
        let manifest = if configuration.is_empty() {
            String::new()
        } else {
            format!("//- /foundry.toml\n{configuration}\n")
        };
        let nested = if nested_manifest {
            format!("//- /{dependency_root}/foundry.toml\n[profile.default]\n")
        } else {
            String::new()
        };
        let fixture = RequestFixture::new(
            &format!(
                r#"{manifest}{nested}//- /{dependency_root}/src/IDep.sol
interface IDep {{ function ping() external; }}
//- /src/Impl.sol
import {{IDep}} from "../{dependency_root}/src/IDep.sol";
contract Impl is $1IDep {{
    function $2ping() external override {{}}
}}
"#,
            ),
            "/src/Impl.sol",
        );
        for marker in ["$1", "$2"] {
            let (mut state, params) = fixture.rename_state_and_params(marker, "renamed");
            assert_dependency_rename_rejected(&mut state, params).await;
        }
    }
}

#[test]
fn allows_local_import_aliases_of_dependencies() {
    let fixture = RequestFixture::new(
        r#"
        //- /foundry.toml
        //- /lib/dep/IDep.sol
        interface IDep {}
        //- /src/Impl.sol
        import {IDep as $1LocalDep} from "../lib/dep/IDep.sol";
        contract Impl {
            $2LocalDep dep;
        }
        "#,
        "/src/Impl.sol",
    );
    for marker in ["$1", "$2"] {
        fixture.check_rename(
            marker,
            "RenamedLocal",
            str![[r#"
/src/Impl.sol:0:16-0:24 -> RenamedLocal
/src/Impl.sol:2:4-2:12 -> RenamedLocal

"#]],
        );
    }
    fixture.check_prepare_rename("$1", "0:16-0:24\n");
}

#[test]
fn allows_project_override_families_and_source_alias_remappings() {
    let fixture = RequestFixture::new(
        r#"
        //- /foundry.toml
        [profile.default]
        remappings = ["local/=src/"]
        //- /src/IProject.sol
        interface IProject {
            function $1ping() external;
        }
        //- /src/Impl.sol
        import {IProject} from "./IProject.sol";
        contract Impl is IProject {
            function $2ping() external override {}
            function callProject(IProject project) external { project.$3ping(); }
        }
        "#,
        "/src/Impl.sol",
    );
    for marker in ["$1", "$2", "$3"] {
        fixture.check_rename(
            marker,
            "pong",
            str![[r#"
/src/IProject.sol:1:13-1:17 -> pong
/src/Impl.sol:2:13-2:17 -> pong
/src/Impl.sol:3:62-3:66 -> pong

"#]],
        );
    }
    fixture.check_prepare_rename("$2", "2:13-2:17\n");
}

#[test]
fn allows_explicit_project_sources_under_library_roots() {
    let fixture = RequestFixture::new(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "lib/contracts"
        remappings = ["local/=lib/contracts/"]
        //- /lib/contracts/Owned.sol
        contract $1Owned {}
        //- /lib/contracts/Impl.sol
        import {Owned} from "./Owned.sol";
        contract Impl is $2Owned {}
        "#,
        "/lib/contracts/Impl.sol",
    );
    for marker in ["$1", "$2"] {
        fixture.check_rename(
            marker,
            "Renamed",
            str![[r#"
/lib/contracts/Impl.sol:0:8-0:13 -> Renamed
/lib/contracts/Impl.sol:1:17-1:22 -> Renamed
/lib/contracts/Owned.sol:0:9-0:14 -> Renamed

"#]],
        );
    }
    fixture.check_prepare_rename("$1", "0:9-0:14\n");
}

#[tokio::test]
async fn allows_dependency_projects_explicitly_opened_as_workspace_roots() {
    let fixture = RequestFixture::new(
        r#"
        //- /foundry.toml
        //- /lib/dep/foundry.toml
        //- /lib/dep/src/Owned.sol
        contract $1Owned {}
        //- /src/Impl.sol
        import {Owned} from "../lib/dep/src/Owned.sol";
        contract Impl is $2Owned {}
        "#,
        "/src/Impl.sol",
    );
    for marker in ["$1", "$2"] {
        let (mut state, params) = fixture.rename_state_and_params(marker, "Renamed");
        let initialize = InitializeParams {
            workspace_folders: Some(
                ["/", "/lib/dep"]
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
        assert!(
            handlers::prepare_rename(&mut state, params.text_document_position.clone())
                .await
                .unwrap()
                .is_some()
        );
        let edit = handlers::rename(&mut state, params).await.unwrap().unwrap();
        let changes = edit.changes.unwrap();
        assert_eq!(changes.len(), 2);
        for (path, count) in [("/lib/dep/src/Owned.sol", 1), ("/src/Impl.sol", 2)] {
            let uri = Url::from_file_path(fixture.project_path(path)).unwrap();
            assert_eq!(changes[&uri].len(), count);
            assert!(changes[&uri].iter().all(|edit| edit.new_text == "Renamed"));
        }
    }
}

#[tokio::test]
async fn rejects_external_remapped_dependencies() {
    let fixture = RequestFixture::new(
        r#"
        //- /project/foundry.toml
        [profile.default]
        remappings = ["dep/=../external/"]
        //- /external/IDep.sol
        interface IDep {}
        //- /project/src/Impl.sol
        import {IDep} from "../../external/IDep.sol";
        contract Impl is $1IDep {}
        "#,
        "/project/src/Impl.sol",
    );
    let (mut state, params) = fixture.rename_state_and_params("$1", "Renamed");
    let initialize = InitializeParams {
        workspace_folders: Some(vec![WorkspaceFolder {
            uri: Url::from_file_path(fixture.project_path("/project")).unwrap(),
            name: "project".into(),
        }]),
        ..Default::default()
    };
    let (_, mut config) = negotiate_capabilities(initialize);
    config.rediscover_workspaces();
    state.config = Arc::new(config);
    assert_dependency_rename_rejected(&mut state, params).await;
}

#[tokio::test]
async fn rename_uses_dependency_policy_published_with_analysis() {
    let fixture = RequestFixture::new(
        r#"
        //- /foundry.toml
        [profile.default]
        libs = ["vendor"]
        //- /vendor/IDep.sol
        interface IDep {}
        //- /src/Impl.sol
        import {IDep} from "../vendor/IDep.sol";
        contract Impl is $1IDep {}
        "#,
        "/src/Impl.sol",
    );
    let (mut state, params) = fixture.rename_state_and_params("$1", "Renamed");
    let published = state.config.clone();
    fixture.write_file("/foundry.toml", "[profile.default]\nlibs = []\n");
    Arc::make_mut(&mut state.config).rediscover_workspaces();
    state.analysis_commit.lock().analysis_config = Some(published);
    assert_dependency_rename_rejected(&mut state, params).await;
}

#[test]
fn allows_first_party_library_directories_inside_sources() {
    let fixture = RequestFixture::new(
        r#"
        //- /foundry.toml
        [profile.default]
        libs = []
        //- /src/lib/Math.sol
        library $1Math {}
        "#,
        "/src/lib/Math.sol",
    );
    fixture.check_rename(
        "$1",
        "Numbers",
        str![[r#"
/src/lib/Math.sol:0:8-0:12 -> Numbers

"#]],
    );
}

#[cfg(unix)]
#[tokio::test]
async fn rejects_symlinks_from_source_files_and_roots_into_dependencies() {
    for directory in [false, true] {
        let fixture = RequestFixture::new(
            r#"
            //- /foundry.toml
            [profile.default]
            //- /lib/dep/src/IDep.sol
            interface IDep {}
            //- /src/IDep.sol
            interface $1IDep {}
            "#,
            "/src/IDep.sol",
        );
        if directory {
            fs::remove_dir_all(fixture.project_path("/src")).unwrap();
            symlink(fixture.project_path("/lib/dep/src"), fixture.project_path("/src")).unwrap();
        } else {
            fs::remove_file(fixture.project_path("/src/IDep.sol")).unwrap();
            symlink(
                fixture.project_path("/lib/dep/src/IDep.sol"),
                fixture.project_path("/src/IDep.sol"),
            )
            .unwrap();
        }
        let (mut state, params) = fixture.rename_state_and_params("$1", "Renamed");
        assert_dependency_rename_rejected(&mut state, params).await;
    }
}

#[tokio::test]
async fn protects_parent_project_libraries_when_source_directory_is_workspace_root() {
    let fixture = RequestFixture::new(
        r#"
        //- /foundry.toml
        [profile.default]
        libs = ["src/vendor"]
        //- /src/vendor/IDep.sol
        interface $1IDep {}
        //- /src/Impl.sol
        import {IDep} from "./vendor/IDep.sol";
        contract Impl is $2IDep {}
        "#,
        "/src/Impl.sol",
    );
    for marker in ["$1", "$2"] {
        let (mut state, params) = fixture.rename_state_and_params(marker, "Renamed");
        let initialize = InitializeParams {
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: Url::from_file_path(fixture.project_path("/src")).unwrap(),
                name: "src".into(),
            }]),
            ..Default::default()
        };
        let (_, mut config) = negotiate_capabilities(initialize);
        config.rediscover_workspaces();
        state.config = Arc::new(config);
        assert_dependency_rename_rejected(&mut state, params).await;
    }
}

#[test]
fn allows_source_alias_remappings_when_project_root_is_explicit_source() {
    let fixture = RequestFixture::new(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "."
        remappings = ["local/=modules/"]
        //- /modules/Owned.sol
        contract $1Owned {}
        //- /Main.sol
        import {Owned} from "./modules/Owned.sol";
        contract Impl is $2Owned {}
        "#,
        "/Main.sol",
    );
    for marker in ["$1", "$2"] {
        fixture.check_rename(
            marker,
            "Renamed",
            str![[r#"
/Main.sol:0:8-0:13 -> Renamed
/Main.sol:1:17-1:22 -> Renamed
/modules/Owned.sol:0:9-0:14 -> Renamed

"#]],
        );
    }
    fixture.check_prepare_rename("$1", "0:9-0:14\n");
}

#[test]
fn allows_remappings_to_sibling_project_sources() {
    let fixture = RequestFixture::new(
        r#"
        //- /a/foundry.toml
        [profile.default]
        remappings = ["b/=../b/src/"]
        //- /b/foundry.toml
        [profile.default]
        //- /b/src/Owned.sol
        contract $1Owned {}
        //- /a/src/Impl.sol
        import {Owned} from "../../b/src/Owned.sol";
        contract Impl is $2Owned {}
        "#,
        "/a/src/Impl.sol",
    );
    for marker in ["$1", "$2"] {
        fixture.check_rename(
            marker,
            "Renamed",
            str![[r#"
/a/src/Impl.sol:0:8-0:13 -> Renamed
/a/src/Impl.sol:1:17-1:22 -> Renamed
/b/src/Owned.sol:0:9-0:14 -> Renamed

"#]],
        );
    }
    fixture.check_prepare_rename("$1", "0:9-0:14\n");
}

#[cfg(unix)]
#[tokio::test]
async fn rejects_source_root_symlinks_into_remapping_only_dependencies() {
    let fixture = RequestFixture::new(
        r#"
        //- /foundry.toml
        [profile.default]
        libs = []
        remappings = ["dep/=vendor/dep/src/"]
        //- /vendor/dep/src/IDep.sol
        interface IDep {}
        //- /src/IDep.sol
        interface $1IDep {}
        "#,
        "/src/IDep.sol",
    );
    fs::remove_dir_all(fixture.project_path("/src")).unwrap();
    symlink(fixture.project_path("/vendor/dep/src"), fixture.project_path("/src")).unwrap();
    let (mut state, params) = fixture.rename_state_and_params("$1", "Renamed");
    assert_dependency_rename_rejected(&mut state, params).await;
}

#[cfg(unix)]
#[tokio::test]
async fn rejects_dangling_dependency_links_but_allows_unsaved_files() {
    for link in [false, true] {
        let fixture = RequestFixture::new(
            r#"
            //- /foundry.toml
            //- /src/New.sol open
            contract $1New {}
            "#,
            "/src/New.sol",
        );
        fs::remove_file(fixture.project_path("/src/New.sol")).unwrap();
        if link {
            symlink(
                fixture.project_path("/lib/dep/Missing.sol"),
                fixture.project_path("/src/New.sol"),
            )
            .unwrap();
        }
        let (mut state, params) = fixture.rename_state_and_params("$1", "Renamed");
        if link {
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
            assert_eq!(changes.len(), 1);
            assert_eq!(changes.values().next().unwrap().len(), 1);
        }
    }
}

use super::*;
use lsp_types::RenameParams;
use snapbox::{IntoData, assert_data_eq};

#[cfg(unix)]
use std::{fs, os::unix::fs::symlink};

mod nested_repositories;
mod repository_grants;

const DEPENDENCY: &str = "cannot rename this symbol because it would modify dependency files\n";
const OUTSIDE: &str =
    "cannot rename this symbol because it would modify files outside the workspace\n";
const INCOMPLETE: &str =
    "cannot rename this symbol because workspace indexing may omit source files\n";

async fn check_report(
    fixture: &RequestFixture,
    state: &mut GlobalState,
    params: RenameParams,
    expected: impl IntoData,
) {
    assert_data_eq!(rename_report(state, params, &fixture.project_path("/")).await, expected);
}

/// Checks the report of renaming `marker` to `Renamed` with the given workspace `roots`.
async fn check_marker(
    fixture: &RequestFixture,
    marker: &str,
    roots: &[&str],
    expected: impl IntoData,
) {
    let (mut state, params) = fixture.rename_state_with_roots(marker, "Renamed", roots);
    check_report(fixture, &mut state, params, expected).await;
}

fn check_allowed_each(
    fixture: &str,
    path: &str,
    markers: &str,
    prepare: &str,
    expected: impl IntoData,
) {
    let fixture = RequestFixture::new(fixture, path);
    fixture.check_renames(&[(markers, "Renamed")], expected);
    fixture.check_prepare_rename(markers.split_whitespace().next().unwrap(), prepare);
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
            check_marker(&fixture, marker, &["/"], DEPENDENCY).await;
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
        (
            "[profile.default]\nlibs = [\"vendor/dep/src\"]\nauto_detect_remappings = false",
            "vendor/dep",
            true,
        ),
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
            check_marker(&fixture, marker, &["/"], DEPENDENCY).await;
        }
    }
}

#[tokio::test]
async fn protects_dependencies_outside_or_above_workspace_roots() {
    for (fixture, path, root, markers) in [
        (
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
            "/project",
            &["$1"][..],
        ),
        (
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
            "/src",
            &["$1", "$2"],
        ),
    ] {
        let fixture = RequestFixture::new(fixture, path);
        for marker in markers {
            check_marker(&fixture, marker, &[root], DEPENDENCY).await;
        }
    }
}

#[tokio::test]
async fn discovered_dependency_cannot_grant_sources_in_another_projects_library() {
    for target in ["vendor/dep/", "vendor/dep/src/"] {
        let fixture = RequestFixture::new(
            &format!(
                r#"
                //- /a/foundry.toml
                [profile.default]
                libs = ["vendor"]
                auto_detect_remappings = false
                //- /a/vendor/owned/Owned.sol
                contract $1Owned {{}}
                //- /a/src/Main.sol
                import "../vendor/owned/Owned.sol";
                contract Main is $2Owned {{}}
                //- /c/foundry.toml
                [profile.default]
                libs = []
                auto_detect_remappings = false
                remappings = ["dep/={target}"]
                //- /c/vendor/dep/foundry.toml
                [profile.default]
                src = "../../../a/vendor/owned"
                //- /c/vendor/dep/src/Dep.sol
                contract Dep {{}}
                "#,
            ),
            "/a/src/Main.sol",
        );
        for marker in ["$1", "$2"] {
            let (mut state, params) = fixture.rename_state_and_params(marker, "Renamed");
            // Exercise real manifest discovery, not a manually synthesized workspace list.
            assert_eq!(state.config.workspaces().len(), 3);
            let dependency = workspace_at(&state.config, &fixture.project_path("/c/vendor/dep"));
            assert!(
                dependency.import_source_roots().contains(&fixture.project_path("/a/vendor/owned"))
            );
            check_report(&fixture, &mut state, params, DEPENDENCY).await;
        }
    }
}

#[test]
fn allows_local_aliases_and_project_sources() {
    check_allowed_each(
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
        "$1 $2",
        "0:16-0:24\n",
        str![[r#"
$1 $2:
/src/Impl.sol:0:16-0:24 -> Renamed
/src/Impl.sol:2:4-2:12 -> Renamed

"#]],
    );
    check_allowed_each(
        r#"
        //- /foundry.toml
        [profile.default]
        remappings = ["local/=src/"]
        //- /src/IProject.sol
        interface IProject {
            function $2ping() external;
        }
        //- /src/Impl.sol
        import {IProject} from "./IProject.sol";
        contract Impl is IProject {
            function $1ping() external override {}
            function callProject(IProject project) external { project.$3ping(); }
        }
        "#,
        "/src/Impl.sol",
        "$1 $2 $3",
        "2:13-2:17\n",
        str![[r#"
$1 $2 $3:
/src/IProject.sol:1:13-1:17 -> Renamed
/src/Impl.sol:2:13-2:17 -> Renamed
/src/Impl.sol:3:62-3:66 -> Renamed

"#]],
    );
    check_allowed_each(
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
        "$1 $2",
        "0:9-0:14\n",
        str![[r#"
$1 $2:
/lib/contracts/Impl.sol:0:8-0:13 -> Renamed
/lib/contracts/Impl.sol:1:17-1:22 -> Renamed
/lib/contracts/Owned.sol:0:9-0:14 -> Renamed

"#]],
    );
    check_allowed_each(
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
        "$1 $2",
        "0:9-0:14\n",
        str![[r#"
$1 $2:
/Main.sol:0:8-0:13 -> Renamed
/Main.sol:1:17-1:22 -> Renamed
/modules/Owned.sol:0:9-0:14 -> Renamed

"#]],
    );
}

#[test]
fn allows_sources_owned_by_other_workspaces() {
    check_allowed_each(
        r#"
        //- /a/foundry.toml
        [profile.default]
        src = "src"
        libs = ["vendor"]
        auto_detect_remappings = false
        //- /b/foundry.toml
        [profile.default]
        src = "../a/vendor/owned"
        libs = []
        auto_detect_remappings = false
        //- /a/vendor/owned/Owned.sol
        contract Owned {
            function $1ping() external {}
        }
        //- /a/src/Impl.sol
        import {Owned} from "../vendor/owned/Owned.sol";
        contract Impl {
            function call(Owned owned) external { owned.$2ping(); }
        }
        "#,
        "/a/src/Impl.sol",
        "$1 $2",
        "1:13-1:17\n",
        str![[r#"
$1 $2:
/a/src/Impl.sol:2:48-2:52 -> Renamed
/a/vendor/owned/Owned.sol:1:13-1:17 -> Renamed

"#]],
    );
    check_allowed_each(
        r#"
        //- /a/foundry.toml
        [profile.default]
        remappings = ["shared/=../shared/"]
        auto_detect_remappings = false
        //- /b/foundry.toml
        [profile.default]
        src = "../shared"
        libs = []
        auto_detect_remappings = false
        //- /shared/Owned.sol
        contract Owned {
            function $1ping() external {}
        }
        //- /a/src/Impl.sol
        import {Owned} from "../../shared/Owned.sol";
        contract Impl {
            function call(Owned owned) external { owned.$2ping(); }
        }
        "#,
        "/a/src/Impl.sol",
        "$1 $2",
        "1:13-1:17\n",
        str![[r#"
$1 $2:
/a/src/Impl.sol:2:48-2:52 -> Renamed
/shared/Owned.sol:1:13-1:17 -> Renamed

"#]],
    );
    for target in ["../b/src/", "../b/"] {
        check_allowed_each(
            &format!(
                r#"
                //- /a/foundry.toml
                [profile.default]
                remappings = ["b/={target}"]
                //- /b/foundry.toml
                [profile.default]
                //- /b/src/Owned.sol
                contract $1Owned {{}}
                //- /a/src/Impl.sol
                import {{Owned}} from "../../b/src/Owned.sol";
                contract Impl is $2Owned {{}}
                "#,
            ),
            "/a/src/Impl.sol",
            "$1 $2",
            "0:9-0:14\n",
            str![[r#"
$1 $2:
/a/src/Impl.sol:0:8-0:13 -> Renamed
/a/src/Impl.sol:1:17-1:22 -> Renamed
/b/src/Owned.sol:0:9-0:14 -> Renamed

"#]],
        );
    }
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
        check_marker(
            &fixture,
            marker,
            &["/", "/lib/dep"],
            str![[r#"
/lib/dep/src/Owned.sol:0:9-0:14 -> Renamed
/src/Impl.sol:0:8-0:13 -> Renamed
/src/Impl.sol:1:17-1:22 -> Renamed

"#]],
        )
        .await;
    }
}

#[tokio::test]
async fn rename_refreshes_dependency_policy_after_workspace_rediscovery() {
    let fixture = RequestFixture::new(
        r#"
        //- /foundry.toml
        [profile.default]
        libs = []
        auto_detect_remappings = false
        //- /vendor/Owned.sol
        contract Owned {}
        //- /src/Impl.sol
        import {Owned} from "../vendor/Owned.sol";
        contract Impl is $1Owned {}
        "#,
        "/src/Impl.sol",
    );
    let (mut state, params) = fixture.rename_state_and_params("$1", "Renamed");
    // Fixtures without a published analysis configuration use the current configuration.
    assert!(state.analysis_commit.lock().analysis_config.is_none());
    check_report(
        &fixture,
        &mut state,
        params.clone(),
        str![[r#"
/src/Impl.sol:0:8-0:13 -> Renamed
/src/Impl.sol:1:17-1:22 -> Renamed
/vendor/Owned.sol:0:9-0:14 -> Renamed

"#]],
    )
    .await;

    fixture.write_file(
        "/foundry.toml",
        "[profile.default]\nlibs = [\"vendor\"]\nauto_detect_remappings = false\n",
    );
    Arc::make_mut(&mut state.config).rediscover_workspaces();
    check_report(&fixture, &mut state, params, DEPENDENCY).await;
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
    check_report(&fixture, &mut state, params, DEPENDENCY).await;
}

#[tokio::test]
async fn allows_first_party_library_directories_inside_sources() {
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
    for (use_default_excludes, expected) in
        [(true, INCOMPLETE), (false, "/src/lib/Math.sol:0:8-0:12 -> Numbers\n")]
    {
        let (mut state, params) = fixture.rename_state_and_params("$1", "Numbers");
        let options = json!({ "indexing": { "useDefaultExcludes": use_default_excludes } });
        let config = config_with_options(fixture.project().initialize_params(), options);
        assert_eq!(config.may_omit_source_files(), use_default_excludes);
        state.config = Arc::new(config);
        check_report(&fixture, &mut state, params, expected).await;
    }
}

#[cfg(unix)]
#[tokio::test]
async fn rejects_source_symlinks_into_dependencies() {
    for (configuration, dependency, directory) in [
        ("", "/lib/dep/src", false),
        ("", "/lib/dep/src", true),
        ("libs = []\nremappings = [\"dep/=vendor/dep/src/\"]", "/vendor/dep/src", true),
    ] {
        let fixture = RequestFixture::new(
            &format!(
                "//- /foundry.toml\n[profile.default]\n{configuration}\n\
                 //- {dependency}/IDep.sol\ninterface IDep {{}}\n\
                 //- /src/IDep.sol\ninterface $1IDep {{}}\n"
            ),
            "/src/IDep.sol",
        );
        let (link, target) = if directory {
            fs::remove_dir_all(fixture.project_path("/src")).unwrap();
            ("/src".to_owned(), dependency.to_owned())
        } else {
            fs::remove_file(fixture.project_path("/src/IDep.sol")).unwrap();
            ("/src/IDep.sol".to_owned(), format!("{dependency}/IDep.sol"))
        };
        symlink(fixture.project_path(&target), fixture.project_path(&link)).unwrap();
        check_marker(&fixture, "$1", &["/"], DEPENDENCY).await;
    }
}

#[cfg(unix)]
#[tokio::test]
async fn rejects_retargeted_dependency_symlinks_without_configuration_changes() {
    let fixture = RequestFixture::new(
        r#"
        //- /foundry.toml
        [profile.default]
        auto_detect_remappings = false
        //- /vendor_old/IDep.sol
        interface IDep {}
        //- /vendor_new/IDep.sol
        interface IDep {}
        //- /src/IDep.sol
        interface $1IDep {}
        "#,
        "/src/IDep.sol",
    );
    let library = fixture.project_path("/lib");
    let source = fixture.project_path("/src/IDep.sol");
    symlink(fixture.project_path("/vendor_old"), &library).unwrap();
    let (mut state, params) = fixture.rename_state_and_params("$1", "Renamed");
    let config = Arc::clone(&state.config);
    check_report(&fixture, &mut state, params.clone(), "/src/IDep.sol:0:10-0:14 -> Renamed\n")
        .await;

    fs::remove_file(&library).unwrap();
    symlink(fixture.project_path("/vendor_new"), &library).unwrap();
    fs::remove_file(&source).unwrap();
    symlink(fixture.project_path("/vendor_new/IDep.sol"), &source).unwrap();

    assert!(Arc::ptr_eq(&state.config, &config));
    check_report(&fixture, &mut state, params, DEPENDENCY).await;
    assert!(Arc::ptr_eq(&state.config, &config));
}

#[cfg(unix)]
#[tokio::test]
async fn rejects_dangling_links_but_allows_unsaved_files() {
    for target in [None, Some("/lib/dep/Missing.sol"), Some("/scratch/Missing.sol")] {
        let fixture = RequestFixture::new(
            "//- /foundry.toml\n//- /src/New.sol open\ncontract $1New {}\n",
            "/src/New.sol",
        );
        fs::remove_file(fixture.project_path("/src/New.sol")).unwrap();
        if let Some(target) = target {
            symlink(fixture.project_path(target), fixture.project_path("/src/New.sol")).unwrap();
        }
        let expected = if target.is_some() {
            "cannot rename this symbol because its file paths could not be verified\n"
        } else {
            "/src/New.sol:0:9-0:12 -> Renamed\n"
        };
        check_marker(&fixture, "$1", &["/"], expected).await;
    }
}

#[tokio::test]
async fn keeps_dependency_locals_read_only() {
    let fixture = RequestFixture::new(
        r#"
        //- /foundry.toml
        [profile.default]
        //- /lib/dep/Dep.sol
        contract Dep {
            function value() public pure returns (uint256) {
                uint256 $1local = 1;
                return local;
            }
        }
        //- /src/Main.sol
        import "../lib/dep/Dep.sol";
        contract Main {
            function $3read(Dep dep) public pure returns (uint256) {
                uint256 $2local = dep.value();
                return local;
            }
        }
        "#,
        "/src/Main.sol",
    );
    let state = |marker: &str, complete: bool| {
        let (state, params) = fixture.rename_state_and_params(marker, "renamed");
        publish_analysis_config(&state, complete);
        (state, params)
    };
    for complete in [true, false] {
        let (mut dependency, params) = state("$1", complete);
        check_report(&fixture, &mut dependency, params, DEPENDENCY).await;

        let (mut local, params) = state("$2", complete);
        check_report(
            &fixture,
            &mut local,
            params,
            str![[r#"
/src/Main.sol:3:16-3:21 -> renamed
/src/Main.sol:4:15-4:20 -> renamed

"#]],
        )
        .await;

        let (mut function, params) = state("$3", complete);
        if complete {
            check_report(&fixture, &mut function, params, "/src/Main.sol:2:13-2:17 -> renamed\n")
                .await;
        } else {
            check_report(&fixture, &mut function, params, INCOMPLETE).await;
        }
    }
}

fn scratch_fixture(source: &str, target: &str) -> RequestFixture {
    RequestFixture::new(
        &format!(
            r#"
            //- /ws/foundry.toml
            //- {source} open
            contract Scratch {{
                function value() public pure returns (uint256) {{
                    uint256 $1local = 1;
                    return local;
                }}
            }}
            //- {target}
            contract Target {{}}
            "#,
        ),
        source,
    )
}

#[tokio::test]
async fn reports_open_files_outside_the_workspace() {
    for unsaved in [false, true] {
        let fixture = scratch_fixture("/scratch/Scratch.sol", "/ws/src/Target.sol");
        let contents = fixture.project_contents("/scratch/Scratch.sol");
        if unsaved {
            std::fs::remove_file(fixture.project_path("/scratch/Scratch.sol")).unwrap();
        }
        // An unanalyzed server must analyze the open buffer before rejecting the rename.
        let (uri, position) = fixture.marker_location("$1");
        let mut state = state_with(fixture.project().config_with_roots(&["/ws"]));
        open(&mut state, &uri, 1, contents);
        let params = rename_params(&uri, position, "Renamed");
        check_report(&fixture, &mut state, params, OUTSIDE).await;
    }
}

#[cfg(unix)]
#[tokio::test]
async fn distinguishes_outside_symlinks_from_dependencies() {
    for (source, target, message) in [
        ("/ws/src/Scratch.sol", "/scratch/Target.sol", OUTSIDE),
        ("/scratch/Scratch.sol", "/ws/src/Target.sol", OUTSIDE),
        ("/scratch/Scratch.sol", "/ws/lib/dep/Target.sol", DEPENDENCY),
        ("/ws/lib/dep/Scratch.sol", "/scratch/Target.sol", DEPENDENCY),
    ] {
        let fixture = scratch_fixture(source, target);
        fs::remove_file(fixture.project_path(source)).unwrap();
        symlink(fixture.project_path(target), fixture.project_path(source)).unwrap();
        check_marker(&fixture, "$1", &["/ws"], message).await;
    }
}

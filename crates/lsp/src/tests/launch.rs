use crate::{
    FoundryWorkspaceConfig, LaunchConfig,
    global_state::GlobalState,
    new_server_service_with_router, proto,
    test_support::TestProject,
    workspace::{Workspace, WorkspaceKind},
};
use async_lsp::{AnyRequest, ClientSocket, router::Router};
use lsp_types::InitializeParams;
use solar_config::{EvmVersion, LspArgs};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tower::Service;

async fn initialized_state(config: LaunchConfig, mut params: InitializeParams) -> GlobalState {
    let mut state = GlobalState::new(ClientSocket::new_closed()).with_launch_config(config);
    params.initialization_options = Some(serde_json::json!({ "flychecks": [] }));
    state.on_initialize(params).await.unwrap();
    rediscover(&mut state);
    state
}

fn rediscover(state: &mut GlobalState) {
    let _ = Arc::make_mut(&mut state.config).rediscover_workspaces();
}

fn workspace_at<'a>(state: &'a GlobalState, root: &Path) -> &'a Workspace {
    state
        .config
        .workspaces()
        .iter()
        .find(|workspace| workspace.compile_opts().base_path.as_deref() == Some(root))
        .unwrap_or_else(|| panic!("expected a workspace at `{}`", root.display()))
}

#[tokio::test(flavor = "current_thread")]
async fn launch_config_supplies_the_default_forge_path() {
    let config = LaunchConfig::from(LspArgs { stdio: true });
    let mut state = GlobalState::new(ClientSocket::new_closed()).with_launch_config(config);
    state.on_initialize(InitializeParams::default()).await.unwrap();
    assert_eq!(state.config.forge_path(), Path::new("forge"));

    let observed_path = Arc::new(Mutex::new(None::<PathBuf>));
    let server_observed_path = observed_path.clone();
    let config = LaunchConfig::default().with_default_forge_path("/embedded/forge");
    let mut service =
        new_server_service_with_router(ClientSocket::new_closed(), config, move |state| {
            let mut router = Router::new(state);
            router.request::<proto::Initialize, _>(move |state, params| {
                let response = state.on_initialize(params.into_inner());
                *server_observed_path.lock().unwrap() = Some(state.config.forge_path());
                response
            });
            router
        });
    let request = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 1,
        "method": "initialize",
        "params": InitializeParams::default(),
    }))
    .unwrap();

    service.call(request).await.unwrap();

    assert_eq!(*observed_path.lock().unwrap(), Some(PathBuf::from("/embedded/forge")));
}

#[tokio::test(flavor = "current_thread")]
async fn initialize_applies_launch_config_selected_profile_to_workspace_discovery() {
    let project = TestProject::from_fixture(
        r#"
        //- /default-src/Default.sol
        contract DefaultContract {}

        //- /custom-src/Custom.sol
        contract CustomContract {}

        //- /foundry.toml
        [profile.default]
        src = "default-src"

        [profile.custom]
        src = "custom-src"
        "#,
    );
    let config = LaunchConfig::default().with_selected_profile("custom");
    let state = initialized_state(config, project.initialize_params()).await;

    let workspace = state
        .config
        .workspaces()
        .iter()
        .find(|workspace| workspace.kind() == WorkspaceKind::Foundry)
        .unwrap();
    assert_eq!(
        workspace.source_roots(),
        &[
            project.path("/"),
            project.path("/custom-src"),
            project.path("/test"),
            project.path("/script")
        ]
    );
    assert_eq!(
        workspace.source_files(),
        &[project.path("/custom-src/Custom.sol"), project.path("/default-src/Default.sol")]
    );
}

#[test]
fn foundry_workspace_config_normalizes_paths_and_replaces_equivalent_roots() {
    let project = TestProject::new();
    let launch_config = LaunchConfig::default().with_foundry_workspace_config(
        FoundryWorkspaceConfig::new(project.path("/workspace/./nested/.."))
            .with_source_roots([PathBuf::from("src/../src"), project.path("/external/src/../src")])
            .with_flycheck_source_roots([PathBuf::from("test/../test")])
            .with_include_paths([
                PathBuf::from("lib/../lib"),
                PathBuf::from("../external/lib/../lib"),
            ]),
    );
    let [config] = launch_config.foundry_workspace_configs() else { panic!("expected one config") };

    assert_eq!(config.workspace_root(), project.path("/workspace"));
    assert_eq!(
        config.source_roots(),
        [project.path("/workspace/src"), project.path("/external/src")]
    );
    assert_eq!(config.flycheck_source_roots(), [project.path("/workspace/test")]);
    assert_eq!(
        config.include_paths(),
        [project.path("/workspace/lib"), project.path("/external/lib")]
    );

    let launch_config = launch_config.with_foundry_workspace_config(
        FoundryWorkspaceConfig::new(project.path("/workspace"))
            .with_source_roots([project.path("/workspace/second")]),
    );
    let [config] = launch_config.foundry_workspace_configs() else { panic!("expected one config") };
    assert_eq!(config.source_roots(), [project.path("/workspace/second")]);
}

#[test]
#[should_panic(expected = "Foundry workspace config root must be absolute")]
fn foundry_workspace_config_rejects_relative_workspace_root() {
    let _ = LaunchConfig::default()
        .with_foundry_workspace_config(FoundryWorkspaceConfig::new("relative/workspace"));
}

#[tokio::test(flavor = "current_thread")]
async fn initialize_applies_host_resolved_foundry_workspace_config() {
    let project = TestProject::from_fixture(
        r#"
        //- /default-src/Default.sol
        contract DefaultContract {}

        //- /custom-src/Custom.sol
        contract CustomContract {}

        //- /default-test/Default.t.sol
        contract DefaultTest {}

        //- /custom-test/Custom.t.sol
        contract CustomTest {}

        //- /custom-libs/pkg/src/Lib.sol
        contract Lib {}

        //- /remappings.txt
        local/=default-src/

        //- /custom-src/nested/foundry.toml
        [profile.default]
        src = "src"

        //- /custom-src/nested/src/Nested.sol
        contract NestedContract {}

        //- /base.toml
        [profile.custom]
        src = "custom-src"
        test = "custom-test"

        //- /foundry.toml
        [profile.default]
        src = "default-src"
        test = "default-test"

        [profile.custom]
        extends = "base.toml"
        "#,
    );
    let resolved = FoundryWorkspaceConfig::new(project.root())
        .with_source_roots(["custom-src"])
        .with_flycheck_source_roots(["custom-src", "custom-test"])
        .with_include_paths(["custom-libs"])
        .with_import_remappings(["host/=custom-src/".parse().unwrap()])
        .with_evm_version(EvmVersion::Cancun);
    let config = LaunchConfig::default()
        .with_selected_profile("custom")
        .with_foundry_workspace_config(resolved);
    let state = initialized_state(config, project.initialize_params()).await;

    let workspace = workspace_at(&state, project.root());
    assert_eq!(workspace.source_roots(), &[project.path("/custom-src")]);
    assert_eq!(workspace.source_files(), &[project.path("/custom-src/Custom.sol")]);
    assert_eq!(
        workspace.flycheck_source_files(),
        &[project.path("/custom-src/Custom.sol"), project.path("/custom-test/Custom.t.sol"),]
    );
    assert_eq!(workspace.compile_opts().include_paths, [project.path("/custom-libs")]);
    assert_eq!(workspace.compile_opts().evm_version, EvmVersion::Cancun);
    assert_eq!(
        workspace
            .compile_opts()
            .import_remappings
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["host/=custom-src/"]
    );
    assert_eq!(
        workspace_at(&state, &project.path("/custom-src/nested")).source_roots(),
        &[
            project.path("/custom-src/nested"),
            project.path("/custom-src/nested/src"),
            project.path("/custom-src/nested/test"),
            project.path("/custom-src/nested/script")
        ]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn host_foundry_workspace_configs_match_their_own_roots() {
    let project = TestProject::from_fixture(
        r#"
        //- /one/local-one/One.sol
        contract OneLocal {}

        //- /one/host-one/One.sol
        contract OneHost {}

        //- /one/foundry.toml
        [profile.default]
        src = "local-one"

        //- /two/local-two/Two.sol
        contract TwoLocal {}

        //- /two/host-two/Two.sol
        contract TwoHost {}

        //- /two/foundry.toml
        [profile.default]
        src = "local-two"

        //- /three/local-three/Three.sol
        contract ThreeLocal {}

        //- /three/foundry.toml
        [profile.default]
        src = "local-three"
        "#,
    );
    let config = LaunchConfig::default().with_foundry_workspace_configs([
        FoundryWorkspaceConfig::new(project.path("/one"))
            .with_source_roots([project.path("/one/host-one")])
            .with_flycheck_source_roots([project.path("/one/host-one")]),
        FoundryWorkspaceConfig::new(project.path("/two"))
            .with_source_roots([project.path("/two/host-two")])
            .with_flycheck_source_roots([project.path("/two/host-two")]),
    ]);
    let params = project.initialize_params_with_roots(&["/one", "/two", "/three"]);
    let state = initialized_state(config, params).await;

    let source_roots = |root: &str| workspace_at(&state, &project.path(root)).source_roots();
    assert_eq!(source_roots("/one"), [project.path("/one/host-one")]);
    assert_eq!(source_roots("/two"), [project.path("/two/host-two")]);
    assert_eq!(
        source_roots("/three"),
        [
            project.path("/three"),
            project.path("/three/local-three"),
            project.path("/three/test"),
            project.path("/three/script")
        ]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn host_foundry_workspace_config_loader_covers_new_nested_workspaces_once_per_pass() {
    let project = TestProject::from_fixture(
        r#"
        //- /packages/Root.sol
        contract RootContract {}

        //- /foundry.toml
        [profile.default]
        src = "packages"
        "#,
    );
    let loads = Arc::new(AtomicUsize::new(0));
    let loader_loads = loads.clone();
    let config = LaunchConfig::default().with_foundry_workspace_config_loader(move |root| {
        loader_loads.fetch_add(1, Ordering::Relaxed);
        let source = if root.ends_with("nested") { "host-src" } else { "packages" };
        Ok::<_, String>(
            FoundryWorkspaceConfig::new(root)
                .with_source_roots([source])
                .with_flycheck_source_roots([source]),
        )
    });
    let mut state = initialized_state(config, project.initialize_params()).await;
    assert_eq!(loads.load(Ordering::Relaxed), 1);

    project.write_file("/packages/nested/foundry.toml", "[profile.default]\nsrc = \"local-src\"\n");
    project.write_file("/packages/nested/host-src/Host.sol", "contract HostContract {}\n");
    project.write_file("/packages/nested/local-src/Local.sol", "contract LocalContract {}\n");
    rediscover(&mut state);

    assert_eq!(loads.load(Ordering::Relaxed), 3);
    let nested = workspace_at(&state, &project.path("/packages/nested"));
    assert_eq!(nested.source_roots(), &[project.path("/packages/nested/host-src")]);
    assert_eq!(nested.source_files(), &[project.path("/packages/nested/host-src/Host.sol")]);
}

#[tokio::test(flavor = "current_thread")]
async fn host_foundry_workspace_config_loader_refreshes_and_keeps_last_good_discovery() {
    let project = TestProject::from_fixture(
        r#"
        //- /old-src/Old.sol
        contract OldContract {}

        //- /new-src/New.sol
        contract NewContract {}

        //- /foundry.toml
        [profile.default]
        src = "old-src"
        "#,
    );
    let fail = Arc::new(AtomicBool::new(false));
    let loader_fail = fail.clone();
    let config = LaunchConfig::default().with_foundry_workspace_config_loader(move |root| {
        if loader_fail.load(Ordering::Relaxed) {
            return Err("host config unavailable");
        }
        let mut manifest = String::new();
        std::fs::File::open(root.join("foundry.toml"))
            .unwrap()
            .read_to_string(&mut manifest)
            .unwrap();
        let source = if manifest.contains("new-src") { "new-src" } else { "old-src" };
        Ok(FoundryWorkspaceConfig::new(root).with_source_roots([source]))
    });
    let mut state = initialized_state(config, project.initialize_params()).await;
    let assert_sources = |state: &GlobalState, source: &str, file: &str| {
        let workspace = workspace_at(state, project.root());
        assert_eq!(workspace.source_roots(), &[project.path(source)]);
        assert_eq!(workspace.source_files(), &[project.path(file)]);
    };
    assert_sources(&state, "/old-src", "/old-src/Old.sol");

    project.write_file("/foundry.toml", "[profile.default]\nsrc = \"new-src\"\n");
    fail.store(true, Ordering::Relaxed);
    rediscover(&mut state);
    assert_sources(&state, "/old-src", "/old-src/Old.sol");

    fail.store(false, Ordering::Relaxed);
    rediscover(&mut state);
    assert_sources(&state, "/new-src", "/new-src/New.sol");
}

#[tokio::test(flavor = "current_thread")]
async fn host_foundry_workspace_config_loader_rejects_invalid_roots_without_panicking() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Test.sol
        contract TestContract {}

        //- /foundry.toml
        [profile.default]
        src = "src"
        "#,
    );
    let loads = Arc::new(AtomicUsize::new(0));
    let loader_loads = loads.clone();
    let wrong_root = project.path("/other");
    let config = LaunchConfig::default().with_foundry_workspace_config_loader(move |root| {
        let root = match loader_loads.fetch_add(1, Ordering::Relaxed) {
            0 => root.to_path_buf(),
            1 => PathBuf::from("relative"),
            _ => wrong_root.clone(),
        };
        Ok::<_, String>(FoundryWorkspaceConfig::new(root).with_source_roots(["src"]))
    });
    let mut state = GlobalState::new(ClientSocket::new_closed()).with_launch_config(config);
    state.on_initialize(project.initialize_params()).await.unwrap();
    for _ in 0..3 {
        rediscover(&mut state);
    }

    let workspace = state.config.workspaces().first().unwrap();
    assert_eq!(workspace.source_roots(), &[project.path("/src")]);
    assert_eq!(workspace.source_files(), &[project.path("/src/Test.sol")]);
}

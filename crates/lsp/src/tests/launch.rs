use crate::{
    LaunchConfig, global_state::GlobalState, new_server_service_with_router, proto,
    test_support::TestProject, workspace::WorkspaceKind,
};
use async_lsp::{AnyRequest, ClientSocket, router::Router};
use lsp_types::InitializeParams;
use solar_config::LspArgs;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tower::Service;

#[tokio::test(flavor = "current_thread")]
async fn lsp_args_use_the_default_launch_configuration() {
    let config = LaunchConfig::from(LspArgs { stdio: true });
    let mut state = GlobalState::new(ClientSocket::new_closed()).with_launch_config(config);

    state.on_initialize(InitializeParams::default()).await.unwrap();

    assert_eq!(state.config.forge_path(), Path::new("forge"));
}

#[tokio::test(flavor = "current_thread")]
async fn initialize_applies_launch_config_default_forge_path() {
    let config = LaunchConfig::default().with_default_forge_path("/embedded/forge");
    let mut state = GlobalState::new(ClientSocket::new_closed()).with_launch_config(config);

    state.on_initialize(InitializeParams::default()).await.unwrap();

    assert_eq!(state.config.forge_path(), Path::new("/embedded/forge"));
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
    let mut state = GlobalState::new(ClientSocket::new_closed()).with_launch_config(config);
    let mut params = project.initialize_params();
    params.initialization_options = Some(serde_json::json!({ "flychecks": [] }));

    state.on_initialize(params).await.unwrap();
    let _ = Arc::make_mut(&mut state.config).rediscover_workspaces();

    let workspace = state
        .config
        .workspaces()
        .iter()
        .find(|workspace| workspace.kind() == WorkspaceKind::Foundry)
        .unwrap();
    assert_eq!(workspace.source_roots(), &[project.path("/custom-src")]);
    assert_eq!(workspace.source_files(), &[project.path("/custom-src/Custom.sol")]);
}

#[tokio::test(flavor = "current_thread")]
async fn configured_server_service_applies_launch_default_during_initialize() {
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

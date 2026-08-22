use crate::{LaunchConfig, global_state::GlobalState, new_server_service_with_router, proto};
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
async fn initialize_applies_launch_config_selected_profile() {
    let config = LaunchConfig::default().with_selected_profile("custom");
    let mut state = GlobalState::new(ClientSocket::new_closed()).with_launch_config(config);

    state.on_initialize(InitializeParams::default()).await.unwrap();

    assert_eq!(state.config.selected_profile(), Some("custom"));
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

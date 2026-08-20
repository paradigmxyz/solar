use crate::{LaunchConfig, global_state::GlobalState};
use async_lsp::ClientSocket;
use lsp_types::InitializeParams;
use std::path::Path;

#[test]
fn launch_config_accepts_a_default_forge_path() {
    assert_eq!(LaunchConfig::default().default_forge_path(), None);

    let config = LaunchConfig::default().with_default_forge_path("/embedded/forge");

    assert_eq!(config.default_forge_path(), Some(Path::new("/embedded/forge")));
}

#[tokio::test(flavor = "current_thread")]
async fn initialize_applies_launch_config_default_forge_path() {
    let config = LaunchConfig::default().with_default_forge_path("/embedded/forge");
    let mut state = GlobalState::new(ClientSocket::new_closed()).with_launch_config(config);

    state.on_initialize(InitializeParams::default()).await.unwrap();

    assert_eq!(state.config.forge_path(), Path::new("/embedded/forge"));
}

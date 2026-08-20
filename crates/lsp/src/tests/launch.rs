use crate::{LaunchConfig, global_state::GlobalState};
#[cfg(unix)]
use crate::{new_server_service_with_launch_config, test_support::TestProject};
use async_lsp::ClientSocket;
#[cfg(unix)]
use async_lsp::{LanguageServer, router::Router};
use lsp_types::InitializeParams;
#[cfg(unix)]
use lsp_types::{
    DocumentFormattingParams, FormattingOptions, InitializedParams, TextDocumentIdentifier,
    WorkDoneProgressParams, notification as notif,
};
use solar_config::LspArgs;
use std::path::Path;
#[cfg(unix)]
use std::{ops::ControlFlow, os::unix::fs::PermissionsExt};
#[cfg(unix)]
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

#[test]
fn launch_config_converts_lsp_args_and_accepts_a_default_forge_path() {
    assert_eq!(LaunchConfig::default().default_forge_path(), None);

    let config = LaunchConfig::from(LspArgs::default());
    assert_eq!(config.default_forge_path(), None);

    let config = config.with_default_forge_path("/embedded/forge");

    assert_eq!(config.default_forge_path(), Some(Path::new("/embedded/forge")));
}

#[tokio::test(flavor = "current_thread")]
async fn initialize_applies_launch_config_default_forge_path() {
    let config = LaunchConfig::default().with_default_forge_path("/embedded/forge");
    let mut state = GlobalState::new(ClientSocket::new_closed()).with_launch_config(config);

    state.on_initialize(InitializeParams::default()).await.unwrap();

    assert_eq!(state.config.forge_path(), Path::new("/embedded/forge"));
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn configured_server_service_uses_launch_default_for_formatting() {
    let project = TestProject::from_fixture(
        r#"
        //- /workspace/Test.sol
        contract Test{}
        "#,
    );
    project.write_file(
        "/embedded-forge",
        r#"#!/bin/sh
set -eu
case "${1-}" in
config)
printf '%s\n' "$@" > "$0.config-args"
printf '%s' '{"fmt":{"ignore":[]}}'
;;
fmt)
printf '%s\n' "$@" > "$0.fmt-args"
cat > "$0.stdin"
printf 'contract Test {}'
;;
esac
"#,
    );
    let forge = project.path("/embedded-forge");
    let mut permissions = std::fs::metadata(&forge).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&forge, permissions).unwrap();

    let launch_config = LaunchConfig::default().with_default_forge_path(&forge);
    let (server_main, _client) = async_lsp::MainLoop::new_server(move |client| {
        new_server_service_with_launch_config(client, launch_config)
    });
    let (client_main, mut server) = async_lsp::MainLoop::new_client(|_| {
        let mut router = Router::new(());
        router.notification::<notif::LogMessage>(|_, _| ControlFlow::Continue(()));
        router.notification::<notif::PublishDiagnostics>(|_, _| ControlFlow::Continue(()));
        router
    });
    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_rx, server_tx) = tokio::io::split(server_stream);
    let server_main =
        tokio::spawn(server_main.run_buffered(server_rx.compat(), server_tx.compat_write()));
    let (client_rx, client_tx) = tokio::io::split(client_stream);
    let client_main =
        tokio::spawn(client_main.run_buffered(client_rx.compat(), client_tx.compat_write()));

    server.initialize(project.initialize_params_with_roots(&["/workspace"])).await.unwrap();
    server.initialized(InitializedParams {}).unwrap();
    let path = project.path("/workspace/Test.sol");
    let edits = server
        .request::<lsp_types::request::Formatting>(DocumentFormattingParams {
            text_document: TextDocumentIdentifier::new(
                lsp_types::Url::from_file_path(&path).unwrap(),
            ),
            options: FormattingOptions::default(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .await
        .unwrap()
        .unwrap();

    assert_eq!(edits[0].new_text, "contract Test {}");
    assert_eq!(project.read_file("/embedded-forge.stdin"), "contract Test{}");
    assert_eq!(
        project.read_file("/embedded-forge.config-args"),
        format!("config\n--json\n--root\n{}\n", project.path("/workspace").display())
    );
    assert_eq!(
        project.read_file("/embedded-forge.fmt-args"),
        format!("fmt\n--raw\n--root\n{}\n-\n", project.path("/workspace").display())
    );

    server.shutdown(()).await.unwrap();
    server.exit(()).unwrap();
    assert!(server_main.await.unwrap().is_ok());
    assert!(matches!(client_main.await.unwrap(), Err(async_lsp::Error::Eof)));
}

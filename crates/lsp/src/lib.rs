#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

use async_lsp::{
    client_monitor::ClientProcessMonitorLayer, concurrency::ConcurrencyLayer,
    server::LifecycleLayer,
};
use tower::ServiceBuilder;

use crate::server::Server;

mod server;

/// Start the LSP server over stdin/stdout.
///
/// This future is long running and will not stop until the server exits.
pub async fn run_server_stdio() -> solar_interface::Result<()> {
    // Prefer truly asynchronous piped stdin/stdout without blocking tasks.
    #[cfg(unix)]
    let (stdin, stdout) = (
        async_lsp::stdio::PipeStdin::lock_tokio().unwrap(),
        async_lsp::stdio::PipeStdout::lock_tokio().unwrap(),
    );

    // Fallback to spawn blocking read/write otherwise.
    #[cfg(not(unix))]
    let (stdin, stdout) = (
        tokio_util::compat::TokioAsyncReadCompatExt::compat(tokio::io::stdin()),
        tokio_util::compat::TokioAsyncWriteCompatExt::compat_write(tokio::io::stdout()),
    );

    let (eloop, _) = async_lsp::MainLoop::new_server(|client| {
        ServiceBuilder::new()
            .layer(LifecycleLayer::default())
            // TODO: infer concurrency
            .layer(ConcurrencyLayer::new(2.try_into().unwrap()))
            .layer(ClientProcessMonitorLayer::new(client.clone()))
            .service(Server::new_router(client))
    });

    eloop.run_buffered(stdin, stdout).await.unwrap();
    Ok(())
}

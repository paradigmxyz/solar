#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

use std::ops::ControlFlow;

use async_lsp::{
    ClientSocket, client_monitor::ClientProcessMonitorLayer, concurrency::ConcurrencyLayer,
    router::Router, server::LifecycleLayer, tracing::TracingLayer,
};
use lsp_types::{notification as notif, request as req};
use tower::ServiceBuilder;

use crate::global_state::GlobalState;

mod global_state;
mod handlers;
mod proto;
mod utils;
mod vfs;

pub(crate) type NotifyResult = ControlFlow<async_lsp::Result<()>>;

fn new_router(client: ClientSocket) -> Router<GlobalState> {
    let this = GlobalState::new(client);
    let mut router = Router::new(this);

    // Lifecycle
    router
        .request::<req::Initialize, _>(GlobalState::on_initialize)
        .notification::<notif::Initialized>(GlobalState::on_initialized)
        .request::<req::Shutdown, _>(|_, _| std::future::ready(Ok(())))
        .notification::<notif::Exit>(|_, _| ControlFlow::Break(Ok(())));

    // Notifications
    router
        .notification::<notif::DidOpenTextDocument>(handlers::did_open_text_document)
        .notification::<notif::DidCloseTextDocument>(handlers::did_close_text_document)
        .notification::<notif::DidChangeTextDocument>(handlers::did_change_text_document);

    router
}

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
            .layer(TracingLayer::default())
            .layer(LifecycleLayer::default())
            // TODO: infer concurrency
            .layer(ConcurrencyLayer::new(2.try_into().unwrap()))
            .layer(ClientProcessMonitorLayer::new(client.clone()))
            .service(new_router(client))
    });

    eloop.run_buffered(stdin, stdout).await.unwrap();
    Ok(())
}

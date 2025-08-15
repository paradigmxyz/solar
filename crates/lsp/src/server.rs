use std::ops::ControlFlow;

use async_lsp::{ClientSocket, ResponseError, router::Router};
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    InitializeParams, InitializeResult, InitializedParams, LogMessageParams, MessageType,
    ServerInfo, notification as notif, request as req,
};
use solar_config::version::SHORT_VERSION;

type NotifyResult = ControlFlow<async_lsp::Result<()>>;

pub(crate) struct Server {
    client: ClientSocket,
}

impl Server {
    pub(crate) fn new_router(client: ClientSocket) -> Router<Self> {
        let this = Self::new(client);
        let mut router = Router::new(this);

        // Lifecycle
        router
            .request::<req::Initialize, _>(Self::on_initialize)
            .notification::<notif::Initialized>(Self::on_initialized)
            .request::<req::Shutdown, _>(|_, _| std::future::ready(Ok(())))
            .notification::<notif::Exit>(|_, _| ControlFlow::Break(Ok(())));

        // Notifications
        router
            .notification::<notif::DidOpenTextDocument>(Self::on_did_open)
            .notification::<notif::DidCloseTextDocument>(Self::on_did_close)
            .notification::<notif::DidChangeTextDocument>(Self::on_did_change);

        router
    }

    fn new(client: ClientSocket) -> Self {
        Self { client }
    }

    fn on_initialize(
        &mut self,
        _: InitializeParams,
    ) -> impl Future<Output = Result<InitializeResult, ResponseError>> + use<> {
        std::future::ready(Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "solar".into(),
                version: Some(SHORT_VERSION.into()),
            }),
            ..Default::default()
        }))
    }

    fn on_initialized(&mut self, _: InitializedParams) -> NotifyResult {
        let _ = self.client.notify::<notif::LogMessage>(LogMessageParams {
            typ: MessageType::INFO,
            message: "solar initialized".into(),
        });
        ControlFlow::Continue(())
    }

    fn on_did_open(&mut self, _: DidOpenTextDocumentParams) -> NotifyResult {
        todo!()
    }

    fn on_did_close(&mut self, _: DidCloseTextDocumentParams) -> NotifyResult {
        todo!()
    }

    fn on_did_change(&mut self, _: DidChangeTextDocumentParams) -> NotifyResult {
        todo!()
    }
}

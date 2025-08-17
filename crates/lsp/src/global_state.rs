use std::{ops::ControlFlow, sync::Arc};

use async_lsp::{ClientSocket, ResponseError};
use lsp_types::{
    InitializeParams, InitializeResult, InitializedParams, LogMessageParams, MessageType,
    ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, notification as notif,
};
use solar_config::version::SHORT_VERSION;
use solar_interface::data_structures::sync::RwLock;

use crate::{NotifyResult, vfs::Vfs};

pub(crate) struct GlobalState {
    client: ClientSocket,
    pub(crate) vfs: Arc<RwLock<Vfs>>,
}

impl GlobalState {
    pub(crate) fn new(client: ClientSocket) -> Self {
        Self { client, vfs: Arc::new(Default::default()) }
    }

    pub(crate) fn on_initialize(
        &mut self,
        _: InitializeParams,
    ) -> impl Future<Output = Result<InitializeResult, ResponseError>> + use<> {
        std::future::ready(Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        will_save: None,
                        will_save_wait_until: None,
                        ..Default::default()
                    },
                )),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "solar".into(),
                version: Some(SHORT_VERSION.into()),
            }),
        }))
    }

    pub(crate) fn on_initialized(&mut self, _: InitializedParams) -> NotifyResult {
        self.info_msg("solar initialized");
        ControlFlow::Continue(())
    }
}

impl GlobalState {
    #[expect(unused)]
    fn warn_msg(&self, message: impl Into<String>) {
        let _ = self.client.notify::<notif::LogMessage>(LogMessageParams {
            typ: MessageType::WARNING,
            message: message.into(),
        });
    }

    #[expect(unused)]
    fn error_msg(&self, message: impl Into<String>) {
        let _ = self.client.notify::<notif::LogMessage>(LogMessageParams {
            typ: MessageType::ERROR,
            message: message.into(),
        });
    }

    fn info_msg(&self, message: impl Into<String>) {
        let _ = self.client.notify::<notif::LogMessage>(LogMessageParams {
            typ: MessageType::INFO,
            message: message.into(),
        });
    }
}

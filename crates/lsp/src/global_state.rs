use std::{ops::ControlFlow, sync::Arc};

use async_lsp::{ClientSocket, ResponseError};
use lsp_types::{
    InitializeParams, InitializeResult, InitializedParams, LogMessageParams, MessageType,
    ServerInfo, notification as notif,
};
use solar_config::version::SHORT_VERSION;
use solar_interface::data_structures::sync::RwLock;

use crate::{NotifyResult, config::negotiate_capabilities, vfs::Vfs};

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
        params: InitializeParams,
    ) -> impl Future<Output = Result<InitializeResult, ResponseError>> + use<> {
        let (capabilities, _config) = negotiate_capabilities(params);

        std::future::ready(Ok(InitializeResult {
            capabilities,
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

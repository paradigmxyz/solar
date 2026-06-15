use lsp_types::{
    ClientCapabilities, InitializeParams, InitializeResult, ServerCapabilities, ServerInfo,
};

use crate::version;

#[derive(Default)]
pub(super) struct State {
    client_capabilities: Option<ClientCapabilities>,
}

impl State {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn initialize(&mut self, params: InitializeParams) -> InitializeResult {
        self.client_capabilities = Some(params.capabilities);
        InitializeResult {
            capabilities: ServerCapabilities::default(),
            server_info: Some(ServerInfo {
                name: "solar".into(),
                version: Some(version::short_version().into()),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_advertises_server_identity_without_capabilities() {
        let mut state = State::new();
        let result = state.initialize(InitializeParams::default());

        let server_info = result.server_info.unwrap();
        assert_eq!(server_info.name, "solar");
        assert_eq!(server_info.version.as_deref(), Some(version::short_version()));
        assert_eq!(result.capabilities, ServerCapabilities::default());
        assert!(state.client_capabilities.is_some());
    }
}

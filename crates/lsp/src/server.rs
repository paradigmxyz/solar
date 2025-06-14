//! Main LSP server implementation.

use crate::{capabilities::server_capabilities, config::ServerConfig, session::SessionManager};
use parking_lot::RwLock;
use std::sync::Arc;
use tower_lsp::{jsonrpc, lsp_types::*, Client, LanguageServer};

/// The Solar Language Server.
#[derive(Clone)]
pub struct SolarLanguageServer {
    /// LSP client for sending messages.
    client: Client,
    /// Session manager for documents and Solar compilation.
    pub session_manager: Arc<SessionManager>,
    /// Server configuration.
    config: Arc<RwLock<ServerConfig>>,
}

impl SolarLanguageServer {
    /// Create a new Solar Language Server.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            session_manager: Arc::new(SessionManager::new()),
            config: Arc::new(RwLock::new(ServerConfig::default())),
        }
    }

    /// Log a message to the client.
    async fn log(&self, typ: MessageType, message: String) {
        self.client.log_message(typ, message).await;
    }

    /// Log an error message.
    async fn log_error(&self, message: String) {
        self.log(MessageType::ERROR, message).await;
    }

    /// Log an info message.
    async fn log_info(&self, message: String) {
        self.log(MessageType::INFO, message).await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for SolarLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> jsonrpc::Result<InitializeResult> {
        self.log_info("Initializing Solar Language Server".to_string()).await;

        // Store workspace root if provided
        if let Some(folders) = params.workspace_folders {
            if let Some(folder) = folders.first() {
                let mut config = self.config.write();
                config.workspace_root = folder.uri.to_file_path().ok();
            }
        } else if let Some(root) = params.root_uri {
            #[allow(deprecated)]
            let mut config = self.config.write();
            config.workspace_root = root.to_file_path().ok();
        }

        // Initialize Solar session
        self.session_manager.initialize_session().map_err(jsonrpc::Error::from)?;

        Ok(InitializeResult { capabilities: server_capabilities(), ..Default::default() })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.log_info("Solar Language Server initialized successfully".to_string()).await;
    }

    async fn shutdown(&self) -> jsonrpc::Result<()> {
        self.log_info("Shutting down Solar Language Server".to_string()).await;
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let content = params.text_document.text;

        self.session_manager.open_document(uri.clone(), version, content);
        self.log_info(format!("Opened document: {uri}")).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        // Apply content changes
        let mut content = match self.session_manager.get_document(&uri) {
            Some(doc) => doc.content,
            None => {
                self.log_error(format!("Document not open: {uri}")).await;
                return;
            }
        };

        for change in params.content_changes {
            match change.range {
                Some(_range) => {
                    // Incremental update - not implemented yet
                    self.log_error("Incremental updates not yet supported".to_string()).await;
                    return;
                }
                None => {
                    // Full document update
                    content = change.text;
                }
            }
        }

        if let Err(e) = self.session_manager.update_document(&uri, version, content) {
            self.log_error(format!("Failed to update document: {e}")).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Err(e) = self.session_manager.close_document(&uri) {
            self.log_error(format!("Failed to close document: {e}")).await;
        }
        self.log_info(format!("Closed document: {uri}")).await;
    }

    async fn hover(&self, _params: HoverParams) -> jsonrpc::Result<Option<Hover>> {
        // TODO: Implement hover
        Ok(None)
    }

    async fn completion(
        &self,
        _params: CompletionParams,
    ) -> jsonrpc::Result<Option<CompletionResponse>> {
        // TODO: Implement completion
        Ok(None)
    }

    async fn goto_definition(
        &self,
        _params: GotoDefinitionParams,
    ) -> jsonrpc::Result<Option<GotoDefinitionResponse>> {
        // TODO: Implement goto definition
        Ok(None)
    }

    async fn references(&self, _params: ReferenceParams) -> jsonrpc::Result<Option<Vec<Location>>> {
        // TODO: Implement find references
        Ok(None)
    }

    async fn document_symbol(
        &self,
        _params: DocumentSymbolParams,
    ) -> jsonrpc::Result<Option<DocumentSymbolResponse>> {
        // TODO: Implement document symbols
        Ok(None)
    }

    async fn symbol(
        &self,
        _params: WorkspaceSymbolParams,
    ) -> jsonrpc::Result<Option<Vec<SymbolInformation>>> {
        // TODO: Implement workspace symbols
        Ok(None)
    }

    async fn code_action(
        &self,
        _params: CodeActionParams,
    ) -> jsonrpc::Result<Option<CodeActionResponse>> {
        // TODO: Implement code actions
        Ok(None)
    }

    async fn formatting(
        &self,
        _params: DocumentFormattingParams,
    ) -> jsonrpc::Result<Option<Vec<TextEdit>>> {
        // TODO: Implement formatting
        Ok(None)
    }

    async fn range_formatting(
        &self,
        _params: DocumentRangeFormattingParams,
    ) -> jsonrpc::Result<Option<Vec<TextEdit>>> {
        // TODO: Implement range formatting
        Ok(None)
    }

    async fn semantic_tokens_full(
        &self,
        _params: SemanticTokensParams,
    ) -> jsonrpc::Result<Option<SemanticTokensResult>> {
        // TODO: Implement semantic tokens
        Ok(None)
    }

    async fn semantic_tokens_range(
        &self,
        _params: SemanticTokensRangeParams,
    ) -> jsonrpc::Result<Option<SemanticTokensRangeResult>> {
        // TODO: Implement semantic tokens range
        Ok(None)
    }
}

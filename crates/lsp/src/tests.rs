//! Tests for the LSP server.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::SolarLanguageServer;
    use tower_lsp::{lsp_types::*, LanguageServer, LspService};

    /// Create a test server with a mock client.
    async fn create_test_server() -> SolarLanguageServer {
        let (service, _socket) = LspService::new(SolarLanguageServer::new);
        service.inner().clone()
    }

    /// Create test initialization params.
    fn create_test_init_params() -> InitializeParams {
        InitializeParams {
            process_id: Some(std::process::id()),
            client_info: Some(ClientInfo {
                name: "test-client".to_string(),
                version: Some("0.1.0".to_string()),
            }),
            root_uri: Some(Url::parse("file:///test/workspace").unwrap()),
            capabilities: ClientCapabilities::default(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_server_initialization() {
        let server = create_test_server().await;
        let init_params = create_test_init_params();
        let result = server.initialize(init_params).await.unwrap();

        // Verify capabilities are declared
        assert!(result.capabilities.text_document_sync.is_some());
        assert!(result.capabilities.hover_provider.is_some());
        assert!(result.capabilities.completion_provider.is_some());
        assert!(result.capabilities.definition_provider.is_some());
        assert!(result.capabilities.references_provider.is_some());
        assert!(result.capabilities.document_symbol_provider.is_some());
        assert!(result.capabilities.workspace_symbol_provider.is_some());
        assert!(result.capabilities.code_action_provider.is_some());
        assert!(result.capabilities.document_formatting_provider.is_some());
        assert!(result.capabilities.semantic_tokens_provider.is_some());
    }

    #[tokio::test]
    async fn test_server_shutdown() {
        let server = create_test_server().await;

        // Initialize first
        let init_params = create_test_init_params();
        let _ = server.initialize(init_params).await.unwrap();

        // Test shutdown
        let result = server.shutdown().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_document_lifecycle() {
        let server = create_test_server().await;

        // Initialize first
        let init_params = create_test_init_params();
        let _ = server.initialize(init_params).await.unwrap();

        // Test document open
        let uri = Url::parse("file:///test/contract.sol").unwrap();
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "solidity".to_string(),
                version: 1,
                text: "contract Test {}".to_string(),
            },
        };
        server.did_open(params).await;

        // Verify document was opened
        let doc = server.session_manager.get_document(&uri);
        assert!(doc.is_some());
        let doc = doc.unwrap();
        assert_eq!(doc.version, 1);
        assert_eq!(doc.content, "contract Test {}");

        // Test document change
        let change_params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier { uri: uri.clone(), version: 2 },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "contract Test { uint256 x; }".to_string(),
            }],
        };
        server.did_change(change_params).await;

        // Verify document was updated
        let doc = server.session_manager.get_document(&uri).unwrap();
        assert_eq!(doc.version, 2);
        assert_eq!(doc.content, "contract Test { uint256 x; }");

        // Test document close
        let close_params = DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
        };
        server.did_close(close_params).await;

        // Verify document was closed
        assert!(server.session_manager.get_document(&uri).is_none());
    }

    #[tokio::test]
    async fn test_multiple_documents() {
        let server = create_test_server().await;

        // Initialize
        let _ = server.initialize(create_test_init_params()).await.unwrap();

        // Open multiple documents
        let uris = vec![
            Url::parse("file:///test/contract1.sol").unwrap(),
            Url::parse("file:///test/contract2.sol").unwrap(),
            Url::parse("file:///test/contract3.sol").unwrap(),
        ];

        for (i, uri) in uris.iter().enumerate() {
            let params = DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "solidity".to_string(),
                    version: 1,
                    text: format!("contract Test{} {{}}", i + 1),
                },
            };
            server.did_open(params).await;
        }

        // Verify all documents are open
        let all_docs = server.session_manager.all_documents();
        assert_eq!(all_docs.len(), 3);

        // Close one document
        server
            .did_close(DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier { uri: uris[1].clone() },
            })
            .await;

        // Verify correct number of documents
        let all_docs = server.session_manager.all_documents();
        assert_eq!(all_docs.len(), 2);
    }
}

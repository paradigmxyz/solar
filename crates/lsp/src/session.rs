//! Solar session management for the LSP server.

use crate::{document::DocumentStore, error::Result};
use parking_lot::RwLock;
use solar_interface::{diagnostics::DiagCtxt, Session, SessionGlobals, SourceMap};
use std::sync::Arc;
use tower_lsp::lsp_types::{TextDocumentContentChangeEvent, Url};

/// Manages open documents and their Solar sessions.
pub struct SessionManager {
    /// Document store with thread-safe access.
    document_store: RwLock<DocumentStore>,
    /// The Solar session.
    session: RwLock<Option<Arc<Session>>>,
}

impl SessionManager {
    /// Create a new session manager.
    pub fn new() -> Self {
        Self { document_store: RwLock::new(DocumentStore::new()), session: RwLock::new(None) }
    }

    /// Initialize the Solar session.
    pub fn initialize_session(&self) -> Result<()> {
        let globals = SessionGlobals::new();
        globals.set(|| {
            let source_map = Arc::new(SourceMap::empty());
            let dcx = DiagCtxt::with_stderr_emitter(Some(source_map.clone()));
            let session = Session::new(dcx, source_map);
            *self.session.write() = Some(Arc::new(session));
        });
        Ok(())
    }

    /// Get the current session.
    pub fn session(&self) -> Option<Arc<Session>> {
        self.session.read().clone()
    }

    /// Open a document.
    pub fn open_document(
        &self,
        uri: Url,
        version: i32,
        content: String,
        language_id: Option<String>,
    ) -> Result<()> {
        let mut store = self.document_store.write();
        store.open_document(uri, version, content, language_id)
    }

    /// Update a document with incremental changes.
    pub fn update_document(
        &self,
        uri: &Url,
        version: i32,
        changes: Vec<TextDocumentContentChangeEvent>,
    ) -> Result<()> {
        let mut store = self.document_store.write();
        store.update_document(uri, version, changes)
    }

    /// Close a document.
    pub fn close_document(&self, uri: &Url) -> Result<()> {
        let mut store = self.document_store.write();
        store.close_document(uri)
    }

    /// Get a document by URI (cloned for thread safety).
    pub fn get_document(&self, uri: &Url) -> Option<crate::document::Document> {
        let store = self.document_store.read();
        store.get_document(uri).cloned()
    }

    /// Get all open documents (cloned for thread safety).
    pub fn all_documents(&self) -> Vec<(Url, crate::document::Document)> {
        let store = self.document_store.read();
        store.all_documents().map(|(uri, doc)| (uri.clone(), doc.clone())).collect()
    }

    /// Add a document watcher.
    pub fn add_document_watcher(
        &self,
        uri: &Url,
        watcher: Arc<dyn crate::document::DocumentWatcher>,
    ) {
        let mut store = self.document_store.write();
        store.add_watcher(uri, watcher);
    }

    /// Remove a document watcher.
    pub fn remove_document_watcher(
        &self,
        uri: &Url,
        watcher: &Arc<dyn crate::document::DocumentWatcher>,
    ) {
        let mut store = self.document_store.write();
        store.remove_watcher(uri, watcher);
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

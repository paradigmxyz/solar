//! Solar session management for the LSP server.

use crate::error::Result;
use dashmap::DashMap;
use solar_interface::{diagnostics::DiagCtxt, Session, SessionGlobals, SourceMap};
use std::sync::Arc;
use tower_lsp::lsp_types::Url;

/// Document content stored by the LSP server.
#[derive(Debug, Clone)]
pub struct Document {
    /// The document URI.
    pub uri: Url,
    /// The document version.
    pub version: i32,
    /// The document content.
    pub content: String,
}

/// Manages open documents and their Solar sessions.
pub struct SessionManager {
    /// Currently open documents.
    documents: DashMap<Url, Document>,
    /// The Solar session.
    session: parking_lot::RwLock<Option<Arc<Session>>>,
}

impl SessionManager {
    /// Create a new session manager.
    pub fn new() -> Self {
        Self { documents: DashMap::new(), session: parking_lot::RwLock::new(None) }
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
    pub fn open_document(&self, uri: Url, version: i32, content: String) {
        let doc = Document { uri: uri.clone(), version, content };
        self.documents.insert(uri, doc);
    }

    /// Update a document.
    pub fn update_document(&self, uri: &Url, version: i32, content: String) -> Result<()> {
        if let Some(mut doc) = self.documents.get_mut(uri) {
            doc.version = version;
            doc.content = content;
            Ok(())
        } else {
            Err(crate::error::Error::InvalidParams(format!("Document not open: {uri}")))
        }
    }

    /// Close a document.
    pub fn close_document(&self, uri: &Url) -> Result<()> {
        self.documents.remove(uri);
        Ok(())
    }

    /// Get a document by URI.
    pub fn get_document(&self, uri: &Url) -> Option<Document> {
        self.documents.get(uri).map(|doc| doc.clone())
    }

    /// Get all open documents.
    pub fn all_documents(&self) -> Vec<Document> {
        self.documents.iter().map(|entry| entry.value().clone()).collect()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

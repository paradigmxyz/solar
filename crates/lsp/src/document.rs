//! Document management for LSP server.

use crate::error::{Error, Result};
use std::{collections::HashMap, sync::Arc, time::Instant};
use tower_lsp::lsp_types::{Position, Range, TextDocumentContentChangeEvent, Url};

/// Line index for efficient position ↔ offset mapping.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Byte offsets of newline characters.
    newlines: Vec<usize>,
    /// Total content length in bytes.
    content_len: usize,
}

impl LineIndex {
    /// Create a new line index from content.
    pub fn new(content: &str) -> Self {
        let mut newlines = Vec::new();
        let mut offset = 0;

        for ch in content.chars() {
            if ch == '\n' {
                newlines.push(offset);
            }
            offset += ch.len_utf8();
        }

        Self { newlines, content_len: content.len() }
    }

    /// Convert a LSP Position to a byte offset.
    pub fn offset(&self, position: Position) -> Result<usize> {
        let line = position.line as usize;
        let character = position.character as usize;

        // Calculate the start of the line
        let line_start = if line == 0 {
            0
        } else if line <= self.newlines.len() {
            self.newlines[line - 1] + 1 // +1 to skip the newline character
        } else {
            return Err(Error::InvalidPosition(position));
        };

        // Calculate the character offset within the line
        // We need to handle UTF-8 properly here
        let line_end =
            if line < self.newlines.len() { self.newlines[line] } else { self.content_len };

        if line_start > self.content_len {
            return Err(Error::InvalidPosition(position));
        }

        // For now, we'll assume character == byte offset within the line
        // This is a simplification - proper implementation would need UTF-8 handling
        let offset = line_start + character;

        if offset > line_end {
            return Err(Error::InvalidPosition(position));
        }

        Ok(offset)
    }

    /// Convert a byte offset to a LSP Position.
    pub fn position(&self, offset: usize) -> Result<Position> {
        if offset > self.content_len {
            return Err(Error::InvalidOffset(offset));
        }

        // Find the line containing this offset
        let line = match self.newlines.binary_search(&offset) {
            Ok(line) => line,  // If offset is exactly at a newline, we're still on this line
            Err(line) => line, // Insert position gives us the line number
        };

        // Calculate the start of the line
        let line_start = if line == 0 {
            0
        } else if line > 0 && line - 1 < self.newlines.len() {
            self.newlines[line - 1] + 1
        } else {
            0
        };

        let character = offset.saturating_sub(line_start);

        Ok(Position::new(line as u32, character as u32))
    }

    /// Update the line index after a text change.
    pub fn update(&mut self, range: Range, new_text: &str) -> Result<()> {
        let start_offset = self.offset(range.start)?;
        let end_offset = self.offset(range.end)?;

        // Calculate the change in length
        let old_len = end_offset - start_offset;
        let new_len = new_text.len();
        let len_diff = new_len as i64 - old_len as i64;

        // Update newlines that come after the change
        for newline_offset in &mut self.newlines {
            if *newline_offset >= end_offset {
                *newline_offset = (*newline_offset as i64 + len_diff) as usize;
            }
        }

        // Remove newlines within the changed range
        self.newlines.retain(|&offset| offset < start_offset || offset >= end_offset);

        // Add new newlines from the replacement text
        let mut offset = start_offset;
        for ch in new_text.chars() {
            if ch == '\n' {
                self.newlines.push(offset);
            }
            offset += ch.len_utf8();
        }

        // Sort newlines to maintain order
        self.newlines.sort_unstable();

        // Update content length
        self.content_len = (self.content_len as i64 + len_diff) as usize;

        Ok(())
    }
}

/// Document stored by the LSP server.
#[derive(Debug, Clone)]
pub struct Document {
    /// The document URI.
    pub uri: Url,
    /// The document version.
    pub version: i32,
    /// The document content.
    pub content: String,
    /// The language ID (e.g., "solidity", "yul").
    pub language_id: String,
    /// When the document was last modified.
    pub last_modified: Instant,
    /// Line index for position mapping.
    pub line_index: LineIndex,
}

impl Document {
    /// Create a new document.
    pub fn new(uri: Url, version: i32, content: String, language_id: String) -> Result<Self> {
        let line_index = LineIndex::new(&content);

        Ok(Self { uri, version, content, language_id, last_modified: Instant::now(), line_index })
    }

    /// Apply an incremental text change to the document.
    pub fn apply_change(&mut self, change: TextDocumentContentChangeEvent) -> Result<()> {
        match change.range {
            Some(range) => {
                // Incremental update
                let start_offset = self.line_index.offset(range.start)?;
                let end_offset = self.line_index.offset(range.end)?;

                // Validate offsets
                if start_offset > self.content.len()
                    || end_offset > self.content.len()
                    || start_offset > end_offset
                {
                    return Err(Error::InvalidRange(range));
                }

                // Apply the change to content
                self.content.replace_range(start_offset..end_offset, &change.text);

                // Update the line index
                self.line_index.update(range, &change.text)?;
            }
            None => {
                // Full document replacement
                self.content = change.text;
                self.line_index = LineIndex::new(&self.content);
            }
        }

        self.last_modified = Instant::now();
        Ok(())
    }

    /// Get the text within a given range.
    pub fn text_in_range(&self, range: Range) -> Result<String> {
        let start_offset = self.line_index.offset(range.start)?;
        let end_offset = self.line_index.offset(range.end)?;

        if start_offset > self.content.len()
            || end_offset > self.content.len()
            || start_offset > end_offset
        {
            return Err(Error::InvalidRange(range));
        }

        Ok(self.content[start_offset..end_offset].to_string())
    }
}

/// Detect language ID from file extension.
pub fn detect_language_id(uri: &Url) -> String {
    if let Some(path) = uri.path().split('/').next_back() {
        if let Some(extension) = path.split('.').next_back() {
            match extension {
                "sol" => "solidity".to_string(),
                "yul" => "yul".to_string(),
                _ => "solidity".to_string(), // Default to Solidity
            }
        } else {
            "solidity".to_string()
        }
    } else {
        "solidity".to_string()
    }
}

/// Events that can occur to documents.
#[derive(Debug, Clone)]
pub enum DocumentEvent {
    /// Document was opened.
    Opened,
    /// Document was changed.
    Changed,
    /// Document was closed.
    Closed,
}

/// Trait for objects that want to be notified of document changes.
pub trait DocumentWatcher: Send + Sync {
    /// Called when a document is opened.
    fn on_document_opened(&self, document: &Document);

    /// Called when a document is changed.
    fn on_document_changed(&self, document: &Document, changes: &[TextDocumentContentChangeEvent]);

    /// Called when a document is closed.
    fn on_document_closed(&self, uri: &Url);
}

/// Store for managing documents and their watchers.
pub struct DocumentStore {
    /// Currently open documents.
    documents: HashMap<Url, Document>,
    /// Watchers registered for document events.
    watchers: HashMap<Url, Vec<Arc<dyn DocumentWatcher>>>,
}

impl DocumentStore {
    /// Create a new document store.
    pub fn new() -> Self {
        Self { documents: HashMap::new(), watchers: HashMap::new() }
    }

    /// Open a document.
    pub fn open_document(
        &mut self,
        uri: Url,
        version: i32,
        content: String,
        language_id: Option<String>,
    ) -> Result<()> {
        let language_id = language_id.unwrap_or_else(|| detect_language_id(&uri));
        let document = Document::new(uri.clone(), version, content, language_id)?;

        // Notify watchers
        self.notify_watchers(&uri, DocumentEvent::Opened, &document, &[]);

        self.documents.insert(uri, document);
        Ok(())
    }

    /// Update a document with incremental changes.
    pub fn update_document(
        &mut self,
        uri: &Url,
        version: i32,
        changes: Vec<TextDocumentContentChangeEvent>,
    ) -> Result<()> {
        {
            let document =
                self.documents.get_mut(uri).ok_or_else(|| Error::DocumentNotFound(uri.clone()))?;

            // Validate version to prevent race conditions
            if version <= document.version {
                return Err(Error::StaleVersion { current: document.version, received: version });
            }

            // Apply all changes
            for change in &changes {
                document.apply_change(change.clone())?;
            }

            document.version = version;
        }

        // Notify watchers (borrow ends above)
        let document = self.documents.get(uri).unwrap(); // Safe because we just modified it
        self.notify_watchers(uri, DocumentEvent::Changed, document, &changes);

        Ok(())
    }

    /// Close a document.
    pub fn close_document(&mut self, uri: &Url) -> Result<()> {
        self.documents.remove(uri).ok_or_else(|| Error::DocumentNotFound(uri.clone()))?;

        // Notify watchers
        if let Some(watchers) = self.watchers.get(uri) {
            for watcher in watchers {
                watcher.on_document_closed(uri);
            }
        }

        // Remove watchers for this document
        self.watchers.remove(uri);

        Ok(())
    }

    /// Get a document by URI.
    pub fn get_document(&self, uri: &Url) -> Option<&Document> {
        self.documents.get(uri)
    }

    /// Get a mutable reference to a document by URI.
    pub fn get_document_mut(&mut self, uri: &Url) -> Option<&mut Document> {
        self.documents.get_mut(uri)
    }

    /// Get all open documents.
    pub fn all_documents(&self) -> impl Iterator<Item = (&Url, &Document)> {
        self.documents.iter()
    }

    /// Add a watcher for a specific document.
    pub fn add_watcher(&mut self, uri: &Url, watcher: Arc<dyn DocumentWatcher>) {
        self.watchers.entry(uri.clone()).or_default().push(watcher);
    }

    /// Remove a watcher for a specific document.
    pub fn remove_watcher(&mut self, uri: &Url, watcher: &Arc<dyn DocumentWatcher>) {
        if let Some(watchers) = self.watchers.get_mut(uri) {
            watchers.retain(|w| !Arc::ptr_eq(w, watcher));
            if watchers.is_empty() {
                self.watchers.remove(uri);
            }
        }
    }

    /// Notify all watchers for a document event.
    fn notify_watchers(
        &self,
        uri: &Url,
        event: DocumentEvent,
        document: &Document,
        changes: &[TextDocumentContentChangeEvent],
    ) {
        if let Some(watchers) = self.watchers.get(uri) {
            for watcher in watchers {
                match event {
                    DocumentEvent::Opened => watcher.on_document_opened(document),
                    DocumentEvent::Changed => watcher.on_document_changed(document, changes),
                    DocumentEvent::Closed => watcher.on_document_closed(uri),
                }
            }
        }
    }
}

impl Default for DocumentStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Position;

    #[test]
    fn test_line_index_creation() {
        let content = "line1\nline2\nline3";
        let line_index = LineIndex::new(content);

        assert_eq!(line_index.newlines, vec![5, 11]); // Positions of '\n' characters
        assert_eq!(line_index.content_len, 17);
    }

    #[test]
    fn test_position_to_offset() {
        let content = "line1\nline2\nline3";
        let line_index = LineIndex::new(content);

        // Test various positions
        assert_eq!(line_index.offset(Position::new(0, 0)).unwrap(), 0); // Start of first line
        assert_eq!(line_index.offset(Position::new(0, 5)).unwrap(), 5); // End of first line (at \n)
        assert_eq!(line_index.offset(Position::new(1, 0)).unwrap(), 6); // Start of second line
        assert_eq!(line_index.offset(Position::new(1, 2)).unwrap(), 8); // 'n' in "line2"
        assert_eq!(line_index.offset(Position::new(2, 0)).unwrap(), 12); // Start of third line
    }

    #[test]
    fn test_offset_to_position() {
        let content = "line1\nline2\nline3";
        let line_index = LineIndex::new(content);

        assert_eq!(line_index.position(0).unwrap(), Position::new(0, 0));
        assert_eq!(line_index.position(5).unwrap(), Position::new(0, 5));
        assert_eq!(line_index.position(6).unwrap(), Position::new(1, 0));
        assert_eq!(line_index.position(8).unwrap(), Position::new(1, 2));
        assert_eq!(line_index.position(12).unwrap(), Position::new(2, 0));
    }

    #[test]
    fn test_incremental_update() {
        let mut content = "contract Test {}".to_string();
        let mut line_index = LineIndex::new(&content);

        // Insert text at position (0, 13) - before the closing brace
        let range = Range::new(Position::new(0, 13), Position::new(0, 13));
        let new_text = "\n    uint256 x;";

        // Apply the change
        content.replace_range(13..13, new_text);
        line_index.update(range, new_text).unwrap();

        // Verify the line index was updated correctly
        assert_eq!(line_index.newlines, vec![13]); // Position of the inserted newline
        assert_eq!(line_index.content_len, content.len());

        // Test position mapping on the updated content
        assert_eq!(line_index.position(14).unwrap(), Position::new(1, 0)); // Start of new line
    }

    #[test]
    fn test_document_creation() {
        let uri = Url::parse("file:///test.sol").unwrap();
        let content = "contract Test {}".to_string();
        let doc = Document::new(uri.clone(), 1, content.clone(), "solidity".to_string()).unwrap();

        assert_eq!(doc.uri, uri);
        assert_eq!(doc.version, 1);
        assert_eq!(doc.content, content);
        assert_eq!(doc.language_id, "solidity");
    }

    #[test]
    fn test_document_incremental_change() {
        let uri = Url::parse("file:///test.sol").unwrap();
        let mut doc =
            Document::new(uri, 1, "contract Test {}".to_string(), "solidity".to_string()).unwrap();

        let change = TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(0, 15), Position::new(0, 15))),
            range_length: None,
            text: "\n    uint256 x;".to_string(),
        };

        doc.apply_change(change).unwrap();

        assert_eq!(doc.content, "contract Test {\n    uint256 x;}");
    }

    #[test]
    fn test_detect_language_id() {
        let sol_uri = Url::parse("file:///test.sol").unwrap();
        let yul_uri = Url::parse("file:///test.yul").unwrap();
        let unknown_uri = Url::parse("file:///test.txt").unwrap();

        assert_eq!(detect_language_id(&sol_uri), "solidity");
        assert_eq!(detect_language_id(&yul_uri), "yul");
        assert_eq!(detect_language_id(&unknown_uri), "solidity"); // Default
    }

    #[test]
    fn test_document_store_open_close() {
        let mut store = DocumentStore::new();
        let uri = Url::parse("file:///test.sol").unwrap();
        let content = "contract Test {}".to_string();

        // Open document
        store.open_document(uri.clone(), 1, content.clone(), None).unwrap();

        // Verify document exists
        let doc = store.get_document(&uri).unwrap();
        assert_eq!(doc.uri, uri);
        assert_eq!(doc.version, 1);
        assert_eq!(doc.content, content);
        assert_eq!(doc.language_id, "solidity");

        // Close document
        store.close_document(&uri).unwrap();
        assert!(store.get_document(&uri).is_none());
    }

    #[test]
    fn test_document_store_update() {
        let mut store = DocumentStore::new();
        let uri = Url::parse("file:///test.sol").unwrap();

        // Open document
        store.open_document(uri.clone(), 1, "contract Test {}".to_string(), None).unwrap();

        // Update document
        let changes = vec![TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(0, 15), Position::new(0, 15))),
            range_length: None,
            text: "\n    uint256 x;".to_string(),
        }];

        store.update_document(&uri, 2, changes).unwrap();

        let doc = store.get_document(&uri).unwrap();
        assert_eq!(doc.version, 2);
        assert_eq!(doc.content, "contract Test {\n    uint256 x;}");
    }

    #[test]
    fn test_document_store_version_validation() {
        let mut store = DocumentStore::new();
        let uri = Url::parse("file:///test.sol").unwrap();

        // Open document
        store.open_document(uri.clone(), 5, "contract Test {}".to_string(), None).unwrap();

        // Try to update with stale version
        let changes = vec![TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "contract Updated {}".to_string(),
        }];

        let result = store.update_document(&uri, 3, changes); // Lower version
        assert!(matches!(result, Err(Error::StaleVersion { current: 5, received: 3 })));
    }

    // Mock DocumentWatcher for testing
    struct MockWatcher {
        events: std::sync::Mutex<Vec<String>>,
    }

    impl MockWatcher {
        fn new() -> Self {
            Self { events: std::sync::Mutex::new(Vec::new()) }
        }

        fn get_events(&self) -> Vec<String> {
            self.events.lock().unwrap().clone()
        }
    }

    impl DocumentWatcher for MockWatcher {
        fn on_document_opened(&self, document: &Document) {
            self.events.lock().unwrap().push(format!("opened:{}", document.uri));
        }

        fn on_document_changed(
            &self,
            document: &Document,
            _changes: &[TextDocumentContentChangeEvent],
        ) {
            self.events.lock().unwrap().push(format!("changed:{}", document.uri));
        }

        fn on_document_closed(&self, uri: &Url) {
            self.events.lock().unwrap().push(format!("closed:{uri}"));
        }
    }

    #[test]
    fn test_document_watchers() {
        let mut store = DocumentStore::new();
        let uri = Url::parse("file:///test.sol").unwrap();
        let watcher = Arc::new(MockWatcher::new());

        // Add watcher
        store.add_watcher(&uri, watcher.clone());

        // Open document - should trigger watcher
        store.open_document(uri.clone(), 1, "contract Test {}".to_string(), None).unwrap();

        // Update document - should trigger watcher
        let changes = vec![TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "contract Updated {}".to_string(),
        }];
        store.update_document(&uri, 2, changes).unwrap();

        // Close document - should trigger watcher
        store.close_document(&uri).unwrap();

        let events = watcher.get_events();
        assert_eq!(events.len(), 3);
        assert!(events[0].starts_with("opened:"));
        assert!(events[1].starts_with("changed:"));
        assert!(events[2].starts_with("closed:"));
    }
}

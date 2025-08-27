use crate::diagnostics::{Diag, Emitter};
use solar_data_structures::sync::RwLock;
use std::sync::Arc;

/// An in-memory diagnostics emitter.
///
/// Diagnostics are pushed to a shared buffer as-is.
///
/// # Warning
///
/// Do **NOT** hold a read lock on the buffer across compiler passes as this will prevent the
/// compiler from pushing diagnostics.
pub struct InMemoryEmitter {
    buffer: Arc<RwLock<Vec<Diag>>>,
}

impl InMemoryEmitter {
    /// Creates a new emitter, returning the emitter itself and the buffer.
    pub fn new() -> (Self, Arc<RwLock<Vec<Diag>>>) {
        let buffer = Default::default();
        (Self { buffer: Arc::clone(&buffer) }, buffer)
    }
}

impl Emitter for InMemoryEmitter {
    fn emit_diagnostic(&mut self, diagnostic: &crate::diagnostics::Diag) {
        self.buffer.write().push(diagnostic.clone());
    }
}

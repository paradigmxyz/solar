use super::{DiagCtxt, Diagnostic, Level};
use crate::SourceMap;
use std::sync::Arc;

mod human;
pub use human::HumanEmitter;

#[cfg(feature = "json")]
mod json;
#[cfg(feature = "json")]
pub use json::JsonEmitter;

mod rustc;

/// Dynamic diagnostic emitter. See [`Emitter`].
pub type DynEmitter = dyn Emitter + Send;

/// Diagnostic emitter.
pub trait Emitter {
    /// Emits a diagnostic.
    fn emit_diagnostic(&mut self, diagnostic: &Diagnostic);

    /// Returns a reference to the source map, if any.
    #[inline]
    fn source_map(&self) -> Option<&Arc<SourceMap>> {
        None
    }

    /// Returns `true` if we can use colors in the current output stream.
    #[inline]
    fn supports_color(&self) -> bool {
        false
    }
}

/// Diagnostic emitter that only emits fatal diagnostics.
pub struct SilentEmitter {
    fatal_dcx: DiagCtxt,
    note: Option<String>,
}

impl SilentEmitter {
    /// Creates a new `SilentEmitter`. `fatal_dcx` is only used to emit fatal diagnostics.
    pub fn new(fatal_dcx: DiagCtxt) -> Self {
        Self { fatal_dcx, note: None }
    }

    /// Sets the note to be emitted for fatal diagnostics.
    pub fn with_note(mut self, note: Option<String>) -> Self {
        self.note = note;
        self
    }
}

impl Emitter for SilentEmitter {
    fn emit_diagnostic(&mut self, diagnostic: &Diagnostic) {
        if diagnostic.level != Level::Bug {
            return;
        }

        let mut diagnostic = diagnostic.clone();
        if let Some(note) = &self.note {
            diagnostic.note(note.clone());
        }
        let _ = self.fatal_dcx.emit_diagnostic(diagnostic);
    }
}

/// Diagnostic emitter that only stores emitted diagnostics.
#[derive(Clone, Debug)]
pub struct LocalEmitter {
    diagnostics: Vec<Diagnostic>,
}

impl Default for LocalEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalEmitter {
    /// Creates a new `LocalEmitter`.
    pub fn new() -> Self {
        Self { diagnostics: Vec::new() }
    }

    /// Returns a reference to the emitted diagnostics.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Consumes the emitter and returns the emitted diagnostics.
    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}

impl Emitter for LocalEmitter {
    fn emit_diagnostic(&mut self, diagnostic: &Diagnostic) {
        self.diagnostics.push(diagnostic.clone());
    }
}

#[cold]
#[inline(never)]
fn io_panic(error: std::io::Error) -> ! {
    panic!("failed to emit diagnostic: {error}");
}

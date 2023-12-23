use super::{DiagCtxt, Diagnostic, Level};
use crate::SourceMap;
use sulk_data_structures::sync::Lrc;

/// Dynamic diagnostic emitter. See [`Emitter`].
pub type DynEmitter = dyn Emitter + Send;

/// Diagnostic emitter.
pub trait Emitter {
    /// Emits a diagnostic.
    fn emit_diagnostic(&mut self, diagnostic: &Diagnostic);

    /// Returns whether we can use colors in the current output stream.
    fn supports_color(&self) -> bool {
        false
    }

    /// Returns a reference to the source map, if any.
    fn source_map(&self) -> Option<&Lrc<SourceMap>>;
}

/// Diagnostic emitter that only emits fatal diagnostics.
pub struct SilentEmitter {
    fatal_dcx: DiagCtxt,
    note: Option<String>,
}

impl SilentEmitter {
    /// Creates a new `SilentEmitter`.
    pub fn new(dcx: DiagCtxt) -> Self {
        Self { fatal_dcx: dcx, note: None }
    }

    /// Sets the note to be emitted for fatal diagnostics.
    pub fn with_note(mut self, note: String) -> Self {
        self.note = Some(note);
        self
    }
}

impl Emitter for SilentEmitter {
    fn emit_diagnostic(&mut self, diagnostic: &Diagnostic) {
        if diagnostic.level != Level::Fatal {
            return;
        }

        let mut diagnostic = diagnostic.clone();
        if let Some(note) = &self.note {
            diagnostic.note(note.clone());
        }
        self.fatal_dcx.emit_diagnostic(diagnostic);
    }

    fn source_map(&self) -> Option<&Lrc<SourceMap>> {
        None
    }
}

/// Diagnostic emitter that only stores emitted diagnostics.
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
        Self { diagnostics: vec![] }
    }

    /// Returns the emitted diagnostics.
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

    fn source_map(&self) -> Option<&Lrc<SourceMap>> {
        None
    }
}
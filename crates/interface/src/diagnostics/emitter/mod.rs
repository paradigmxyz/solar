use super::{Diag, Level};
use crate::SourceMap;
use std::{any::Any, sync::Arc};

mod human;
pub use human::{HumanBufferEmitter, HumanEmitter};

#[cfg(feature = "json")]
mod json;
#[cfg(feature = "json")]
pub use json::JsonEmitter;

mod rustc;

/// Dynamic diagnostic emitter. See [`Emitter`].
pub type DynEmitter = dyn Emitter + Send;

/// Diagnostic emitter.
pub trait Emitter: Any {
    /// Emits a diagnostic.
    fn emit_diagnostic(&mut self, diagnostic: &Diag);

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

impl DynEmitter {
    pub(crate) fn local_buffer(&self) -> Option<&str> {
        (self as &dyn Any).downcast_ref::<HumanBufferEmitter>().map(HumanBufferEmitter::buffer)
    }
}

/// Diagnostic emitter.
///
/// Emits fatal diagnostics by default, with `note` if set.
pub struct SilentEmitter {
    fatal_emitter: Option<Box<DynEmitter>>,
    note: Option<String>,
}

impl SilentEmitter {
    /// Creates a new `SilentEmitter`. Emits fatal diagnostics with `fatal_emitter`.
    pub fn new(fatal_emitter: impl Emitter + Send) -> Self {
        Self::new_boxed(Some(Box::new(fatal_emitter)))
    }

    /// Creates a new `SilentEmitter`. Emits fatal diagnostics with `fatal_emitter` if `Some`.
    pub fn new_boxed(fatal_emitter: Option<Box<DynEmitter>>) -> Self {
        Self { fatal_emitter, note: None }
    }

    /// Creates a new `SilentEmitter` that does not emit any diagnostics at all.
    ///
    /// Same as `new_boxed(None)`.
    pub fn new_silent() -> Self {
        Self::new_boxed(None)
    }

    /// Sets the note to be emitted for fatal diagnostics.
    pub fn with_note(mut self, note: Option<String>) -> Self {
        self.note = note;
        self
    }
}

impl Emitter for SilentEmitter {
    fn emit_diagnostic(&mut self, diagnostic: &Diag) {
        let Some(fatal_emitter) = self.fatal_emitter.as_deref_mut() else { return };
        if diagnostic.level != Level::Fatal {
            return;
        }

        if let Some(note) = &self.note {
            let mut diagnostic = diagnostic.clone();
            diagnostic.note(note.clone());
            fatal_emitter.emit_diagnostic(&diagnostic);
        } else {
            fatal_emitter.emit_diagnostic(diagnostic);
        }
    }
}

/// Diagnostic emitter that only stores emitted diagnostics.
#[derive(Clone, Debug)]
pub struct LocalEmitter {
    diagnostics: Vec<Diag>,
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
    pub fn diagnostics(&self) -> &[Diag] {
        &self.diagnostics
    }

    /// Consumes the emitter and returns the emitted diagnostics.
    pub fn into_diagnostics(self) -> Vec<Diag> {
        self.diagnostics
    }
}

impl Emitter for LocalEmitter {
    fn emit_diagnostic(&mut self, diagnostic: &Diag) {
        self.diagnostics.push(diagnostic.clone());
    }
}

#[cold]
#[inline(never)]
fn io_panic(error: std::io::Error) -> ! {
    panic!("failed to emit diagnostic: {error}");
}

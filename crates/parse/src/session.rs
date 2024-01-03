use sulk_data_structures::sync::Lrc;
use sulk_interface::{diagnostics::DiagCtxt, SourceMap};

/// Information about the current parsing session.
pub struct ParseSess {
    /// The diagnostics context.
    pub dcx: DiagCtxt,
    /// The source map.
    source_map: Lrc<SourceMap>,
}

impl ParseSess {
    /// Creates a new parser session with the given diagnostics context and source map.
    pub fn new(dcx: DiagCtxt, source_map: Lrc<SourceMap>) -> Self {
        Self { dcx, source_map }
    }

    /// Creates a new parser session with an empty source map.
    pub fn empty(dcx: DiagCtxt) -> Self {
        Self::new(dcx, Lrc::new(SourceMap::empty()))
    }

    /// Creates a new parser session with a test emitter.
    pub fn with_test_emitter(ui: bool) -> Self {
        Self::empty(DiagCtxt::with_test_emitter(ui))
    }

    /// Creates a new parser session with a TTY emitter.
    pub fn with_tty_emitter(source_map: Lrc<SourceMap>) -> Self {
        let dcx = DiagCtxt::with_tty_emitter(Some(source_map.clone()));
        Self::new(dcx, source_map)
    }

    /// Creates a new parser session with a silent emitter.
    pub fn with_silent_emitter(fatal_note: Option<String>) -> Self {
        let dcx = DiagCtxt::with_silent_emitter(fatal_note);
        let source_map = Lrc::new(SourceMap::empty());
        Self::new(dcx, source_map)
    }

    /// Returns a reference to the source map.
    #[inline]
    pub fn source_map(&self) -> &SourceMap {
        &self.source_map
    }
}

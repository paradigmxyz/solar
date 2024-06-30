use crate::{diagnostics::DiagCtxt, ColorChoice, SourceMap};
use std::num::NonZeroUsize;
use sulk_config::{EvmVersion, Language, StopAfter};
use sulk_data_structures::sync::Lrc;

/// Information about the current compiler session.
pub struct Session {
    /// The diagnostics context.
    pub dcx: DiagCtxt,
    /// The source map.
    source_map: Lrc<SourceMap>,

    /// EVM version.
    pub evm_version: EvmVersion,
    /// Source code language.
    pub language: Language,
    /// Stop execution after the given compiler stage.
    pub stop_after: Option<StopAfter>,
    /// Number of threads to use. Already resolved to a non-zero value.
    pub jobs: NonZeroUsize,
}

impl Session {
    /// Creates a new parser session with the given diagnostics context and source map.
    pub fn new(dcx: DiagCtxt, source_map: Lrc<SourceMap>) -> Self {
        Self {
            dcx,
            source_map,
            evm_version: EvmVersion::default(),
            language: Language::default(),
            stop_after: None,
            jobs: NonZeroUsize::MIN,
        }
    }

    /// Creates a new parser session with an empty source map.
    pub fn empty(dcx: DiagCtxt) -> Self {
        Self::new(dcx, Lrc::new(SourceMap::empty()))
    }

    /// Creates a new parser session with a test emitter.
    pub fn with_test_emitter() -> Self {
        Self::empty(DiagCtxt::with_test_emitter())
    }

    /// Creates a new parser session with a TTY emitter.
    pub fn with_tty_emitter(source_map: Lrc<SourceMap>) -> Self {
        Self::with_tty_emitter_and_color(source_map, ColorChoice::Auto)
    }

    /// Creates a new parser session with a TTY emitter and a color choice.
    pub fn with_tty_emitter_and_color(
        source_map: Lrc<SourceMap>,
        color_choice: ColorChoice,
    ) -> Self {
        let dcx = DiagCtxt::with_tty_emitter_and_color(Some(source_map.clone()), color_choice);
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

    /// Clones the source map.
    pub fn clone_source_map(&self) -> Lrc<SourceMap> {
        self.source_map.clone()
    }
}

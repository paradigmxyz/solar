use crate::{diagnostics::DiagCtxt, ColorChoice, SourceMap};
use std::{num::NonZeroUsize, sync::Arc};
use sulk_config::{CompilerOutput, CompilerStage, EvmVersion, Language};

/// Information about the current compiler session.
pub struct Session {
    /// The diagnostics context.
    pub dcx: DiagCtxt,
    /// The source map.
    source_map: Arc<SourceMap>,

    /// EVM version.
    pub evm_version: EvmVersion,
    /// Source code language.
    pub language: Language,
    /// Stop execution after the given compiler stage.
    pub stop_after: Option<CompilerStage>,
    /// Types of output to emit.
    pub emit: Vec<CompilerOutput>,
    /// Number of threads to use. Already resolved to a non-zero value.
    pub jobs: NonZeroUsize,
}

impl Session {
    /// Creates a new parser session with the given diagnostics context and source map.
    pub fn new(dcx: DiagCtxt, source_map: Arc<SourceMap>) -> Self {
        Self {
            dcx,
            source_map,
            evm_version: EvmVersion::default(),
            language: Language::default(),
            stop_after: None,
            emit: Vec::new(),
            jobs: NonZeroUsize::MIN,
        }
    }

    /// Creates a new parser session with an empty source map.
    pub fn empty(dcx: DiagCtxt) -> Self {
        Self::new(dcx, Arc::new(SourceMap::empty()))
    }

    /// Creates a new parser session with a test emitter.
    pub fn with_test_emitter() -> Self {
        Self::empty(DiagCtxt::with_test_emitter())
    }

    /// Creates a new parser session with a TTY emitter.
    pub fn with_tty_emitter(source_map: Arc<SourceMap>) -> Self {
        Self::with_tty_emitter_and_color(source_map, ColorChoice::Auto)
    }

    /// Creates a new parser session with a TTY emitter and a color choice.
    pub fn with_tty_emitter_and_color(
        source_map: Arc<SourceMap>,
        color_choice: ColorChoice,
    ) -> Self {
        let dcx = DiagCtxt::with_tty_emitter_and_color(Some(source_map.clone()), color_choice);
        Self::new(dcx, source_map)
    }

    /// Creates a new parser session with a silent emitter.
    pub fn with_silent_emitter(fatal_note: Option<String>) -> Self {
        let dcx = DiagCtxt::with_silent_emitter(fatal_note);
        let source_map = Arc::new(SourceMap::empty());
        Self::new(dcx, source_map)
    }

    /// Returns a reference to the source map.
    #[inline]
    pub fn source_map(&self) -> &SourceMap {
        &self.source_map
    }

    /// Clones the source map.
    #[inline]
    pub fn clone_source_map(&self) -> Arc<SourceMap> {
        self.source_map.clone()
    }

    /// Returns `true` if compilation should stop after the given stage.
    #[inline]
    pub fn stop_after(&self, stage: CompilerStage) -> bool {
        self.stop_after >= Some(stage)
    }

    /// Returns `true` if parallelism is not enabled.
    #[inline]
    pub fn is_sequential(&self) -> bool {
        self.jobs.get() == 1
    }

    /// Returns `true` if the given output should be emitted.
    pub fn do_emit(&self, output: CompilerOutput) -> bool {
        self.emit.contains(&output)
    }

    /// Spawns the given closure on the thread pool or executes it immediately if parallelism is not
    /// enabled.
    // NOTE: This only exists because on a `use_current_thread` thread pool `rayon::spawn` will
    // never execute.
    #[inline]
    pub fn spawn(&self, f: impl FnOnce() + Send + 'static) {
        if self.is_sequential() {
            f();
        } else {
            rayon::spawn(f);
        }
    }
}

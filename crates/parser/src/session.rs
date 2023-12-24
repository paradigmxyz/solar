use sulk_interface::{diagnostics::DiagCtxt, SourceMap};

/// Information about the current parsing session.
pub struct ParseSess {
    pub dcx: DiagCtxt,
    _source_map: SourceMap,
}

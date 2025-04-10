//! SourceMap related types and operations.

use crate::{BytePos, CharPos, Span};
use solar_data_structures::{
    map::FxBuildHasher,
    sync::{ReadGuard, RwLock},
};
use std::{
    io::{self, Read},
    path::Path,
    sync::Arc,
};

mod analyze;

mod file;
pub use file::*;

mod file_resolver;
pub use file_resolver::{FileResolver, ResolveError};

#[cfg(test)]
mod tests;

pub type FileLinesResult = Result<FileLines, SpanLinesError>;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SpanLinesError {
    DistinctSources(Box<DistinctSources>),
}

/// An error that can occur when converting a `Span` to a snippet.
///
/// In general these errors only occur on malformed spans created by the user.
/// The parser never creates a span that would cause these errors.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SpanSnippetError {
    IllFormedSpan(Span),
    DistinctSources(Box<DistinctSources>),
    MalformedForSourcemap(MalformedSourceMapPositions),
    SourceNotAvailable { filename: FileName },
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DistinctSources {
    pub begin: (FileName, BytePos),
    pub end: (FileName, BytePos),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MalformedSourceMapPositions {
    pub name: FileName,
    pub source_len: usize,
    pub begin_pos: BytePos,
    pub end_pos: BytePos,
}

/// A source code location used for error reporting.
#[derive(Clone, Debug)]
pub struct Loc {
    /// Information about the original source.
    pub file: Arc<SourceFile>,
    /// The (1-based) line number.
    pub line: usize,
    /// The (0-based) column offset.
    pub col: CharPos,
    /// The (0-based) column offset when displayed.
    pub col_display: usize,
}

// Used to be structural records.
#[derive(Debug)]
pub struct SourceFileAndLine {
    pub sf: Arc<SourceFile>,
    /// Index of line, starting from 0.
    pub line: usize,
}

#[derive(Debug)]
pub struct SourceFileAndBytePos {
    pub sf: Arc<SourceFile>,
    pub pos: BytePos,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct LineInfo {
    /// Index of line, starting from 0.
    pub line_index: usize,

    /// Column in line where span begins, starting from 0.
    pub start_col: CharPos,

    /// Column in line where span ends, starting from 0, exclusive.
    pub end_col: CharPos,
}

pub struct FileLines {
    pub file: Arc<SourceFile>,
    pub lines: Vec<LineInfo>,
}

/// Stores all the sources of the current compilation session.
#[derive(derive_more::Debug)]
pub struct SourceMap {
    // INVARIANT: The only operation allowed on `source_files` is `push`.
    source_files: RwLock<Vec<Arc<SourceFile>>>,
    #[debug(skip)]
    stable_id_to_source_file: scc::HashIndex<StableSourceFileId, Arc<SourceFile>, FxBuildHasher>,
    hash_kind: SourceFileHashAlgorithm,
}

impl Default for SourceMap {
    fn default() -> Self {
        Self::empty()
    }
}

impl SourceMap {
    /// Creates a new empty source map with the given hash algorithm.
    pub fn new(hash_kind: SourceFileHashAlgorithm) -> Self {
        Self {
            source_files: RwLock::new(Vec::new()),
            stable_id_to_source_file: Default::default(),
            hash_kind,
        }
    }

    /// Creates a new empty source map.
    pub fn empty() -> Self {
        Self::new(SourceFileHashAlgorithm::default())
    }

    /// Returns the source file with the given path, if it exists.
    /// Does not attempt to load the file.
    pub fn get_file(&self, path: &Path) -> Option<Arc<SourceFile>> {
        self.get_file_by_name(&path.to_path_buf().into())
    }

    /// Returns the source file with the given name, if it exists.
    /// Does not attempt to load the file.
    pub fn get_file_by_name(&self, name: &FileName) -> Option<Arc<SourceFile>> {
        let stable_id = StableSourceFileId::from_filename_in_current_crate(name);
        self.stable_id_to_source_file.get(&stable_id).map(|entry| entry.get().clone())
    }

    /// Loads a file from the given path.
    pub fn load_file(&self, path: &Path) -> io::Result<Arc<SourceFile>> {
        self.load_file_with_name(path.to_owned().into(), path)
    }

    /// Loads a file with the given name from the given path.
    pub fn load_file_with_name(&self, name: FileName, path: &Path) -> io::Result<Arc<SourceFile>> {
        self.new_source_file_with(name, || std::fs::read_to_string(path))
    }

    /// Loads `stdin`.
    pub fn load_stdin(&self) -> io::Result<Arc<SourceFile>> {
        self.new_source_file_with(FileName::Stdin, || {
            let mut src = String::new();
            io::stdin().read_to_string(&mut src)?;
            Ok(src)
        })
    }

    /// Creates a new `SourceFile` with the given name and source string.
    ///
    /// See [`new_source_file_with`](Self::new_source_file_with) for more details.
    pub fn new_source_file(
        &self,
        name: impl Into<FileName>,
        src: impl Into<String>,
    ) -> io::Result<Arc<SourceFile>> {
        self.new_source_file_with(name.into(), || Ok(src.into()))
    }

    /// Creates a new `SourceFile` with the given name and source string closure.
    ///
    /// If a file already exists in the `SourceMap` with the same ID, that file is returned
    /// unmodified, and `get_src` is not called.
    ///
    /// Returns an error if the file is larger than 4GiB or other errors occur while creating the
    /// `SourceFile`.
    #[instrument(level = "debug", skip_all, fields(filename = %filename.display()))]
    pub fn new_source_file_with(
        &self,
        filename: FileName,
        get_src: impl FnOnce() -> io::Result<String>,
    ) -> io::Result<Arc<SourceFile>> {
        let stable_id = StableSourceFileId::from_filename_in_current_crate(&filename);
        match self.stable_id_to_source_file.entry(stable_id) {
            scc::hash_index::Entry::Occupied(entry) => Ok(entry.get().clone()),
            scc::hash_index::Entry::Vacant(entry) => {
                let file = SourceFile::new(filename, get_src()?, self.hash_kind)?;
                let file = self.new_source_file_inner(file, stable_id)?;
                entry.insert_entry(file.clone());
                Ok(file)
            }
        }
    }

    fn new_source_file_inner(
        &self,
        mut file: SourceFile,
        stable_id: StableSourceFileId,
    ) -> io::Result<Arc<SourceFile>> {
        // Let's make sure the file_id we generated above actually matches
        // the ID we generate for the SourceFile we just created.
        debug_assert_eq!(file.stable_id, stable_id);

        trace!(name=%file.name.display(), len=file.src.len(), loc=file.count_lines(), "adding to source map");

        let mut source_files = self.source_files.write();

        file.start_pos = BytePos(if let Some(last_file) = source_files.last() {
            // Add one so there is some space between files. This lets us distinguish
            // positions in the `SourceMap`, even in the presence of zero-length files.
            last_file.end_position().0.checked_add(1).ok_or(OffsetOverflowError(()))?
        } else {
            0
        });

        let file = Arc::new(file);
        source_files.push(file.clone());

        Ok(file)
    }

    pub fn files(&self) -> ReadGuard<'_, Vec<Arc<SourceFile>>> {
        self.source_files.read()
    }

    pub fn source_file_by_file_name(&self, filename: &FileName) -> Option<Arc<SourceFile>> {
        let stable_id = StableSourceFileId::from_filename_in_current_crate(filename);
        self.source_file_by_stable_id(stable_id)
    }

    pub fn source_file_by_stable_id(
        &self,
        stable_id: StableSourceFileId,
    ) -> Option<Arc<SourceFile>> {
        self.stable_id_to_source_file.get(&stable_id).as_deref().cloned()
    }

    pub fn filename_for_diagnostics<'a>(&self, filename: &'a FileName) -> FileNameDisplay<'a> {
        filename.display()
    }

    /// Returns `true` if the given span is multi-line.
    pub fn is_multiline(&self, span: Span) -> bool {
        let lo = self.lookup_source_file_idx(span.lo());
        let hi = self.lookup_source_file_idx(span.hi());
        if lo != hi {
            return true;
        }
        let f = self.files()[lo].clone();
        let lo = f.relative_position(span.lo());
        let hi = f.relative_position(span.hi());
        f.lookup_line(lo) != f.lookup_line(hi)
    }

    /// Returns the source snippet as `String` corresponding to the given `Span`.
    pub fn span_to_snippet(&self, span: Span) -> Result<String, SpanSnippetError> {
        let (sf, range) = self.span_to_source(span)?;
        sf.src.get(range).map(|s| s.to_string()).ok_or(SpanSnippetError::IllFormedSpan(span))
    }

    /// Returns the source snippet as `String` before the given `Span`.
    pub fn span_to_prev_source(&self, sp: Span) -> Result<String, SpanSnippetError> {
        let (sf, range) = self.span_to_source(sp)?;
        sf.src.get(..range.start).map(|s| s.to_string()).ok_or(SpanSnippetError::IllFormedSpan(sp))
    }

    /// For a global `BytePos`, computes the local offset within the containing `SourceFile`.
    pub fn lookup_byte_offset(&self, bpos: BytePos) -> SourceFileAndBytePos {
        let idx = self.lookup_source_file_idx(bpos);
        let sf = self.files()[idx].clone();
        let offset = bpos - sf.start_pos;
        SourceFileAndBytePos { sf, pos: offset }
    }

    /// Returns the index of the [`SourceFile`] (in `self.files`) that contains `pos`.
    ///
    /// This index is guaranteed to be valid for the lifetime of this `SourceMap`.
    pub fn lookup_source_file_idx(&self, pos: BytePos) -> usize {
        assert!(!self.files().is_empty(), "attempted to lookup source file in empty `SourceMap`");
        self.files().partition_point(|x| x.start_pos <= pos) - 1
    }

    /// Return the SourceFile that contains the given `BytePos`.
    pub fn lookup_source_file(&self, pos: BytePos) -> Arc<SourceFile> {
        let idx = self.lookup_source_file_idx(pos);
        self.files()[idx].clone()
    }

    /// Looks up source information about a `BytePos`.
    pub fn lookup_char_pos(&self, pos: BytePos) -> Loc {
        let sf = self.lookup_source_file(pos);
        let (line, col, col_display) = sf.lookup_file_pos_with_col_display(pos);
        Loc { file: sf, line, col, col_display }
    }

    /// If the corresponding `SourceFile` is empty, does not return a line number.
    pub fn lookup_line(&self, pos: BytePos) -> Result<SourceFileAndLine, Arc<SourceFile>> {
        let f = self.lookup_source_file(pos);
        let pos = f.relative_position(pos);
        match f.lookup_line(pos) {
            Some(line) => Ok(SourceFileAndLine { sf: f, line }),
            None => Err(f),
        }
    }

    pub fn is_valid_span(&self, sp: Span) -> Result<(Loc, Loc), SpanLinesError> {
        let lo = self.lookup_char_pos(sp.lo());
        let hi = self.lookup_char_pos(sp.hi());
        if lo.file.start_pos != hi.file.start_pos {
            return Err(SpanLinesError::DistinctSources(Box::new(DistinctSources {
                begin: (lo.file.name.clone(), lo.file.start_pos),
                end: (hi.file.name.clone(), hi.file.start_pos),
            })));
        }
        Ok((lo, hi))
    }

    pub fn is_line_before_span_empty(&self, sp: Span) -> bool {
        match self.span_to_prev_source(sp) {
            Ok(s) => s.rsplit_once('\n').unwrap_or(("", &s)).1.trim_start().is_empty(),
            Err(_) => false,
        }
    }

    pub fn span_to_lines(&self, sp: Span) -> FileLinesResult {
        let (lo, hi) = self.is_valid_span(sp)?;
        assert!(hi.line >= lo.line);

        if sp.is_dummy() {
            return Ok(FileLines { file: lo.file, lines: Vec::new() });
        }

        let mut lines = Vec::with_capacity(hi.line - lo.line + 1);

        // The span starts partway through the first line,
        // but after that it starts from offset 0.
        let mut start_col = lo.col;

        // For every line but the last, it extends from `start_col`
        // and to the end of the line. Be careful because the line
        // numbers in Loc are 1-based, so we subtract 1 to get 0-based
        // lines.
        //
        // FIXME: now that we handle DUMMY_SP up above, we should consider
        // asserting that the line numbers here are all indeed 1-based.
        let hi_line = hi.line.saturating_sub(1);
        for line_index in lo.line.saturating_sub(1)..hi_line {
            let line_len = lo.file.get_line(line_index).map_or(0, |s| s.chars().count());
            lines.push(LineInfo { line_index, start_col, end_col: CharPos::from_usize(line_len) });
            start_col = CharPos::from_usize(0);
        }

        // For the last line, it extends from `start_col` to `hi.col`:
        lines.push(LineInfo { line_index: hi_line, start_col, end_col: hi.col });

        Ok(FileLines { file: lo.file, lines })
    }

    /// Returns the source file and the range of text corresponding to the given span.
    pub fn span_to_source(
        &self,
        sp: Span,
    ) -> Result<(Arc<SourceFile>, std::ops::Range<usize>), SpanSnippetError> {
        let local_begin = self.lookup_byte_offset(sp.lo());
        let local_end = self.lookup_byte_offset(sp.hi());

        if local_begin.sf.start_pos != local_end.sf.start_pos {
            return Err(SpanSnippetError::DistinctSources(Box::new(DistinctSources {
                begin: (local_begin.sf.name.clone(), local_begin.sf.start_pos),
                end: (local_end.sf.name.clone(), local_end.sf.start_pos),
            })));
        }

        // self.ensure_source_file_source_present(&local_begin.sf);

        let start_index = local_begin.pos.to_usize();
        let end_index = local_end.pos.to_usize();
        let source_len = local_begin.sf.source_len.to_usize();

        if start_index > end_index || end_index > source_len {
            return Err(SpanSnippetError::MalformedForSourcemap(MalformedSourceMapPositions {
                name: local_begin.sf.name.clone(),
                source_len,
                begin_pos: local_begin.pos,
                end_pos: local_end.pos,
            }));
        }

        Ok((local_begin.sf, start_index..end_index))
    }

    /// Format the span location to be printed in diagnostics. Must not be emitted
    /// to build artifacts as this may leak local file paths. Use span_to_embeddable_string
    /// for string suitable for embedding.
    pub fn span_to_diagnostic_string(&self, sp: Span) -> String {
        self.span_to_string(sp)
    }

    pub fn span_to_string(&self, sp: Span) -> String {
        let (source_file, lo_line, lo_col, hi_line, hi_col) = self.span_to_location_info(sp);

        let file_name = match source_file {
            Some(sf) => sf.name.display().to_string(),
            None => return "no-location".to_string(),
        };

        format!("{file_name}:{lo_line}:{lo_col}: {hi_line}:{hi_col}")
    }

    pub fn span_to_location_info(
        &self,
        sp: Span,
    ) -> (Option<Arc<SourceFile>>, usize, usize, usize, usize) {
        if self.files().is_empty() || sp.is_dummy() {
            return (None, 0, 0, 0, 0);
        }

        let lo = self.lookup_char_pos(sp.lo());
        let hi = self.lookup_char_pos(sp.hi());
        (Some(lo.file), lo.line, lo.col.to_usize() + 1, hi.line, hi.col.to_usize() + 1)
    }
}

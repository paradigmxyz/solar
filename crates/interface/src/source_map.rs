//! SourceMap related types and operations.

#![allow(dead_code)]

use crate::{pos::RelativeBytePos, BytePos, CharPos, Pos, Span};
use std::{
    io,
    path::{Path, PathBuf},
};
use sulk_data_structures::{
    map::FxHashMap,
    sync::{Lrc, RwLock},
};

/// An abstraction over the fs operations used by the Parser.
pub trait FileLoader {
    /// Query the existence of a file.
    fn file_exists(&self, path: &Path) -> bool;

    /// Read the contents of a UTF-8 file into memory.
    /// This function must return a String because we normalize
    /// source files, which may require resizing.
    fn read_file(&self, path: &Path) -> io::Result<String>;
}

/// A FileLoader that uses std::fs to load real files.
pub struct RealFileLoader;

impl FileLoader for RealFileLoader {
    fn file_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn read_file(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }
}

/// A single source in the [`SourceMap`].
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// The name of the file that the source came from. Source that doesn't
    /// originate from files has names between angle brackets by convention
    /// (e.g., `<anon>`).
    pub name: FileName,
    /// The complete source code.
    pub src: Option<Lrc<String>>,
    // /// The source code's hash.
    // pub src_hash: SourceFileHash,
    /// The start position of this source in the `SourceMap`.
    pub start_pos: BytePos,
    /// The byte length of this source.
    pub source_len: RelativeBytePos,
    /// Locations of lines beginnings in the source code.
    pub lines: Vec<RelativeBytePos>,
    /*
    /// Locations of multi-byte characters in the source code.
    pub multibyte_chars: Vec<MultiByteChar>,
    /// Width of characters that are not narrow in the source code.
    pub non_narrow_chars: Vec<NonNarrowChar>,
    /// Locations of characters removed during normalization.
    pub normalized_pos: Vec<NormalizedPos>,
    /// A hash of the filename & crate-id, used for uniquely identifying source
    /// files within the crate graph and for speeding up hashing in incremental
    /// compilation.
    pub stable_id: StableSourceFileId,
    /// Indicates which crate this `SourceFile` was imported from.
    pub cnum: CrateNum,
    */
}

/*
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceFileHashAlgorithm {
    Md5,
    Sha1,
    Sha256,
}

/// The hash of the on-disk source file used for debug info.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub struct SourceFileHash {
    pub kind: SourceFileHashAlgorithm,
    value: [u8; 32],
}

impl SourceFileHash {
    pub fn new(kind: SourceFileHashAlgorithm, src: &str) -> SourceFileHash {
        let mut hash = SourceFileHash { kind, value: Default::default() };
        let len = hash.hash_len();
        let _value = &mut hash.value[..len];
        let _data = src.as_bytes();
        // TODO
        match kind {
            SourceFileHashAlgorithm::Md5 => {
                // value.copy_from_slice(&Md5::digest(data));
            }
            SourceFileHashAlgorithm::Sha1 => {
                // value.copy_from_slice(&Sha1::digest(data));
            }
            SourceFileHashAlgorithm::Sha256 => {
                // value.copy_from_slice(&Sha256::digest(data));
            }
        }
        hash
    }

    /// Check if the stored hash matches the hash of the string.
    pub fn matches(&self, src: &str) -> bool {
        Self::new(self.kind, src) == *self
    }

    /// The bytes of the hash.
    pub fn hash_bytes(&self) -> &[u8] {
        let len = self.hash_len();
        &self.value[..len]
    }

    fn hash_len(&self) -> usize {
        match self.kind {
            SourceFileHashAlgorithm::Md5 => 16,
            SourceFileHashAlgorithm::Sha1 => 20,
            SourceFileHashAlgorithm::Sha256 => 32,
        }
    }
}
*/

#[derive(Debug)]
pub enum SpanSnippetError {
    IllFormedSpan(Span),
    DistinctSources(Box<DistinctSources>),
    MalformedForSourcemap(MalformedSourceMapPositions),
    SourceNotAvailable { filename: FileName },
}

#[derive(Debug)]
pub struct DistinctSources {
    pub begin: (FileName, BytePos),
    pub end: (FileName, BytePos),
}

#[derive(Debug)]
pub struct MalformedSourceMapPositions {
    pub name: FileName,
    pub source_len: usize,
    pub begin_pos: BytePos,
    pub end_pos: BytePos,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FileName {
    Real(PathBuf),
}

/// A source code location used for error reporting.
#[derive(Debug, Clone)]
pub struct Loc {
    /// Information about the original source.
    pub file: Lrc<SourceFile>,
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
    pub sf: Lrc<SourceFile>,
    /// Index of line, starting from 0.
    pub line: usize,
}

#[derive(Debug)]
pub struct SourceFileAndBytePos {
    pub sf: Lrc<SourceFile>,
    pub pos: BytePos,
}

pub struct StableSourceFileId(pub sulk_data_structures::index::BaseIndex32);

#[derive(Default)]
struct SourceMapFiles {
    source_files: Vec<Lrc<SourceFile>>,
    stable_id_to_source_file: FxHashMap<StableSourceFileId, Lrc<SourceFile>>,
}

/// TODO
pub struct SourceMap {
    files: RwLock<SourceMapFiles>,
    file_loader: Box<dyn FileLoader + Sync + Send>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self { files: Default::default(), file_loader: Box::new(RealFileLoader) }
    }

    /// Creates a new empty source map.
    pub fn empty() -> Self {
        Self::new()
    }

    /// Returns `true` if the given span is multi-line.
    pub fn is_multiline(&self, span: Span) -> bool {
        // TODO
        let _ = span;
        false
    }

    /// Returns the source snippet as `String` corresponding to the given `Span`.
    pub fn span_to_snippet(&self, span: Span) -> Result<String, SpanSnippetError> {
        self.span_to_source(span, |src, start_index, end_index| {
            src.get(start_index..end_index)
                .map(|s| s.to_string())
                .ok_or(SpanSnippetError::IllFormedSpan(span))
        })
    }

    /// For a global `BytePos`, computes the local offset within the containing `SourceFile`.
    pub fn lookup_byte_offset(&self, bpos: BytePos) -> SourceFileAndBytePos {
        let idx = self.lookup_source_file_idx(bpos);
        let sf = (*self.files.borrow().source_files)[idx].clone();
        let offset = bpos - sf.start_pos;
        SourceFileAndBytePos { sf, pos: offset }
    }

    /// Returns the index of the [`SourceFile`] (in `self.files`) that contains `pos`.
    /// This index is guaranteed to be valid for the lifetime of this `SourceMap`,
    /// since `source_files` is a `MonotonicVec`
    pub fn lookup_source_file_idx(&self, pos: BytePos) -> usize {
        self.files.borrow().source_files.partition_point(|x| x.start_pos <= pos) - 1
    }

    /// Extracts the source surrounding the given `Span` using the `extract_source` function. The
    /// extract function takes three arguments: a string slice containing the source, an index in
    /// the slice for the beginning of the span and an index in the slice for the end of the span.
    fn span_to_source<F, T>(&self, sp: Span, extract_source: F) -> Result<T, SpanSnippetError>
    where
        F: Fn(&str, usize, usize) -> Result<T, SpanSnippetError>,
    {
        let local_begin = self.lookup_byte_offset(sp.lo());
        let local_end = self.lookup_byte_offset(sp.hi());

        if local_begin.sf.start_pos != local_end.sf.start_pos {
            Err(SpanSnippetError::DistinctSources(Box::new(DistinctSources {
                begin: (local_begin.sf.name.clone(), local_begin.sf.start_pos),
                end: (local_end.sf.name.clone(), local_end.sf.start_pos),
            })))
        } else {
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

            if let Some(ref src) = local_begin.sf.src {
                extract_source(src, start_index, end_index)
            // } else if let Some(src) = local_begin.sf.external_src.read().get_source() {
            //     extract_source(src, start_index, end_index)
            } else {
                Err(SpanSnippetError::SourceNotAvailable { filename: local_begin.sf.name.clone() })
            }
        }
    }
}

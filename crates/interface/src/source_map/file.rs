use crate::{BytePos, CharPos, pos::RelativeBytePos};
use std::{
    fmt, io,
    ops::RangeInclusive,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Identifies an offset of a multi-byte character in a `SourceFile`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MultiByteChar {
    /// The relative offset of the character in the `SourceFile`.
    pub pos: RelativeBytePos,
    /// The number of bytes, `>= 2`.
    pub bytes: u8,
}

/// The name of a source file.
///
/// This is used as the key in the source map. See
/// [`SourceMap::get_file`](crate::SourceMap::get_file).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileName {
    /// Files from the file system.
    Real(PathBuf),
    /// Command line.
    Stdin,
    /// Custom sources for explicit parser calls from plugins and drivers.
    Custom(String),
}

impl PartialEq<Path> for FileName {
    fn eq(&self, other: &Path) -> bool {
        match self {
            Self::Real(p) => p == other,
            _ => false,
        }
    }
}

impl PartialEq<&Path> for FileName {
    fn eq(&self, other: &&Path) -> bool {
        match self {
            Self::Real(p) => p == *other,
            _ => false,
        }
    }
}

impl PartialEq<PathBuf> for FileName {
    fn eq(&self, other: &PathBuf) -> bool {
        match self {
            Self::Real(p) => p == other,
            _ => false,
        }
    }
}

impl From<PathBuf> for FileName {
    fn from(p: PathBuf) -> Self {
        Self::Real(p)
    }
}

impl From<&PathBuf> for FileName {
    fn from(p: &PathBuf) -> Self {
        Self::Real(p.clone())
    }
}

impl From<&Path> for FileName {
    fn from(p: &Path) -> Self {
        Self::Real(p.to_path_buf())
    }
}

impl From<String> for FileName {
    fn from(s: String) -> Self {
        Self::Custom(s)
    }
}

impl From<&Self> for FileName {
    fn from(s: &Self) -> Self {
        s.clone()
    }
}

impl FileName {
    /// Creates a new `FileName` from a path.
    pub fn real(path: impl Into<PathBuf>) -> Self {
        Self::Real(path.into())
    }

    /// Creates a new `FileName` from a string.
    pub fn custom(s: impl Into<String>) -> Self {
        Self::Custom(s.into())
    }

    /// Displays the filename.
    #[inline]
    pub fn display(&self) -> FileNameDisplay<'_> {
        let base_path = crate::SessionGlobals::try_with(|g| {
            g.and_then(|g| g.source_map.base_path.get().cloned())
        });
        FileNameDisplay { inner: self, base_path }
    }

    /// Returns the path if the file name is a real file.
    #[inline]
    pub fn as_real(&self) -> Option<&Path> {
        match self {
            Self::Real(path) => Some(path),
            _ => None,
        }
    }
}

/// A display wrapper for `FileName`.
///
/// Created by [`FileName::display`].
pub struct FileNameDisplay<'a> {
    pub(crate) inner: &'a FileName,
    pub(crate) base_path: Option<PathBuf>,
}

impl fmt::Display for FileNameDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.inner {
            FileName::Real(path) => {
                let path = if let Some(base_path) = &self.base_path
                    && let Ok(rpath) = path.strip_prefix(base_path)
                {
                    rpath
                } else {
                    path.as_path()
                };
                path.display().fmt(f)
            }
            FileName::Stdin => f.write_str("<stdin>"),
            FileName::Custom(s) => write!(f, "<{s}>"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SourceFileId(u64);

impl SourceFileId {
    pub(crate) fn new(filename: &FileName) -> Self {
        use std::hash::{Hash, Hasher};
        let mut hasher = solar_data_structures::map::FxHasher::with_seed(0);
        filename.hash(&mut hasher);
        Self(hasher.finish())
    }
}

/// Sum of all file lengths is over [`u32::MAX`].
#[derive(Debug)]
pub struct OffsetOverflowError(pub(crate) ());

impl fmt::Display for OffsetOverflowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("files larger than 4GiB are not supported")
    }
}

impl std::error::Error for OffsetOverflowError {}

impl From<OffsetOverflowError> for io::Error {
    fn from(e: OffsetOverflowError) -> Self {
        Self::new(io::ErrorKind::FileTooLarge, e)
    }
}

/// A single source in the `SourceMap`.
#[derive(Clone, derive_more::Debug)]
#[non_exhaustive]
pub struct SourceFile {
    /// The name of the file that the source came from. Source that doesn't
    /// originate from files has names between angle brackets by convention
    /// (e.g., `<stdin>`).
    pub name: FileName,
    /// The complete source code.
    #[debug(skip)]
    pub src: Arc<String>,
    /// The start position of this source in the `SourceMap`.
    pub start_pos: BytePos,
    /// The byte length of this source.
    pub source_len: RelativeBytePos,
    /// Locations of lines beginnings in the source code.
    #[debug(skip)]
    pub lines: Vec<RelativeBytePos>,
    /// Locations of multi-byte characters in the source code.
    #[debug(skip)]
    pub multibyte_chars: Vec<MultiByteChar>,
}

impl PartialEq for SourceFile {
    fn eq(&self, other: &Self) -> bool {
        self.start_pos == other.start_pos
    }
}

impl Eq for SourceFile {}

impl std::hash::Hash for SourceFile {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.start_pos.hash(state);
    }
}

impl SourceFile {
    /// Creates a new `SourceFile`. Use the [`SourceMap`](crate::SourceMap) methods instead.
    pub(crate) fn new(
        name: FileName,
        id: SourceFileId,
        mut src: String,
    ) -> Result<Self, OffsetOverflowError> {
        // Compute the file hash before any normalization.
        // let src_hash = SourceFileHash::new(hash_kind, &src);

        // let normalized_pos = normalize_src(&mut src);

        debug_assert_eq!(id, SourceFileId::new(&name));
        let source_len = src.len();
        let source_len = u32::try_from(source_len).map_err(|_| OffsetOverflowError(()))?;

        let (lines, multibyte_chars) = super::analyze::analyze_source_file(&src);

        src.shrink_to_fit();
        Ok(Self {
            name,
            src: Arc::new(src),
            start_pos: BytePos::from_u32(0),
            source_len: RelativeBytePos::from_u32(source_len),
            lines,
            multibyte_chars,
        })
    }

    pub fn lines(&self) -> &[RelativeBytePos] {
        &self.lines
    }

    pub fn count_lines(&self) -> usize {
        self.lines().len()
    }

    #[inline]
    pub fn absolute_position(&self, pos: RelativeBytePos) -> BytePos {
        BytePos::from_u32(pos.to_u32() + self.start_pos.to_u32())
    }

    #[inline]
    pub fn relative_position(&self, pos: BytePos) -> RelativeBytePos {
        RelativeBytePos::from_u32(pos.to_u32() - self.start_pos.to_u32())
    }

    #[inline]
    pub fn end_position(&self) -> BytePos {
        self.absolute_position(self.source_len)
    }

    /// Finds the line containing the given position. The return value is the
    /// index into the `lines` array of this `SourceFile`, not the 1-based line
    /// number. If the source_file is empty or the position is located before the
    /// first line, `None` is returned.
    pub fn lookup_line(&self, pos: RelativeBytePos) -> Option<usize> {
        self.lines().partition_point(|x| x <= &pos).checked_sub(1)
    }

    /// Returns the relative byte position of the start of the line at the given
    /// 0-based line index.
    pub fn line_position(&self, line_number: usize) -> Option<usize> {
        self.lines().get(line_number).map(|x| x.to_usize())
    }

    /// Converts a `RelativeBytePos` to a `CharPos` relative to the `SourceFile`.
    pub(crate) fn bytepos_to_file_charpos(&self, bpos: RelativeBytePos) -> CharPos {
        // The number of extra bytes due to multibyte chars in the `SourceFile`.
        let mut total_extra_bytes = 0;

        for mbc in self.multibyte_chars.iter() {
            if mbc.pos < bpos {
                // Every character is at least one byte, so we only
                // count the actual extra bytes.
                total_extra_bytes += mbc.bytes as u32 - 1;
                // We should never see a byte position in the middle of a
                // character.
                assert!(bpos.to_u32() >= mbc.pos.to_u32() + mbc.bytes as u32);
            } else {
                break;
            }
        }

        assert!(total_extra_bytes <= bpos.to_u32());
        CharPos(bpos.to_usize() - total_extra_bytes as usize)
    }

    /// Looks up the file's (1-based) line number and (0-based `CharPos`) column offset, for a
    /// given `RelativeBytePos`.
    fn lookup_file_pos(&self, pos: RelativeBytePos) -> (usize, CharPos) {
        let chpos = self.bytepos_to_file_charpos(pos);
        match self.lookup_line(pos) {
            Some(a) => {
                let line = a + 1; // Line numbers start at 1
                let linebpos = self.lines()[a];
                let linechpos = self.bytepos_to_file_charpos(linebpos);
                let col = chpos - linechpos;
                assert!(chpos >= linechpos);
                (line, col)
            }
            None => (0, chpos),
        }
    }

    /// Looks up the file's (1-based) line number, (0-based `CharPos`) column offset, and (0-based)
    /// column offset when displayed, for a given `BytePos`.
    pub fn lookup_file_pos_with_col_display(&self, pos: BytePos) -> (usize, CharPos, usize) {
        let pos = self.relative_position(pos);
        let (line, col_or_chpos) = self.lookup_file_pos(pos);
        if line > 0 {
            let Some(code) = self.get_line(line - 1) else {
                // If we don't have the code available, it is ok as a fallback to return the bytepos
                // instead of the "display" column, which is only used to properly show underlines
                // in the terminal.
                // FIXME: we'll want better handling of this in the future for the sake of tools
                // that want to use the display col instead of byte offsets to modify code, but
                // that is a problem for another day, the previous code was already incorrect for
                // both displaying *and* third party tools using the json output naïvely.
                debug!("couldn't find line {line} in {:?}", self.name);
                return (line, col_or_chpos, col_or_chpos.0);
            };
            let display_col = code.chars().take(col_or_chpos.0).map(char_width).sum();
            (line, col_or_chpos, display_col)
        } else {
            // This is never meant to happen?
            (0, col_or_chpos, col_or_chpos.0)
        }
    }

    /// Gets a line from the list of pre-computed line-beginnings.
    /// The line number here is 0-based.
    pub fn get_line(&self, line_number: usize) -> Option<&str> {
        fn get_until_newline(src: &str, begin: usize) -> &str {
            // We can't use `lines.get(line_number+1)` because we might
            // be parsing when we call this function and thus the current
            // line is the last one we have line info for.
            let slice = &src[begin..];
            match slice.find('\n') {
                Some(e) => &slice[..e],
                None => slice,
            }
        }

        let start = self.lines().get(line_number)?.to_usize();
        Some(get_until_newline(&self.src, start))
    }

    /// Gets a slice of the source text between two lines, including the
    /// terminator of the second line (if any).
    pub fn get_lines(&self, range: RangeInclusive<usize>) -> Option<&str> {
        fn get_until_newline(src: &str, start: usize, end: usize) -> &str {
            match src[end..].find('\n') {
                Some(e) => &src[start..end + e + 1],
                None => &src[start..],
            }
        }

        let (start, end) = range.into_inner();
        let lines = self.lines();
        let start = lines.get(start)?.to_usize();
        let end = lines.get(end)?.to_usize();
        Some(get_until_newline(&self.src, start, end))
    }

    /// Returns whether or not the file contains the given `SourceMap` byte
    /// position. The position one past the end of the file is considered to be
    /// contained by the file. This implies that files for which `is_empty`
    /// returns true still contain one byte position according to this function.
    #[inline]
    pub fn contains(&self, byte_pos: BytePos) -> bool {
        byte_pos >= self.start_pos && byte_pos <= self.end_position()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.source_len.to_u32() == 0
    }

    /// Calculates the original byte position relative to the start of the file
    /// based on the given byte position.
    pub fn original_relative_byte_pos(&self, pos: BytePos) -> RelativeBytePos {
        let pos = self.relative_position(pos);
        RelativeBytePos::from_u32(pos.0)
    }
}

pub fn char_width(ch: char) -> usize {
    match ch {
        '\t' => 4,
        _ => unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1),
    }
}

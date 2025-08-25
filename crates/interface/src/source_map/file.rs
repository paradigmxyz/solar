use crate::{BytePos, CharPos, SourceMap, pos::RelativeBytePos};
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

impl From<String> for FileName {
    fn from(s: String) -> Self {
        Self::Custom(s)
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
        let sm = crate::SessionGlobals::try_with(|g| g.map(|g| g.source_map.clone()));
        FileNameDisplay { inner: self, sm }
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

pub struct FileNameDisplay<'a> {
    inner: &'a FileName,
    sm: Option<Arc<SourceMap>>,
}

impl fmt::Display for FileNameDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.inner {
            FileName::Real(path) => {
                let path = if let Some(sm) = &self.sm
                    && let Some(base_path) = sm.base_path.get()
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
pub struct StableSourceFileId(u64);

impl StableSourceFileId {
    pub(super) fn from_filename_in_current_crate(filename: &FileName) -> Self {
        Self::new(
            filename,
            // None
        )
    }

    // pub fn from_filename_for_export(
    //     filename: &FileName,
    //     local_crate_stable_crate_id: StableCrateId,
    // ) -> Self {
    //     Self::new(filename, Some(local_crate_stable_crate_id))
    // }

    fn new(
        filename: &FileName,
        // stable_crate_id: Option<StableCrateId>,
    ) -> Self {
        use std::hash::{Hash, Hasher};
        let mut hasher = solar_data_structures::map::FxHasher::default();
        filename.hash(&mut hasher);
        // stable_crate_id.hash(&mut hasher);
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
pub struct SourceFile {
    /// The name of the file that the source came from. Source that doesn't
    /// originate from files has names between angle brackets by convention
    /// (e.g., `<stdin>`).
    pub name: FileName,
    /// The complete source code.
    #[debug(skip)]
    pub src: Arc<String>,
    /// The source code's hash.
    #[debug(skip)]
    pub src_hash: SourceFileHash,
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
    /// A hash of the filename & crate-id, used for uniquely identifying source
    /// files within the crate graph and for speeding up hashing in incremental
    /// compilation.
    #[debug(skip)]
    pub stable_id: StableSourceFileId,
}

impl SourceFile {
    pub fn new(
        name: FileName,
        mut src: String,
        hash_kind: SourceFileHashAlgorithm,
    ) -> Result<Self, OffsetOverflowError> {
        // Compute the file hash before any normalization.
        let src_hash = SourceFileHash::new(hash_kind, &src);
        normalize_newlines(&mut src);

        let stable_id = StableSourceFileId::from_filename_in_current_crate(&name);
        let source_len = src.len();
        let source_len = u32::try_from(source_len).map_err(|_| OffsetOverflowError(()))?;

        let (lines, multibyte_chars) = super::analyze::analyze_source_file(&src);

        src.shrink_to_fit();
        Ok(Self {
            name,
            src: Arc::new(src),
            src_hash,
            start_pos: BytePos::from_u32(0),
            source_len: RelativeBytePos::from_u32(source_len),
            lines,
            multibyte_chars,
            stable_id,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceFileHashAlgorithm {
    #[default]
    None,
    // Md5,
    // Sha1,
    // Sha256,
}

impl std::str::FromStr for SourceFileHashAlgorithm {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // match s {
        //     "md5" => Ok(Self::Md5),
        //     "sha1" => Ok(Self::Sha1),
        //     "sha256" => Ok(Self::Sha256),
        //     _ => Err(()),
        // }
        let _ = s;
        Err(())
    }
}

impl SourceFileHashAlgorithm {
    /// The length of the hash in bytes.
    #[inline]
    pub const fn hash_len(self) -> usize {
        match self {
            Self::None => 0,
            // Self::Md5 => 16,
            // Self::Sha1 => 20,
            // Self::Sha256 => 32,
        }
    }
}

const MAX_HASH_SIZE: usize = 32;

/// The hash of the on-disk source file used for debug info.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceFileHash {
    kind: SourceFileHashAlgorithm,
    value: [u8; MAX_HASH_SIZE],
}

impl fmt::Debug for SourceFileHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut dbg = f.debug_struct("SourceFileHash");
        dbg.field("kind", &self.kind);
        if self.kind != SourceFileHashAlgorithm::None {
            dbg.field("value", &format_args!("{}", hex::encode(self.hash_bytes())));
        }
        dbg.finish()
    }
}

impl SourceFileHash {
    pub fn new(kind: SourceFileHashAlgorithm, src: &str) -> Self {
        // use md5::digest::{typenum::Unsigned, Digest, OutputSizeUser};

        // fn digest_into<D: Digest>(data: &[u8], out: &mut [u8; MAX_HASH_SIZE]) {
        //     let mut hasher = D::new();
        //     hasher.update(data);
        //     hasher.finalize_into((&mut out[..<D as OutputSizeUser>::OutputSize::USIZE]).into());
        // }

        // let mut hash = Self { kind, value: Default::default() };
        // let value = &mut hash.value;
        // let data = src.as_bytes();
        // match kind {
        //     SourceFileHashAlgorithm::None => (),
        //     SourceFileHashAlgorithm::Md5 => digest_into::<md5::Md5>(data, value),
        //     SourceFileHashAlgorithm::Sha1 => digest_into::<sha1::Sha1>(data, value),
        //     SourceFileHashAlgorithm::Sha256 => digest_into::<sha256::Sha256>(data, value),
        // }
        // hash
        let _ = src;
        Self { kind, value: Default::default() }
    }

    /// Check if the stored hash matches the hash of the string.
    pub fn matches(&self, src: &str) -> bool {
        Self::new(self.kind, src).hash_bytes() == self.hash_bytes()
    }

    /// The bytes of the hash.
    pub fn hash_bytes(&self) -> &[u8] {
        &self.value[..self.hash_len()]
    }

    /// The hash algorithm used.
    pub const fn kind(&self) -> SourceFileHashAlgorithm {
        self.kind
    }

    /// Returns the length of the hash in bytes.
    #[inline]
    pub const fn hash_len(&self) -> usize {
        self.kind.hash_len()
    }
}

/// Replaces `\r\n` with `\n` in-place in `src`.
///
/// Leaves any occurrences of lone `\r` unchanged.
// Taken from: https://github.com/rust-lang/rust/blob/ee361e8fca1c30e13e7a31cc82b64c045339d3a8/compiler/rustc_span/src/lib.rs#L2353C1-L2412C2
fn normalize_newlines(src: &mut String) {
    if !src.as_bytes().contains(&b'\r') {
        return;
    }

    // We replace `\r\n` with `\n` in-place, which doesn't break utf-8 encoding.
    // While we *can* call `as_mut_vec` and do surgery on the live string
    // directly, let's rather steal the contents of `src`. This makes the code
    // safe even if a panic occurs.

    let mut buf = std::mem::replace(src, String::new()).into_bytes();
    let mut gap_len = 0;
    let mut tail = buf.as_mut_slice();
    loop {
        let idx = match find_crlf(&tail[gap_len..]) {
            None => tail.len(),
            Some(idx) => idx + gap_len,
        };
        tail.copy_within(gap_len..idx, 0);
        tail = &mut tail[idx - gap_len..];
        if tail.len() == gap_len {
            break;
        }
        gap_len += 1;
    }

    // Account for removed `\r`.
    // After `set_len`, `buf` is guaranteed to contain utf-8 again.
    let new_len = buf.len() - gap_len;
    unsafe {
        buf.set_len(new_len);
        *src = String::from_utf8_unchecked(buf);
    }

    fn find_crlf(src: &[u8]) -> Option<usize> {
        let mut search_idx = 0;
        while let Some(idx) = find_cr(&src[search_idx..]) {
            if src[search_idx..].get(idx + 1) != Some(&b'\n') {
                search_idx += idx + 1;
                continue;
            }
            return Some(search_idx + idx);
        }
        None
    }

    fn find_cr(src: &[u8]) -> Option<usize> {
        src.iter().position(|&b| b == b'\r')
    }
}

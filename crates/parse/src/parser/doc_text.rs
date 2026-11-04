//! The text of a doc comment as solc's scanner hands it to its NatSpec parser.
//!
//! solc merges adjacent `///` lines into one comment and strips the decoration of `/** */`
//! comments before the NatSpec parser looks for tags (`Scanner::scanSingleLineDocComment` and
//! `Scanner::scanMultiLineDocComment`). Reading the same text keeps the whitespace of each
//! continuation line identical to solc's.

use solar_ast::{self as ast, DocComment};
use solar_interface::BytePos;

/// The text of one doc comment, with a map back to source positions.
#[derive(Default)]
pub(super) struct DocText {
    text: Vec<u8>,
    /// Runs of bytes that are contiguous in the source, as `(offset in text, source position)`.
    segments: Vec<(u32, u32)>,
}

impl DocText {
    /// Reads the text of a comment, or of a run of adjacent `///` comments.
    pub(super) fn read(docs: &[DocComment<'_>]) -> Self {
        let mut text = Self::default();
        match docs[0].kind {
            ast::CommentKind::Line => text.read_lines(docs),
            ast::CommentKind::Block => text.read_block(&docs[0]),
        }
        text
    }

    pub(super) fn as_str(&self) -> &str {
        // The text only lacks ASCII bytes of a valid `str`.
        std::str::from_utf8(&self.text).expect("doc comment text is valid UTF-8")
    }

    /// Returns the source position of the byte at `offset`.
    pub(super) fn source_pos(&self, offset: usize) -> BytePos {
        let index = self.segments.partition_point(|&(start, _)| start as usize <= offset);
        let (start, source) = self.segments[index.saturating_sub(1)];
        BytePos(source + (offset as u32 - start))
    }

    fn push(&mut self, byte: u8, source: u32) {
        let offset = self.text.len() as u32;
        match self.segments.last() {
            Some(&(start, first)) if first + (offset - start) == source => {}
            _ => self.segments.push((offset, source)),
        }
        self.text.push(byte);
    }

    fn push_str(&mut self, text: &str, source: u32) {
        for (i, byte) in text.bytes().enumerate() {
            self.push(byte, source + i as u32);
        }
    }

    /// Joins `///` lines with `\n`. The first line loses its leading blanks, the others keep them,
    /// and empty lines vanish.
    fn read_lines(&mut self, docs: &[DocComment<'_>]) {
        for (index, doc) in docs.iter().enumerate() {
            let source = doc.span.lo().0 + 3;
            let line = doc.symbol.as_str();
            let line = line.strip_suffix('\r').unwrap_or(line);
            if index == 0 {
                let blanks = line.len() - line.trim_start_matches([' ', '\t']).len();
                self.push_str(&line[blanks..], source + blanks as u32);
            } else if !line.is_empty() {
                self.push(b'\n', source);
                self.push_str(line, source);
            }
        }
    }

    /// Drops the leading blanks and the first `*` of every line after the first, and the line
    /// breaks of empty lines.
    fn read_block(&mut self, doc: &DocComment<'_>) {
        let source = doc.span.lo().0 + 3;
        let content = doc.symbol.as_str().as_bytes();
        // The scanner stops at the closing `*/`, which the token does not include.
        let total = content.len() + 2;
        let at = |i: usize| match content.get(i) {
            Some(&byte) => byte,
            None => b"*/".get(i - content.len()).copied().unwrap_or(0),
        };
        let is_blank = |byte: u8| matches!(byte, b' ' | b'\t' | b'\n' | b'\r');
        let is_eol = |byte: u8| matches!(byte, b'\n' | b'\r');

        let mut i = 0;
        let mut chars_added = false;
        while is_blank(at(i)) && !is_eol(at(i)) {
            i += 1;
        }
        while i < total {
            if is_eol(at(i)) {
                while i < total && is_blank(at(i)) {
                    i += 1;
                }
                if i + 1 < total && at(i) == b'*' && at(i + 1) == b'*' {
                    // It is unknown if this leads to the end of the comment.
                    self.push(b'*', source + i as u32);
                    i += 1;
                } else if i + 1 < total && at(i) == b'*' && at(i + 1) != b'/' {
                    // Skip the first `*` of the line.
                    i += 1;
                    if is_eol(at(i)) {
                        continue;
                    }
                    if chars_added {
                        self.push(b'\n', source + i as u32);
                    }
                } else if i + 1 < total && at(i) == b'*' && at(i + 1) == b'/' {
                    break;
                } else if chars_added {
                    self.push(b'\n', source + i as u32);
                }
            }
            if i + 1 < total && at(i) == b'*' && at(i + 1) == b'/' {
                break;
            }
            self.push(at(i), source + i as u32);
            chars_added = true;
            i += 1;
        }
    }
}

/// Returns whether the text between two `///` comments lets the second continue the first: one
/// line break, then blanks.
pub(super) fn continues_line_comment(between: &str) -> bool {
    let rest = between
        .strip_prefix("\r\n")
        .or_else(|| between.strip_prefix('\n'))
        .or_else(|| between.strip_prefix('\r'));
    rest.is_some_and(|rest| rest.bytes().all(|byte| matches!(byte, b' ' | b'\t')))
}

//! NatSpec parsing of doc comments.
//!
//! Solc's scanner merges adjacent `///` lines into one doc comment and strips the decoration of
//! `/** */` comments (`Scanner::scanSingleLineDocComment` and `Scanner::scanMultiLineDocComment`).
//! Its NatSpec parser then finds the tags and joins the lines of each tag (`DocStringParser`).
//! This does the same on the lines of the original comments, so that the content of a tag on a
//! single line is a slice of its comment and only a tag over several lines needs a new symbol:
//!
//! - A run of adjacent `///` comments is one comment, and its items are on the first of them.
//! - Text before the first tag is a `@notice`.
//! - The content of a tag starts after the blanks that follow the tag, or on the next line if it
//!   has none on its own line. Unlike solc, a tag is never content, so that `@custom:solar-cold`
//!   can be followed by another tag.
//! - The lines of a tag are joined with line breaks, so that the content of a tag keeps its
//!   paragraphs, and a line with text loses a leading space. The outputs join the lines like solc
//!   does.
//!
//! Validation is deferred to lowering. Unknown tags are ignored in Yul, like solc does:
//! <https://github.com/argotorg/solidity/blob/2ca5fb3b6adcb1a8fb2c0904fb37526121cf2c72/libyul/AsmParser.cpp#L151>

use smallvec::SmallVec;
use solar_ast::{self as ast, DocComment};
use solar_interface::{BytePos, Ident, Span, Symbol, diagnostics::DiagCtxt, error_code};

/// A line of the text that solc's scanner reads from a doc comment.
struct Line<'a> {
    text: &'a str,
    /// The symbol of the doc comment that has `text`.
    symbol: Symbol,
    /// The offset of `text` in `symbol`.
    start: u32,
    /// The source position of the first byte of `symbol`.
    base: u32,
    /// Whether the scanner appends the line to the previous one without a line break, like it does
    /// for a line that starts with `**`.
    glued: bool,
}

impl Line<'_> {
    /// Returns the source position of the byte at `offset` in the text.
    fn pos(&self, offset: usize) -> BytePos {
        BytePos(self.base + self.start + offset as u32)
    }
}

type Lines<'a> = SmallVec<[Line<'a>; 16]>;

/// The item whose content is being read.
struct PendingItem {
    kind: ast::NatSpecKind,
    span: Span,
    /// The line where the content starts, and its offset in the line.
    line: usize,
    offset: usize,
    /// Whether the item is the `@notice` of text before any tag, which is dropped when blank.
    implicit: bool,
}

/// Returns whether only one line break and blanks separate two `///` comments, which makes the
/// second continue the first.
pub(super) fn continues_line_comment(between: &str) -> bool {
    let rest = between
        .strip_prefix("\r\n")
        .or_else(|| between.strip_prefix('\n'))
        .or_else(|| between.strip_prefix('\r'));
    rest.is_some_and(|rest| rest.bytes().all(|byte| matches!(byte, b' ' | b'\t')))
}

/// Parses the NatSpec items of a doc comment, or of a run of adjacent `///` comments, and appends
/// them to `items`.
pub(super) fn parse_natspec(
    docs: &[DocComment<'_>],
    in_yul: bool,
    dcx: &DiagCtxt,
    items: &mut SmallVec<[ast::NatSpecItem; 6]>,
) {
    let comment_kind = docs[0].kind;
    let comment_span = docs[0].span.to(docs[docs.len() - 1].span);
    let lines = match comment_kind {
        ast::CommentKind::Line => line_comment_lines(docs),
        ast::CommentKind::Block => block_comment_lines(&docs[0]),
    };
    if lines.is_empty() {
        return;
    }
    // The token has no `*/`, so a `*` at its end is the first of a `**/`.
    let closing_star =
        comment_kind == ast::CommentKind::Block && docs[0].symbol.as_str().ends_with('*');

    #[cold]
    #[inline(never)]
    fn invalid_tag(dcx: &DiagCtxt, tag: &str, span: Span) {
        dcx.emit_warn_code(
            span,
            error_code!(6546),
            format!("invalid natspec tag '@{tag}', custom tags must use format '@custom:name'"),
        );
    }

    // Returns the offset of the first non-blank byte at or after `from`.
    let skip_blanks = |text: &str, from: usize| {
        from + text.as_bytes()[from..].iter().take_while(|b| matches!(b, b' ' | b'\t')).count()
    };

    // Check if '@' is located at the logical start of the line.
    let is_line_start = |text: &str, at_pos: usize| {
        let before = &text.as_bytes()[..at_pos];
        match comment_kind {
            // can only be followed by whitespace
            ast::CommentKind::Line => before.trim_ascii().is_empty(),

            // can only be followed by whitespaces and '*' decorations, which
            // may repeat (`* * @return` still starts a tag, as in solc, where
            // the scanner strips one decoration and the parser finds the tag
            // anywhere in the line).
            ast::CommentKind::Block => {
                let mut rest = before.trim_ascii_start();
                while let Some(stripped) = rest.strip_prefix(b"*") {
                    rest = stripped.trim_ascii_start();
                }
                rest.is_empty()
            }
        }
    };

    // Text before the first tag is a `@notice`.
    let mut pending = Some(PendingItem {
        kind: ast::NatSpecKind::Notice,
        span: comment_span,
        line: 0,
        offset: skip_blanks(lines[0].text, 0),
        implicit: true,
    });
    // Whether the tag has no content on its line, so its content starts on the next one.
    let mut content_on_next_line = false;
    // The first line of the logical line that solc reads, which includes the lines glued to it,
    // and whether solc consumed it as the first line of an item, so that it does not look for tags
    // in it again.
    let mut logical_start = 0;
    let mut consumed = false;

    // Iterate over each line and look for tags.
    for (index, line) in lines.iter().enumerate() {
        let text = line.text;
        // An empty line is only part of the content of a tag.
        if text.is_empty() {
            continue;
        }
        if !line.glued {
            (logical_start, consumed) = (index, false);
        }
        // Like solc, a tag starts at the first `@` of a line, wherever it is, and drops the text
        // before it, including the lines glued before it.
        let tag_offset =
            if line.glued && consumed { None } else { memchr::memchr(b'@', text.as_bytes()) };
        let end = if line.glued { logical_start } else { index };
        let starts_tag = tag_offset.is_some_and(|offset| {
            is_line_start(text, offset)
                && text.as_bytes().get(offset + 1).is_some_and(|b| !matches!(b, b' ' | b'\t'))
        });
        if content_on_next_line && !starts_tag {
            // Like solc, a line after a tag without content is its content.
            if let Some(item) = &mut pending {
                (item.line, item.offset) = (index, skip_blanks(text, 0));
            }
            content_on_next_line = false;
            consumed = true;
        } else if let Some(tag_offset) = tag_offset {
            content_on_next_line = false;
            let tag_start = tag_offset + 1;

            // The tag name must immediately follow the '@': `@ param` is
            // plain text continuing the previous tag, matching solc.
            let name_end = text.as_bytes()[tag_start..]
                .iter()
                .position(|b| matches!(b, b' ' | b'\t'))
                .map_or(text.len(), |offset| tag_start + offset);
            let tag = &text[tag_start..name_end];
            if tag.is_empty() {
                // solc appends the rest of the logical line to the previous tag.
                consumed = true;
                continue;
            }
            let rest_start = skip_blanks(text, name_end);

            // Calculate span: from the char after '@' to end of tag name.
            let tag_lo = line.pos(tag_offset) + 1; // +1 for '@'
            let span = Span::new(tag_lo, tag_lo + tag.len() as u32);
            let mut offset = rest_start;

            let kind = match tag {
                "title" => ast::NatSpecKind::Title,
                "author" => ast::NatSpecKind::Author,
                "notice" => ast::NatSpecKind::Notice,
                "dev" => ast::NatSpecKind::Dev,
                "param" | "inheritdoc" => {
                    // Names are identifier prefixes, mirroring solc: `@param -`
                    // documents an unnamed parameter with an empty name.
                    let (name, content_start) =
                        crate::natspec::split_once_ident(text, rest_start, text.len());
                    offset = content_start;
                    let name_lo = line.pos(rest_start);
                    let name_span = Span::new(name_lo, name_lo + name.len() as u32);
                    let ident = Ident::new(Symbol::intern(name), name_span);
                    match tag {
                        "param" => ast::NatSpecKind::Param { name: ident },
                        _ => ast::NatSpecKind::Inheritdoc { contract: ident },
                    }
                }
                "return" => ast::NatSpecKind::Return { name: None },
                _ => {
                    if let Some(custom_tag) = tag.strip_prefix("custom:") {
                        let ident = Ident::new(Symbol::intern(custom_tag), comment_span);
                        ast::NatSpecKind::Custom { name: ident }
                    } else if ast::NATSPEC_INTERNAL_TAGS[..].contains(&tag) {
                        let ident = Ident::new(Symbol::intern(tag), comment_span);
                        ast::NatSpecKind::Internal { tag: ident }
                    } else {
                        // Emit error for invalid solidity tags, but ignore in Yul.
                        if !in_yul {
                            invalid_tag(dcx, tag, comment_span);
                        }
                        // Like solc, the tag and its lines are not content of the previous tag.
                        if let Some(item) = pending.take() {
                            push_item(items, &lines, item, end, closing_star);
                        }
                        consumed = true;
                        continue;
                    }
                }
            };

            if let Some(item) = pending.take() {
                push_item(items, &lines, item, end, closing_star);
            }
            consumed = true;
            content_on_next_line = name_end == text.len() && !matches!(tag, "param" | "inheritdoc");
            pending = Some(PendingItem { kind, span, line: index, offset, implicit: false });
        }
    }
    if let Some(item) = pending {
        push_item(items, &lines, item, lines.len(), closing_star);
    }
}

/// Pushes the item whose content is on the lines before `end`.
fn push_item(
    items: &mut SmallVec<[ast::NatSpecItem; 6]>,
    lines: &Lines<'_>,
    item: PendingItem,
    end: usize,
    closing_star: bool,
) {
    // The empty lines at the end are not content.
    let mut end = end;
    while end > item.line + 1 && lines[end - 1].text.is_empty() {
        end -= 1;
    }
    let (symbol, content_start, mut content_end, content_lo) = match &lines[item.line..end] {
        // Only the notice before the first tag has no lines, when the first line is a tag.
        [] => {
            debug_assert!(item.implicit);
            return;
        }
        // A single line is a slice of its comment.
        [line] => {
            let start = line.start as usize + item.offset;
            let content_lo = first_non_blank(line, item.offset).unwrap_or(item.span.hi());
            (line.symbol, start, line.start as usize + line.text.len(), content_lo)
        }
        [first, rest @ ..] => {
            let (joined, content_lo) = join_lines(first, item.offset, rest);
            (Symbol::intern(&joined), 0, joined.len(), content_lo.unwrap_or(item.span.hi()))
        }
    };
    if item.implicit && symbol.as_str()[content_start..content_end].trim_ascii().is_empty() {
        return;
    }

    // The `*` of a `**/` that closes the comment is not part of the arguments of a
    // `@custom:solar-*` tag. Anywhere else, a `*` is part of them.
    if closing_star
        && end == lines.len()
        && let ast::NatSpecKind::Custom { name } = item.kind
        && name.name.as_str().starts_with("solar-")
        && let Some(without_star) = symbol.as_str()[content_start..content_end].strip_suffix('*')
    {
        content_end = content_start + without_star.trim_end().len();
    }
    items.push(ast::NatSpecItem {
        kind: item.kind,
        span: item.span,
        symbol,
        content_lo,
        content_start: content_start as u32,
        content_end: content_end as u32,
    });
}

/// Returns the source position of the first non-blank byte of the text of `line` from `offset`.
fn first_non_blank(line: &Line<'_>, offset: usize) -> Option<BytePos> {
    let lead = blanks_len(&line.text[offset..]);
    (offset + lead < line.text.len()).then(|| line.pos(offset + lead))
}

/// Returns the length of the leading whitespace of `text`.
fn blanks_len(text: &str) -> usize {
    text.len() - text.trim_ascii_start().len()
}

/// Joins the lines of a tag's content with line breaks. A logical line is a line and the lines
/// that the scanner glues to it. Joining them like solc is up to the output: it drops the empty
/// lines and puts a space before a line with text, which lost its first space for that.
///
/// Also returns the source position of the first non-blank byte of the joined text.
fn join_lines(first: &Line<'_>, offset: usize, rest: &[Line<'_>]) -> (String, Option<BytePos>) {
    let capacity = rest.iter().map(|line| line.text.len() + 1).sum::<usize>();
    let mut joined = String::with_capacity(first.text.len() - offset + capacity);
    joined.push_str(&first.text[offset..]);
    let mut content_lo = first_non_blank(first, offset);

    // The logical line of the first line is content as it is.
    let mut index = 0;
    while let Some(line) = rest.get(index).filter(|line| line.glued) {
        joined.push_str(line.text);
        content_lo = content_lo.or_else(|| first_non_blank(line, 0));
        index += 1;
    }
    let mut logical = String::new();
    while index < rest.len() {
        let start = index;
        index += 1;
        while rest.get(index).is_some_and(|line| line.glued) {
            index += 1;
        }
        let group = &rest[start..index];
        logical.clear();
        for line in group {
            logical.push_str(line.text);
        }

        // An `@` that names no tag is dropped with the blank after it, and the text before it,
        // as solc does.
        let mut kept = 0;
        if let Some(at) = logical.find('@')
            && let after = &logical[at + 1..]
            && (after.is_empty() || after.starts_with([' ', '\t']))
        {
            // A lone `@` has no text to join.
            if after.is_empty() {
                continue;
            }
            kept = at + 2;
        }
        let text = &logical[kept..];
        joined.push('\n');
        // Solc joins an `@ ` with a space, but drops an empty line.
        if kept > 0 && text.is_empty() {
            joined.push(' ');
        } else {
            // The output puts back the space that it joins the line with, unless a tab follows.
            let has_text = !text.trim_ascii().is_empty();
            let stripped = text.strip_prefix(' ').filter(|rest| !rest.starts_with('\t'));
            joined.push_str(if has_text { stripped.unwrap_or(text) } else { text });
        }
        if content_lo.is_none() {
            let lead = blanks_len(text);
            if lead < text.len() {
                content_lo = Some(position_in_logical_line(group, kept + lead));
            }
        }
    }
    (joined, content_lo)
}

/// Returns the source position of the byte at `offset` in the logical line made of `group`.
fn position_in_logical_line(group: &[Line<'_>], mut offset: usize) -> BytePos {
    for line in group {
        if offset < line.text.len() {
            return line.pos(offset);
        }
        offset -= line.text.len();
    }
    group[group.len() - 1].pos(group[group.len() - 1].text.len())
}

/// Returns the lines of adjacent `///` comments. The first loses its leading blanks and the
/// others keep them. An empty line has empty text.
fn line_comment_lines<'a>(docs: &'a [DocComment<'_>]) -> Lines<'a> {
    let mut lines = Lines::new();
    for (index, doc) in docs.iter().enumerate() {
        let text = doc.symbol.as_str();
        let text = text.strip_suffix('\r').unwrap_or(text);
        let start =
            if index == 0 { text.len() - text.trim_start_matches([' ', '\t']).len() } else { 0 };
        lines.push(Line {
            text: &text[start..],
            symbol: doc.symbol,
            start: start as u32,
            base: doc.span.lo().0 + 3,
            glued: false,
        });
    }
    lines
}

/// Returns the lines of a `/** */` comment. Every line after the first loses its leading blanks
/// and its first `*`. An empty line has empty text, except before the content and at the closing
/// `*/`, where it is not a line.
fn block_comment_lines<'a>(doc: &'a DocComment<'_>) -> Lines<'a> {
    let content = doc.symbol.as_str();
    let bytes = content.as_bytes();
    let mut lines = Lines::new();
    let mut line_start = 0;
    while line_start <= bytes.len() {
        let (line_end, next) = match memchr::memchr2(b'\n', b'\r', &bytes[line_start..]) {
            Some(offset) => {
                let end = line_start + offset;
                let crlf = bytes[end] == b'\r' && bytes.get(end + 1) == Some(&b'\n');
                (end, end + 1 + crlf as usize)
            }
            None => (bytes.len(), bytes.len() + 1),
        };
        let is_last = line_end == bytes.len();
        let mut start = line_start;
        let mut glued = false;
        let mut skip = false;
        while start < line_end && matches!(bytes[start], b' ' | b'\t') {
            start += 1;
        }
        // The decoration of the lines after the first.
        if line_start > 0 {
            match &bytes[start..line_end] {
                [] => skip = is_last || lines.is_empty(),
                // The scanner reads the `*` before the closing `*/` like the first of `**`.
                [b'*'] if is_last => glued = true,
                [b'*'] => {
                    start = line_end;
                    skip = lines.is_empty();
                }
                [b'*', b'*', ..] => glued = true,
                [b'*', ..] => {
                    start += 1;
                    skip = start == line_end && lines.is_empty();
                }
                _ => {}
            }
        } else if start == line_end {
            skip = true;
        }
        if !skip {
            // Solc drops the empty lines, so the line is glued to the last that is not empty.
            if glued {
                while lines.last().is_some_and(|line| line.text.is_empty()) {
                    lines.pop();
                }
            }
            lines.push(Line {
                text: &content[start..line_end],
                symbol: doc.symbol,
                start: start as u32,
                base: doc.span.lo().0 + 3,
                glued: glued && !lines.is_empty(),
            });
        }
        line_start = next;
    }
    lines
}

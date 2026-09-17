//! Syntax-based folding-range construction.

use crop::Rope;
use lsp_types::{FoldingRange, FoldingRangeKind, Position};
use solar_config::CompileOpts;
use solar_interface::{
    Session, Span,
    data_structures::{Never, map::FxHashMap},
    source_map::{FileName, SourceFile},
};
use solar_parse::{
    Cursor, Parser,
    ast::{self, token::Delimiter, visit::Visit},
    lexer::token::RawTokenKind,
};
use std::{
    cmp::Reverse,
    ops::{ControlFlow, Range as ByteRange},
    sync::Arc,
};

pub(crate) fn folding_ranges(source: String) -> Vec<FoldingRange> {
    if memchr::memchr2(b'\r', b'\n', source.as_bytes()).is_none() {
        return Vec::new();
    }
    let source = Arc::new(source);
    let (file, ranges) = collect_ranges(source.clone()).map_or_else(
        || {
            let LexicalInfo { mut ranges, fallback_ranges, .. } =
                collect_lexical_info(&source, true);
            ranges.extend(fallback_ranges);
            (None, ranges)
        },
        |(file, ranges)| (Some(file), ranges),
    );
    let index = FlatPositionIndex::new(&source, file.as_deref());
    let mut ranges = ranges
        .into_iter()
        .filter_map(|candidate| folding_range(&index, candidate))
        .collect::<Vec<_>>();
    ranges.sort_unstable_by_key(folding_range_sort_key);
    ranges.dedup();
    ranges
}

pub(crate) fn folding_ranges_from_rope(rope: Rope) -> Vec<FoldingRange> {
    if is_single_line(&rope) {
        return Vec::new();
    }
    folding_ranges(crate::utils::rope_to_string(&rope))
}

fn is_single_line(rope: &Rope) -> bool {
    // LSP counts lone CRs and a trailing empty line, unlike Rope's line metric.
    rope.line_len() <= 1 && rope.chunks().all(|chunk| !chunk.contains(['\r', '\n']))
}

/// Collects comments without tokenizing the code that the parser has already consumed.
///
/// Only slashes and quotes can begin a comment or a string that hides comment delimiters. The
/// lexer still determines their exact ends, including escapes and unterminated strings.
fn collect_comment_ranges(source: &str) -> Vec<Candidate> {
    let mut ranges = Vec::new();
    let mut line_group = None::<ByteRange<usize>>;
    let mut cursor = 0;
    while let Some(offset) = memchr::memchr3(b'/', b'\'', b'"', &source.as_bytes()[cursor..]) {
        let start = cursor + offset;
        let token = Cursor::new(&source[start..]).next().unwrap();
        let end = start + token.len as usize;
        match token.kind {
            RawTokenKind::LineComment { .. } => {
                if let Some(group) = &mut line_group
                    && has_one_line_break(&source[group.end..start])
                {
                    group.end = end;
                } else {
                    flush_line_comment_group(&mut ranges, &mut line_group);
                    line_group = Some(start..end);
                }
            }
            RawTokenKind::BlockComment { .. } => {
                flush_line_comment_group(&mut ranges, &mut line_group);
                ranges.push(Candidate { range: start..end, kind: Some(FoldingRangeKind::Comment) });
            }
            _ => {}
        }
        cursor = end;
    }
    flush_line_comment_group(&mut ranges, &mut line_group);
    ranges
}

fn collect_lexical_info(source: &str, include_fallback: bool) -> LexicalInfo {
    if !include_fallback {
        return LexicalInfo {
            ranges: collect_comment_ranges(source),
            fallback_ranges: Vec::new(),
            unclosed_braces: Vec::new(),
        };
    }
    let mut ranges = Vec::new();
    let mut fallback_ranges = Vec::new();
    let mut line_group = None::<ByteRange<usize>>;
    let mut brace_stack = Vec::new();
    let mut syntax_tokens = Vec::new();

    for (start, token) in Cursor::new(source).with_position() {
        let end = start + token.len as usize;
        if !matches!(token.kind, RawTokenKind::LineComment { .. } | RawTokenKind::Whitespace) {
            flush_line_comment_group(&mut ranges, &mut line_group);
        }
        match token.kind {
            RawTokenKind::LineComment { .. } => {
                if let Some(group) = &mut line_group
                    && has_one_line_break(&source[group.end..start])
                {
                    group.end = end;
                } else {
                    flush_line_comment_group(&mut ranges, &mut line_group);
                    line_group = Some(start..end);
                }
            }
            RawTokenKind::Whitespace => {}
            RawTokenKind::BlockComment { .. } => {
                ranges.push(Candidate { range: start..end, kind: Some(FoldingRangeKind::Comment) });
            }
            RawTokenKind::OpenDelim(Delimiter::Brace) if include_fallback => {
                let class = classify_fallback_block(source, &syntax_tokens, &brace_stack);
                brace_stack.push(OpenBrace { start, class });
                syntax_tokens.push(SyntaxToken {
                    kind: token.kind,
                    range: start..end,
                    closes_yul_for_init: false,
                });
            }
            RawTokenKind::CloseDelim(Delimiter::Brace) if include_fallback => {
                let closed_block = brace_stack.pop().and_then(|open| {
                    if let Some(class) = open.class {
                        fallback_ranges.push(class.candidate(open.start, end));
                    }
                    open.class
                });
                syntax_tokens.push(SyntaxToken {
                    kind: token.kind,
                    range: start..end,
                    closes_yul_for_init: closed_block
                        .is_some_and(|block| matches!(block.kind, FallbackBlockKind::YulForInit)),
                });
            }
            _ if include_fallback => {
                syntax_tokens.push(SyntaxToken {
                    kind: token.kind,
                    range: start..end,
                    closes_yul_for_init: false,
                });
            }
            _ => {}
        }
    }
    flush_line_comment_group(&mut ranges, &mut line_group);
    let unclosed_braces = if include_fallback {
        fallback_ranges.extend(collect_fallback_import_ranges(source, &syntax_tokens));
        let unclosed_braces = brace_stack.iter().map(|brace| brace.start).collect();
        for open in brace_stack {
            if let Some(class) = open.class {
                fallback_ranges.push(class.candidate(open.start, source.len()));
            }
        }
        fallback_ranges.sort_unstable_by_key(|candidate| {
            (candidate.range.start, Reverse(candidate.range.end))
        });
        fallback_ranges.dedup_by_key(|candidate| candidate.range.start);
        unclosed_braces
    } else {
        Vec::new()
    };
    LexicalInfo { ranges, fallback_ranges, unclosed_braces }
}

fn flush_line_comment_group(
    ranges: &mut Vec<Candidate>,
    line_group: &mut Option<ByteRange<usize>>,
) {
    if let Some(range) = line_group.take() {
        ranges.push(Candidate { range, kind: Some(FoldingRangeKind::Comment) });
    }
}

fn collect_fallback_import_ranges(source: &str, tokens: &[SyntaxToken]) -> Vec<Candidate> {
    let mut ranges = Vec::new();
    let mut current = None::<ByteRange<usize>>;
    let mut start = None;
    let mut brace_depth = 0usize;

    for (index, token) in tokens.iter().enumerate() {
        if start.is_none()
            && brace_depth == 0
            && token.kind == RawTokenKind::Ident
            && source[token.range.clone()] == *"import"
            && fallback_import_starts_item(source, tokens[..index].last(), token)
        {
            start = Some(token.range.start);
        }

        match token.kind {
            RawTokenKind::OpenDelim(Delimiter::Brace) => brace_depth += 1,
            RawTokenKind::CloseDelim(Delimiter::Brace) => {
                brace_depth = brace_depth.saturating_sub(1);
            }
            RawTokenKind::Semi if brace_depth == 0 => {
                if let Some(start) = start.take() {
                    let import = start..token.range.end;
                    let split = current.as_ref().is_some_and(|group| {
                        let between = &source[group.end..import.start];
                        has_blank_line_between(between.bytes())
                            || Cursor::new(between).any(|token| !token.kind.is_trivial())
                    });
                    if split && let Some(range) = current.take() {
                        ranges.push(Candidate { range, kind: Some(FoldingRangeKind::Imports) });
                    }
                    if let Some(group) = &mut current {
                        group.end = import.end;
                    } else {
                        current = Some(import);
                    }
                }
            }
            _ => {}
        }
    }

    if let Some(range) = current {
        ranges.push(Candidate { range, kind: Some(FoldingRangeKind::Imports) });
    }
    ranges
}

fn fallback_import_starts_item(
    source: &str,
    previous: Option<&SyntaxToken>,
    import: &SyntaxToken,
) -> bool {
    let Some(previous) = previous else { return true };
    if previous.kind == RawTokenKind::Dot {
        return false;
    }
    matches!(previous.kind, RawTokenKind::Semi | RawTokenKind::CloseDelim(Delimiter::Brace))
        || source[previous.range.end..import.range.start]
            .bytes()
            .any(|byte| matches!(byte, b'\r' | b'\n'))
}

fn classify_fallback_block(
    source: &str,
    tokens: &[SyntaxToken],
    brace_stack: &[OpenBrace],
) -> Option<FallbackBlock> {
    if let Some(block) = classify_yul_for_continuation(tokens) {
        return Some(block);
    }

    let parent = brace_stack.last().and_then(|brace| brace.class);
    let declaration_is_allowed = brace_stack.is_empty() || parent.is_some();
    let mut parenthesis_depth = 0;
    let mut bracket_depth = 0;
    let mut declaration = None;
    let mut body = None;

    for (index, token) in tokens.iter().enumerate().rev() {
        match token.kind {
            RawTokenKind::CloseDelim(Delimiter::Parenthesis) => {
                parenthesis_depth += 1;
                continue;
            }
            RawTokenKind::OpenDelim(Delimiter::Parenthesis) => {
                if parenthesis_depth == 0 {
                    return None;
                }
                parenthesis_depth -= 1;
                continue;
            }
            RawTokenKind::CloseDelim(Delimiter::Bracket) => {
                bracket_depth += 1;
                continue;
            }
            RawTokenKind::OpenDelim(Delimiter::Bracket) => {
                if bracket_depth == 0 {
                    return None;
                }
                bracket_depth -= 1;
                continue;
            }
            RawTokenKind::Semi
            | RawTokenKind::OpenDelim(Delimiter::Brace)
            | RawTokenKind::CloseDelim(Delimiter::Brace)
                if parenthesis_depth == 0 && bracket_depth == 0 =>
            {
                break;
            }
            _ => {}
        }
        if parenthesis_depth != 0 || bracket_depth != 0 {
            continue;
        }
        if token.kind != RawTokenKind::Ident {
            continue;
        }

        let text = &source[token.range.clone()];
        if matches!(text, "import" | "using") {
            return None;
        }
        let is_function_type = text == "function"
            && tokens
                .get(index + 1)
                .is_some_and(|token| token.kind == RawTokenKind::OpenDelim(Delimiter::Parenthesis));
        if declaration_is_allowed
            && !is_function_type
            && matches!(
                text,
                "contract"
                    | "interface"
                    | "library"
                    | "function"
                    | "constructor"
                    | "fallback"
                    | "receive"
                    | "modifier"
                    | "struct"
                    | "enum"
            )
        {
            let start = if text == "contract"
                && index > 0
                && source[tokens[index - 1].range.clone()] == *"abstract"
            {
                tokens[index - 1].range.start
            } else {
                token.range.start
            };
            let language = if text == "function"
                && parent.is_some_and(|parent| parent.language == BlockLanguage::Yul)
            {
                BlockLanguage::Yul
            } else {
                BlockLanguage::Solidity
            };
            declaration = Some(FallbackBlock::declaration(start, language));
            continue;
        }
        if body.is_none()
            && let Some(parent) = parent
            && matches!(
                text,
                "if" | "else"
                    | "for"
                    | "while"
                    | "do"
                    | "try"
                    | "catch"
                    | "unchecked"
                    | "assembly"
                    | "case"
                    | "default"
            )
            && fallback_body_is_well_formed(source, tokens, text, parent.language)
        {
            let language = if text == "assembly" { BlockLanguage::Yul } else { parent.language };
            body = Some(if text == "for" && language == BlockLanguage::Yul {
                FallbackBlock::yul_for_init()
            } else {
                FallbackBlock::body(language)
            });
        }
    }

    declaration.or(body).or_else(|| {
        let parent = parent?;
        if parent.language == BlockLanguage::Yul {
            return Some(FallbackBlock::body(BlockLanguage::Yul));
        }
        tokens
            .last()
            .is_some_and(|token| {
                matches!(
                    token.kind,
                    RawTokenKind::Semi
                        | RawTokenKind::OpenDelim(Delimiter::Brace)
                        | RawTokenKind::CloseDelim(Delimiter::Brace)
                )
            })
            .then_some(FallbackBlock::body(parent.language))
    })
}

fn classify_yul_for_continuation(tokens: &[SyntaxToken]) -> Option<FallbackBlock> {
    for token in tokens.iter().rev() {
        match token.kind {
            RawTokenKind::Semi | RawTokenKind::OpenDelim(Delimiter::Brace) => return None,
            RawTokenKind::CloseDelim(Delimiter::Brace) => {
                return token.closes_yul_for_init.then(|| FallbackBlock::body(BlockLanguage::Yul));
            }
            _ => {}
        }
    }
    None
}

fn fallback_body_is_well_formed(
    source: &str,
    tokens: &[SyntaxToken],
    keyword: &str,
    parent_language: BlockLanguage,
) -> bool {
    let last = tokens.last();
    let last_is_close_parenthesis =
        last.is_some_and(|token| token.kind == RawTokenKind::CloseDelim(Delimiter::Parenthesis));
    let last_identifier = last
        .filter(|token| token.kind == RawTokenKind::Ident)
        .map(|token| &source[token.range.clone()]);

    match keyword {
        "if" | "for" | "while" if parent_language == BlockLanguage::Solidity => {
            last_is_close_parenthesis
        }
        "if" | "for" | "while" => true,
        "else" | "do" | "unchecked" => last_identifier == Some(keyword),
        "try" => last_is_close_parenthesis,
        "catch" => last_is_close_parenthesis || last_identifier == Some(keyword),
        "assembly" => parent_language == BlockLanguage::Solidity,
        "case" | "default" => parent_language == BlockLanguage::Yul,
        _ => false,
    }
}

fn has_one_line_break(text: &str) -> bool {
    let mut line_breaks = 0;
    let mut bytes = text.bytes().peekable();
    while let Some(byte) = bytes.next() {
        match byte {
            b'\r' => {
                if bytes.peek() == Some(&b'\n') {
                    bytes.next();
                }
                line_breaks += 1;
                if line_breaks > 1 {
                    return false;
                }
            }
            b'\n' => {
                line_breaks += 1;
                if line_breaks > 1 {
                    return false;
                }
            }
            b' ' | b'\t' | 0x0b | 0x0c => {}
            _ => return false,
        }
    }
    line_breaks == 1
}

fn collect_ranges(source: Arc<String>) -> Option<(Arc<SourceFile>, Vec<Candidate>)> {
    let mut opts = CompileOpts::default();
    opts.unstable.recover_incomplete_input = true;
    let sess = Session::builder().opts(opts).with_silent_emitter(None).single_threaded().build();

    sess.enter_sequential(|| {
        let arena = ast::Arena::new();
        let file = sess
            .source_map()
            .new_source_file_shared(FileName::Custom("lsp-folding-range.sol".into()), source)
            .ok()?;
        let mut parser = Parser::from_source_file(&sess, &arena, &file);
        let source_unit = match parser.parse_file() {
            Ok(source_unit) => Some(source_unit),
            Err(error) => {
                error.emit();
                None
            }
        };
        drop(parser);

        let has_errors = sess.dcx.has_errors().is_err();
        let include_fallback = has_errors || source_unit.is_none();
        let LexicalInfo { mut ranges, fallback_ranges, unclosed_braces } =
            collect_lexical_info(&file.src, include_fallback);
        let Some(source_unit) = source_unit else {
            ranges.extend(fallback_ranges);
            return Some((file, ranges));
        };

        let mut collector = AstRangeCollector::new(&file, &unclosed_braces);
        let _ = collector.visit_source_unit(&source_unit);
        let mut ast_ranges = collect_import_ranges(&source_unit, &file);
        ast_ranges.extend(collector.ranges);

        if has_errors {
            let mut ast_ends = FxHashMap::<(u8, usize), usize>::default();
            for ast in &ast_ranges {
                ast_ends
                    .entry((folding_range_kind_rank(ast.kind.as_ref()), ast.range.start))
                    .and_modify(|end| *end = (*end).max(ast.range.end))
                    .or_insert(ast.range.end);
            }
            ranges.extend(fallback_ranges.into_iter().filter(|candidate| {
                ast_ends
                    .get(&(folding_range_kind_rank(candidate.kind.as_ref()), candidate.range.start))
                    .is_none_or(|&end| candidate.range.end > end)
            }));
        }
        ranges.extend(ast_ranges);
        Some((file, ranges))
    })
}

fn collect_import_ranges(source_unit: &ast::SourceUnit<'_>, file: &SourceFile) -> Vec<Candidate> {
    let mut ranges = Vec::new();
    let mut current = None::<ByteRange<usize>>;

    for item in source_unit.items.iter() {
        if !matches!(item.kind, ast::ItemKind::Import(_)) {
            if let Some(range) = current.take() {
                ranges.push(Candidate { range, kind: Some(FoldingRangeKind::Imports) });
            }
            continue;
        }

        let Some(range) = checked_span_range(file, item.span) else {
            if let Some(range) = current.take() {
                ranges.push(Candidate { range, kind: Some(FoldingRangeKind::Imports) });
            }
            continue;
        };
        let split = current
            .as_ref()
            .is_some_and(|group| has_blank_line_between(file.src[group.end..range.start].bytes()));
        if split && let Some(range) = current.take() {
            ranges.push(Candidate { range, kind: Some(FoldingRangeKind::Imports) });
        }
        if let Some(group) = &mut current {
            group.end = range.end;
        } else {
            current = Some(range);
        }
    }
    if let Some(range) = current {
        ranges.push(Candidate { range, kind: Some(FoldingRangeKind::Imports) });
    }
    ranges
}

fn has_blank_line_between(bytes: impl IntoIterator<Item = u8>) -> bool {
    let mut bytes = bytes.into_iter().peekable();
    let mut saw_line_break = false;
    let mut line_has_content = false;
    while let Some(byte) = bytes.next() {
        let is_line_break = match byte {
            b'\r' => {
                if bytes.peek() == Some(&b'\n') {
                    bytes.next();
                }
                true
            }
            b'\n' => true,
            b' ' | b'\t' | 0x0b | 0x0c => false,
            _ => {
                line_has_content = true;
                false
            }
        };
        if is_line_break {
            if saw_line_break && !line_has_content {
                return true;
            }
            saw_line_break = true;
            line_has_content = false;
        }
    }
    false
}

/// One-way source positions for the batch of ranges collected from a flat parser input.
///
/// Range construction already owns contiguous source text. Looking up byte boundaries and line
/// endings directly avoids repeatedly descending the editor rope for every range endpoint.
struct FlatPositionIndex<'a> {
    source: &'a str,
    line_starts: Vec<usize>,
    /// Character ends and cumulative UTF-8 bytes beyond their UTF-16 code-unit count.
    multibyte_offsets: Vec<(usize, usize)>,
}

impl<'a> FlatPositionIndex<'a> {
    fn new(source: &'a str, file: Option<&SourceFile>) -> Self {
        let mut line_starts = vec![0];
        let mut previous_cr_end = None;
        for offset in memchr::memchr2_iter(b'\r', b'\n', source.as_bytes()) {
            let is_cr = source.as_bytes()[offset] == b'\r';
            if !is_cr && previous_cr_end == Some(offset) {
                *line_starts.last_mut().unwrap() = offset + 1;
            } else {
                line_starts.push(offset + 1);
            }
            previous_cr_end = is_cr.then_some(offset + 1);
        }
        let mut multibyte_offsets = Vec::new();
        let mut extra_bytes = 0;
        if let Some(file) = file {
            for ch in &file.multibyte_chars {
                let bytes = usize::from(ch.bytes);
                extra_bytes += bytes - if bytes == 4 { 2 } else { 1 };
                multibyte_offsets.push((ch.pos.to_usize() + bytes, extra_bytes));
            }
        } else {
            for (line, &start) in line_starts.iter().enumerate() {
                let end = line_starts.get(line + 1).copied().unwrap_or(source.len());
                let contents = &source[start..end];
                if !contents.is_ascii() {
                    for (byte, ch) in contents.char_indices() {
                        if !ch.is_ascii() {
                            extra_bytes += ch.len_utf8() - ch.len_utf16();
                            multibyte_offsets.push((start + byte + ch.len_utf8(), extra_bytes));
                        }
                    }
                }
            }
        }
        Self { source, line_starts, multibyte_offsets }
    }

    fn line_at_byte(&self, byte: usize) -> Option<usize> {
        if !self.source.is_char_boundary(byte) {
            return None;
        }
        let line = self.line_starts.partition_point(|&start| start <= byte).checked_sub(1)?;
        if let Some(&next_start) = self.line_starts.get(line + 1) {
            let bytes = self.source.as_bytes();
            let end = if bytes[next_start - 1] == b'\n'
                && next_start >= 2
                && bytes[next_start - 2] == b'\r'
            {
                next_start - 2
            } else {
                next_start - 1
            };
            if byte > end {
                return None;
            }
        }
        Some(line)
    }

    fn position_in_line(&self, line: usize, byte: usize) -> Option<Position> {
        let start = self.line_starts[line];
        let character = byte - start - (self.extra_bytes(byte) - self.extra_bytes(start));
        Some(Position::new(u32::try_from(line).ok()?, u32::try_from(character).ok()?))
    }

    fn extra_bytes(&self, byte: usize) -> usize {
        let end = self.multibyte_offsets.partition_point(|&(end, _)| end <= byte);
        end.checked_sub(1).map_or(0, |index| self.multibyte_offsets[index].1)
    }
}

fn folding_range(index: &FlatPositionIndex<'_>, candidate: Candidate) -> Option<FoldingRange> {
    let start_line = index.line_at_byte(candidate.range.start)?;
    let end_line = index.line_at_byte(candidate.range.end)?;
    if start_line >= end_line {
        return None;
    }
    let start = index.position_in_line(start_line, candidate.range.start)?;
    let end = index.position_in_line(end_line, candidate.range.end)?;
    Some(FoldingRange {
        start_line: start.line,
        start_character: Some(start.character),
        end_line: end.line,
        end_character: Some(end.character),
        kind: candidate.kind,
        collapsed_text: None,
    })
}

fn folding_range_sort_key(range: &FoldingRange) -> (u32, u32, u32, u32, u8) {
    (
        range.start_line,
        range.start_character.unwrap_or_default(),
        range.end_line,
        range.end_character.unwrap_or_default(),
        folding_range_kind_rank(range.kind.as_ref()),
    )
}

fn folding_range_kind_rank(kind: Option<&FoldingRangeKind>) -> u8 {
    match kind {
        None => 0,
        Some(FoldingRangeKind::Comment) => 1,
        Some(FoldingRangeKind::Imports) => 2,
        Some(FoldingRangeKind::Region) => 3,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Candidate {
    range: ByteRange<usize>,
    kind: Option<FoldingRangeKind>,
}

struct LexicalInfo {
    ranges: Vec<Candidate>,
    fallback_ranges: Vec<Candidate>,
    unclosed_braces: Vec<usize>,
}

#[derive(Clone, Copy)]
struct OpenBrace {
    start: usize,
    class: Option<FallbackBlock>,
}

#[derive(Clone)]
struct SyntaxToken {
    kind: RawTokenKind,
    range: ByteRange<usize>,
    closes_yul_for_init: bool,
}

#[derive(Clone, Copy)]
struct FallbackBlock {
    kind: FallbackBlockKind,
    language: BlockLanguage,
}

#[derive(Clone, Copy)]
enum FallbackBlockKind {
    Declaration(usize),
    Body,
    YulForInit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlockLanguage {
    Solidity,
    Yul,
}

impl FallbackBlock {
    fn declaration(start: usize, language: BlockLanguage) -> Self {
        Self { kind: FallbackBlockKind::Declaration(start), language }
    }

    fn body(language: BlockLanguage) -> Self {
        Self { kind: FallbackBlockKind::Body, language }
    }

    fn yul_for_init() -> Self {
        Self { kind: FallbackBlockKind::YulForInit, language: BlockLanguage::Yul }
    }

    fn candidate(self, brace: usize, end: usize) -> Candidate {
        let start = match self.kind {
            FallbackBlockKind::Declaration(start) => start,
            FallbackBlockKind::Body | FallbackBlockKind::YulForInit => brace,
        };
        Candidate { range: start..end, kind: None }
    }
}

struct AstRangeCollector<'a> {
    file: &'a SourceFile,
    ranges: Vec<Candidate>,
    suppressed_block: Option<Span>,
    unclosed_braces: &'a [usize],
}

impl<'a> AstRangeCollector<'a> {
    fn new(file: &'a SourceFile, unclosed_braces: &'a [usize]) -> Self {
        Self { file, ranges: Vec::new(), suppressed_block: None, unclosed_braces }
    }

    fn push(&mut self, span: Span) {
        if let Some(range) = checked_span_range(self.file, span) {
            self.ranges.push(Candidate { range, kind: None });
        }
    }

    fn push_block(&mut self, span: Span) {
        let Some(mut range) = checked_span_range(self.file, span) else {
            return;
        };
        if self.unclosed_braces.binary_search(&range.start).is_ok() {
            range.end = self.file.src.len();
        }
        self.ranges.push(Candidate { range, kind: None });
    }

    fn push_braced_declaration(&mut self, span: Span, body: Option<Span>) {
        let Some(mut range) = checked_span_range(self.file, span) else {
            return;
        };
        let brace = body
            .and_then(|body| checked_span_range(self.file, body))
            .map(|body| body.start)
            .or_else(|| {
                self.unclosed_braces
                    .iter()
                    .copied()
                    .find(|&brace| range.start <= brace && brace < range.end)
            });
        if brace.is_some_and(|brace| self.unclosed_braces.binary_search(&brace).is_ok()) {
            range.end = self.file.src.len();
        }
        self.ranges.push(Candidate { range, kind: None });
    }

    fn suppress_block(&mut self, span: Span, f: impl FnOnce(&mut Self)) {
        let previous = self.suppressed_block.replace(span);
        f(self);
        self.suppressed_block = previous;
    }

    fn block_is_suppressed(&self, span: Span) -> bool {
        self.suppressed_block == Some(span)
    }
}

fn checked_span_range(file: &SourceFile, span: Span) -> Option<ByteRange<usize>> {
    if span.is_dummy()
        || span.lo() >= span.hi()
        || !file.contains(span.lo())
        || !file.contains(span.hi())
    {
        return None;
    }
    let range =
        file.relative_position(span.lo()).to_usize()..file.relative_position(span.hi()).to_usize();
    (file.src.is_char_boundary(range.start) && file.src.is_char_boundary(range.end))
        .then_some(range)
}

impl<'ast> Visit<'ast> for AstRangeCollector<'_> {
    type BreakValue = Never;

    fn visit_item(&mut self, item: &'ast ast::Item<'ast>) -> ControlFlow<Self::BreakValue> {
        if item.name().is_some() || matches!(item.kind, ast::ItemKind::Function(_)) {
            match &item.kind {
                ast::ItemKind::Contract(_) | ast::ItemKind::Struct(_) | ast::ItemKind::Enum(_) => {
                    self.push_braced_declaration(item.span, None);
                }
                ast::ItemKind::Function(function) => {
                    self.push_braced_declaration(
                        item.span,
                        function.body.as_ref().map(|body| body.span),
                    );
                }
                _ => self.push(item.span),
            }
        }

        if let ast::ItemKind::Function(function) = &item.kind
            && let Some(body) = &function.body
        {
            self.suppress_block(body.span, |this| {
                let _ = this.walk_item(item);
            });
            ControlFlow::Continue(())
        } else {
            self.walk_item(item)
        }
    }

    fn visit_block(&mut self, block: &'ast ast::Block<'ast>) -> ControlFlow<Self::BreakValue> {
        if !self.block_is_suppressed(block.span) {
            self.push_block(block.span);
        }
        self.walk_block(block)
    }

    fn visit_yul_stmt(
        &mut self,
        statement: &'ast ast::yul::Stmt<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        if let ast::yul::StmtKind::FunctionDef(function) = &statement.kind {
            self.push_braced_declaration(statement.span, Some(function.body.span));
            self.suppress_block(function.body.span, |this| {
                let _ = this.walk_yul_stmt(statement);
            });
            ControlFlow::Continue(())
        } else {
            self.walk_yul_stmt(statement)
        }
    }

    fn visit_yul_block(
        &mut self,
        block: &'ast ast::yul::Block<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        if !self.block_is_suppressed(block.span) {
            self.push_block(block.span);
        }
        self.walk_yul_block(block)
    }
}

#[cfg(test)]
mod position_tests {
    use super::{
        FlatPositionIndex, checked_span_range, collect_comment_ranges, collect_lexical_info,
    };
    use crate::proto::LspPositionIndex;
    use crop::Rope;
    use solar_interface::{BytePos, SourceMap, Span, source_map::FileName};

    #[test]
    fn sparse_comments_match_full_lexing() {
        let fragments = [
            "",
            " ",
            "\t",
            "\n",
            "\r",
            "\r\n",
            "\n\n",
            "x /= 2;",
            "/* block */",
            "/* open",
            "// line",
            "/// doc\n",
            "\"// string\"",
            "'/* string */'",
            "unicode\"中😀//\"",
            "hex\"abcd\"",
            "\"escaped\\\"//\"",
            "\"open\\\n",
            "/",
            "*",
            "\\",
            "'",
        ];
        for left in fragments {
            for middle in fragments {
                for right in fragments {
                    let source = format!("{left}{middle}{right}");
                    assert_eq!(
                        collect_comment_ranges(&source),
                        collect_lexical_info(&source, true).ranges,
                        "{source:?}",
                    );
                }
            }
        }
    }

    #[test]
    fn flat_positions_match_editor_positions() {
        for text in ["", "plain", "é", "中", "😀", "aé中😀z"] {
            for ending in ["", "\n", "\r", "\r\n", "\r\n\r\n", "\r\r\n\n"] {
                for repeat in [1, 400] {
                    let source = format!("{text}{ending}").repeat(repeat);
                    let flat = FlatPositionIndex::new(&source, None);
                    let source_map = SourceMap::empty();
                    let file = source_map
                        .new_source_file(FileName::Custom("input".into()), source.clone())
                        .unwrap();
                    let parsed = FlatPositionIndex::new(&source, Some(&file));
                    let rope = Rope::from(source.as_str());
                    let editor = LspPositionIndex::new(&rope);
                    for byte in 0..=source.len() + 1 {
                        let line = flat.line_at_byte(byte);
                        assert_eq!(line, editor.line_at_byte(byte), "{text:?} {ending:?} {byte}");
                        assert_eq!(
                            line.and_then(|line| flat.position_in_line(line, byte)),
                            editor.position_at_byte(byte),
                            "{text:?} {ending:?} {byte}",
                        );
                        assert_eq!(
                            line.and_then(|line| parsed.position_in_line(line, byte)),
                            editor.position_at_byte(byte),
                            "{text:?} {ending:?} {byte}",
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn spans_are_checked_against_the_parsed_file() {
        let source_map = SourceMap::empty();
        let first = source_map.new_source_file(FileName::Custom("first".into()), "other").unwrap();
        let file = source_map.new_source_file(FileName::Custom("input".into()), "aé😀z").unwrap();
        let span = |start, end| {
            Span::new(
                file.start_pos + BytePos::from_usize(start),
                file.start_pos + BytePos::from_usize(end),
            )
        };
        assert_eq!(checked_span_range(&file, span(1, 7)), Some(1..7));
        assert_eq!(checked_span_range(&file, span(0, file.src.len())), Some(0..file.src.len()));
        for span in [
            Span::DUMMY,
            span(1, 1),
            span(2, 7),
            span(1, 6),
            span(0, file.src.len() + 1),
            Span::new(first.start_pos, first.end_position()),
            Span::new(first.start_pos, file.end_position()),
        ] {
            assert_eq!(checked_span_range(&file, span), None);
        }
    }
}

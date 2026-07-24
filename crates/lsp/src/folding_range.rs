//! Syntax-based folding-range construction.

use crate::proto;
use crop::Rope;
use lsp_types::{FoldingRange, FoldingRangeKind};
use solar_config::CompileOpts;
use solar_interface::{Session, SourceMap, Span, data_structures::Never, source_map::FileName};
use solar_parse::{
    Cursor, Parser,
    ast::{self, token::Delimiter, visit::Visit},
    lexer::token::RawTokenKind,
};
use std::{
    cmp::Reverse,
    ops::{ControlFlow, Range as ByteRange},
};

pub(crate) fn folding_ranges(source: String) -> Vec<FoldingRange> {
    let rope = Rope::from(source.as_str());
    let index = proto::LspPositionIndex::new(&rope);
    let LexicalInfo { mut ranges, fallback_ranges, unclosed_braces } =
        collect_lexical_info(&source);
    match collect_ast_ranges(source, &rope, &unclosed_braces) {
        Some(info) => {
            if info.has_errors {
                ranges.extend(fallback_ranges.into_iter().filter(|candidate| {
                    !info.ranges.iter().any(|ast| {
                        ast.kind == candidate.kind
                            && ast.range.start == candidate.range.start
                            && candidate.range.end <= ast.range.end
                    })
                }));
            }
            ranges.extend(info.ranges);
        }
        None => ranges.extend(fallback_ranges),
    }
    let mut ranges = ranges
        .into_iter()
        .filter_map(|candidate| folding_range(&index, candidate))
        .collect::<Vec<_>>();
    ranges.sort_unstable_by_key(folding_range_sort_key);
    ranges.dedup();
    ranges
}

fn collect_lexical_info(source: &str) -> LexicalInfo {
    let mut ranges = Vec::new();
    let mut fallback_ranges = Vec::new();
    let mut line_group = None::<ByteRange<usize>>;
    let mut brace_stack = Vec::new();
    let mut syntax_tokens = Vec::new();

    for (start, token) in Cursor::new(source).with_position() {
        let end = start + token.len as usize;
        match token.kind {
            RawTokenKind::LineComment { .. } => {
                if let Some(group) = &mut line_group
                    && has_one_line_break(&source[group.end..start])
                {
                    group.end = end;
                } else {
                    if let Some(range) = line_group.take() {
                        ranges.push(Candidate { range, kind: Some(FoldingRangeKind::Comment) });
                    }
                    line_group = Some(start..end);
                }
            }
            RawTokenKind::Whitespace => {}
            RawTokenKind::BlockComment { .. } => {
                if let Some(range) = line_group.take() {
                    ranges.push(Candidate { range, kind: Some(FoldingRangeKind::Comment) });
                }
                ranges.push(Candidate { range: start..end, kind: Some(FoldingRangeKind::Comment) });
            }
            RawTokenKind::OpenDelim(Delimiter::Brace) => {
                if let Some(range) = line_group.take() {
                    ranges.push(Candidate { range, kind: Some(FoldingRangeKind::Comment) });
                }
                let class = classify_fallback_block(source, &syntax_tokens, &brace_stack);
                brace_stack.push(OpenBrace { start, class });
                syntax_tokens.push(SyntaxToken {
                    kind: token.kind,
                    range: start..end,
                    closed_block: None,
                });
            }
            RawTokenKind::CloseDelim(Delimiter::Brace) => {
                if let Some(range) = line_group.take() {
                    ranges.push(Candidate { range, kind: Some(FoldingRangeKind::Comment) });
                }
                let closed_block = brace_stack.pop().and_then(|open| {
                    if let Some(class) = open.class {
                        fallback_ranges.push(class.candidate(open.start, end));
                    }
                    open.class
                });
                syntax_tokens.push(SyntaxToken {
                    kind: token.kind,
                    range: start..end,
                    closed_block,
                });
            }
            _ => {
                if let Some(range) = line_group.take() {
                    ranges.push(Candidate { range, kind: Some(FoldingRangeKind::Comment) });
                }
                syntax_tokens.push(SyntaxToken {
                    kind: token.kind,
                    range: start..end,
                    closed_block: None,
                });
            }
        }
    }
    if let Some(range) = line_group {
        ranges.push(Candidate { range, kind: Some(FoldingRangeKind::Comment) });
    }
    fallback_ranges.extend(collect_fallback_import_ranges(source, &syntax_tokens));
    let unclosed_braces = brace_stack.iter().map(|brace| brace.start).collect();
    for open in brace_stack {
        if let Some(class) = open.class {
            fallback_ranges.push(class.candidate(open.start, source.len()));
        }
    }
    fallback_ranges
        .sort_unstable_by_key(|candidate| (candidate.range.start, Reverse(candidate.range.end)));
    fallback_ranges.dedup_by_key(|candidate| candidate.range.start);
    LexicalInfo { ranges, fallback_ranges, unclosed_braces }
}

fn collect_fallback_import_ranges(source: &str, tokens: &[SyntaxToken]) -> Vec<Candidate> {
    let mut imports = Vec::new();
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
                    imports.push(start..token.range.end);
                }
            }
            _ => {}
        }
    }

    let mut ranges = Vec::new();
    let mut current = None::<ByteRange<usize>>;
    for import in imports {
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
                return match token.closed_block.map(|block| block.kind) {
                    Some(FallbackBlockKind::YulForInit) => {
                        Some(FallbackBlock::body(BlockLanguage::Yul))
                    }
                    _ => None,
                };
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
            }
            b'\n' => line_breaks += 1,
            b' ' | b'\t' | 0x0b | 0x0c => {}
            _ => return false,
        }
    }
    line_breaks == 1
}

fn collect_ast_ranges(source: String, rope: &Rope, unclosed_braces: &[usize]) -> Option<AstInfo> {
    let mut opts = CompileOpts::default();
    opts.unstable.recover_incomplete_input = true;
    let sess = Session::builder().opts(opts).with_silent_emitter(None).single_threaded().build();

    sess.enter_sequential(|| {
        let arena = ast::Arena::new();
        let Ok(mut parser) = Parser::from_source_code(
            &sess,
            &arena,
            FileName::Custom("lsp-folding-range.sol".into()),
            source,
        ) else {
            return None;
        };
        let source_unit = match parser.parse_file() {
            Ok(source_unit) => source_unit,
            Err(error) => {
                error.emit();
                return None;
            }
        };
        drop(parser);

        let mut collector = AstRangeCollector::new(sess.source_map(), rope, unclosed_braces);
        let _ = collector.visit_source_unit(&source_unit);
        let mut ranges = collect_import_ranges(&source_unit, sess.source_map(), rope);
        ranges.extend(collector.ranges);
        Some(AstInfo { ranges, has_errors: sess.dcx.has_errors().is_err() })
    })
}

fn collect_import_ranges(
    source_unit: &ast::SourceUnit<'_>,
    source_map: &SourceMap,
    rope: &Rope,
) -> Vec<Candidate> {
    let mut ranges = Vec::new();
    let mut current = None::<ByteRange<usize>>;

    for item in source_unit.items.iter() {
        if !matches!(item.kind, ast::ItemKind::Import(_)) {
            if let Some(range) = current.take() {
                ranges.push(Candidate { range, kind: Some(FoldingRangeKind::Imports) });
            }
            continue;
        }

        let Some(range) = checked_span_range(source_map, rope, item.span) else {
            if let Some(range) = current.take() {
                ranges.push(Candidate { range, kind: Some(FoldingRangeKind::Imports) });
            }
            continue;
        };
        let split = current.as_ref().is_some_and(|group| {
            has_blank_line_between(rope.byte_slice(group.end..range.start).bytes())
        });
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

fn folding_range(
    index: &proto::LspPositionIndex<'_>,
    candidate: Candidate,
) -> Option<FoldingRange> {
    let start = index.position_at_byte(candidate.range.start)?;
    let end = index.position_at_byte(candidate.range.end)?;
    if start.line >= end.line {
        return None;
    }
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
        match range.kind {
            None => 0,
            Some(FoldingRangeKind::Comment) => 1,
            Some(FoldingRangeKind::Imports) => 2,
            Some(FoldingRangeKind::Region) => 3,
        },
    )
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

struct AstInfo {
    ranges: Vec<Candidate>,
    has_errors: bool,
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
    closed_block: Option<FallbackBlock>,
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
    source_map: &'a SourceMap,
    rope: &'a Rope,
    ranges: Vec<Candidate>,
    suppressed_blocks: Vec<Span>,
    unclosed_braces: &'a [usize],
}

impl<'a> AstRangeCollector<'a> {
    fn new(source_map: &'a SourceMap, rope: &'a Rope, unclosed_braces: &'a [usize]) -> Self {
        Self {
            source_map,
            rope,
            ranges: Vec::new(),
            suppressed_blocks: Vec::new(),
            unclosed_braces,
        }
    }

    fn push(&mut self, span: Span) {
        if let Some(range) = checked_span_range(self.source_map, self.rope, span) {
            self.ranges.push(Candidate { range, kind: None });
        }
    }

    fn push_block(&mut self, span: Span) {
        let Some(mut range) = checked_span_range(self.source_map, self.rope, span) else { return };
        if self.unclosed_braces.binary_search(&range.start).is_ok() {
            range.end = self.rope.byte_len();
        }
        self.ranges.push(Candidate { range, kind: None });
    }

    fn push_braced_declaration(&mut self, span: Span, body: Option<Span>) {
        let Some(mut range) = checked_span_range(self.source_map, self.rope, span) else { return };
        let brace = body
            .and_then(|body| checked_span_range(self.source_map, self.rope, body))
            .map(|body| body.start)
            .or_else(|| {
                self.unclosed_braces
                    .iter()
                    .copied()
                    .find(|&brace| range.start <= brace && brace < range.end)
            });
        if brace.is_some_and(|brace| self.unclosed_braces.binary_search(&brace).is_ok()) {
            range.end = self.rope.byte_len();
        }
        self.ranges.push(Candidate { range, kind: None });
    }

    fn suppress_block(&mut self, span: Span, f: impl FnOnce(&mut Self)) {
        self.suppressed_blocks.push(span);
        f(self);
        self.suppressed_blocks.pop();
    }

    fn block_is_suppressed(&self, span: Span) -> bool {
        self.suppressed_blocks.last() == Some(&span)
    }
}

fn checked_span_range(source_map: &SourceMap, rope: &Rope, span: Span) -> Option<ByteRange<usize>> {
    if span.is_dummy() {
        return None;
    }
    let range = source_map.span_to_range(span).ok()?;
    (!range.is_empty()
        && range.end <= rope.byte_len()
        && rope.is_char_boundary(range.start)
        && rope.is_char_boundary(range.end))
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
mod tests {
    use super::*;
    use lsp_types::FoldingRangeKind;
    use snapbox::{assert_data_eq, str};
    use std::fmt::Write as _;

    #[test]
    fn folds_declarations_and_nested_solidity_blocks() {
        let source = concat!(
            "contract C {\n",
            "    function f() external {\n",
            "        if (true) {\n",
            "            {\n",
            "                uint256 x;\n",
            "            }\n",
            "        }\n",
            "    }\n",
            "}\n",
        );

        assert_data_eq!(
            folding_range_output(&folding_ranges(source.into())),
            str![[r#"
0:0-8:1 code
1:4-7:5 code
2:18-6:9 code
3:12-5:13 code

"#]],
        );
    }

    #[test]
    fn folds_full_multiline_named_declaration_ranges() {
        let source = concat!(
            "interface I {\n",
            "    event Changed(\n",
            "        uint256 value\n",
            "    );\n",
            "\n",
            "    function read(\n",
            "        uint256 key\n",
            "    ) external view returns (\n",
            "        uint256 value\n",
            "    );\n",
            "}\n",
        );

        assert_data_eq!(
            folding_range_output(&folding_ranges(source.into())),
            str![[r#"
0:0-10:1 code
1:4-3:6 code
5:4-9:6 code

"#]],
        );
    }

    #[test]
    fn folds_comments_at_every_nesting_level_and_splits_groups_on_blank_lines() {
        let source = concat!(
            "// alpha\n",
            "// beta\n",
            "\n",
            "/// gamma\n",
            "// delta\n",
            "contract C {\n",
            "    /* nested\n",
            "       block */\n",
            "    function f() external {\n",
            "        // inner\n",
            "        // group\n",
            "    }\n",
            "}\n",
        );

        assert_data_eq!(
            folding_range_output(&folding_ranges(source.into())),
            str![[r#"
0:0-1:7 comment
3:0-4:8 comment
5:0-12:1 code
6:4-7:15 comment
8:4-11:5 code
9:8-10:16 comment

"#]],
        );
    }

    #[test]
    fn folds_import_groups_and_splits_them_on_blank_lines_or_items() {
        let source = concat!(
            "import \"a.sol\";\n",
            "import {A} from \"b.sol\";\n",
            "// keep this group together\n",
            "import \"c.sol\";\n",
            "\n",
            "import \"d.sol\";\n",
            "import \"e.sol\";\n",
            "pragma solidity ^0.8.0;\n",
            "import \"f.sol\";\n",
            "import \"g.sol\";\n",
        );

        assert_data_eq!(
            folding_range_output(&folding_ranges(source.into())),
            str![[r#"
0:0-3:15 imports
5:0-6:15 imports
8:0-9:15 imports

"#]],
        );
    }

    #[test]
    fn falls_back_to_import_groups_after_parse_errors() {
        let source = concat!("@ invalid\n", "import \"a.sol\";\n", "import \"b.sol\";\n",);

        assert_data_eq!(
            folding_range_output(&folding_ranges(source.into())),
            str![[r#"
1:0-2:15 imports

"#]],
        );
    }

    #[test]
    fn lexical_import_fallback_ignores_member_accesses() {
        let source = concat!("uint256 constant X = Foo.import\n", "    + 1;\n", "@ invalid\n",);

        assert_data_eq!(
            folding_range_output(&folding_ranges(source.into())),
            str![[r#"
0:0-1:8 code

"#]],
        );
    }

    #[test]
    fn extends_recognized_incomplete_blocks_to_physical_eof() {
        let source = concat!(
            "contract C {\n",
            "    function f() external {\n",
            "        if (true) {\n",
            "            uint256 x\n",
            "            // trailing comment\n",
        );

        assert_data_eq!(
            folding_range_output(&folding_ranges(source.into())),
            str![[r#"
0:0-5:0 code
1:4-5:0 code
2:18-5:0 code

"#]],
        );
    }

    #[test]
    fn falls_back_to_recognized_blocks_when_parsing_fails() {
        let source = concat!(
            "@ invalid\n",
            "contract Broken {\n",
            "    function f() external {\n",
            "        if (true) {\n",
        );

        assert_data_eq!(
            folding_range_output(&folding_ranges(source.into())),
            str![[r#"
1:0-4:0 code
2:4-4:0 code
3:18-4:0 code

"#]],
        );
    }

    #[test]
    fn lexical_fallback_recognizes_incomplete_yul_for_post_blocks() {
        let source = concat!(
            "@ invalid\n",
            "contract C {\n",
            "    function f() external {\n",
            "        assembly {\n",
            "            for {} 1 {\n",
            "                let x := 1\n",
        );

        assert_data_eq!(
            folding_range_output(&folding_ranges(source.into())),
            str![[r#"
1:0-6:0 code
2:4-6:0 code
3:17-6:0 code
4:21-6:0 code

"#]],
        );
    }

    #[test]
    fn lexical_fallback_recognizes_yul_bare_blocks_after_unterminated_statements() {
        let source = concat!(
            "@ invalid\n",
            "contract C {\n",
            "    function f() external {\n",
            "        assembly {\n",
            "            let x := 1\n",
            "            {\n",
            "                if x {\n",
            "                    pop(x)\n",
            "                }\n",
            "            }\n",
            "        }\n",
            "    }\n",
            "}\n",
        );

        assert_data_eq!(
            folding_range_output(&folding_ranges(source.into())),
            str![[r#"
1:0-12:1 code
2:4-11:5 code
3:17-10:9 code
5:12-9:13 code
6:21-8:17 code

"#]],
        );
    }

    #[test]
    fn supplements_descendants_of_a_recovered_unclosed_declaration() {
        let source = concat!(
            "contract C {\n",
            "    @ invalid\n",
            "    function f() external {\n",
            "    }\n",
        );

        assert_data_eq!(
            folding_range_output(&folding_ranges(source.into())),
            str![[r#"
0:0-4:0 code
2:4-3:5 code

"#]],
        );
    }

    #[test]
    fn lexical_fallback_ignores_call_options_in_single_statement_control_flow() {
        let source = concat!(
            "@ invalid\n",
            "contract C {\n",
            "    function f() external {\n",
            "        if (true) this.f{\n",
            "            gas: 1\n",
            "        }();\n",
            "    }\n",
            "}\n",
        );

        assert_data_eq!(
            folding_range_output(&folding_ranges(source.into())),
            str![[r#"
1:0-7:1 code
2:4-6:5 code

"#]],
        );
    }

    #[test]
    fn lexical_fallback_does_not_treat_function_types_as_declarations() {
        let source = concat!(
            "@ invalid\n",
            "contract C {\n",
            "    function() external callback = this.f{\n",
            "        gas: 1\n",
            "    };\n",
            "}\n",
        );

        assert_data_eq!(
            folding_range_output(&folding_ranges(source.into())),
            str![[r#"
1:0-5:1 code

"#]],
        );
    }

    #[test]
    fn supplements_partial_ast_with_recognized_lexical_blocks() {
        let source = concat!(
            "contract Before {\n",
            "}\n",
            "@ invalid\n",
            "contract After {\n",
            "    function f() external {\n",
            "    }\n",
            "}\n",
        );

        assert_data_eq!(
            folding_range_output(&folding_ranges(source.into())),
            str![[r#"
0:0-1:1 code
3:0-6:1 code
4:4-5:5 code

"#]],
        );
    }

    #[test]
    fn preserves_ast_authority_when_supplementing_parse_errors() {
        let source = concat!(
            "contract C {\n",
            "    function target() external {}\n",
            "    function f() external {\n",
            "        if (this.target{\n",
            "            gas: 1\n",
            "        }()) {\n",
            "        }\n",
            "    }\n",
            "}\n",
            "@ invalid\n",
            "contract After {\n",
            "}\n",
        );

        assert_data_eq!(
            folding_range_output(&folding_ranges(source.into())),
            str![[r#"
0:0-8:1 code
2:4-7:5 code
5:13-6:9 code
10:0-11:1 code

"#]],
        );
    }

    #[test]
    fn ignores_call_options_inside_contract_headers() {
        let source = concat!(
            "contract C layout at this.f{\n",
            "    value: 123\n",
            "}() {\n",
            "}\n",
            "@ invalid\n",
        );

        assert_data_eq!(
            folding_range_output(&folding_ranges(source.into())),
            str![[r#"
0:0-3:1 code

"#]],
        );
    }

    #[test]
    fn folds_yul_declarations_and_nested_bodies() {
        let source = concat!(
            "contract C {\n",
            "    function f() external {\n",
            "        assembly {\n",
            "            function y(x) -> r {\n",
            "                if x {\n",
            "                    r := x\n",
            "                }\n",
            "            }\n",
            "            {\n",
            "                let z := 1\n",
            "            }\n",
            "            switch x\n",
            "            case 0 {\n",
            "                pop(0)\n",
            "            }\n",
            "            default {\n",
            "                pop(1)\n",
            "            }\n",
            "        }\n",
            "    }\n",
            "}\n",
        );

        assert_data_eq!(
            folding_range_output(&folding_ranges(source.into())),
            str![[r#"
0:0-20:1 code
1:4-19:5 code
2:17-18:9 code
3:12-7:13 code
4:21-6:17 code
8:12-10:13 code
12:19-14:13 code
15:20-17:13 code

"#]],
        );
    }

    #[test]
    fn ignores_unclassified_braces_during_lexical_fallback() {
        let source = concat!(
            "@ invalid\n",
            "import {\n",
            "    A,\n",
            "    B\n",
            "} from \"x.sol\";\n",
            "foo{\n",
            "    value: 1\n",
            "}\n",
            "\"literal { brace }\"; // comment { brace }\n",
        );

        assert_data_eq!(
            folding_range_output(&folding_ranges(source.into())),
            str![[r#"
1:0-4:15 imports

"#]],
        );
    }

    #[test]
    fn uses_utf16_positions_and_crlf_line_endings() {
        let source = concat!("😀 /* first\r\n", "second */\r\n", "// 一😀\r\n", "// 二😀\r\n",);

        assert_data_eq!(
            folding_range_output(&folding_ranges(source.into())),
            str![[r#"
0:3-1:9 comment
2:0-3:6 comment

"#]],
        );
    }

    fn folding_range_output(ranges: &[FoldingRange]) -> String {
        let mut output = String::new();
        for range in ranges {
            let kind = match range.kind {
                None => "code",
                Some(FoldingRangeKind::Comment) => "comment",
                Some(FoldingRangeKind::Imports) => "imports",
                Some(FoldingRangeKind::Region) => "region",
            };
            writeln!(
                output,
                "{}:{}-{}:{} {kind}",
                range.start_line,
                range.start_character.expect("start character should be present"),
                range.end_line,
                range.end_character.expect("end character should be present"),
            )
            .unwrap();
            assert_eq!(range.collapsed_text, None);
        }
        output
    }
}

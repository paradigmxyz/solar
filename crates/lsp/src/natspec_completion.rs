use crate::{config::CompletionClientOptions, proto};
use crop::Rope;
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, InsertTextFormat, Position, Range,
    TextEdit,
};
use solar_config::CompileOpts;
use solar_interface::{Session, source_map::FileName};
use solar_parse::{Cursor, Parser, ast, lexer::token::RawTokenKind};
use std::ops::Range as ByteRange;

mod index;
use index::syntax_fingerprint;

pub(crate) use index::{
    DeclarationKey, DeclarationPath, NatSpecCompletionIndex, NatSpecTargetSemantics, TargetKind,
};

pub(crate) fn source_syntax_fingerprint(source: &str) -> Box<str> {
    syntax_fingerprint(source)
}

pub(crate) enum NatSpecCompletionResult {
    NotApplicable,
    Claimed(Option<Box<NatSpecCompletionTarget>>),
}

#[derive(Clone, Copy)]
enum CommentStyle {
    Line,
    Block,
}

impl CommentStyle {
    fn ast_kind(self) -> ast::CommentKind {
        match self {
            Self::Line => ast::CommentKind::Line,
            Self::Block => ast::CommentKind::Block,
        }
    }
}

pub(crate) struct NatSpecCompletionTarget {
    key: DeclarationKey,
    source_fingerprint: Box<str>,
    kind: TargetKind,
    parameters: Vec<String>,
    returns: Vec<Option<String>>,
    variable_visibility: Option<ast::Visibility>,
    edit_range: Range,
    additional_text_edits: Option<Vec<TextEdit>>,
    indent: String,
    eol: String,
    comment_style: CommentStyle,
}

impl NatSpecCompletionTarget {
    pub(crate) fn key(&self) -> &DeclarationKey {
        &self.key
    }

    pub(crate) fn source_fingerprint(&self) -> &str {
        &self.source_fingerprint
    }

    pub(crate) fn completion_items(
        &self,
        options: CompletionClientOptions,
        semantics: Option<&NatSpecTargetSemantics>,
    ) -> Vec<CompletionItem> {
        let mut template = Template::new(options.snippet_support);
        let Some((label, detail, mut lines)) = self.full_template(&mut template, semantics) else {
            return Vec::new();
        };
        template.finish(&mut lines);

        let mut items =
            vec![self.completion_item(label, detail, "0".into(), self.render(&lines), options)];
        if let Some(semantics) = semantics {
            for contract in &semantics.inheritdoc_contracts {
                let template = Template::new(options.snippet_support);
                let mut lines = vec![template.literal(&format!("@inheritdoc {contract}"))];
                template.finish(&mut lines);
                items.push(self.completion_item(
                    format!("NatSpec @inheritdoc {contract}"),
                    format!("Inherit documentation from {contract}"),
                    format!("1:{contract}"),
                    self.render(&lines),
                    options,
                ));
            }
        }
        items
    }

    fn full_template(
        &self,
        template: &mut Template,
        semantics: Option<&NatSpecTargetSemantics>,
    ) -> Option<(String, String, Vec<String>)> {
        let name = self.key.name.as_deref();
        let (label, detail, lines) = match self.kind {
            TargetKind::Contract(kind) => {
                let name = name?;
                (
                    format!("NatSpec {kind} documentation"),
                    format!("{kind} {name}"),
                    vec![
                        template.described("@title"),
                        template.described("@author"),
                        template.described("@notice"),
                    ],
                )
            }
            TargetKind::Function(kind) => {
                if matches!(kind, ast::FunctionKind::Modifier) {
                    return None;
                }
                let detail = match kind {
                    ast::FunctionKind::Function => format!("function {}", name?),
                    _ => kind.to_string(),
                };
                let mut lines = vec![template.described("")];
                push_parameters(&mut lines, template, &self.parameters);
                if !matches!(kind, ast::FunctionKind::Constructor | ast::FunctionKind::Receive) {
                    push_returns(&mut lines, template, &self.returns);
                }
                (format!("NatSpec {kind} documentation"), detail, lines)
            }
            TargetKind::Variable => {
                let visibility = self.variable_visibility?;
                let name = name?;
                let mut lines =
                    vec![template.described(if visibility == ast::Visibility::Public {
                        "@notice"
                    } else {
                        "@dev"
                    })];
                if visibility == ast::Visibility::Public
                    && let Some(semantics) = semantics
                {
                    push_returns(&mut lines, template, &semantics.getter_returns);
                }
                (
                    format!("NatSpec {visibility} state variable documentation"),
                    format!("{visibility} state variable {name}"),
                    lines,
                )
            }
            TargetKind::Struct => {
                let name = name?;
                let mut lines = vec![template.described("")];
                push_parameters(&mut lines, template, &self.parameters);
                ("NatSpec struct documentation".into(), format!("struct {name}"), lines)
            }
            TargetKind::Enum => {
                let name = name?;
                (
                    "NatSpec enum documentation".into(),
                    format!("enum {name}"),
                    vec![template.described("")],
                )
            }
            TargetKind::Event => {
                let name = name?;
                let mut lines = vec![template.described("")];
                push_parameters(&mut lines, template, &self.parameters);
                ("NatSpec event documentation".into(), format!("event {name}"), lines)
            }
            TargetKind::Error => {
                let name = name?;
                let mut lines = vec![template.described("")];
                push_parameters(&mut lines, template, &self.parameters);
                ("NatSpec error documentation".into(), format!("error {name}"), lines)
            }
        };
        Some((label, detail, lines))
    }

    fn completion_item(
        &self,
        label: String,
        detail: String,
        sort_text: String,
        new_text: String,
        options: CompletionClientOptions,
    ) -> CompletionItem {
        CompletionItem {
            label,
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some(detail),
            sort_text: Some(sort_text),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range: self.edit_range,
                new_text,
            })),
            additional_text_edits: self.additional_text_edits.clone(),
            insert_text_format: Some(if options.snippet_support {
                InsertTextFormat::SNIPPET
            } else {
                InsertTextFormat::PLAIN_TEXT
            }),
            ..Default::default()
        }
    }

    fn render(&self, lines: &[String]) -> String {
        match self.comment_style {
            CommentStyle::Line => render_line_comment(lines, &self.indent, &self.eol),
            CommentStyle::Block => render_block_comment(lines, &self.indent, &self.eol),
        }
    }
}

fn push_parameters(lines: &mut Vec<String>, template: &mut Template, parameters: &[String]) {
    lines.extend(parameters.iter().map(|name| template.described(&format!("@param {name}"))));
}

fn push_returns(lines: &mut Vec<String>, template: &mut Template, returns: &[Option<String>]) {
    lines.extend(returns.iter().map(|name| match name {
        Some(name) => template.described(&format!("@return {name}")),
        None => template.described("@return"),
    }));
}

struct Template {
    snippet: bool,
    next_placeholder: usize,
}

impl Template {
    fn new(snippet: bool) -> Self {
        Self { snippet, next_placeholder: 1 }
    }

    fn described(&mut self, prefix: &str) -> String {
        if !self.snippet {
            return prefix.into();
        }
        let prefix = self.literal(prefix);
        let placeholder = format!("${}", self.next_placeholder);
        self.next_placeholder += 1;
        if prefix.is_empty() { placeholder } else { format!("{prefix} {placeholder}") }
    }

    fn literal(&self, text: &str) -> String {
        if self.snippet { text.replace('$', r"\$") } else { text.into() }
    }

    fn finish(&self, lines: &mut Vec<String>) {
        if self.snippet {
            if let Some(line) = lines.last_mut() {
                line.push_str("$0");
            } else {
                lines.push("$0".into());
            }
        }
    }
}

fn render_line_comment(lines: &[String], indent: &str, eol: &str) -> String {
    let mut output = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            output.push_str(eol);
            output.push_str(indent);
        }
        output.push_str("///");
        if !line.is_empty() {
            output.push(' ');
            output.push_str(line);
        }
    }
    output
}

fn render_block_comment(lines: &[String], indent: &str, eol: &str) -> String {
    let mut output = String::from("/**");
    for line in lines {
        output.push_str(eol);
        output.push_str(indent);
        output.push_str(" *");
        if !line.is_empty() {
            output.push(' ');
            output.push_str(line);
        }
    }
    output.push_str(eol);
    output.push_str(indent);
    output.push_str(" */");
    output
}

struct CommentCandidate {
    parse_source: String,
    marker_range: ByteRange<usize>,
    edit_range: ByteRange<usize>,
    additional_edit_range: Option<ByteRange<usize>>,
    indent: String,
    eol: String,
    style: CommentStyle,
}

enum CandidateResult {
    NotApplicable,
    Invalid,
    Candidate(CommentCandidate),
}

pub(crate) fn target(contents: &Rope, position: Position) -> NatSpecCompletionResult {
    let Some(cursor) = proto::checked_text_range(contents, Range::new(position, position))
        .map(|range| range.start)
    else {
        return NatSpecCompletionResult::Claimed(None);
    };
    if !has_natspec_prefix(contents, cursor) {
        return NatSpecCompletionResult::NotApplicable;
    }
    let source = rope_to_string(contents);
    let candidate = match comment_candidate(&source, cursor) {
        CandidateResult::NotApplicable => return NatSpecCompletionResult::NotApplicable,
        CandidateResult::Invalid => return NatSpecCompletionResult::Claimed(None),
        CandidateResult::Candidate(candidate) => candidate,
    };
    let source_fingerprint = syntax_fingerprint(&candidate.parse_source);

    match parse_target(
        &candidate.parse_source,
        candidate.marker_range,
        &candidate.indent,
        &candidate.eol,
        candidate.style,
    ) {
        Some(mut target) => {
            let Some(edit_range) = byte_range_to_lsp(contents, candidate.edit_range) else {
                return NatSpecCompletionResult::Claimed(None);
            };
            target.edit_range = edit_range;
            target.source_fingerprint = source_fingerprint;
            target.additional_text_edits = candidate
                .additional_edit_range
                .and_then(|range| byte_range_to_lsp(contents, range))
                .map(|range| vec![TextEdit { range, new_text: String::new() }]);
            NatSpecCompletionResult::Claimed(Some(Box::new(target)))
        }
        None => NatSpecCompletionResult::Claimed(None),
    }
}

fn has_natspec_prefix(contents: &Rope, cursor: usize) -> bool {
    let line = contents.line_of_byte(cursor);
    let line_start = contents.byte_of_line(line);
    let mut chars = contents.byte_slice(line_start..cursor).chars();
    let first = chars.find(|ch| !matches!(ch, ' ' | '\t'));
    first == Some('/')
        && matches!((chars.next(), chars.next()), (Some('/'), Some('/')) | (Some('*'), Some('*')))
}

fn comment_candidate(source: &str, cursor: usize) -> CandidateResult {
    let line_start = source[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let mut line_end = source[cursor..].find('\n').map_or(source.len(), |index| cursor + index);
    if line_end > line_start && source.as_bytes()[line_end - 1] == b'\r' {
        line_end -= 1;
    }
    let before_cursor = &source[line_start..cursor];
    let indent_len = before_cursor.len() - before_cursor.trim_start_matches([' ', '\t']).len();
    let marker_start = line_start + indent_len;
    let marker_prefix = &source[marker_start..cursor];
    let indent = &source[line_start..marker_start];
    let eol = source_eol(source, line_end);

    if let Some(marker_suffix) = marker_prefix.strip_prefix("///") {
        if !marker_suffix.trim_ascii().is_empty()
            || !source[cursor..line_end].trim_ascii().is_empty()
        {
            return CandidateResult::Invalid;
        }
        return CandidateResult::Candidate(CommentCandidate {
            parse_source: source.into(),
            marker_range: marker_start..line_end,
            edit_range: marker_start..line_end,
            additional_edit_range: None,
            indent: indent.into(),
            eol: eol.into(),
            style: CommentStyle::Line,
        });
    }

    if !marker_prefix.starts_with("/**") {
        return CandidateResult::NotApplicable;
    }
    let Some((is_doc, terminated, block_end)) = raw_block_comment(source, marker_start) else {
        return CandidateResult::Invalid;
    };
    if !is_doc {
        return CandidateResult::Invalid;
    }

    if terminated {
        if cursor > block_end || !empty_block_comment(source, marker_start, block_end) {
            return CandidateResult::Invalid;
        }
        let (edit_range, additional_edit_range) = if block_end <= line_end {
            if !source[block_end..line_end].trim_ascii().is_empty() {
                return CandidateResult::Invalid;
            }
            (marker_start..block_end, None)
        } else {
            (marker_start..line_end, Some(line_end..block_end))
        };
        CandidateResult::Candidate(CommentCandidate {
            parse_source: source.into(),
            marker_range: marker_start..block_end,
            edit_range,
            additional_edit_range,
            indent: indent.into(),
            eol: eol.into(),
            style: CommentStyle::Block,
        })
    } else {
        if marker_prefix != "/**" || !source[cursor..line_end].trim_ascii().is_empty() {
            return CandidateResult::Invalid;
        }
        let mut parse_source = source.to_owned();
        parse_source.insert_str(cursor, " */");
        CandidateResult::Candidate(CommentCandidate {
            parse_source,
            marker_range: marker_start..cursor + 3,
            edit_range: marker_start..cursor,
            additional_edit_range: None,
            indent: indent.into(),
            eol: eol.into(),
            style: CommentStyle::Block,
        })
    }
}

fn raw_block_comment(source: &str, marker_start: usize) -> Option<(bool, bool, usize)> {
    Cursor::new(source).with_position().find_map(|(position, token)| {
        if position != marker_start {
            return None;
        }
        let RawTokenKind::BlockComment { is_doc, terminated } = token.kind else {
            return None;
        };
        Some((is_doc, terminated, position + token.len as usize))
    })
}

fn empty_block_comment(source: &str, marker_start: usize, block_end: usize) -> bool {
    let Some(content) = source.get(marker_start + 3..block_end.saturating_sub(2)) else {
        return false;
    };
    content.lines().all(|line| {
        let line = line.trim_ascii();
        line.is_empty() || line.strip_prefix('*').is_some_and(|rest| rest.trim_ascii().is_empty())
    })
}

fn source_eol(source: &str, line_end: usize) -> &str {
    if source.get(line_end..).is_some_and(|rest| rest.starts_with("\r\n")) {
        "\r\n"
    } else if source.get(line_end..).is_some_and(|rest| rest.starts_with('\n')) {
        "\n"
    } else if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn byte_range_to_lsp(contents: &Rope, range: ByteRange<usize>) -> Option<Range> {
    Some(Range::new(
        proto::position_at_byte(contents, range.start)?,
        proto::position_at_byte(contents, range.end)?,
    ))
}

fn parse_target(
    source: &str,
    marker_range: ByteRange<usize>,
    indent: &str,
    eol: &str,
    comment_style: CommentStyle,
) -> Option<NatSpecCompletionTarget> {
    let mut opts = CompileOpts::default();
    opts.unstable.recover_incomplete_input = true;
    let sess = Session::builder().opts(opts).with_silent_emitter(None).single_threaded().build();

    sess.enter_sequential(|| {
        let arena = ast::Arena::new();
        let mut parser = Parser::from_source_code(
            &sess,
            &arena,
            FileName::Custom("lsp-natspec-completion.sol".into()),
            source,
        )
        .ok()?;
        let source_unit = match parser.parse_file() {
            Ok(source_unit) => source_unit,
            Err(error) => {
                error.emit();
                return None;
            }
        };
        drop(parser);
        let file = sess.source_map().files().first()?.clone();

        for (source_ordinal, item) in source_unit.items.iter().enumerate() {
            let path = DeclarationPath::Source { item_ordinal: source_ordinal };
            if let Some(target) = target_from_item(
                &file,
                source,
                marker_range.clone(),
                indent,
                eol,
                comment_style,
                path,
                item,
            ) {
                return Some(target);
            }
            let ast::ItemKind::Contract(contract) = &item.kind else { continue };
            for (item_ordinal, item) in contract.body.iter().enumerate() {
                let path = DeclarationPath::Contract {
                    contract_ordinal: source_ordinal,
                    contract_name: contract.name.to_string().into_boxed_str(),
                    item_ordinal,
                };
                if let Some(target) = target_from_item(
                    &file,
                    source,
                    marker_range.clone(),
                    indent,
                    eol,
                    comment_style,
                    path,
                    item,
                ) {
                    return Some(target);
                }
            }
        }
        None
    })
}

#[allow(clippy::too_many_arguments)]
fn target_from_item(
    file: &solar_interface::source_map::SourceFile,
    source: &str,
    marker_range: ByteRange<usize>,
    indent: &str,
    eol: &str,
    comment_style: CommentStyle,
    path: DeclarationPath,
    item: &ast::Item<'_>,
) -> Option<NatSpecCompletionTarget> {
    if item.docs.len() != 1 || item.docs[0].kind != comment_style.ast_kind() {
        return None;
    }
    let local_range = |span: solar_interface::Span| {
        file.relative_position(span.lo()).to_usize()..file.relative_position(span.hi()).to_usize()
    };
    let doc_range = local_range(item.docs[0].span);
    if doc_range != marker_range {
        return None;
    }
    let item_range = local_range(item.span);
    if !is_adjacent_doc_comment(&source[doc_range.end..item_range.start], comment_style) {
        return None;
    }

    let in_contract = matches!(&path, DeclarationPath::Contract { .. });
    let key = DeclarationKey::from_ast(file, path, item)?;
    let (kind, parameters, returns, variable_visibility) = match &item.kind {
        ast::ItemKind::Contract(contract) => {
            (TargetKind::Contract(contract.kind), Vec::new(), Vec::new(), None)
        }
        ast::ItemKind::Function(function)
            if !matches!(function.kind, ast::FunctionKind::Modifier) =>
        {
            let returns = if matches!(
                function.kind,
                ast::FunctionKind::Constructor | ast::FunctionKind::Receive
            ) {
                Vec::new()
            } else {
                function
                    .header
                    .returns
                    .as_ref()
                    .map(|returns| {
                        returns.iter().map(|var| var.name.map(|name| name.to_string())).collect()
                    })
                    .unwrap_or_default()
            };
            (
                TargetKind::Function(function.kind),
                named_variables(&function.header.parameters),
                returns,
                None,
            )
        }
        ast::ItemKind::Variable(variable) if in_contract => (
            TargetKind::Variable,
            Vec::new(),
            Vec::new(),
            Some(variable.visibility.unwrap_or(ast::Visibility::Internal)),
        ),
        ast::ItemKind::Struct(item) => {
            (TargetKind::Struct, named_variables(item.fields), Vec::new(), None)
        }
        ast::ItemKind::Enum(_) => (TargetKind::Enum, Vec::new(), Vec::new(), None),
        ast::ItemKind::Event(item) => {
            (TargetKind::Event, named_variables(&item.parameters), Vec::new(), None)
        }
        ast::ItemKind::Error(item) => {
            (TargetKind::Error, named_variables(&item.parameters), Vec::new(), None)
        }
        _ => return None,
    };

    Some(NatSpecCompletionTarget {
        key,
        source_fingerprint: Box::default(),
        kind,
        parameters,
        returns,
        variable_visibility,
        edit_range: Range::default(),
        additional_text_edits: None,
        indent: indent.into(),
        eol: eol.into(),
        comment_style,
    })
}

fn named_variables(variables: &[ast::VariableDefinition<'_>]) -> Vec<String> {
    let mut names = Vec::new();
    for name in variables.iter().filter_map(|variable| variable.name) {
        let name = name.to_string();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

fn is_adjacent_doc_comment(gap: &str, style: CommentStyle) -> bool {
    if matches!(style, CommentStyle::Block) && gap.bytes().all(|byte| matches!(byte, b' ' | b'\t'))
    {
        return true;
    }
    gap.strip_prefix("\r\n")
        .or_else(|| gap.strip_prefix('\n'))
        .is_some_and(|rest| rest.bytes().all(|byte| matches!(byte, b' ' | b'\t')))
}

fn rope_to_string(contents: &Rope) -> String {
    let mut source = String::with_capacity(contents.byte_len());
    for chunk in contents.chunks() {
        source.push_str(chunk);
    }
    source
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_line_doc_comment_separated_by_a_blank_line() {
        let contents = Rope::from("///\n\ncontract C {}");
        assert!(matches!(
            target(&contents, Position::new(0, 3)),
            NatSpecCompletionResult::Claimed(None)
        ));
    }

    #[test]
    fn leaves_valid_empty_and_trailing_lines_for_ordinary_completion() {
        for (source, position) in [
            ("", Position::new(0, 0)),
            ("contract C {}\n", Position::new(1, 0)),
            ("contract C {}\r\n", Position::new(1, 0)),
        ] {
            assert!(matches!(
                target(&Rope::from(source), position),
                NatSpecCompletionResult::NotApplicable
            ));
        }
    }

    #[test]
    fn preserves_crlf_in_generated_comments() {
        let contents = Rope::from("///\r\ncontract C {}");
        let NatSpecCompletionResult::Claimed(Some(target)) = target(&contents, Position::new(0, 3))
        else {
            panic!("expected a NatSpec completion target");
        };
        let item = target
            .completion_items(CompletionClientOptions { snippet_support: true }, None)
            .into_iter()
            .next()
            .unwrap();
        let Some(CompletionTextEdit::Edit(edit)) = item.text_edit else {
            panic!("expected a completion text edit");
        };
        assert_eq!(edit.new_text, "/// @title $1\r\n/// @author $2\r\n/// @notice $3$0");
    }

    #[test]
    fn replaces_multiline_crlf_blocks_with_non_overlapping_edits() {
        let contents = Rope::from("/**\r\n *\r\n */\r\ncontract C {}");
        let NatSpecCompletionResult::Claimed(Some(target)) = target(&contents, Position::new(0, 3))
        else {
            panic!("expected a NatSpec completion target");
        };
        let item = target
            .completion_items(CompletionClientOptions { snippet_support: true }, None)
            .into_iter()
            .next()
            .unwrap();
        let Some(CompletionTextEdit::Edit(edit)) = item.text_edit else {
            panic!("expected a completion text edit");
        };

        assert_eq!(edit.range, Range::new(Position::new(0, 0), Position::new(0, 3)));
        assert_eq!(edit.new_text, "/**\r\n * @title $1\r\n * @author $2\r\n * @notice $3$0\r\n */");
        assert_eq!(
            item.additional_text_edits,
            Some(vec![TextEdit {
                range: Range::new(Position::new(0, 3), Position::new(2, 3)),
                new_text: String::new(),
            }])
        );
    }

    #[test]
    fn does_not_recognize_a_block_marker_inside_a_string() {
        let contents = Rope::from("string constant VALUE = \"/**\";");
        assert!(matches!(
            target(&contents, Position::new(0, 29)),
            NatSpecCompletionResult::NotApplicable
        ));
    }

    #[test]
    fn escapes_dollar_identifiers_and_inheritdoc_names_in_snippets() {
        let contents = Rope::from(
            "contract C {\n    ///\n    function value(uint256 $amount) external returns (uint256 $result);\n}",
        );
        let NatSpecCompletionResult::Claimed(Some(target)) = target(&contents, Position::new(1, 7))
        else {
            panic!("expected a NatSpec completion target");
        };
        let semantics = NatSpecTargetSemantics {
            getter_returns: Vec::new(),
            inheritdoc_contracts: vec!["$Alias".into()],
        };
        let items = target
            .completion_items(CompletionClientOptions { snippet_support: true }, Some(&semantics));

        assert_eq!(
            completion_new_text(&items[0]),
            "/// $1\n    /// @param \\$amount $2\n    /// @return \\$result $3$0"
        );
        assert_eq!(completion_new_text(&items[1]), "/// @inheritdoc \\$Alias$0");
    }

    fn completion_new_text(item: &CompletionItem) -> &str {
        let Some(CompletionTextEdit::Edit(edit)) = &item.text_edit else {
            panic!("expected a completion text edit");
        };
        &edit.new_text
    }
}

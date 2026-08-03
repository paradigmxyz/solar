use crate::proto;
use crop::Rope;
use lsp_types::{CodeActionKind, CodeActionParams, Diagnostic, NumberOrString, TextEdit, Url};
use serde::{Deserialize, Serialize};
use solar_config::CompileOpts;
use solar_interface::{
    Session,
    diagnostics::Applicability,
    source_map::{FileName, SourceFile},
    sym,
};
use solar_parse::{
    Parser,
    ast::{self, visit::Visit},
};
use std::ops::ControlFlow;

const DIAGNOSTIC_DATA_VERSION: u8 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DiagnosticData {
    version: u8,
    uri: Url,
    source_fingerprint: String,
    suggestions: Vec<DiagnosticSuggestion>,
}

impl DiagnosticData {
    pub(crate) fn new(uri: Url, source: &str, suggestions: Vec<DiagnosticSuggestion>) -> Self {
        Self {
            version: DIAGNOSTIC_DATA_VERSION,
            uri,
            source_fingerprint: source_fingerprint(source),
            suggestions,
        }
    }

    pub(crate) fn from_rope(
        uri: Url,
        source: &Rope,
        suggestions: Vec<DiagnosticSuggestion>,
    ) -> Self {
        Self {
            version: DIAGNOSTIC_DATA_VERSION,
            uri,
            source_fingerprint: source_fingerprint_chunks(source.chunks()),
            suggestions,
        }
    }

    pub(crate) fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("diagnostic data is serializable")
    }

    fn from_value(value: &serde_json::Value, uri: &Url) -> Option<Self> {
        let data = Self::deserialize(value).ok()?;
        (data.version == DIAGNOSTIC_DATA_VERSION && data.uri == *uri).then_some(data)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DiagnosticSuggestion {
    title: String,
    applicability: Applicability,
    alternatives: Vec<Vec<TextEdit>>,
}

impl DiagnosticSuggestion {
    pub(crate) fn new(
        title: String,
        applicability: Applicability,
        alternatives: Vec<Vec<TextEdit>>,
    ) -> Self {
        Self { title, applicability, alternatives }
    }

    pub(crate) fn merge_alternatives(&mut self, other: &mut Self) -> bool {
        if self.title != other.title || self.applicability != other.applicability {
            return false;
        }
        self.alternatives.append(&mut other.alternatives);
        true
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CodeActionPlan {
    pub(crate) title: String,
    pub(crate) applicability: Applicability,
    pub(crate) diagnostic: Diagnostic,
    pub(crate) uri: Url,
    pub(crate) source_fingerprint: String,
    pub(crate) edits: Vec<TextEdit>,
}

pub(crate) fn plans(
    params: &CodeActionParams,
    current_diagnostics: &[Diagnostic],
    contents: &Rope,
) -> Vec<CodeActionPlan> {
    if params
        .context
        .only
        .as_ref()
        .is_some_and(|only| !only.iter().any(|kind| kind_contains(kind, &CodeActionKind::QUICKFIX)))
    {
        return Vec::new();
    }
    let index = proto::LspPositionIndex::new(contents);
    let Some(request_range) = exact_byte_range(&index, params.range) else {
        return Vec::new();
    };
    let uri = &params.text_document.uri;
    let mut plans = Vec::new();
    let candidates = current_diagnostics
        .iter()
        .filter_map(|diagnostic| {
            if !matches!(diagnostic.source.as_deref(), Some("solar" | "flycheck" | "forge-lint"))
                || !exact_byte_range(&index, diagnostic.range)
                    .is_some_and(|range| code_action_ranges_intersect(&request_range, &range))
            {
                return None;
            }
            let data = diagnostic
                .data
                .as_ref()
                .and_then(|value| DiagnosticData::from_value(value, uri))?;
            Some((diagnostic, data))
        })
        .collect::<Vec<_>>();
    let client_selected = candidates
        .iter()
        .filter_map(|(diagnostic, _)| {
            params
                .context
                .diagnostics
                .iter()
                .any(|requested| {
                    diagnostics_match(requested, diagnostic)
                        && requested.data.as_ref() == diagnostic.data.as_ref()
                })
                .then_some(*diagnostic)
        })
        .collect::<Vec<_>>();
    for (diagnostic, data) in candidates {
        let group_was_disambiguated =
            client_selected.iter().any(|selected| diagnostics_match(selected, diagnostic));
        if group_was_disambiguated
            && !client_selected.iter().any(|selected| std::ptr::eq(*selected, diagnostic))
        {
            continue;
        }
        if data.suggestions.is_empty() {
            for plan in fallback_plans(diagnostic, data, contents) {
                push_unique_plan(&mut plans, plan);
            }
        } else {
            for suggestion in data.suggestions {
                let title = suggestion.title;
                let applicability = suggestion.applicability;
                for edits in suggestion.alternatives {
                    push_unique_plan(
                        &mut plans,
                        CodeActionPlan {
                            title: title.clone(),
                            applicability,
                            diagnostic: diagnostic.clone(),
                            uri: data.uri.clone(),
                            source_fingerprint: data.source_fingerprint.clone(),
                            edits,
                        },
                    );
                }
            }
        }
    }
    plans
}

fn exact_byte_range(
    index: &proto::LspPositionIndex<'_>,
    range: lsp_types::Range,
) -> Option<std::ops::Range<usize>> {
    let bytes = index.checked_text_range(range)?;
    (index.position_at_byte(bytes.start) == Some(range.start)
        && index.position_at_byte(bytes.end) == Some(range.end))
    .then_some(bytes)
}

fn code_action_ranges_intersect(
    request: &std::ops::Range<usize>,
    diagnostic: &std::ops::Range<usize>,
) -> bool {
    if request.is_empty() {
        diagnostic.start <= request.start && request.start <= diagnostic.end
    } else if diagnostic.is_empty() {
        request.start <= diagnostic.start && diagnostic.start <= request.end
    } else {
        request.start < diagnostic.end && diagnostic.start < request.end
    }
}

fn diagnostics_match(requested: &Diagnostic, current: &Diagnostic) -> bool {
    requested.range == current.range
        && requested.severity == current.severity
        && requested.code == current.code
        && requested.source == current.source
        && requested.message == current.message
}

fn push_unique_plan(plans: &mut Vec<CodeActionPlan>, plan: CodeActionPlan) {
    let duplicate = plans.iter().any(|existing| {
        existing.title == plan.title
            && existing.applicability == plan.applicability
            && existing.uri == plan.uri
            && existing.source_fingerprint == plan.source_fingerprint
            && existing.edits == plan.edits
    });
    if !duplicate {
        plans.push(plan);
    }
}

fn fallback_plans(
    diagnostic: &Diagnostic,
    data: DiagnosticData,
    contents: &Rope,
) -> Vec<CodeActionPlan> {
    let fixes = if is_unused_import_diagnostic(diagnostic) {
        unused_import_fix(diagnostic, contents).into_iter().collect()
    } else {
        let Some(NumberOrString::String(code)) = diagnostic.code.as_ref() else {
            return Vec::new();
        };
        match code.as_str() {
            "1878" => spdx_fixes(diagnostic, contents),
            "2018" => function_mutability_fix(diagnostic, contents).into_iter().collect(),
            "2072" => unused_local_variable_fix(diagnostic, contents).into_iter().collect(),
            "3420" => compiler_pragma_fix(diagnostic, contents).into_iter().collect(),
            "5424" => unimplemented_function_fix(diagnostic, contents).into_iter().collect(),
            "9456" => missing_override_fix(diagnostic, contents).into_iter().collect(),
            _ => return Vec::new(),
        }
    };
    fixes
        .into_iter()
        .map(|(title, applicability, edits)| CodeActionPlan {
            title,
            applicability,
            diagnostic: diagnostic.clone(),
            uri: data.uri.clone(),
            source_fingerprint: data.source_fingerprint.clone(),
            edits,
        })
        .collect()
}

fn is_unused_import_diagnostic(diagnostic: &Diagnostic) -> bool {
    match (diagnostic.source.as_deref(), diagnostic.code.as_ref(), diagnostic.message.as_str()) {
        (Some("solar"), None, "unused import") => true,
        (Some("solar"), Some(NumberOrString::String(code)), "unused import")
        | (
            Some("forge-lint"),
            Some(NumberOrString::String(code)),
            "unused imports should be removed",
        ) => code == "unused-import",
        _ => false,
    }
}

fn unused_local_variable_fix(
    diagnostic: &Diagnostic,
    contents: &Rope,
) -> Option<(String, Applicability, Vec<TextEdit>)> {
    let message = diagnostic.message.trim_end();
    let message = message.strip_suffix('.').unwrap_or(message);
    if !matches!(message, "unused local variable" | "Unused local variable") {
        return None;
    }
    with_parsed_target(diagnostic, contents, |source_unit, file, source, target| {
        let mut finder = UnusedLocalStatementFinder { file, source, target };
        let ControlFlow::Break(range) = finder.visit_source_unit(source_unit) else { return None };
        let range = standalone_statement_range(source, range);
        let edit = TextEdit::new(byte_range_to_lsp(contents, range)?, String::new());
        Some(("Remove unused local variable".into(), Applicability::MachineApplicable, vec![edit]))
    })
}

struct UnusedLocalStatementFinder<'a> {
    file: &'a SourceFile,
    source: &'a str,
    target: &'a std::ops::Range<usize>,
}

impl<'ast> Visit<'ast> for UnusedLocalStatementFinder<'_> {
    type BreakValue = std::ops::Range<usize>;

    fn visit_block(&mut self, block: &'ast ast::Block<'ast>) -> ControlFlow<Self::BreakValue> {
        for statement in block.iter() {
            if let ast::StmtKind::DeclSingle(variable) = &statement.kind
                && variable.initializer.is_none()
            {
                let variable_range = local_range(self.file, variable.span);
                let statement_range = local_range(self.file, statement.span);
                if &variable_range == self.target
                    && self
                        .source
                        .get(variable_range.end..statement_range.end)
                        .is_some_and(|trailing| trailing.trim() == ";")
                {
                    return ControlFlow::Break(statement_range);
                }
            }
        }
        self.walk_block(block)
    }
}

fn unused_import_fix(
    diagnostic: &Diagnostic,
    contents: &Rope,
) -> Option<(String, Applicability, Vec<TextEdit>)> {
    with_parsed_target(diagnostic, contents, |source_unit, file, source, target| {
        let item = source_unit.items.iter().find(|item| {
            matches!(item.kind, ast::ItemKind::Import(_)) && {
                let range = local_range(file, item.span);
                range.start <= target.start && target.end <= range.end
            }
        })?;
        let ast::ItemKind::Import(import) = &item.kind else { return None };
        let item_range = local_range(file, item.span);
        let range = if item_range == *target {
            if !matches!(
                &import.items,
                ast::ImportItems::Plain(Some(_)) | ast::ImportItems::Glob(_)
            ) {
                return None;
            }
            standalone_statement_range(source, item_range)
        } else {
            named_import_removal_range(source, file, import, target, item_range)?
        };
        let edit = TextEdit::new(byte_range_to_lsp(contents, range)?, String::new());
        Some(("Remove unused import".into(), Applicability::MachineApplicable, vec![edit]))
    })
}

fn named_import_removal_range(
    source: &str,
    file: &SourceFile,
    import: &ast::ImportDirective<'_>,
    target: &std::ops::Range<usize>,
    item_range: std::ops::Range<usize>,
) -> Option<std::ops::Range<usize>> {
    let ast::ImportItems::Aliases(bindings) = &import.items else { return None };
    let ranges = bindings
        .iter()
        .map(|(original, alias)| {
            let end = alias.as_ref().map_or(original.span, |alias| alias.span);
            local_range(file, original.span.to(end))
        })
        .collect::<Vec<_>>();
    let index = ranges.iter().position(|range| range == target)?;
    if ranges.len() == 1 {
        return Some(standalone_statement_range(source, item_range));
    }
    if let Some(next) = ranges.get(index + 1) {
        comma_gap(source.get(ranges[index].end..next.start)?)
            .then_some(ranges[index].start..next.start)
    } else {
        let previous = &ranges[index - 1];
        comma_gap(source.get(previous.end..ranges[index].start)?)
            .then_some(previous.end..ranges[index].end)
    }
}

fn comma_gap(gap: &str) -> bool {
    let mut punctuation = gap.bytes().filter(|byte| !byte.is_ascii_whitespace());
    punctuation.next() == Some(b',') && punctuation.next().is_none()
}

fn standalone_statement_range(
    source: &str,
    range: std::ops::Range<usize>,
) -> std::ops::Range<usize> {
    let line_start = source[..range.start].rfind('\n').map_or(0, |position| position + 1);
    if !source[line_start..range.start].bytes().all(|byte| matches!(byte, b' ' | b'\t')) {
        return range;
    }
    let rest = &source[range.end..];
    let trailing_whitespace = rest.bytes().take_while(|byte| matches!(byte, b' ' | b'\t')).count();
    let after_whitespace = &rest[trailing_whitespace..];
    let line_ending = if after_whitespace.starts_with("\r\n") {
        2
    } else if after_whitespace.starts_with('\n') {
        1
    } else if after_whitespace.is_empty() {
        0
    } else {
        return range;
    };
    line_start..range.end + trailing_whitespace + line_ending
}

fn spdx_fixes(
    diagnostic: &Diagnostic,
    contents: &Rope,
) -> Vec<(String, Applicability, Vec<TextEdit>)> {
    if diagnostic.range != lsp_types::Range::default()
        || !diagnostic.message.starts_with("SPDX license identifier not provided in source file.")
    {
        return Vec::new();
    }
    let source = rope_to_string(contents);
    let eol = if source.contains("\r\n") { "\r\n" } else { "\n" };
    ["MIT", "UNLICENSED"]
        .into_iter()
        .map(|license| {
            let identifier = format!("SPDX-License-Identifier: {license}");
            (
                format!("Add `{identifier}`"),
                Applicability::MaybeIncorrect,
                vec![TextEdit::new(lsp_types::Range::default(), format!("// {identifier}{eol}"))],
            )
        })
        .collect()
}

fn compiler_pragma_fix(
    diagnostic: &Diagnostic,
    contents: &Rope,
) -> Option<(String, Applicability, Vec<TextEdit>)> {
    const PREFIX: &str =
        "Source file does not specify required compiler version! Consider adding \"";

    if diagnostic.range != lsp_types::Range::default() {
        return None;
    }
    let source = rope_to_string(contents);
    let recommendation = diagnostic.message.trim_end().strip_prefix(PREFIX)?;
    let recommendation = recommendation.strip_suffix('.').unwrap_or(recommendation);
    let pragma = recommendation.strip_suffix('"')?;
    if pragma.len() > 128 || pragma.contains(['\r', '\n']) || !is_single_solidity_pragma(pragma) {
        return None;
    }
    let eol = if source.contains("\r\n") { "\r\n" } else { "\n" };
    Some((
        format!("Add `{pragma}`"),
        Applicability::MaybeIncorrect,
        vec![TextEdit::new(lsp_types::Range::default(), format!("{pragma}{eol}"))],
    ))
}

fn is_single_solidity_pragma(pragma: &str) -> bool {
    let Some(requirement) =
        pragma.strip_prefix("pragma solidity ").and_then(|s| s.strip_suffix(';'))
    else {
        return false;
    };
    if requirement.is_empty()
        || !requirement.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b' ' | b'\t'
                        | b'.'
                        | b'<'
                        | b'>'
                        | b'='
                        | b'^'
                        | b'~'
                        | b'*'
                        | b'|'
                        | b'+'
                        | b'-'
                )
        })
    {
        return false;
    }
    let sess = Session::builder()
        .opts(CompileOpts::default())
        .with_silent_emitter(None)
        .single_threaded()
        .build();
    sess.enter_sequential(|| {
        let arena = ast::Arena::new();
        let Ok(mut parser) = Parser::from_source_code(
            &sess,
            &arena,
            FileName::Custom("lsp-code-action-pragma.sol".into()),
            pragma,
        ) else {
            return false;
        };
        let Ok(source_unit) = parser.parse_file() else { return false };
        if sess.dcx.has_errors().is_err() {
            return false;
        }
        let [item] = source_unit.items.as_raw_slice() else { return false };
        matches!(&item.kind, ast::ItemKind::Pragma(pragma)
            if matches!(&pragma.tokens, ast::PragmaTokens::Version(name, _) if name.name == sym::solidity))
    })
}

fn function_mutability_fix(
    diagnostic: &Diagnostic,
    contents: &Rope,
) -> Option<(String, Applicability, Vec<TextEdit>)> {
    let message = diagnostic.message.trim_end();
    let message = message.strip_suffix('.').unwrap_or(message);
    let target = match message {
        "function state mutability can be restricted to view"
        | "Function state mutability can be restricted to view" => ast::StateMutability::View,
        "function state mutability can be restricted to pure"
        | "Function state mutability can be restricted to pure" => ast::StateMutability::Pure,
        _ => return None,
    };
    with_target_item(diagnostic, contents, |item, file, source| {
        let ast::ItemKind::Function(function) = &item.kind else { return None };
        if !function.kind.is_ordinary() || function.body.is_none() {
            return None;
        }

        let edit = match (function.header.state_mutability, target) {
            (None, ast::StateMutability::View | ast::StateMutability::Pure) => {
                let position = qualifier_insertion_position(function, file, source)?;
                keyword_insertion(contents, source, position, target.to_str())?
            }
            (Some(current), ast::StateMutability::Pure)
                if current.data == ast::StateMutability::View =>
            {
                let range = local_range(file, current.span);
                TextEdit::new(byte_range_to_lsp(contents, range)?, target.to_string())
            }
            _ => return None,
        };
        Some((
            format!("Change state mutability to `{target}`"),
            Applicability::MachineApplicable,
            vec![edit],
        ))
    })
}

fn unimplemented_function_fix(
    diagnostic: &Diagnostic,
    contents: &Rope,
) -> Option<(String, Applicability, Vec<TextEdit>)> {
    let message = diagnostic.message.trim_end();
    let message = message.strip_suffix('.').unwrap_or(message);
    if !matches!(
        message,
        "functions without implementation must be marked virtual"
            | "Functions without implementation must be marked virtual"
    ) {
        return None;
    }
    with_target_item(diagnostic, contents, |item, file, source| {
        let ast::ItemKind::Function(function) = &item.kind else { return None };
        if !function.kind.is_ordinary()
            || function.body.is_some()
            || function.header.virtual_.is_some()
            || function.header.visibility() == Some(ast::Visibility::Private)
        {
            return None;
        }
        let position = qualifier_insertion_position(function, file, source)?;
        let edit = keyword_insertion(contents, source, position, "virtual")?;
        Some(("Add `virtual`".into(), Applicability::MachineApplicable, vec![edit]))
    })
}

fn missing_override_fix(
    diagnostic: &Diagnostic,
    contents: &Rope,
) -> Option<(String, Applicability, Vec<TextEdit>)> {
    #[derive(Clone, Copy)]
    enum Target {
        Function,
        Modifier,
        PublicVariable,
    }

    let message = diagnostic.message.trim_end();
    let message = message.strip_suffix('.').unwrap_or(message);
    let target = match message {
        "overriding function is missing `override` specifier"
        | "Overriding function is missing \"override\" specifier" => Target::Function,
        "overriding modifier is missing `override` specifier"
        | "Overriding modifier is missing \"override\" specifier" => Target::Modifier,
        "overriding public state variable is missing `override` specifier"
        | "Overriding public state variable is missing \"override\" specifier" => {
            Target::PublicVariable
        }
        _ => return None,
    };
    with_target_item(diagnostic, contents, |item, file, source| {
        let position = match (&item.kind, target) {
            (ast::ItemKind::Function(function), Target::Function)
                if matches!(
                    function.kind,
                    ast::FunctionKind::Function
                        | ast::FunctionKind::Fallback
                        | ast::FunctionKind::Receive
                ) && function.header.override_.is_none() =>
            {
                qualifier_insertion_position(function, file, source)?
            }
            (ast::ItemKind::Function(function), Target::Modifier)
                if function.kind == ast::FunctionKind::Modifier
                    && function.header.override_.is_none() =>
            {
                qualifier_insertion_position(function, file, source)?
            }
            (ast::ItemKind::Variable(variable), Target::PublicVariable)
                if variable.visibility == Some(ast::Visibility::Public)
                    && variable.override_.is_none() =>
            {
                file.relative_position(variable.name?.span.lo()).to_usize()
            }
            _ => return None,
        };
        let edit = keyword_insertion(contents, source, position, "override")?;
        Some(("Add `override`".into(), Applicability::MaybeIncorrect, vec![edit]))
    })
}

fn with_target_item<T>(
    diagnostic: &Diagnostic,
    contents: &Rope,
    f: impl FnOnce(&ast::Item<'_>, &SourceFile, &str) -> Option<T>,
) -> Option<T> {
    with_parsed_target(diagnostic, contents, |source_unit, file, source, target| {
        let item = find_item(source_unit.items.as_raw_slice(), file, target)?;
        f(item, file, source)
    })
}

fn with_parsed_target<T>(
    diagnostic: &Diagnostic,
    contents: &Rope,
    f: impl for<'ast> FnOnce(
        &'ast ast::SourceUnit<'ast>,
        &SourceFile,
        &str,
        &std::ops::Range<usize>,
    ) -> Option<T>,
) -> Option<T> {
    let target_range =
        proto::LspPositionIndex::new(contents).checked_text_range(diagnostic.range)?;
    let source = rope_to_string(contents);
    let sess = Session::builder()
        .opts(CompileOpts::default())
        .with_silent_emitter(None)
        .single_threaded()
        .build();

    sess.enter_sequential(|| {
        let arena = ast::Arena::new();
        let mut parser = Parser::from_source_code(
            &sess,
            &arena,
            FileName::Custom("lsp-code-action.sol".into()),
            source.as_str(),
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
        f(&source_unit, &file, &source, &target_range)
    })
}

fn find_item<'ast, 'a>(
    items: &'a [ast::Item<'ast>],
    file: &SourceFile,
    target: &std::ops::Range<usize>,
) -> Option<&'a ast::Item<'ast>> {
    for item in items {
        let range = local_range(file, item.span);
        if range.start <= target.start && target.end <= range.end {
            if let ast::ItemKind::Contract(contract) = &item.kind
                && let Some(item) = find_item(contract.body, file, target)
            {
                return Some(item);
            }
            return Some(item);
        }
    }
    None
}

fn qualifier_insertion_position(
    function: &ast::ItemFunction<'_>,
    file: &SourceFile,
    source: &str,
) -> Option<usize> {
    if let Some(returns) = &function.header.returns {
        let open_paren = file.relative_position(returns.span.lo()).to_usize();
        let before_paren = source.get(..open_paren)?.trim_end();
        let position = before_paren.len().checked_sub("returns".len())?;
        (source.get(position..before_paren.len())? == "returns").then_some(position)
    } else {
        Some(file.relative_position(function.body_span.lo()).to_usize())
    }
}

fn keyword_insertion(
    contents: &Rope,
    source: &str,
    position: usize,
    keyword: &str,
) -> Option<TextEdit> {
    let before = source.get(..position)?.chars().next_back();
    let after = source.get(position..)?.chars().next();
    let mut new_text = String::with_capacity(keyword.len() + 2);
    if before.is_some_and(|character| !character.is_whitespace()) {
        new_text.push(' ');
    }
    new_text.push_str(keyword);
    if after.is_some_and(|character| !character.is_whitespace() && character != ';') {
        new_text.push(' ');
    }
    let position = proto::position_at_byte(contents, position)?;
    Some(TextEdit::new(lsp_types::Range::new(position, position), new_text))
}

fn local_range(file: &SourceFile, span: solar_interface::Span) -> std::ops::Range<usize> {
    file.relative_position(span.lo()).to_usize()..file.relative_position(span.hi()).to_usize()
}

fn byte_range_to_lsp(contents: &Rope, range: std::ops::Range<usize>) -> Option<lsp_types::Range> {
    Some(lsp_types::Range::new(
        proto::position_at_byte(contents, range.start)?,
        proto::position_at_byte(contents, range.end)?,
    ))
}

fn rope_to_string(contents: &Rope) -> String {
    let mut source = String::with_capacity(contents.byte_len());
    for chunk in contents.chunks() {
        source.push_str(chunk);
    }
    source
}

fn kind_contains(requested: &CodeActionKind, action: &CodeActionKind) -> bool {
    let requested = requested.as_str();
    let action = action.as_str();
    requested.is_empty()
        || action == requested
        || action.strip_prefix(requested).is_some_and(|suffix| suffix.starts_with('.'))
}

pub(crate) fn source_fingerprint(source: &str) -> String {
    source_fingerprint_chunks(std::iter::once(source))
}

pub(crate) fn rope_source_fingerprint(source: &Rope) -> String {
    source_fingerprint_chunks(source.chunks())
}

pub(crate) fn ranges_overlap(ranges: &mut [std::ops::Range<usize>]) -> bool {
    ranges.sort_unstable_by_key(|range| (range.start, range.end));
    ranges.windows(2).any(|ranges| {
        let [previous, next] = ranges else { unreachable!() };
        previous.end > next.start
            || previous.start == next.start && (previous.is_empty() || next.is_empty())
    })
}

fn source_fingerprint_chunks<'a>(chunks: impl IntoIterator<Item = &'a str>) -> String {
    const OFFSET_BASIS: u128 = 0x6c62272e07bb014262b821756295c58d;
    const PRIME: u128 = 0x0000000001000000000000000000013b;

    let mut fingerprint = OFFSET_BASIS;
    for byte in chunks.into_iter().flat_map(str::bytes) {
        fingerprint ^= u128::from(byte);
        fingerprint = fingerprint.wrapping_mul(PRIME);
    }
    format!("{fingerprint:032x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{
        CodeActionContext, PartialResultParams, Position, Range, TextDocumentIdentifier,
        WorkDoneProgressParams,
    };

    #[test]
    fn diagnostic_data_is_bound_to_its_uri_and_version() {
        let first = Url::from_file_path(std::env::temp_dir().join("First.sol")).unwrap();
        let second = Url::from_file_path(std::env::temp_dir().join("Second.sol")).unwrap();
        let edit = TextEdit::new(Range::new(Position::new(0, 0), Position::new(0, 1)), "x".into());
        let data = DiagnosticData::new(
            first.clone(),
            "a",
            vec![DiagnosticSuggestion::new(
                "replace".into(),
                Applicability::MachineApplicable,
                vec![vec![edit]],
            )],
        )
        .to_value();

        let contents = Rope::from("a");
        let first_params = params(first.clone(), data.clone());
        assert_eq!(plans(&first_params, &first_params.context.diagnostics, &contents).len(), 1);
        let second_params = params(second, data.clone());
        assert!(plans(&second_params, &second_params.context.diagnostics, &contents).is_empty());

        let mut wrong_version = data.clone();
        wrong_version["version"] = serde_json::json!(2);
        let wrong_version = params(first.clone(), wrong_version);
        assert!(plans(&wrong_version, &wrong_version.context.diagnostics, &contents).is_empty());

        let mut extra_field = data;
        extra_field["unexpected"] = serde_json::json!(true);
        let extra_field = params(first, extra_field);
        assert!(plans(&extra_field, &extra_field.context.diagnostics, &contents).is_empty());
    }

    #[test]
    fn duplicate_diagnostics_use_server_data_without_dropping_fixes() {
        let uri = Url::from_file_path(std::env::temp_dir().join("Duplicate.sol")).unwrap();
        let data = |title: &str, start| {
            DiagnosticData::new(
                uri.clone(),
                "ab",
                vec![DiagnosticSuggestion::new(
                    title.into(),
                    Applicability::MachineApplicable,
                    vec![vec![TextEdit::new(
                        Range::new(Position::new(0, start), Position::new(0, start + 1)),
                        title.into(),
                    )]],
                )],
            )
            .to_value()
        };
        let mut requested = params(uri.clone(), data("first", 0));
        let first = requested.context.diagnostics[0].clone();
        let mut second = first.clone();
        second.data = Some(data("second", 1));
        let current = [first, second];
        let contents = Rope::from("ab");

        let exact = plans(&requested, &current, &contents);
        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].title, "first");

        requested.context.diagnostics[0].data = None;
        let without_data = plans(&requested, &current, &contents);
        assert_eq!(
            without_data.iter().map(|plan| plan.title.as_str()).collect::<Vec<_>>(),
            ["first", "second"]
        );

        requested.context.diagnostics[0].data = Some(serde_json::json!({ "modified": true }));
        let modified = plans(&requested, &current, &contents);
        assert_eq!(
            modified.iter().map(|plan| plan.title.as_str()).collect::<Vec<_>>(),
            ["first", "second"]
        );
    }

    #[test]
    fn diagnostic_matching_ignores_optional_presentation_fields() {
        let uri = Url::from_file_path(std::env::temp_dir().join("OptionalFields.sol")).unwrap();
        let data = DiagnosticData::new(
            uri.clone(),
            "a",
            vec![DiagnosticSuggestion::new(
                "replace".into(),
                Applicability::MachineApplicable,
                vec![vec![TextEdit::new(
                    Range::new(Position::new(0, 0), Position::new(0, 1)),
                    "b".into(),
                )]],
            )],
        )
        .to_value();
        let params = params(uri, data);
        let mut current = params.context.diagnostics[0].clone();
        current.code_description = Some(lsp_types::CodeDescription {
            href: Url::parse("https://example.invalid/diagnostic").unwrap(),
        });
        current.related_information = Some(Vec::new());
        current.tags = Some(Vec::new());

        assert_eq!(plans(&params, &[current], &Rope::from("a")).len(), 1);
    }

    fn params(uri: Url, data: serde_json::Value) -> CodeActionParams {
        CodeActionParams {
            text_document: TextDocumentIdentifier { uri },
            range: Range::default(),
            context: CodeActionContext {
                diagnostics: vec![Diagnostic {
                    source: Some("solar".into()),
                    data: Some(data),
                    ..Diagnostic::new_simple(Range::default(), "diagnostic".into())
                }],
                ..Default::default()
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        }
    }
}

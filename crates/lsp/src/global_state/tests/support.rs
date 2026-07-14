use super::super::{AnalysisBatch, AnalysisResult, GlobalState, analyze};
use crate::test_support::MarkedProject;
use async_lsp::{ClientSocket, ErrorCode};
use lsp_types::{
    CompletionItem, CompletionParams, CompletionResponse, GotoDefinitionParams,
    GotoDefinitionResponse, InlayHint, InlayHintKind, InlayHintLabel, InlayHintParams, Location,
    PartialResultParams, Position, PrepareRenameResponse, Range, ReferenceContext, ReferenceParams,
    RenameParams, TextDocumentIdentifier, TextDocumentPositionParams, Url, WorkDoneProgressParams,
    WorkspaceEdit,
};
use snapbox::{IntoData, assert_data_eq};
use solar_config::CompileOpts;
use solar_interface::data_structures::map::FxHashSet;
use std::{
    fmt::Write as _,
    future::Future,
    io::Read as _,
    path::Path,
    sync::Arc,
    task::{Context, Poll, Waker},
};

pub(super) struct RequestFixture {
    marked: MarkedProject,
    result: AnalysisResult,
}

impl RequestFixture {
    pub(super) fn new(fixture: &str, path: &str) -> Self {
        let fixture = Self::new_allowing_diagnostics(fixture, path);
        assert!(fixture.result.diagnostics.is_empty(), "{:#?}", fixture.result.diagnostics);
        fixture
    }

    pub(super) fn new_allowing_diagnostics(fixture: &str, path: &str) -> Self {
        let marked = MarkedProject::from_fixture(fixture);
        let contents = marked.project().read_file(path);
        let path = marked.project().path(path);
        let result = analyze(AnalysisBatch {
            opts: CompileOpts::default(),
            files: vec![(path, contents)],
            seen_paths: FxHashSet::default(),
        });
        Self { marked, result }
    }

    pub(super) fn new_in_batches(fixture: &str, paths: &[&str]) -> Self {
        let marked = MarkedProject::from_fixture(fixture);
        let mut result =
            AnalysisResult { diagnostics: Default::default(), symbol_tables: Default::default() };
        for path in paths {
            let contents = marked.project().read_file(path);
            let path = marked.project().path(path);
            let batch = analyze(AnalysisBatch {
                opts: CompileOpts::default(),
                files: vec![(path, contents)],
                seen_paths: FxHashSet::default(),
            });
            result.symbol_tables.extend(batch.symbol_tables);
            for (uri, mut diagnostics) in batch.diagnostics {
                result.diagnostics.entry(uri).or_default().append(&mut diagnostics);
            }
        }
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        Self { marked, result }
    }

    pub(super) fn check_completion(&self, marker: &str, expected: impl IntoData) {
        let mut state = self.state();
        let (uri, position) = self.marker_location(marker);
        let response =
            expect_ready(crate::handlers::completion(&mut state, completion_params(uri, position)))
                .unwrap()
                .unwrap();
        let CompletionResponse::Array(items) = response else {
            panic!("expected completion array");
        };
        assert_data_eq!(completion_output(&items), expected);
    }

    pub(super) fn check_goto_definition(&self, marker: &str, expected: impl IntoData) {
        let mut state = self.state();
        let (uri, position) = self.marker_location(marker);
        let response =
            expect_ready(crate::handlers::goto_definition(&mut state, goto_params(uri, position)))
                .unwrap();
        assert_data_eq!(self.goto_output(response), expected);
    }

    pub(super) fn check_goto_declaration(&self, marker: &str, expected: impl IntoData) {
        let mut state = self.state();
        let (uri, position) = self.marker_location(marker);
        let response =
            expect_ready(crate::handlers::goto_declaration(&mut state, goto_params(uri, position)))
                .unwrap();
        assert_data_eq!(self.goto_output(response), expected);
    }

    pub(super) fn check_references(
        &self,
        marker: &str,
        include_declaration: bool,
        expected: impl IntoData,
    ) {
        let mut state = self.state();
        let (uri, position) = self.marker_location(marker);
        let response = expect_ready(crate::handlers::references(
            &mut state,
            reference_params(uri, position, include_declaration),
        ))
        .unwrap();
        assert_data_eq!(self.locations_output(response), expected);
    }

    pub(super) fn check_prepare_rename(&self, marker: &str, expected: impl IntoData) {
        let mut state = self.state();
        let (uri, position) = self.marker_location(marker);
        let response = expect_ready(crate::handlers::prepare_rename(
            &mut state,
            text_document_position(uri, position),
        ))
        .unwrap();
        assert_data_eq!(prepare_rename_output(response), expected);
    }

    pub(super) fn check_rename(&self, marker: &str, new_name: &str, expected: impl IntoData) {
        let mut state = self.state();
        let (uri, position) = self.marker_location(marker);
        let response =
            block_on(crate::handlers::rename(&mut state, rename_params(uri, position, new_name)))
                .unwrap();
        assert_data_eq!(self.rename_output(response), expected);
    }

    pub(super) fn check_rename_error(&self, marker: &str, new_name: &str, expected: ErrorCode) {
        let mut state = self.state();
        let (uri, position) = self.marker_location(marker);
        let error =
            block_on(crate::handlers::rename(&mut state, rename_params(uri, position, new_name)))
                .expect_err("rename should fail");
        assert_eq!(error.code, expected);
    }

    pub(super) fn write_file(&self, path: &str, contents: &str) {
        self.marked.project().write_file(path, contents);
    }

    pub(super) fn set_open_file_contents(&mut self, path: &str, contents: &str) {
        self.marked.project_mut().open_file(path, contents);
    }

    pub(super) fn check_inlay_hints(&self, path: &str, expected: impl IntoData) {
        let uri = Url::from_file_path(self.marked.project().path(path)).unwrap();
        assert_data_eq!(inlay_hint_output(&self.inlay_hints(uri, full_range())), expected);
    }

    pub(super) fn check_inlay_hints_between(
        &self,
        start_marker: &str,
        end_marker: &str,
        expected: impl IntoData,
    ) {
        let (start_uri, start) = self.marker_location(start_marker);
        let (end_uri, end) = self.marker_location(end_marker);
        assert_eq!(start_uri, end_uri);
        assert_data_eq!(
            inlay_hint_output(&self.inlay_hints(start_uri, Range { start, end })),
            expected
        );
    }

    fn inlay_hints(&self, uri: Url, range: Range) -> Vec<InlayHint> {
        let mut state = self.state();
        let response =
            expect_ready(crate::handlers::inlay_hints(&mut state, inlay_hint_params(uri, range)))
                .unwrap();
        response.unwrap_or_default()
    }

    fn state(&self) -> GlobalState {
        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(self.marked.project().config());
        *state.vfs.write() = self.marked.project().vfs();
        *state.symbol_tables.write() = self.result.symbol_tables.clone();
        state
    }

    fn marker_location(&self, marker: &str) -> (Url, Position) {
        let marker = self.marked.marker(marker);
        let path = self.marked.project().path(marker.path());
        (Url::from_file_path(path).unwrap(), marker.position())
    }

    fn goto_output(&self, response: Option<GotoDefinitionResponse>) -> String {
        match response {
            Some(GotoDefinitionResponse::Array(locations)) => {
                self.locations_output(Some(locations))
            }
            Some(GotoDefinitionResponse::Scalar(location)) => {
                self.locations_output(Some(vec![location]))
            }
            Some(GotoDefinitionResponse::Link(links)) => {
                let locations = links
                    .into_iter()
                    .map(|link| Location { uri: link.target_uri, range: link.target_range })
                    .collect();
                self.locations_output(Some(locations))
            }
            None => "<none>\n".to_string(),
        }
    }

    fn locations_output(&self, response: Option<Vec<Location>>) -> String {
        let Some(locations) = response else { return "<none>\n".to_string() };
        let mut output = String::new();
        for location in locations {
            writeln!(output, "{}", self.location_output(location)).unwrap();
        }
        output
    }

    fn location_output(&self, location: Location) -> String {
        let path = location.uri.to_file_path().unwrap();
        let display_path = display_path(self.marked.project().root(), &path);
        let line = read_file(&path)
            .and_then(|contents| {
                contents.lines().nth(location.range.start.line as usize).map(str::to_owned)
            })
            .unwrap_or_default();
        format!(
            "{display_path}:{}:{} {}",
            location.range.start.line,
            location.range.start.character,
            line.trim()
        )
    }

    fn rename_output(&self, response: Option<WorkspaceEdit>) -> String {
        let Some(edit) = response else { return "<none>\n".to_string() };
        assert!(edit.document_changes.is_none());
        assert!(edit.change_annotations.is_none());

        let mut changes = edit.changes.unwrap_or_default().into_iter().collect::<Vec<_>>();
        changes.sort_by(|(a, _), (b, _)| a.as_str().cmp(b.as_str()));

        let mut output = String::new();
        for (uri, mut edits) in changes {
            edits.sort_by_key(|edit| {
                (edit.range.start.line, edit.range.start.character, edit.range.end)
            });
            let path = uri.to_file_path().unwrap();
            let display_path = display_path(self.marked.project().root(), &path);
            for edit in edits {
                writeln!(
                    output,
                    "{display_path}:{}:{}-{}:{} -> {}",
                    edit.range.start.line,
                    edit.range.start.character,
                    edit.range.end.line,
                    edit.range.end.character,
                    edit.new_text,
                )
                .unwrap();
            }
        }
        output
    }
}

fn expect_ready<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("request handler future should complete immediately"),
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(future)
}

fn read_file(path: &Path) -> Option<String> {
    let mut contents = String::new();
    std::fs::File::open(path).ok()?.read_to_string(&mut contents).ok()?;
    Some(contents)
}

fn completion_output(items: &[CompletionItem]) -> String {
    let mut output = String::new();
    for item in items {
        let kind = item.kind.map(|kind| format!("{kind:?}")).unwrap_or_else(|| "UNKNOWN".into());
        writeln!(output, "{} {kind}", item.label).unwrap();
    }
    output
}

fn inlay_hint_output(hints: &[InlayHint]) -> String {
    let mut output = String::new();
    for hint in hints {
        writeln!(output, "{} {}", inlay_hint_kind(hint.kind), inlay_hint_label(&hint.label))
            .unwrap();
    }
    output
}

fn prepare_rename_output(response: Option<PrepareRenameResponse>) -> String {
    let Some(response) = response else { return "<none>\n".to_string() };
    let range = match response {
        PrepareRenameResponse::Range(range) => range,
        PrepareRenameResponse::RangeWithPlaceholder { range, .. } => range,
        PrepareRenameResponse::DefaultBehavior { .. } => return "<default>\n".to_string(),
    };
    format!(
        "{}:{}-{}:{}\n",
        range.start.line, range.start.character, range.end.line, range.end.character
    )
}

fn inlay_hint_kind(kind: Option<InlayHintKind>) -> &'static str {
    match kind {
        Some(InlayHintKind::PARAMETER) => "PARAMETER",
        Some(InlayHintKind::TYPE) => "TYPE",
        _ => "UNKNOWN",
    }
}

fn inlay_hint_label(label: &InlayHintLabel) -> String {
    match label {
        InlayHintLabel::String(label) => label.clone(),
        InlayHintLabel::LabelParts(parts) => parts.iter().map(|part| part.value.as_str()).collect(),
    }
}

fn display_path(root: &Path, path: &Path) -> String {
    let path = path.strip_prefix(root).unwrap_or(path);
    format!("/{}", path.display())
}

fn completion_params(uri: Url, position: Position) -> CompletionParams {
    CompletionParams {
        text_document_position: text_document_position(uri, position),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: None,
    }
}

fn goto_params(uri: Url, position: Position) -> GotoDefinitionParams {
    GotoDefinitionParams {
        text_document_position_params: text_document_position(uri, position),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }
}

fn reference_params(uri: Url, position: Position, include_declaration: bool) -> ReferenceParams {
    ReferenceParams {
        text_document_position: text_document_position(uri, position),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: ReferenceContext { include_declaration },
    }
}

fn rename_params(uri: Url, position: Position, new_name: &str) -> RenameParams {
    RenameParams {
        text_document_position: text_document_position(uri, position),
        new_name: new_name.into(),
        work_done_progress_params: WorkDoneProgressParams::default(),
    }
}

fn inlay_hint_params(uri: Url, range: Range) -> InlayHintParams {
    InlayHintParams {
        text_document: TextDocumentIdentifier { uri },
        range,
        work_done_progress_params: WorkDoneProgressParams::default(),
    }
}

fn full_range() -> Range {
    Range { start: Position::new(0, 0), end: Position::new(u32::MAX, u32::MAX) }
}

fn text_document_position(uri: Url, position: Position) -> TextDocumentPositionParams {
    TextDocumentPositionParams { text_document: TextDocumentIdentifier { uri }, position }
}

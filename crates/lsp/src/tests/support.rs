use super::super::{AnalysisBatch, AnalysisResult, GlobalState, analyze};
use crate::test_support::MarkedProject;
use async_lsp::{ClientSocket, ErrorCode};
use lsp_types::{
    CompletionContext, CompletionItem, CompletionParams, CompletionResponse, CompletionTextEdit,
    CompletionTriggerKind, DocumentHighlight, DocumentHighlightKind, DocumentHighlightParams,
    DocumentLink, DocumentLinkParams, Documentation, GotoDefinitionParams, GotoDefinitionResponse,
    Hover, HoverContents, HoverParams, InlayHint, InlayHintKind, InlayHintLabel, InlayHintParams,
    Location, MarkupKind, ParameterLabel, PartialResultParams, Position, PrepareRenameResponse,
    Range, ReferenceContext, ReferenceParams, RenameParams, SelectionRange, SelectionRangeParams,
    SignatureHelp, SignatureHelpParams, TextDocumentIdentifier, TextDocumentPositionParams, Url,
    WorkDoneProgressParams, WorkspaceEdit,
};
use snapbox::{IntoData, assert_data_eq};
use solar_config::CompileOpts;
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
        let result = analyze(AnalysisBatch::from_files(CompileOpts::default(), [(path, contents)]));
        Self { marked, result }
    }

    pub(super) fn new_in_batches(fixture: &str, paths: &[&str]) -> Self {
        let marked = MarkedProject::from_fixture(fixture);
        Self::analyze_batches(marked, paths, None)
    }

    pub(super) fn new_in_batches_with_stale_disk(
        fixture: &str,
        open_path: &str,
        disk_contents: &str,
        paths: &[&str],
    ) -> Self {
        let marked = MarkedProject::from_fixture(fixture);
        let open_contents = marked.project().read_file(open_path);
        marked.project().write_file(open_path, disk_contents);
        Self::analyze_batches(marked, paths, Some((open_path, open_contents)))
    }

    fn analyze_batches(
        marked: MarkedProject,
        paths: &[&str],
        open_file: Option<(&str, String)>,
    ) -> Self {
        let mut result =
            AnalysisResult { diagnostics: Default::default(), symbol_tables: Default::default() };
        for path in paths {
            let contents = open_file
                .as_ref()
                .filter(|(open_path, _)| open_path == path)
                .map_or_else(|| marked.project().read_file(path), |(_, contents)| contents.clone());
            let path = marked.project().path(path);
            let batch =
                analyze(AnalysisBatch::from_files(CompileOpts::default(), [(path, contents)]));
            result.symbol_tables.extend(batch.symbol_tables);
            for (uri, mut diagnostics) in batch.diagnostics {
                result.diagnostics.entry(uri).or_default().append(&mut diagnostics);
            }
        }
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        Self { marked, result }
    }

    pub(super) fn project_contents(&self, path: &str) -> String {
        self.marked.project().read_file(path)
    }

    pub(super) fn rename_state_and_params(
        &self,
        marker: &str,
        new_name: &str,
    ) -> (GlobalState, RenameParams) {
        let (uri, position) = self.marker_location(marker);
        (self.state(), rename_params(uri, position, new_name))
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

    pub(super) fn check_completion_details(&self, marker: &str, expected: impl IntoData) {
        self.check_completion_details_with_snippets(marker, true, expected);
    }

    pub(super) fn check_completion_details_with_snippets(
        &self,
        marker: &str,
        snippet_support: bool,
        expected: impl IntoData,
    ) {
        let mut state = self.state_with_completion_snippets(snippet_support);
        let (uri, position) = self.marker_location(marker);
        let response =
            expect_ready(crate::handlers::completion(&mut state, completion_params(uri, position)))
                .unwrap()
                .unwrap();
        let CompletionResponse::Array(items) = response else {
            panic!("expected completion array");
        };
        assert_data_eq!(completion_details_output(&items), expected);
    }

    pub(super) fn check_completion_details_with_trigger(
        &self,
        marker: &str,
        trigger_character: &str,
        expected: impl IntoData,
    ) {
        let mut state = self.state_with_completion_snippets(true);
        let (uri, position) = self.marker_location(marker);
        let response = expect_ready(crate::handlers::completion(
            &mut state,
            completion_params_with_trigger(uri, position, trigger_character),
        ))
        .unwrap()
        .unwrap();
        let CompletionResponse::Array(items) = response else {
            panic!("expected completion array");
        };
        assert_data_eq!(completion_details_output(&items), expected);
    }

    pub(super) fn check_completion_details_after_change(
        &self,
        marker: &str,
        path: &str,
        changed_contents: &str,
        expected: impl IntoData,
    ) {
        let mut state = self.state_with_completion_snippets(true);
        let path = self.marked.project().path(path);
        state.mark_source_analysis_pending_for_test(path.clone());
        let uri = Url::from_file_path(&path).unwrap();
        state.vfs.write().set_file_contents(
            crate::vfs::VfsPath::from(path),
            Some(crop::Rope::from(changed_contents)),
        );
        let position = self.marked.marker(marker).position();
        let response =
            expect_ready(crate::handlers::completion(&mut state, completion_params(uri, position)))
                .unwrap()
                .unwrap();
        let CompletionResponse::Array(items) = response else {
            panic!("expected completion array");
        };
        assert_data_eq!(completion_details_output(&items), expected);
    }

    pub(super) fn check_completion_details_after_changes(
        &self,
        marker: &str,
        request_path: &str,
        changes: &[(&str, &str)],
        expected: impl IntoData,
    ) {
        let mut state = self.state_with_completion_snippets(true);
        for &(path, contents) in changes {
            let path = self.marked.project().path(path);
            state.mark_source_analysis_pending_for_test(path.clone());
            state.vfs.write().set_file_contents(
                crate::vfs::VfsPath::from(path),
                Some(crop::Rope::from(contents)),
            );
        }
        let uri = Url::from_file_path(self.marked.project().path(request_path)).unwrap();
        let position = self.marked.marker(marker).position();
        let response =
            expect_ready(crate::handlers::completion(&mut state, completion_params(uri, position)))
                .unwrap()
                .unwrap();
        let CompletionResponse::Array(items) = response else {
            panic!("expected completion array");
        };
        assert_data_eq!(completion_details_output(&items), expected);
    }

    pub(super) fn check_completion_details_after_deleted_source(
        &self,
        marker: &str,
        request_path: &str,
        deleted_path: &str,
        expected: impl IntoData,
    ) {
        let mut state = self.state_with_completion_snippets(true);
        let deleted_path = self.marked.project().path(deleted_path);
        state.mark_source_analysis_pending_for_test(deleted_path.clone());
        std::fs::remove_file(deleted_path).unwrap();
        let uri = Url::from_file_path(self.marked.project().path(request_path)).unwrap();
        let position = self.marked.marker(marker).position();
        let response =
            expect_ready(crate::handlers::completion(&mut state, completion_params(uri, position)))
                .unwrap()
                .unwrap();
        let CompletionResponse::Array(items) = response else {
            panic!("expected completion array");
        };
        assert_data_eq!(completion_details_output(&items), expected);
    }

    pub(super) fn check_completion_details_after_context_change(
        &self,
        marker: &str,
        expected: impl IntoData,
    ) {
        let mut state = self.state_with_completion_snippets(true);
        state.mark_context_analysis_pending_for_test();
        let (uri, position) = self.marker_location(marker);
        let response =
            expect_ready(crate::handlers::completion(&mut state, completion_params(uri, position)))
                .unwrap()
                .unwrap();
        let CompletionResponse::Array(items) = response else {
            panic!("expected completion array");
        };
        assert_data_eq!(completion_details_output(&items), expected);
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

    pub(super) fn check_goto_implementation(&self, marker: &str, expected: impl IntoData) {
        let mut state = self.state();
        let (uri, position) = self.marker_location(marker);
        let response = expect_ready(crate::handlers::goto_implementation(
            &mut state,
            goto_params(uri, position),
        ))
        .unwrap();
        assert_data_eq!(self.goto_output(response), expected);
    }

    pub(super) fn check_goto_type_definition(&self, marker: &str, expected: impl IntoData) {
        let mut state = self.state();
        let (uri, position) = self.marker_location(marker);
        let response = expect_ready(crate::handlers::goto_type_definition(
            &mut state,
            goto_params(uri, position),
        ))
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

    pub(super) fn check_document_highlights(&self, marker: &str, expected: impl IntoData) {
        let mut state = self.state();
        let (uri, position) = self.marker_location(marker);
        let response = expect_ready(crate::handlers::document_highlight(
            &mut state,
            document_highlight_params(uri, position),
        ))
        .unwrap();
        assert_data_eq!(document_highlight_output(response), expected);
    }

    pub(super) fn check_hover(&self, marker: &str, expected: impl IntoData) {
        let mut state = self.state();
        let (uri, position) = self.marker_location(marker);
        let response =
            expect_ready(crate::handlers::hover(&mut state, hover_params(uri, position))).unwrap();
        assert_data_eq!(hover_output(response), expected);
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

    pub(super) fn check_document_links(&self, path: &str, expected: impl IntoData) {
        let mut state = self.state();
        let uri = Url::from_file_path(self.marked.project().path(path)).unwrap();
        let links =
            expect_ready(crate::handlers::document_links(&mut state, document_link_params(uri)))
                .unwrap()
                .unwrap_or_default();
        assert_data_eq!(self.document_links_output(links), expected);
    }

    pub(super) fn check_selection_ranges(&self, markers: &[&str], expected: impl IntoData) {
        let mut state = self.state();
        let (params, positions) = self.selection_range_request(markers);
        let response =
            expect_ready(crate::handlers::selection_range(&mut state, params)).unwrap().unwrap();
        check_selection_range_response(response, &positions, expected);
    }

    pub(super) fn check_selection_ranges_from_disk(
        &self,
        markers: &[&str],
        expected: impl IntoData,
    ) {
        let mut state = self.state();
        let (params, positions) = self.selection_range_request(markers);
        let response =
            block_on(crate::handlers::selection_range(&mut state, params)).unwrap().unwrap();
        check_selection_range_response(response, &positions, expected);
    }

    pub(super) fn check_selection_ranges_while_analysis_pending(
        &self,
        markers: &[&str],
        expected: impl IntoData,
    ) {
        let mut state = self.state();
        state.mark_analysis_pending_for_test();
        let (params, positions) = self.selection_range_request(markers);
        let response =
            expect_ready(crate::handlers::selection_range(&mut state, params)).unwrap().unwrap();
        check_selection_range_response(response, &positions, expected);
    }

    pub(super) fn check_selection_range_error(
        &self,
        path: &str,
        positions: Vec<Position>,
        expected: ErrorCode,
    ) {
        let mut state = self.state();
        let uri = Url::from_file_path(self.marked.project().path(path)).unwrap();
        let params = selection_range_params(uri, positions);
        let error = expect_ready(crate::handlers::selection_range(&mut state, params))
            .expect_err("selection-range request should fail");
        assert_eq!(error.code, expected);
        assert!(!error.message.ends_with('.'));
    }

    pub(super) fn check_signature_help(&self, marker: &str, expected: impl IntoData) {
        let mut state = self.state();
        let (uri, position) = self.marker_location(marker);
        self.check_signature_help_in_state(&mut state, uri, position, expected);
    }

    pub(super) fn check_signature_help_without_label_offsets(
        &self,
        marker: &str,
        expected: impl IntoData,
    ) {
        let mut state = self.state_with_label_offsets(false);
        let (uri, position) = self.marker_location(marker);
        self.check_signature_help_in_state(&mut state, uri, position, expected);
    }

    pub(super) fn signature_help_response(&self, marker: &str) -> Option<SignatureHelp> {
        let mut state = self.state();
        let (uri, position) = self.marker_location(marker);
        expect_ready(crate::handlers::signature_help(
            &mut state,
            signature_help_params(uri, position),
        ))
        .unwrap()
    }

    pub(super) fn check_signature_help_after_change(
        &self,
        marker: &str,
        path: &str,
        changed_contents: &str,
        expected: impl IntoData,
    ) {
        let path = self.marked.project().path(path);
        let uri = Url::from_file_path(&path).unwrap();
        let result = analyze(AnalysisBatch::from_files(
            CompileOpts::default(),
            [(path.clone(), changed_contents.to_string())],
        ));
        assert!(!result.diagnostics.is_empty(), "changed source should fail analysis");

        let mut state = self.state();
        state.vfs.write().set_file_contents(
            crate::vfs::VfsPath::from(path),
            Some(crop::Rope::from(changed_contents)),
        );
        *state.symbol_tables.write() = result.symbol_tables;
        let position = self.marked.marker(marker).position();
        self.check_signature_help_in_state(&mut state, uri, position, expected);
    }

    fn check_signature_help_in_state(
        &self,
        state: &mut GlobalState,
        uri: Url,
        position: Position,
        expected: impl IntoData,
    ) {
        let response = expect_ready(crate::handlers::signature_help(
            state,
            signature_help_params(uri, position),
        ))
        .unwrap();
        assert_data_eq!(signature_help_output(response), expected);
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
        self.state_with_label_offsets(true)
    }

    fn state_with_completion_snippets(&self, completion_snippets: bool) -> GlobalState {
        let mut state = self.state_with_label_offsets(true);
        if completion_snippets {
            Arc::make_mut(&mut state.config).enable_completion_snippets();
        }
        state
    }

    fn state_with_label_offsets(&self, label_offsets: bool) -> GlobalState {
        let mut state = GlobalState::new(ClientSocket::new_closed());
        let mut config = self.marked.project().config();
        if label_offsets {
            config.enable_signature_help_label_offsets();
        }
        state.config = Arc::new(config);
        *state.vfs.write() = self.marked.project().vfs();
        *state.symbol_tables.write() = self.result.symbol_tables.clone();
        state
    }

    fn marker_location(&self, marker: &str) -> (Url, Position) {
        let marker = self.marked.marker(marker);
        let path = self.marked.project().path(marker.path());
        (Url::from_file_path(path).unwrap(), marker.position())
    }

    fn selection_range_request(&self, markers: &[&str]) -> (SelectionRangeParams, Vec<Position>) {
        let mut uri = None;
        let positions = markers
            .iter()
            .map(|marker| {
                let (marker_uri, position) = self.marker_location(marker);
                if let Some(uri) = &uri {
                    assert_eq!(uri, &marker_uri, "selection-range markers must be in one document");
                } else {
                    uri = Some(marker_uri);
                }
                position
            })
            .collect::<Vec<_>>();
        let params = selection_range_params(
            uri.expect("at least one marker is required"),
            positions.clone(),
        );
        (params, positions)
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

    fn document_links_output(&self, links: Vec<DocumentLink>) -> String {
        let mut output = String::new();
        for link in links {
            let target = link.target.unwrap().to_file_path().unwrap();
            let target = display_path(self.marked.project().root(), &target);
            writeln!(
                output,
                "{}:{}..{}:{} -> {target}",
                link.range.start.line,
                link.range.start.character,
                link.range.end.line,
                link.range.end.character,
            )
            .unwrap();
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

fn completion_details_output(items: &[CompletionItem]) -> String {
    let mut output = String::new();
    for (index, item) in items.iter().enumerate() {
        let kind = item.kind.map(|kind| format!("{kind:?}")).unwrap_or_else(|| "<none>".into());
        let insert_text_format = item
            .insert_text_format
            .map(|format| format!("{format:?}"))
            .unwrap_or_else(|| "<none>".into());
        let (text_edit, new_text) = match &item.text_edit {
            Some(CompletionTextEdit::Edit(edit)) => {
                (format!("edit {}", range_output(edit.range)), edit.new_text.as_str())
            }
            Some(CompletionTextEdit::InsertAndReplace(edit)) => (
                format!(
                    "insert-and-replace insert={} replace={}",
                    range_output(edit.insert),
                    range_output(edit.replace)
                ),
                edit.new_text.as_str(),
            ),
            None => ("<none>".into(), "<none>"),
        };

        writeln!(output, "label={}", item.label).unwrap();
        writeln!(output, "kind={kind}").unwrap();
        writeln!(output, "detail={}", item.detail.as_deref().unwrap_or("<none>")).unwrap();
        writeln!(output, "sort_text={}", item.sort_text.as_deref().unwrap_or("<none>")).unwrap();
        writeln!(output, "text_edit={text_edit}").unwrap();
        if let Some(edits) = &item.additional_text_edits {
            for edit in edits {
                writeln!(
                    output,
                    "additional_text_edit={} new_text={:?}",
                    range_output(edit.range),
                    edit.new_text
                )
                .unwrap();
            }
        }
        writeln!(output, "insert_text_format={insert_text_format}").unwrap();
        writeln!(output, "new_text:").unwrap();
        writeln!(output, "{new_text}").unwrap();
        if index + 1 < items.len() {
            writeln!(output).unwrap();
        }
    }
    output
}

fn range_output(range: Range) -> String {
    format!(
        "{}:{}-{}:{}",
        range.start.line, range.start.character, range.end.line, range.end.character
    )
}

fn selection_range_output(ranges: &[SelectionRange], positions: &[Position]) -> String {
    assert_eq!(ranges.len(), positions.len());
    let mut output = String::new();
    for (index, (selection, position)) in ranges.iter().zip(positions).enumerate() {
        writeln!(output, "{index}:").unwrap();
        assert!(range_contains_position(selection.range, *position));
        let mut current = Some(selection);
        while let Some(selection) = current {
            writeln!(output, "  {}", range_output(selection.range)).unwrap();
            if let Some(parent) = selection.parent.as_deref() {
                assert_ne!(parent.range, selection.range);
                assert!(range_contains_range(parent.range, selection.range));
            }
            current = selection.parent.as_deref();
        }
    }
    output
}

fn check_selection_range_response(
    response: Vec<SelectionRange>,
    positions: &[Position],
    expected: impl IntoData,
) {
    assert_eq!(response.len(), positions.len());
    assert_data_eq!(selection_range_output(&response, positions), expected);
}

fn range_contains_position(range: Range, position: Position) -> bool {
    range.start <= position && position <= range.end
}

fn range_contains_range(outer: Range, inner: Range) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
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

fn document_highlight_output(response: Option<Vec<DocumentHighlight>>) -> String {
    let Some(highlights) = response else { return "<none>\n".to_string() };
    let mut output = String::new();
    for highlight in highlights {
        writeln!(
            output,
            "{}:{}-{}:{} {}",
            highlight.range.start.line,
            highlight.range.start.character,
            highlight.range.end.line,
            highlight.range.end.character,
            document_highlight_kind(highlight.kind),
        )
        .unwrap();
    }
    output
}

fn hover_output(response: Option<Hover>) -> String {
    let Some(hover) = response else { return "<none>\n".to_string() };
    let range = hover.range.expect("hover response should include the current identifier range");
    let HoverContents::Markup(contents) = hover.contents else {
        panic!("hover response should contain markup");
    };
    assert_eq!(contents.kind, MarkupKind::Markdown);
    format!(
        "{}:{}-{}:{}\n{}\n",
        range.start.line,
        range.start.character,
        range.end.line,
        range.end.character,
        contents.value,
    )
}

fn document_highlight_kind(kind: Option<DocumentHighlightKind>) -> &'static str {
    match kind {
        Some(DocumentHighlightKind::TEXT) => "TEXT",
        Some(DocumentHighlightKind::READ) => "READ",
        Some(DocumentHighlightKind::WRITE) => "WRITE",
        Some(_) | None => "UNKNOWN",
    }
}

fn signature_help_output(help: Option<SignatureHelp>) -> String {
    let Some(help) = help else { return "<none>\n".to_string() };
    let mut output = String::new();
    writeln!(
        output,
        "active signature={:?} parameter={:?}",
        help.active_signature, help.active_parameter
    )
    .unwrap();
    for signature in help.signatures {
        writeln!(output, "{}", signature.label).unwrap();
        if let Some(documentation) = signature.documentation {
            writeln!(output, "  docs={}", documentation_text(&documentation).replace('\n', " | "))
                .unwrap();
        }
        if let Some(parameters) = signature.parameters {
            for parameter in parameters {
                match parameter.label {
                    ParameterLabel::Simple(label) => write!(output, "  {label}").unwrap(),
                    ParameterLabel::LabelOffsets([start, end]) => {
                        write!(output, "  {start}..{end}").unwrap()
                    }
                }
                if let Some(documentation) = parameter.documentation {
                    write!(
                        output,
                        " docs={}",
                        documentation_text(&documentation).replace('\n', " | ")
                    )
                    .unwrap();
                }
                writeln!(output).unwrap();
            }
        }
    }
    output
}

fn documentation_text(documentation: &Documentation) -> &str {
    match documentation {
        Documentation::String(value) => value,
        Documentation::MarkupContent(content) => &content.value,
    }
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

fn completion_params_with_trigger(
    uri: Url,
    position: Position,
    trigger_character: &str,
) -> CompletionParams {
    CompletionParams {
        context: Some(CompletionContext {
            trigger_kind: CompletionTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(trigger_character.into()),
        }),
        ..completion_params(uri, position)
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

fn document_highlight_params(uri: Url, position: Position) -> DocumentHighlightParams {
    DocumentHighlightParams {
        text_document_position_params: text_document_position(uri, position),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }
}

fn hover_params(uri: Url, position: Position) -> HoverParams {
    HoverParams {
        text_document_position_params: text_document_position(uri, position),
        work_done_progress_params: WorkDoneProgressParams::default(),
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

fn document_link_params(uri: Url) -> DocumentLinkParams {
    DocumentLinkParams {
        text_document: TextDocumentIdentifier { uri },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }
}

fn signature_help_params(uri: Url, position: Position) -> SignatureHelpParams {
    SignatureHelpParams {
        context: None,
        text_document_position_params: text_document_position(uri, position),
        work_done_progress_params: WorkDoneProgressParams::default(),
    }
}

fn selection_range_params(uri: Url, positions: Vec<Position>) -> SelectionRangeParams {
    SelectionRangeParams {
        text_document: TextDocumentIdentifier { uri },
        positions,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }
}

fn full_range() -> Range {
    Range { start: Position::new(0, 0), end: Position::new(u32::MAX, u32::MAX) }
}

fn text_document_position(uri: Url, position: Position) -> TextDocumentPositionParams {
    TextDocumentPositionParams { text_document: TextDocumentIdentifier { uri }, position }
}

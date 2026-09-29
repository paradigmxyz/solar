use super::*;
use lsp_types::{
    CodeLens, CompletionResponse, CompletionTextEdit, DocumentHighlight, DocumentHighlightKind,
    DocumentLink, Documentation, FoldingRange, FoldingRangeKind, GotoDefinitionResponse, Hover,
    HoverContents, InlayHint, InlayHintKind, InlayHintLabel, Location, MarkupKind, ParameterLabel,
    PrepareRenameResponse, RenameParams, SelectionRange, SelectionRangeParams, SignatureHelp,
    TypeHierarchyItem, WorkspaceEdit,
};
use snapbox::{IntoData, assert_data_eq};
use std::{fmt::Write as _, io::Read as _, pin::Pin};

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
        let result = analyze_source(path, contents);
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
        let mut results = AnalysisResultAccumulator::default();
        for path in paths {
            let contents = open_file
                .as_ref()
                .filter(|(open_path, _)| open_path == path)
                .map_or_else(|| marked.project().read_file(path), |(_, contents)| contents.clone());
            let path = marked.project().path(path);
            results.push(analyze_source(path, contents));
        }
        let result = results.finish();
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        Self { marked, result }
    }

    pub(super) fn project(&self) -> &TestProject {
        self.marked.project()
    }

    pub(super) fn project_contents(&self, path: &str) -> String {
        self.marked.project().read_file(path)
    }

    pub(super) fn project_path(&self, path: &str) -> std::path::PathBuf {
        self.marked.project().path(path)
    }

    pub(super) fn rename_state_and_params(
        &self,
        marker: &str,
        new_name: &str,
    ) -> (GlobalState, RenameParams) {
        let (uri, position) = self.marker_location(marker);
        (self.state(), rename_params(&uri, position, new_name))
    }

    pub(super) fn rename_state_with_roots(
        &self,
        marker: &str,
        new_name: &str,
        roots: &[&str],
    ) -> (GlobalState, RenameParams) {
        let (mut state, params) = self.rename_state_and_params(marker, new_name);
        state.config = Arc::new(self.marked.project().config_with_roots(roots));
        (state, params)
    }

    pub(super) fn check_completions(&self, markers: &[&str], expected: impl IntoData) {
        self.check_completions_in(&mut self.completion_state(), markers, expected);
    }

    pub(super) fn check_completions_in(
        &self,
        state: &mut GlobalState,
        markers: &[&str],
        expected: impl IntoData,
    ) {
        let requests = markers.iter().map(|&marker| (marker, None)).collect::<Vec<_>>();
        self.check_completion_requests(state, &requests, expected);
    }

    pub(super) fn check_triggered_completions(
        &self,
        requests: &[(&str, &str)],
        expected: impl IntoData,
    ) {
        let requests =
            requests.iter().map(|&(marker, trigger)| (marker, Some(trigger))).collect::<Vec<_>>();
        self.check_completion_requests(&mut self.completion_state(), &requests, expected);
    }

    fn check_completion_requests(
        &self,
        state: &mut GlobalState,
        requests: &[(&str, Option<&str>)],
        expected: impl IntoData,
    ) {
        let requests = requests.iter().map(|&(marker, trigger)| {
            let (uri, position) = self.marker_location(marker);
            (marker.to_string(), uri, position, trigger)
        });
        check_completions_at(state, requests, expected);
    }

    /// Returns a snippet-capable state whose open files changed after the stored analysis.
    pub(super) fn completion_state_after_changes(&self, changes: &[(&str, &str)]) -> GlobalState {
        let state = self.completion_state();
        for &(path, contents) in changes {
            let path = self.marked.project().path(path);
            state.mark_source_analysis_pending_for_test(path.clone());
            set_overlay(&state, &path, contents, None);
        }
        state
    }

    pub(super) fn check_goto_definition(&self, marker: &str, expected: impl IntoData) {
        assert_data_eq!(self.query_output(Query::Definition, marker), expected);
    }

    /// Checks every query at every numbered marker. Each answer starts with its marker and, when
    /// several queries are checked, the query label.
    pub(super) fn check_queries(
        &self,
        queries: &[Query],
        markers: impl IntoIterator<Item = usize>,
        expected: impl IntoData,
    ) {
        let mut output = String::new();
        for marker in markers {
            let marker = format!("${marker}");
            for &query in queries {
                let label =
                    if queries.len() == 1 { String::new() } else { format!(" {}:", query.label()) };
                write!(output, "{marker}{label} {}", self.query_output(query, &marker)).unwrap();
            }
        }
        assert_data_eq!(output, expected);
    }

    fn query_output(&self, query: Query, marker: &str) -> String {
        let mut state = self.state();
        let (uri, position) = self.marker_location(marker);
        self.response_output(expect_ready(query.request(&mut state, uri, position)).unwrap())
    }

    /// Sends a query at a marker to `state` and formats its answer like [`Self::check_queries`].
    pub(super) async fn query_in(
        &self,
        state: &mut GlobalState,
        query: Query,
        marker: &str,
    ) -> String {
        let (uri, position) = self.marker_location(marker);
        self.response_output(query.request(state, uri, position).await.unwrap())
    }

    pub(super) fn response_output(&self, response: QueryResponse) -> String {
        match response {
            QueryResponse::Goto(response) => self.goto_output(response),
            QueryResponse::Locations(response) => self.locations_output(response),
            QueryResponse::Highlights(response) => document_highlight_output(response),
            QueryResponse::Hover(response) => hover_output(response),
        }
    }

    pub(super) fn prepare_type_hierarchy(&self, marker: &str) -> Option<Vec<TypeHierarchyItem>> {
        let mut state = self.state();
        let (uri, position) = self.marker_location(marker);
        expect_ready(crate::handlers::prepare_type_hierarchy(
            &mut state,
            request_params(&uri, position, json!({})),
        ))
        .unwrap()
    }

    pub(super) fn type_hierarchy_supertypes(
        &self,
        item: TypeHierarchyItem,
    ) -> Option<Vec<TypeHierarchyItem>> {
        let mut state = self.state();
        expect_ready(crate::handlers::type_hierarchy_supertypes(
            &mut state,
            from_json(json!({ "item": item })),
        ))
        .unwrap()
    }

    pub(super) fn type_hierarchy_subtypes(
        &self,
        item: TypeHierarchyItem,
    ) -> Option<Vec<TypeHierarchyItem>> {
        let mut state = self.state();
        expect_ready(crate::handlers::type_hierarchy_subtypes(
            &mut state,
            from_json(json!({ "item": item })),
        ))
        .unwrap()
    }

    pub(super) fn check_references(
        &self,
        marker: &str,
        include_declaration: bool,
        expected: impl IntoData,
    ) {
        assert_data_eq!(
            self.query_output(Query::References(include_declaration), marker),
            expected
        );
    }

    pub(super) fn check_code_lenses(&self, path: &str, expected: impl IntoData) {
        assert_data_eq!(code_lens_output(&self.code_lenses(path, true)), expected);
    }

    pub(super) fn check_code_lenses_without_commands(&self, path: &str, expected: impl IntoData) {
        assert_data_eq!(code_lens_output(&self.code_lenses(path, false)), expected);
    }

    /// Checks the exact serialized protocol form, one compact JSON lens per line.
    pub(super) fn check_code_lenses_json(&self, path: &str, expected: impl IntoData) {
        let mut output = String::new();
        for lens in self.code_lenses(path, true) {
            writeln!(output, "{}", serde_json::to_string(&lens).unwrap()).unwrap();
        }
        let output = output.replace(self.path_uri(path).as_str(), &format!("file://{path}"));
        assert_data_eq!(output, expected);
    }

    fn code_lenses(&self, path: &str, client_commands: bool) -> Vec<CodeLens> {
        let mut state = self.state();
        if client_commands {
            Arc::make_mut(&mut state.config).enable_code_lens_client_commands();
        }
        let params = document_params(&self.path_uri(path));
        expect_ready(crate::handlers::code_lens(&mut state, params)).unwrap().unwrap_or_default()
    }

    pub(super) fn check_document_highlights(&self, marker: &str, expected: impl IntoData) {
        assert_data_eq!(self.query_output(Query::Highlights, marker), expected);
    }

    pub(super) fn check_prepare_rename(&self, marker: &str, expected: impl IntoData) {
        let mut state = self.state();
        let (uri, position) = self.marker_location(marker);
        let response = block_on(crate::handlers::prepare_rename(
            &mut state,
            request_params(&uri, position, json!({})),
        ))
        .unwrap();
        assert_data_eq!(prepare_rename_output(response), expected);
    }

    pub(super) fn check_rename(&self, marker: &str, new_name: &str, expected: impl IntoData) {
        assert_data_eq!(self.rename_edits(marker, new_name), expected);
    }

    /// Checks renames from groups of space-separated markers that must produce equal edits.
    pub(super) fn check_renames(&self, renames: &[(&str, &str)], expected: impl IntoData) {
        let mut output = String::new();
        for &(markers, new_name) in renames {
            let mut group = markers.split_whitespace();
            let edits = self.rename_edits(group.next().unwrap(), new_name);
            for marker in group {
                assert_eq!(self.rename_edits(marker, new_name), edits, "{marker}");
            }
            write!(output, "{markers}:\n{edits}").unwrap();
        }
        assert_data_eq!(output, expected);
    }

    fn rename_edits(&self, marker: &str, new_name: &str) -> String {
        let mut state = self.state();
        let (uri, position) = self.marker_location(marker);
        let response =
            block_on(crate::handlers::rename(&mut state, rename_params(&uri, position, new_name)))
                .unwrap();
        self.rename_output(response)
    }

    pub(super) fn check_rename_error(&self, marker: &str, new_name: &str, expected: ErrorCode) {
        let mut state = self.state();
        let (uri, position) = self.marker_location(marker);
        let error =
            block_on(crate::handlers::rename(&mut state, rename_params(&uri, position, new_name)))
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
        assert_data_eq!(
            inlay_hint_output(&self.inlay_hints(self.path_uri(path), full_range())),
            expected
        );
    }

    pub(super) fn check_document_links(&self, path: &str, expected: impl IntoData) {
        self.check_document_links_at(self.path_uri(path), expected);
    }

    pub(super) fn check_document_links_at(&self, uri: Url, expected: impl IntoData) {
        let mut state = self.state();
        let links =
            expect_ready(crate::handlers::document_links(&mut state, document_params(&uri)))
                .unwrap()
                .unwrap_or_default();
        assert_data_eq!(self.document_links_output(links), expected);
    }

    pub(super) fn check_folding_ranges(&self, path: &str, expected: impl IntoData) {
        let ranges = self.folding_ranges(self.path_uri(path));
        let ranges = ranges.expect("folding-range request should return ranges");
        assert_data_eq!(folding_range_output(&ranges), expected);
    }

    pub(super) fn folding_ranges(&self, uri: Url) -> Option<Vec<FoldingRange>> {
        let mut state = self.state();
        // Folding ranges are syntactic and must not wait for analysis.
        state.mark_analysis_pending_for_test();
        block_on(crate::handlers::folding_range(&mut state, document_params(&uri))).unwrap()
    }

    pub(super) fn check_folding_range_uses_blocking_pool(
        &self,
        path: &str,
        expected: impl IntoData,
    ) {
        let params = document_params(&self.path_uri(path));
        let response =
            self.on_paused_blocking_pool(|state| crate::handlers::folding_range(state, params));
        let ranges = response.unwrap().expect("folding-range request should return ranges");
        assert_data_eq!(folding_range_output(&ranges), expected);
    }

    pub(super) fn check_selection_ranges(&self, markers: &[&str], expected: impl IntoData) {
        let (params, positions) = self.selection_range_request(markers);
        self.check_selection_range_params(params, &positions, expected);
    }

    pub(super) fn selection_range_response_in_state(
        &self,
        state: &mut GlobalState,
        markers: &[&str],
    ) -> Vec<SelectionRange> {
        let (params, _) = self.selection_range_request(markers);
        block_on(crate::handlers::selection_range(state, params)).unwrap().unwrap()
    }

    pub(super) fn check_selection_ranges_at(
        &self,
        path: &str,
        positions: Vec<Position>,
        normalized_positions: &[Position],
        expected: impl IntoData,
    ) {
        assert_eq!(positions.len(), normalized_positions.len());
        let params = selection_range_params(&self.path_uri(path), positions);
        self.check_selection_range_params(params, normalized_positions, expected);
    }

    fn check_selection_range_params(
        &self,
        params: SelectionRangeParams,
        positions: &[Position],
        expected: impl IntoData,
    ) {
        let mut state = self.state();
        // Selection ranges are syntactic and must not wait for analysis.
        state.mark_analysis_pending_for_test();
        let response =
            block_on(crate::handlers::selection_range(&mut state, params)).unwrap().unwrap();
        assert_data_eq!(selection_range_output(&response, positions), expected);
    }

    pub(super) fn check_selection_range_uses_blocking_pool(
        &self,
        markers: &[&str],
        expected: impl IntoData,
    ) {
        let (params, positions) = self.selection_range_request(markers);
        let response =
            self.on_paused_blocking_pool(|state| crate::handlers::selection_range(state, params));
        assert_data_eq!(selection_range_output(&response.unwrap().unwrap(), &positions), expected);
    }

    pub(super) fn check_selection_range_error(
        &self,
        path: &str,
        positions: Vec<Position>,
        expected: ErrorCode,
    ) {
        let mut state = self.state();
        let params = selection_range_params(&self.path_uri(path), positions);
        let error = block_on(crate::handlers::selection_range(&mut state, params))
            .expect_err("selection-range request should fail");
        assert_eq!(error.code, expected);
        assert!(!error.message.ends_with('.'));
    }

    /// Runs a request with one paused blocking worker, requiring it to wait for the pool.
    fn on_paused_blocking_pool<F: Future>(
        &self,
        request: impl FnOnce(&mut GlobalState) -> F,
    ) -> F::Output {
        with_paused_blocking_pool(|release_worker| async move {
            let mut state = self.state();
            let mut request = std::pin::pin!(request(&mut state));
            let is_pending =
                request.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending();
            release_worker.send(()).unwrap();
            assert!(is_pending);
            request.await
        })
    }

    pub(super) fn check_signature_help(&self, markers: &[&str], expected: impl IntoData) {
        self.check_signature_help_in(&mut self.state(), markers, expected);
    }

    pub(super) fn check_signature_help_in(
        &self,
        state: &mut GlobalState,
        markers: &[&str],
        expected: impl IntoData,
    ) {
        let outputs = markers.iter().map(|&marker| {
            let (uri, position) = self.marker_location(marker);
            (marker.to_string(), signature_help_output(signature_help_at(state, uri, position)))
        });
        assert_data_eq!(marker_outputs(outputs), expected);
    }

    /// Returns a state that keeps analyzing `path` as `changed_contents`, which must fail analysis.
    pub(super) fn signature_help_state_after_change(
        &self,
        path: &str,
        changed_contents: &str,
    ) -> GlobalState {
        let path = self.marked.project().path(path);
        let result = analyze_source(path.clone(), changed_contents);
        assert!(!result.diagnostics.is_empty(), "changed source should fail analysis");

        let state = self.state();
        set_overlay(&state, &path, changed_contents, None);
        state.symbol_tables.store(Arc::new(result.symbol_tables));
        state
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
        let response = expect_ready(crate::handlers::inlay_hints(
            &mut state,
            request_params(&uri, Position::default(), json!({ "range": range })),
        ))
        .unwrap();
        response.unwrap_or_default()
    }

    pub(super) fn state(&self) -> GlobalState {
        let mut config = self.marked.project().config();
        config.enable_signature_help_label_offsets();
        let state = state_with(config);
        *state.vfs.write() = self.marked.project().vfs();
        state.symbol_tables.store(Arc::new(self.result.symbol_tables.clone()));
        state.analysis_commit.lock().vfs_content_revision = state.vfs.read().content_revision();
        state
    }

    pub(super) fn state_with_workspace_analysis(&self) -> GlobalState {
        let output = analyze_workspace(&snapshot(self.marked.project()));
        let state = self.state();
        state.symbol_tables.store(Arc::new(output.result.symbol_tables));
        state.analysis_commit.lock().analysis_paths = output.analysis_paths;
        state
    }

    pub(super) fn completion_state(&self) -> GlobalState {
        let mut state = self.state();
        Arc::make_mut(&mut state.config).enable_completion_snippets();
        state
    }

    fn path_uri(&self, path: &str) -> Url {
        self.marked.project().uri(path)
    }

    pub(super) fn marker_location(&self, marker: &str) -> (Url, Position) {
        self.marked.location(marker)
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
            &uri.expect("at least one marker is required"),
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

    pub(super) fn rename_output(&self, response: Option<WorkspaceEdit>) -> String {
        rename_output(self.marked.project().root(), response)
    }
}

pub(super) fn rename_output(root: &Path, response: Option<WorkspaceEdit>) -> String {
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
        let display_path = display_path(root, &path);
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

/// A point query request checked by [`RequestFixture::check_queries`].
#[derive(Clone, Copy, Debug)]
pub(super) enum Query {
    Definition,
    Declaration,
    Implementation,
    TypeDefinition,
    /// References, including the declaration when set.
    References(bool),
    Highlights,
    Hover,
}

pub(super) enum QueryResponse {
    Goto(Option<GotoDefinitionResponse>),
    Locations(Option<Vec<Location>>),
    Highlights(Option<Vec<DocumentHighlight>>),
    Hover(Option<Hover>),
}

type QueryFuture = Pin<Box<dyn Future<Output = Result<QueryResponse, ResponseError>>>>;

impl Query {
    pub(super) const ALL: [Self; 7] = [
        Self::Definition,
        Self::Declaration,
        Self::Implementation,
        Self::TypeDefinition,
        Self::References(true),
        Self::Highlights,
        Self::Hover,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Definition => "definition",
            Self::Declaration => "declaration",
            Self::Implementation => "implementation",
            Self::TypeDefinition => "type definition",
            Self::References(true) => "references",
            Self::References(false) => "references without declaration",
            Self::Highlights => "highlights",
            Self::Hover => "hover",
        }
    }

    pub(super) fn request(
        self,
        state: &mut GlobalState,
        uri: Url,
        position: Position,
    ) -> QueryFuture {
        fn boxed<T: 'static>(
            request: impl Future<Output = Result<T, ResponseError>> + 'static,
            response: fn(T) -> QueryResponse,
        ) -> QueryFuture {
            Box::pin(async move { request.await.map(response) })
        }

        let goto = request_params(&uri, position, json!({}));
        match self {
            Self::Definition => {
                boxed(crate::handlers::goto_definition(state, goto), QueryResponse::Goto)
            }
            Self::Declaration => {
                boxed(crate::handlers::goto_declaration(state, goto), QueryResponse::Goto)
            }
            Self::Implementation => {
                boxed(crate::handlers::goto_implementation(state, goto), QueryResponse::Goto)
            }
            Self::TypeDefinition => {
                boxed(crate::handlers::goto_type_definition(state, goto), QueryResponse::Goto)
            }
            Self::References(include_declaration) => boxed(
                crate::handlers::references(
                    state,
                    request_params(
                        &uri,
                        position,
                        json!({ "context": { "includeDeclaration": include_declaration } }),
                    ),
                ),
                QueryResponse::Locations,
            ),
            Self::Highlights => boxed(
                crate::handlers::document_highlight(
                    state,
                    request_params(&uri, position, json!({})),
                ),
                QueryResponse::Highlights,
            ),
            Self::Hover => boxed(
                crate::handlers::hover(state, request_params(&uri, position, json!({}))),
                QueryResponse::Hover,
            ),
        }
    }
}

fn read_file(path: &Path) -> Option<String> {
    let mut contents = String::new();
    std::fs::File::open(path).ok()?.read_to_string(&mut contents).ok()?;
    Some(contents)
}

fn code_lens_output(lenses: &[CodeLens]) -> String {
    let mut output = String::new();
    for lens in lenses {
        let command = lens.command.as_ref().expect("eager CodeLens should have a title");
        let label = if command.title.starts_with("0x") {
            format!("selector={}", command.title)
        } else if command.title.ends_with(" reference") || command.title.ends_with(" references") {
            let count = command.title.split_once(' ').unwrap().0;
            format!("references={count}")
        } else {
            format!("inheritance={}", command.title)
        };
        let command = if command.command.is_empty() { "<none>" } else { &command.command };
        writeln!(
            output,
            "{}:{} {label} command={command}",
            lens.range.start.line, lens.range.start.character
        )
        .unwrap();
    }
    output
}

fn completion_output(response: CompletionResponse) -> String {
    let (incomplete, items) = match response {
        CompletionResponse::Array(items) => (false, items),
        CompletionResponse::List(list) => (list.is_incomplete, list.items),
    };
    let mut output = if incomplete { "incomplete\n".to_string() } else { String::new() };
    for item in items {
        let kind = item.kind.map_or_else(|| "<none>".into(), |kind| format!("{kind:?}"));
        write!(output, "{} {kind}", item.label).unwrap();
        for (name, value) in
            [("detail", &item.detail), ("sort", &item.sort_text), ("filter", &item.filter_text)]
        {
            if let Some(value) = value {
                write!(output, " {name}={value:?}").unwrap();
            }
        }
        if let Some(format) = item.insert_text_format {
            write!(output, " format={format:?}").unwrap();
        }
        for edit in item.additional_text_edits.iter().flatten() {
            write!(output, " additional={}={:?}", range_output(edit.range), edit.new_text).unwrap();
        }
        let Some(text_edit) = item.text_edit else {
            writeln!(output).unwrap();
            continue;
        };
        let CompletionTextEdit::Edit(edit) = text_edit else {
            panic!("unexpected insert-and-replace completion edit");
        };
        writeln!(output, " edit={}", range_output(edit.range)).unwrap();
        for line in edit.new_text.split('\n') {
            writeln!(output, "|{}{line}", if line.is_empty() { "" } else { " " }).unwrap();
        }
    }
    output
}

/// Checks completions for `(name, uri, position, trigger)` requests.
pub(super) fn check_completions_at<'a>(
    state: &mut GlobalState,
    requests: impl IntoIterator<Item = (String, Url, Position, Option<&'a str>)>,
    expected: impl IntoData,
) {
    let outputs = requests.into_iter().map(|(name, uri, position, trigger)| {
        let context =
            trigger.map(|trigger| json!({ "triggerKind": 2, "triggerCharacter": trigger }));
        let params = request_params(&uri, position, json!({ "context": context }));
        let response = expect_ready(crate::handlers::completion(state, params)).unwrap();
        (name, completion_output(response.unwrap()))
    });
    // Keep backslashes from snippet and string escapes out of path normalization.
    assert_data_eq!(marker_outputs(outputs), expected.raw());
}

/// Joins per-marker outputs, listing markers with identical output under one header.
fn marker_outputs(outputs: impl Iterator<Item = (String, String)>) -> String {
    let mut groups = Vec::<(Vec<String>, String)>::new();
    for (marker, output) in outputs {
        match groups.iter_mut().find(|(_, existing)| *existing == output) {
            Some((markers, _)) => markers.push(marker),
            None => groups.push((vec![marker], output)),
        }
    }
    if let [(_, output)] = groups.as_slice() {
        return output.clone();
    }
    let mut result = String::new();
    for (markers, output) in groups {
        writeln!(result, "{}:", markers.join(" ")).unwrap();
        result.push_str(&output);
    }
    result
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

pub(super) fn folding_range_output(ranges: &[FoldingRange]) -> String {
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

fn range_contains_position(range: Range, position: Position) -> bool {
    (range.start <= position && position < range.end)
        || (range.start == position && range.end == position)
}

fn range_contains_range(outer: Range, inner: Range) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

fn inlay_hint_output(hints: &[InlayHint]) -> String {
    let mut output = String::new();
    for hint in hints {
        writeln!(
            output,
            "{}:{} {} {}",
            hint.position.line,
            hint.position.character,
            inlay_hint_kind(hint.kind),
            inlay_hint_label(&hint.label)
        )
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
    // Print the leading Solidity code block as a plain signature line.
    let (signature, documentation) = contents
        .value
        .strip_prefix("```solidity\n")
        .and_then(|value| value.split_once("\n```"))
        .expect("hover should start with a Solidity code block");
    format!("{} {signature}{documentation}\n", range_output(range))
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
        write!(output, "{}", signature.label).unwrap();
        if let Some(active_parameter) = signature.active_parameter {
            write!(output, " active={active_parameter}").unwrap();
        }
        writeln!(output).unwrap();
        if let Some(documentation) = signature.documentation {
            writeln!(output, "  {}", documentation_output(&documentation)).unwrap();
        }
        for parameter in signature.parameters.into_iter().flatten() {
            match parameter.label {
                ParameterLabel::Simple(label) => write!(output, "  {label}").unwrap(),
                ParameterLabel::LabelOffsets([start, end]) => {
                    write!(output, "  {start}..{end}").unwrap()
                }
            }
            if let Some(documentation) = parameter.documentation {
                write!(output, " {}", documentation_output(&documentation)).unwrap();
            }
            writeln!(output).unwrap();
        }
    }
    output
}

fn documentation_output(documentation: &Documentation) -> String {
    let (kind, value) = match documentation {
        Documentation::String(value) => ("docs", value),
        Documentation::MarkupContent(content) => ("markdown", &content.value),
    };
    format!("{kind}={}", value.replace('\n', " | "))
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

pub(super) fn signature_help_at(
    state: &mut GlobalState,
    uri: Url,
    position: Position,
) -> Option<SignatureHelp> {
    expect_ready(crate::handlers::signature_help(state, request_params(&uri, position, json!({}))))
        .unwrap()
}

fn full_range() -> Range {
    Range { start: Position::new(0, 0), end: Position::new(u32::MAX, u32::MAX) }
}

fn selection_range_params(uri: &Url, positions: Vec<Position>) -> SelectionRangeParams {
    request_params(uri, Position::default(), json!({ "positions": positions }))
}

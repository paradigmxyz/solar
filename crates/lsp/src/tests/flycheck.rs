use crate::{flycheck, global_state::GlobalState, test_support::TestProject};
use async_lsp::ClientSocket;
use lsp_types::{
    CodeActionClientCapabilities, CodeActionContext, CodeActionKind, CodeActionKindLiteralSupport,
    CodeActionLiteralSupport, CodeActionOrCommand, CodeActionParams,
    PublishDiagnosticsClientCapabilities, TextDocumentIdentifier, WorkDoneProgressParams,
};
use solar_interface::{
    BytePos, ColorChoice, Span,
    diagnostics::{Applicability, Diag, DiagCtxt, JsonEmitter, Level},
    source_map::{FileName, SourceMap},
};
use std::{io, path::Path, sync::Arc, time::Duration};
use tokio::sync::oneshot;

const SOURCE: &str = "contract Test { function run() public view {} }";
const FAKE_FLYCHECK_TEST: &str = "flycheck_tests::fake_json_emitter";

#[tokio::test(flavor = "current_thread")]
async fn json_emitter_alternatives_become_separate_flycheck_code_actions() {
    let project = TestProject::new();
    project.write_file("/foundry.toml", "[profile.default]\nsrc = \"src\"\n");
    project.write_file("/src/Test.sol", SOURCE);
    let path = project.path("/src/Test.sol");
    let uri = lsp_types::Url::from_file_path(&path).unwrap();

    let mut params = project.initialize_params();
    let text_document = params.capabilities.text_document.get_or_insert_default();
    text_document.code_action = Some(CodeActionClientCapabilities {
        code_action_literal_support: Some(CodeActionLiteralSupport {
            code_action_kind: CodeActionKindLiteralSupport {
                value_set: vec![CodeActionKind::QUICKFIX.as_str().into()],
            },
        }),
        ..Default::default()
    });
    text_document.publish_diagnostics = Some(PublishDiagnosticsClientCapabilities {
        data_support: Some(true),
        ..Default::default()
    });
    params.initialization_options = Some(serde_json::json!({
        "flychecks": [{
            "id": "json-emitter-contract",
            "command": std::env::current_exe().unwrap(),
            "args": [
                "--ignored",
                "--exact",
                FAKE_FLYCHECK_TEST,
                "--no-capture",
                "--color",
                "never"
            ],
            "output": "forge-lint-json"
        }]
    }));

    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.on_initialize(params).await.unwrap();
    let [flycheck] = state.config.flychecks_for_path(&path).try_into().unwrap();
    let (_cancel, cancelled) = oneshot::channel();
    let diagnostics =
        flycheck::run(flycheck, Duration::from_secs(30), cancelled, vec![path]).await.unwrap();
    let [diagnostic] = diagnostics[&uri].as_slice() else {
        panic!("expected one flycheck diagnostic, got {diagnostics:#?}");
    };
    let diagnostic = diagnostic.clone();
    let request = CodeActionParams {
        text_document: TextDocumentIdentifier::new(uri.clone()),
        range: diagnostic.range,
        context: CodeActionContext { diagnostics: vec![diagnostic], ..Default::default() },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: Default::default(),
    };
    state.replace_diagnostics_for_test(diagnostics);

    let actions = crate::handlers::code_actions(&mut state, request).await.unwrap().unwrap();

    let replacements = actions
        .iter()
        .map(|action| {
            let CodeActionOrCommand::CodeAction(action) = action else {
                panic!("expected code action, got {action:#?}");
            };
            assert_eq!(action.title, "consider changing visibility and mutability");
            action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri]
                .iter()
                .map(|edit| edit.new_text.as_str())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(replacements, [["external", "pure"], ["internal", "payable"]]);
}

#[test]
#[ignore = "subprocess helper"]
fn fake_json_emitter() {
    let source_map = Arc::new(SourceMap::empty());
    let source = source_map.file_loader().load_file(Path::new("src/Test.sol")).unwrap();
    let file = source_map.new_source_file(FileName::real("src/Test.sol"), source.clone()).unwrap();
    let span = |needle: &str| {
        let start = source.find(needle).unwrap();
        Span::new(
            file.start_pos + BytePos::from_usize(start),
            file.start_pos + BytePos::from_usize(start + needle.len()),
        )
    };
    let public = span("public");
    let view = span("view");
    let mut diagnostic = Diag::new(Level::Warning, "inefficient visibility and mutability");
    diagnostic.span(public).multipart_suggestions(
        "consider changing visibility and mutability",
        [
            vec![(public, "external".into()), (view, "pure".into())],
            vec![(public, "internal".into()), (view, "payable".into())],
        ],
        Applicability::MaybeIncorrect,
    );

    let emitter =
        JsonEmitter::new(Box::new(io::stderr()), source_map, ColorChoice::Never).rustc_like(true);
    DiagCtxt::new(Box::new(emitter)).emit_diagnostic(diagnostic).unwrap();
}

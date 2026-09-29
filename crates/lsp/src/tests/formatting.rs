use super::*;
#[cfg(unix)]
use crate::formatter::tests::write_executable;
use crate::{config::negotiate_capabilities, test_support::TestProject};
use async_lsp::ClientSocket;
#[cfg(unix)]
use lsp_types::{
    DidChangeTextDocumentParams, TextDocumentContentChangeEvent, VersionedTextDocumentIdentifier,
};
use lsp_types::{
    FormattingOptions, Position, Range, TextDocumentIdentifier, WorkDoneProgressParams,
};
#[cfg(unix)]
use std::{ops::ControlFlow, time::Duration};
use std::{path::Path, sync::Arc};
#[cfg(unix)]
use tokio::time;

#[test]
fn formatting_edits_replace_the_whole_changed_document() {
    assert_eq!(formatting_edits("contract C {}", "contract C {}".into()), None);
    for source in ["a\r\n🚀中\n", "contract First{}\rcontract Second{}\r"] {
        assert_eq!(
            formatting_edits(source, "formatted".into()),
            Some(vec![TextEdit {
                range: Range::new(Position::new(0, 0), Position::new(2, 0)),
                new_text: "formatted".into(),
            }]),
            "{source:?}"
        );
    }
}

#[test]
fn formatter_failures_map_to_concise_request_failed_errors() {
    let failures = [
        (FormatterError::Timeout, "Forge formatting timed out"),
        (FormatterError::ConfigTimeout, "Forge config resolution timed out"),
        (
            FormatterError::Io(io::Error::new(io::ErrorKind::NotFound, "missing")),
            "Forge executable was not found",
        ),
        (FormatterError::Io(io::Error::other("pipe failed")), "failed to run Forge formatter"),
        (
            FormatterError::Failed { status: Some(1), stderr: "failed".into() },
            "Forge formatting failed",
        ),
        (
            FormatterError::ConfigFailed { status: Some(1), stderr: "failed".into() },
            "Forge config resolution failed",
        ),
        (
            FormatterError::InvalidConfig(
                serde_json::from_slice::<serde_json::Value>(b"{").unwrap_err(),
            ),
            "Forge returned invalid config",
        ),
        (
            FormatterError::InvalidUtf8(String::from_utf8(vec![0xff]).unwrap_err()),
            "Forge returned invalid UTF-8",
        ),
        (FormatterError::EmptyOutput, "Forge formatter returned empty output"),
    ];

    for (failure, message) in failures {
        let response = formatter_failed(failure);
        assert_eq!(response.code, ErrorCode::REQUEST_FAILED);
        assert_eq!(response.message, message);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn missing_forge_returns_request_failed() {
    let mut project = TestProject::from_fixture(
        r#"
        //- /workspace/Test.sol
        contract Test {}
        "#,
    );
    project.open_file("/workspace/Test.sol", "contract Test{}");
    let mut state = formatting_state(&project, &project.path("/missing-forge"), &["/workspace"]);

    let error = format(&mut state, &project, "/workspace/Test.sol").await.unwrap_err();

    assert_eq!(error.code, ErrorCode::REQUEST_FAILED);
    assert_eq!(error.message, "Forge executable was not found");
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn formatting_rejects_empty_output_for_non_whitespace_source() {
    let project = TestProject::from_fixture(
        r#"
        //- /workspace/Test.sol
        contract Test {}
        "#,
    );
    let forge = write_formatter_executable(&project, &[], "cat >/dev/null");
    let mut state = formatting_state(&project, &forge, &["/workspace"]);

    let error = format(&mut state, &project, "/workspace/Test.sol").await.unwrap_err();

    assert_eq!(error.code, ErrorCode::REQUEST_FAILED);
    assert_eq!(error.message, "Forge formatter returned empty output");
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn formatting_sends_vfs_or_disk_source_with_the_owning_foundry_root() {
    let mut project = TestProject::from_fixture(
        r#"
        //- /workspace/A.sol
        contract A {}

        //- /workspace/nested/Test.sol
        contract Test {}

        //- /outside/foundry.toml
        [fmt]
        int_types = "short"

        //- /outside/src/Test.sol
        contract Test {}
        "#,
    );
    let unsaved = "contract Test{string s=\"🚀\";}";
    project.open_file("/workspace/nested/Test.sol", unsaved);
    let forge = write_formatter_executable(
        &project,
        &[],
        r#"printf '%s\n' "$@" > "$0.args"
cat > "$0.stdin"
printf 'contract Test { string s = "🚀"; }'"#,
    );
    let mut state = formatting_state(&project, &forge, &["/workspace", "/workspace/nested"]);

    // Open documents use the unsaved buffer and the most specific workspace; other documents
    // are read from disk and use their nearest Foundry root outside the workspaces.
    for (path, source, root) in [
        ("/workspace/nested/Test.sol", unsaved, "/workspace/nested"),
        ("/outside/src/Test.sol", "contract Test {}", "/outside"),
    ] {
        let edits = format(&mut state, &project, path).await.unwrap().unwrap();

        assert_eq!(edits[0].new_text, "contract Test { string s = \"🚀\"; }");
        assert_eq!(project.read_file("/fake-forge.stdin"), source);
        assert_eq!(
            project.read_file("/fake-forge.args"),
            format!("fmt\n--raw\n--root\n{}\n-\n", project.path(root).display())
        );
    }
    let path = crate::vfs::VfsPath::from(project.path("/workspace/nested/Test.sol"));
    assert_eq!(state.vfs.read().get_file_contents(&path).unwrap().to_string(), unsaved);
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn formatting_skips_files_ignored_by_resolved_forge_config() {
    let mut project = TestProject::from_fixture(
        r#"
        //- /workspace/foundry.toml
        [fmt]
        ignore = ["src/Local.sol"]

        //- /workspace/src/Resolved.sol
        contract Resolved {}
        "#,
    );
    project.open_file("/workspace/src/Resolved.sol", "contract Resolved{uint value;}");
    let forge = write_formatter_executable(
        &project,
        &["src/Resolved.sol", "src/Missing.sol"],
        ": > \"$0.formatted\"\ncat",
    );
    let mut state = formatting_state(&project, &forge, &["/workspace"]);

    // The missing file shows that ignored documents are never read.
    for path in ["/workspace/src/Resolved.sol", "/workspace/src/Missing.sol"] {
        assert_eq!(format(&mut state, &project, path).await.unwrap(), None);
    }
    assert!(!project.path("/fake-forge.formatted").exists());
    assert_eq!(
        project.read_file("/fake-forge.config-args"),
        format!("config\n--json\n--root\n{}\n", project.path("/workspace").display())
    );
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn formatting_stops_when_forge_config_resolution_fails() {
    let mut project = TestProject::from_fixture(
        r#"
        //- /workspace/Test.sol
        contract Test {}
        "#,
    );
    project.open_file("/workspace/Test.sol", "contract Test{}");
    let forge = write_executable(
        &project,
        "/fake-forge",
        r#"#!/bin/sh
set -eu
if [ "${1-}" = lint ]; then exit 1; fi
if [ "${1-}" = config ]; then printf 'invalid config' >&2; exit 7; fi
: > "$0.formatted"
cat
"#,
    );
    let mut state = formatting_state(&project, &forge, &["/workspace"]);

    let error = format(&mut state, &project, "/workspace/Test.sol").await.unwrap_err();

    assert_eq!(error.code, ErrorCode::REQUEST_FAILED);
    assert_eq!(error.message, "Forge config resolution failed");
    assert!(!project.path("/fake-forge.formatted").exists());
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn formatting_rejects_results_after_document_change() {
    let mut project = TestProject::from_fixture(
        r#"
        //- /workspace/Test.sol
        contract Test {}
        "#,
    );
    project.open_file("/workspace/Test.sol", "contract Test{}");
    let forge = write_formatter_executable(
        &project,
        &[],
        r#"cat > "$0.stdin"
: > "$0.ready.tmp"
mv "$0.ready.tmp" "$0.ready"
while [ ! -e "$0.release" ]; do sleep 0.01; done
printf 'contract Test {}'"#,
    );
    let mut state = formatting_state(&project, &forge, &["/workspace"]);
    let uri = project.uri("/workspace/Test.sol");
    let task = tokio::spawn(format(&mut state, &project, "/workspace/Test.sol"));
    let ready = project.path("/fake-forge.ready");
    time::timeout(Duration::from_secs(5), async {
        while !ready.exists() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    let result = crate::handlers::did_change_text_document(
        &mut state,
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri, 2),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "contract Changed {}".into(),
            }],
        },
    );
    assert!(matches!(result, ControlFlow::Continue(())));
    project.write_file("/fake-forge.release", "");

    let error = task.await.unwrap().unwrap_err();

    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
    assert_eq!(error.message, "document changed during formatting");
}

fn format(
    state: &mut GlobalState,
    project: &TestProject,
    path: &str,
) -> impl Future<Output = Result<Option<Vec<TextEdit>>, ResponseError>> + use<> {
    formatting(
        state,
        DocumentFormattingParams {
            text_document: TextDocumentIdentifier { uri: project.uri(path) },
            options: FormattingOptions { tab_size: 99, insert_spaces: false, ..Default::default() },
            work_done_progress_params: WorkDoneProgressParams::default(),
        },
    )
}

fn formatting_state(project: &TestProject, forge: &Path, roots: &[&str]) -> GlobalState {
    let mut params = project.initialize_params_with_roots(roots);
    params.initialization_options =
        Some(serde_json::json!({ "forgePath": forge.display().to_string() }));
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();

    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    *state.vfs.write() = project.vfs();
    state
}

#[cfg(unix)]
fn write_formatter_executable(
    project: &TestProject,
    ignores: &[&str],
    formatter: &str,
) -> std::path::PathBuf {
    let config = serde_json::json!({ "fmt": { "ignore": ignores } });
    let contents = format!(
        r#"#!/bin/sh
set -eu
case "${{1-}}" in
lint)
exit 1
;;
config)
printf '%s\n' "$@" > "$0.config-args"
printf '%s' '{config}'
;;
fmt)
{formatter}
;;
esac
"#
    );
    write_executable(project, "/fake-forge", &contents)
}

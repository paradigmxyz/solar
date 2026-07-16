use super::*;
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
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
#[cfg(unix)]
use std::{ops::ControlFlow, time::Duration};
use std::{path::Path, sync::Arc};
#[cfg(unix)]
use tokio::time;

#[test]
fn unchanged_formatting_returns_none() {
    assert_eq!(formatting_edits("contract C {}", "contract C {}".into()), None);
}

#[test]
fn changed_formatting_returns_one_full_document_edit() {
    let edits = formatting_edits("a\r\n🚀中\n", "formatted".into()).unwrap();

    assert_eq!(edits.len(), 1);
    assert_eq!(
        edits[0],
        TextEdit {
            range: Range::new(Position::new(0, 0), Position::new(2, 0)),
            new_text: "formatted".into(),
        }
    );
}

#[test]
fn changed_formatting_covers_documents_with_bare_carriage_returns() {
    let edits =
        formatting_edits("contract First{}\rcontract Second{}\r", "formatted".into()).unwrap();

    assert_eq!(edits[0].range, Range::new(Position::new(0, 0), Position::new(2, 0)));
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
        assert!(!response.message.ends_with('.'));
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
    let uri = Url::from_file_path(project.path("/workspace/Test.sol")).unwrap();

    let error = formatting(&mut state, formatting_params(uri)).await.unwrap_err();

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
    let forge = write_formatter_executable(&project, "/fake-forge", &[], "cat >/dev/null");
    let mut state = formatting_state(&project, &forge, &["/workspace"]);
    let path = project.path("/workspace/Test.sol");

    let error = formatting(&mut state, formatting_params(Url::from_file_path(path).unwrap()))
        .await
        .unwrap_err();

    assert_eq!(error.code, ErrorCode::REQUEST_FAILED);
    assert_eq!(error.message, "Forge formatter returned empty output");
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn formatting_uses_unsaved_vfs_source_and_most_specific_workspace() {
    let mut project = TestProject::from_fixture(
        r#"
        //- /workspace/A.sol
        contract A {}

        //- /workspace/nested/Test.sol
        contract Test {}
        "#,
    );
    let unsaved = "contract Test{string s=\"🚀\";}";
    project.open_file("/workspace/nested/Test.sol", unsaved);
    let forge = write_formatter_executable(
        &project,
        "/fake-forge",
        &[],
        r#"printf '%s\n' "$@" > "$0.args"
cat > "$0.stdin"
printf 'contract Test { string s = "🚀"; }'"#,
    );
    let mut state = formatting_state(&project, &forge, &["/workspace", "/workspace/nested"]);
    let path = project.path("/workspace/nested/Test.sol");

    let edits = formatting(&mut state, formatting_params(Url::from_file_path(&path).unwrap()))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(edits[0].new_text, "contract Test { string s = \"🚀\"; }");
    assert_eq!(project.read_file("/fake-forge.stdin"), unsaved);
    assert_eq!(
        project.read_file("/fake-forge.args"),
        format!("fmt\n--raw\n--root\n{}\n-\n", project.path("/workspace/nested").display())
    );
    assert_eq!(
        state.vfs.read().get_file_contents(&crate::vfs::VfsPath::from(path)).unwrap().to_string(),
        unsaved
    );
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn formatting_skips_files_ignored_by_foundry_config() {
    let mut project = TestProject::from_fixture(
        r#"
        //- /workspace/foundry.toml
        [fmt]
        ignore = ["src/Ignored.sol"]

        //- /workspace/src/Ignored.sol
        contract Ignored {}
        "#,
    );
    let unsaved = "contract Ignored{uint value;}";
    project.open_file("/workspace/src/Ignored.sol", unsaved);
    let forge = write_formatter_executable(
        &project,
        "/fake-forge",
        &["src/Ignored.sol"],
        "printf '%s\\n' \"$@\" > \"$0.called\"\ncat",
    );
    let mut state = formatting_state(&project, &forge, &["/workspace"]);
    let path = project.path("/workspace/src/Ignored.sol");

    let edits = formatting(&mut state, formatting_params(Url::from_file_path(path).unwrap()))
        .await
        .unwrap();

    assert_eq!(edits, None);
    assert!(!project.path("/fake-forge.called").exists());
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn formatting_uses_resolved_forge_ignore_config() {
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
        "/fake-forge",
        &["src/Resolved.sol"],
        ": > \"$0.formatted\"\ncat",
    );
    let mut state = formatting_state(&project, &forge, &["/workspace"]);
    let path = project.path("/workspace/src/Resolved.sol");

    let edits = formatting(&mut state, formatting_params(Url::from_file_path(path).unwrap()))
        .await
        .unwrap();

    assert_eq!(edits, None);
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
    let path = project.path("/workspace/Test.sol");

    let error = formatting(&mut state, formatting_params(Url::from_file_path(path).unwrap()))
        .await
        .unwrap_err();

    assert_eq!(error.code, ErrorCode::REQUEST_FAILED);
    assert_eq!(error.message, "Forge config resolution failed");
    assert!(!project.path("/fake-forge.formatted").exists());
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn formatting_skips_ignored_files_before_reading_contents() {
    let project = TestProject::from_fixture(
        r#"
        //- /workspace/foundry.toml
        [fmt]
        ignore = ["src/Ignored.sol"]
        "#,
    );
    let forge = write_formatter_executable(&project, "/fake-forge", &["src/Ignored.sol"], "exit 2");
    let mut state = formatting_state(&project, &forge, &["/workspace"]);
    let path = project.path("/workspace/src/Ignored.sol");

    let edits = formatting(&mut state, formatting_params(Url::from_file_path(path).unwrap()))
        .await
        .unwrap();

    assert_eq!(edits, None);
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn formatting_reads_disk_and_discovers_foundry_root_outside_workspaces() {
    let project = TestProject::from_fixture(
        r#"
        //- /workspace/.keep

        //- /outside/foundry.toml
        [fmt]
        int_types = "short"

        //- /outside/src/Test.sol
        contract Test {}
        "#,
    );
    let forge = write_formatter_executable(
        &project,
        "/fake-forge",
        &[],
        r#"printf '%s\n' "$@" > "$0.args"
cat > "$0.stdin"
cat "$0.stdin""#,
    );
    let mut state = formatting_state(&project, &forge, &["/workspace"]);
    let path = project.path("/outside/src/Test.sol");

    let edits = formatting(&mut state, formatting_params(Url::from_file_path(path).unwrap()))
        .await
        .unwrap();

    assert_eq!(edits, None);
    assert_eq!(project.read_file("/fake-forge.stdin"), "contract Test {}");
    assert_eq!(
        project.read_file("/fake-forge.args"),
        format!("fmt\n--raw\n--root\n{}\n-\n", project.path("/outside").display())
    );
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
        "/fake-forge",
        &[],
        r#"cat > "$0.stdin"
: > "$0.ready.tmp"
mv "$0.ready.tmp" "$0.ready"
while [ ! -e "$0.release" ]; do sleep 0.01; done
printf 'contract Test {}'"#,
    );
    let mut state = formatting_state(&project, &forge, &["/workspace"]);
    let uri = Url::from_file_path(project.path("/workspace/Test.sol")).unwrap();
    let request = formatting(&mut state, formatting_params(uri.clone()));
    let task = tokio::spawn(request);
    wait_for_path(&project.path("/fake-forge.ready")).await;

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

fn formatting_params(uri: Url) -> DocumentFormattingParams {
    DocumentFormattingParams {
        text_document: TextDocumentIdentifier { uri },
        options: FormattingOptions { tab_size: 99, insert_spaces: false, ..Default::default() },
        work_done_progress_params: WorkDoneProgressParams::default(),
    }
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
    path: &str,
    ignores: &[&str],
    formatter: &str,
) -> PathBuf {
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
    write_executable(project, path, &contents)
}

#[cfg(unix)]
fn write_executable(project: &TestProject, path: &str, contents: &str) -> PathBuf {
    project.write_file(path, contents);
    let path = project.path(path);
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

#[cfg(unix)]
async fn wait_for_path(path: &Path) {
    time::timeout(Duration::from_secs(5), async {
        while !path.exists() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

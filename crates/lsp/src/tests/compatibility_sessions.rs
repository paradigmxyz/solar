use crate::{
    new_server_service,
    test_support::{TestProject, read_lsp_frame, write_lsp_frame},
};
use async_lsp::ClientSocket;
use lsp_types::Url;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::{
    io::{AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf},
    task::JoinHandle,
};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

const SESSION_TIMEOUT: Duration = Duration::from_secs(5);
const SESSION_SOURCE: &str = "/*😀*/ contract Before { function ping() external {} }\n";
const DIAGNOSTIC_SOURCE: &str = r#"contract Diagnostics {
    function value() external pure returns (uint256) {
        return missingValue;
    }
}
"#;
const CLEARED_DIAGNOSTIC_SOURCE: &str = r#"contract Diagnostics {
    function value() external pure returns (uint256) {
        return 0;
    }
}
"#;
const PROJECT_FIXTURE: &str = r#"
    //- /foundry.toml
    [profile.default]
    src = "before"

    //- /before/Before.sol
    contract Before {}

    //- /after/After.sol
    contract After {}
"#;

#[derive(Clone, Copy, Debug)]
struct ClientProfile {
    name: &'static str,
    version: &'static str,
    capabilities_json: &'static str,
}

impl ClientProfile {
    fn label(&self) -> String {
        format!("{} {}", self.name, self.version)
    }

    fn capabilities(&self) -> Value {
        serde_json::from_str(self.capabilities_json)
            .unwrap_or_else(|error| panic!("{}: invalid capability fixture: {error}", self.label()))
    }
}

// These fixtures pin the capability subset exercised by this module.
// They are not complete initialize payloads.
const CLIENT_PROFILES: [ClientProfile; 5] = [
    // https://github.com/microsoft/vscode-languageserver-node/blob/release/client/10.1.0/client/src/common/client.ts
    // https://github.com/microsoft/vscode-languageserver-node/blob/release/client/10.1.0/client/src/common/codeAction.ts
    // https://github.com/microsoft/vscode-languageserver-node/blob/release/client/10.1.0/client/src/common/diagnostic.ts
    ClientProfile {
        name: "VS Code",
        version: "vscode-languageclient 10.1.0",
        capabilities_json: include_str!(
            "fixtures/client_capabilities/vscode-languageclient-10.1.0.json"
        ),
    },
    // https://github.com/neovim/neovim/blob/v0.12.4/runtime/lua/vim/lsp/protocol.lua
    // Watched-file dynamic registration is enabled only on Darwin and Windows.
    // https://github.com/neovim/neovim/blob/v0.12.4/runtime/lua/vim/lsp/protocol.lua#L606-L612
    ClientProfile {
        name: "Neovim (Darwin/Windows)",
        version: "0.12.4",
        capabilities_json: include_str!(
            "fixtures/client_capabilities/neovim-0.12.4-darwin-windows.json"
        ),
    },
    ClientProfile {
        name: "Neovim (Linux/BSD)",
        version: "0.12.4",
        capabilities_json: include_str!(
            "fixtures/client_capabilities/neovim-0.12.4-linux-bsd.json"
        ),
    },
    // https://github.com/zed-industries/zed/blob/v1.14.2/crates/lsp/src/lsp.rs
    ClientProfile {
        name: "Zed",
        version: "1.14.2",
        capabilities_json: include_str!("fixtures/client_capabilities/zed-1.14.2.json"),
    },
    // https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/
    ClientProfile {
        name: "Minimal LSP client",
        version: "3.17",
        capabilities_json: include_str!("fixtures/client_capabilities/lsp-3.17-minimal.json"),
    },
];

fn client_profile(name: &str) -> &'static ClientProfile {
    CLIENT_PROFILES
        .iter()
        .find(|profile| profile.name == name)
        .unwrap_or_else(|| panic!("missing `{name}` client profile"))
}

#[test]
fn named_client_profiles_match_pinned_diagnostic_capabilities() {
    for profile in &CLIENT_PROFILES[..4] {
        let expected = match (profile.name, profile.version) {
            ("VS Code", "vscode-languageclient 10.1.0") => (true, Some(true), true, true),
            ("Neovim (Darwin/Windows)" | "Neovim (Linux/BSD)", "0.12.4") => {
                (true, Some(true), true, true)
            }
            ("Zed", "1.14.2") => (true, None, true, true),
            _ => unreachable!("unexpected named client profile"),
        };
        let capabilities = profile.capabilities();
        let label = profile.label();
        assert_eq!(
            capabilities.pointer("/textDocument/diagnostic").is_some(),
            expected.0,
            "{label}: document diagnostic capability mismatch"
        );
        assert_eq!(
            capabilities.pointer("/textDocument/diagnostic/dataSupport").and_then(Value::as_bool),
            expected.1,
            "{label}: pull diagnostic data support mismatch"
        );
        assert_eq!(
            capabilities
                .pointer("/textDocument/publishDiagnostics/dataSupport")
                .and_then(Value::as_bool),
            Some(expected.2),
            "{label}: publish diagnostic data support mismatch"
        );
        assert_eq!(
            capabilities.pointer("/workspace/diagnostics/refreshSupport").and_then(Value::as_bool),
            Some(expected.3),
            "{label}: workspace diagnostic refresh support mismatch"
        );
    }
}

#[test]
fn neovim_profiles_match_pinned_watched_file_capabilities() {
    for (name, expected) in [("Neovim (Darwin/Windows)", true), ("Neovim (Linux/BSD)", false)] {
        let profile = client_profile(name);
        let capabilities = profile.capabilities();
        assert_eq!(
            capabilities
                .pointer("/workspace/didChangeWatchedFiles/dynamicRegistration")
                .and_then(Value::as_bool),
            Some(expected),
            "{}: watched-file capability mismatch",
            profile.label()
        );
    }
}

struct RawSession {
    reader: BufReader<ReadHalf<DuplexStream>>,
    writer: WriteHalf<DuplexStream>,
    next_request_id: u64,
    watched_files_registered: bool,
    server_messages: Vec<Value>,
    server_task: JoinHandle<async_lsp::Result<()>>,
    _client: ClientSocket,
}

impl RawSession {
    fn start() -> Self {
        let (main_loop, client) = async_lsp::MainLoop::new_server(new_server_service);
        let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
        let (server_reader, server_writer) = tokio::io::split(server_stream);
        let server_task = tokio::spawn(
            main_loop.run_buffered(server_reader.compat(), server_writer.compat_write()),
        );
        let (client_reader, writer) = tokio::io::split(client_stream);

        Self {
            reader: BufReader::new(client_reader),
            writer,
            next_request_id: 1,
            watched_files_registered: false,
            server_messages: Vec::new(),
            server_task,
            _client: client,
        }
    }

    async fn notify(&mut self, method: &str, params: Value) {
        write_lsp_frame(
            &mut self.writer,
            json!({ "jsonrpc": "2.0", "method": method, "params": params }),
        )
        .await;
    }

    async fn request(&mut self, method: &str, params: Value) -> Value {
        let id = Value::from(self.next_request_id);
        self.next_request_id += 1;
        write_lsp_frame(
            &mut self.writer,
            json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
        )
        .await;

        loop {
            let message = self.read_message().await;
            if message.get("method").is_some() {
                self.handle_server_message(message).await;
                continue;
            }
            if message.get("id") != Some(&id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                panic!("request `{method}` failed: {error}");
            }
            return message.get("result").cloned().unwrap_or(Value::Null);
        }
    }

    async fn initialize(
        &mut self,
        profile: &ClientProfile,
        root_uri: &Url,
        capabilities: &Value,
    ) -> Value {
        self.request(
            "initialize",
            json!({
                "processId": null,
                "clientInfo": { "name": profile.name, "version": profile.version },
                "rootUri": root_uri,
                "capabilities": capabilities,
                "workspaceFolders": [{
                    "uri": root_uri,
                    "name": "compatibility-session",
                }],
            }),
        )
        .await
    }

    async fn open(&mut self, uri: &Url, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "solidity",
                    "version": 1,
                    "text": text,
                },
            }),
        )
        .await;
    }

    async fn change(&mut self, uri: &Url) {
        self.notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": 2 },
                "contentChanges": [{
                    "range": {
                        "start": { "line": 0, "character": 16 },
                        "end": { "line": 0, "character": 22 },
                    },
                    "rangeLength": 6,
                    "text": "After",
                }],
            }),
        )
        .await;
    }

    async fn replace_document(&mut self, uri: &Url, version: i32, text: &str) {
        self.notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [{ "text": text }],
            }),
        )
        .await;
    }

    async fn document_request(&mut self, method: &str, uri: &Url) -> Value {
        self.request(method, json!({ "textDocument": { "uri": uri } })).await
    }

    async fn document_notification(&mut self, method: &str, uri: &Url) {
        self.notify(method, json!({ "textDocument": { "uri": uri } })).await;
    }

    async fn workspace_symbols(&mut self) -> Value {
        self.request("workspace/symbol", json!({ "query": "" })).await
    }

    async fn shutdown(&mut self) {
        assert!(self.request("shutdown", Value::Null).await.is_null());
    }

    async fn exit(mut self) {
        self.notify("exit", Value::Null).await;
        self.writer.shutdown().await.unwrap();
        let result = tokio::time::timeout(SESSION_TIMEOUT, self.server_task)
            .await
            .expect("server loop should stop after `exit`")
            .expect("server task should not panic");
        assert!(result.is_ok(), "server loop failed after graceful shutdown: {result:?}");
    }

    async fn handle_server_message(&mut self, message: Value) {
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .expect("server request should have a method");
        self.server_messages.push(message.clone());
        let Some(id) = message.get("id").cloned() else { return };
        let result = match method {
            "workspace/configuration" => {
                let count =
                    message.pointer("/params/items").and_then(Value::as_array).map_or(0, Vec::len);
                Value::Array(vec![Value::Null; count])
            }
            "client/registerCapability"
            | "window/workDoneProgress/create"
            | "workspace/codeLens/refresh"
            | "workspace/diagnostic/refresh"
            | "workspace/inlayHint/refresh" => Value::Null,
            _ => panic!("unexpected server request `{method}`: {message}"),
        };
        self.watched_files_registered |= message.get("method").and_then(Value::as_str)
            == Some("client/registerCapability")
            && message.pointer("/params/registrations").and_then(Value::as_array).is_some_and(
                |registrations| {
                    registrations.iter().any(|registration| {
                        registration.get("method").and_then(Value::as_str)
                            == Some("workspace/didChangeWatchedFiles")
                    })
                },
            );
        write_lsp_frame(&mut self.writer, json!({ "jsonrpc": "2.0", "id": id, "result": result }))
            .await;
    }

    fn server_message_count(&self, method: &str) -> usize {
        self.server_messages
            .iter()
            .filter(|message| message.get("method").and_then(Value::as_str) == Some(method))
            .count()
    }

    fn server_messages(&self, method: &str) -> Vec<&Value> {
        self.server_messages
            .iter()
            .filter(|message| message.get("method").and_then(Value::as_str) == Some(method))
            .collect()
    }

    async fn wait_for_server_message_count(&mut self, method: &str, expected: usize) {
        while self.server_message_count(method) < expected {
            let message = self.read_message().await;
            assert!(
                message.get("method").is_some(),
                "unexpected response while waiting for server method `{method}`: {message}"
            );
            self.handle_server_message(message).await;
        }
    }

    async fn read_message(&mut self) -> Value {
        tokio::time::timeout(SESSION_TIMEOUT, read_lsp_frame(&mut self.reader))
            .await
            .expect("LSP message should arrive")
    }
}

fn diagnostic_client_capabilities(document_pull: bool, refresh: bool, pull_data: bool) -> Value {
    assert!(!pull_data || document_pull);
    let mut capabilities = json!({
        "textDocument": {
            "publishDiagnostics": { "dataSupport": true },
        },
    });
    let capabilities_object = capabilities.as_object_mut().unwrap();
    if document_pull {
        capabilities_object.get_mut("textDocument").unwrap().as_object_mut().unwrap().insert(
            "diagnostic".into(),
            if pull_data { json!({ "dataSupport": true }) } else { json!({}) },
        );
    }
    if refresh {
        capabilities_object
            .insert("workspace".into(), json!({ "diagnostics": { "refreshSupport": true } }));
    }
    capabilities
}

fn assert_one_unresolved_diagnostic(
    diagnostics: &Value,
    uri: &Url,
    include_data: bool,
    profile: &str,
) {
    let diagnostics = diagnostics
        .as_array()
        .unwrap_or_else(|| panic!("{profile}: diagnostics are not an array: {diagnostics}"));
    let [diagnostic] = diagnostics.as_slice() else {
        panic!("{profile}: expected one diagnostic, got {diagnostics:?}");
    };
    assert_eq!(
        diagnostic.get("message").and_then(Value::as_str),
        Some("unresolved symbol `missingValue`"),
        "{profile}: unexpected diagnostic: {diagnostic}"
    );
    assert_eq!(
        diagnostic.get("range"),
        Some(&json!({
            "start": { "line": 2, "character": 15 },
            "end": { "line": 2, "character": 27 },
        })),
        "{profile}: unexpected diagnostic range"
    );
    assert_eq!(diagnostic.get("severity"), Some(&Value::from(1)));
    assert_eq!(diagnostic.get("source"), Some(&Value::from("solar")));
    assert_eq!(
        diagnostic.get("data").is_some(),
        include_data,
        "{profile}: unexpected diagnostic data support: {diagnostic}"
    );
    if include_data {
        assert_eq!(diagnostic.pointer("/data/uri").and_then(Value::as_str), Some(uri.as_str()));
    }
}

fn symbol_names(response: &Value) -> Vec<&str> {
    response
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|symbol| symbol.get("name").and_then(Value::as_str))
        .collect()
}

fn symbol_start_character(response: &Value, name: &str) -> Option<u64> {
    let symbol = response
        .as_array()?
        .iter()
        .find(|symbol| symbol.get("name").and_then(Value::as_str) == Some(name))?;
    symbol
        .pointer("/range/start/character")
        .or_else(|| symbol.pointer("/location/range/start/character"))
        .and_then(Value::as_u64)
}

fn assert_symbol_replaced(response: &Value, expected: &str, removed: &str, profile: &str) {
    let names = symbol_names(response);
    assert!(names.contains(&expected), "{profile}: missing `{expected}` in {names:?}");
    assert!(!names.contains(&removed), "{profile}: stale `{removed}` in {names:?}");
}

async fn wait_for_workspace_symbols(
    session: &mut RawSession,
    expected: &str,
    removed: &str,
    profile: &str,
) {
    let mut observed = Vec::new();
    let ready = tokio::time::timeout(SESSION_TIMEOUT, async {
        loop {
            let response = session.workspace_symbols().await;
            observed = symbol_names(&response).into_iter().map(str::to_owned).collect();
            if observed.iter().any(|name| name == expected)
                && observed.iter().all(|name| name != removed)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(
        ready.is_ok(),
        "{profile}: workspace symbols never replaced `{removed}` with `{expected}`; last response: {observed:?}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn client_profiles_complete_a_raw_lsp_session() {
    for profile in CLIENT_PROFILES {
        let profile_label = profile.label();
        let project = TestProject::from_fixture(PROJECT_FIXTURE);
        let root_uri = Url::from_file_path(project.root()).unwrap();
        let document_uri = Url::from_file_path(project.path("/Session.sol")).unwrap();
        let mut session = RawSession::start();
        let client_capabilities = profile.capabilities();
        let expects_code_action_provider = client_capabilities
            .pointer("/textDocument/codeAction/codeActionLiteralSupport")
            .is_some();
        let expects_watched_files_registration = client_capabilities
            .pointer("/workspace/didChangeWatchedFiles/dynamicRegistration")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let initialize = session.initialize(&profile, &root_uri, &client_capabilities).await;
        let capabilities = initialize
            .get("capabilities")
            .unwrap_or_else(|| panic!("{profile_label}: missing server capabilities"));
        assert!(
            capabilities.get("positionEncoding").is_none_or(|encoding| encoding == "utf-16"),
            "{profile_label}: server selected a non-UTF-16 position encoding"
        );
        assert_eq!(capabilities.get("documentSymbolProvider"), Some(&Value::Bool(true)));
        assert_eq!(
            capabilities.get("codeActionProvider").is_some(),
            expects_code_action_provider,
            "{profile_label}: CodeAction provider did not match the client profile"
        );
        assert_eq!(
            capabilities.get("diagnosticProvider").is_some(),
            client_capabilities.pointer("/textDocument/diagnostic").is_some(),
            "{profile_label}: diagnostic delivery did not match the client profile"
        );

        session.notify("initialized", json!({})).await;
        session.open(&document_uri, SESSION_SOURCE).await;
        let symbols = session.document_request("textDocument/documentSymbol", &document_uri).await;
        assert_symbol_replaced(&symbols, "Before", "After", &profile_label);
        assert_eq!(
            symbol_start_character(&symbols, "Before"),
            Some(7),
            "{profile_label}: symbol range did not use UTF-16 columns"
        );

        session.change(&document_uri).await;
        let symbols = session.document_request("textDocument/documentSymbol", &document_uri).await;
        assert_symbol_replaced(&symbols, "After", "Before", &profile_label);
        let hover = session
            .request(
                "textDocument/hover",
                json!({
                    "textDocument": { "uri": document_uri },
                    "position": { "line": 0, "character": 17 },
                }),
            )
            .await;
        assert!(
            hover.to_string().contains("contract After"),
            "{profile_label}: hover did not resolve the edited contract: {hover}"
        );

        session.document_notification("textDocument/didSave", &document_uri).await;
        session.document_notification("textDocument/didClose", &document_uri).await;
        wait_for_workspace_symbols(&mut session, "Before", "After", &profile_label).await;

        session.shutdown().await;
        assert_eq!(
            session.watched_files_registered, expects_watched_files_registration,
            "{profile_label}: watched-file registration did not match the client profile"
        );
        session.exit().await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn watched_manifest_change_reloads_workspace_symbols_on_the_wire() {
    let profile = client_profile("VS Code");
    let profile_label = profile.label();
    let project = TestProject::from_fixture(PROJECT_FIXTURE);
    let root_uri = Url::from_file_path(project.root()).unwrap();
    let manifest_uri = Url::from_file_path(project.path("/foundry.toml")).unwrap();
    let mut session = RawSession::start();
    let capabilities = profile.capabilities();

    session.initialize(profile, &root_uri, &capabilities).await;
    session.notify("initialized", json!({})).await;
    session.wait_for_server_message_count("client/registerCapability", 1).await;
    assert!(
        session.watched_files_registered,
        "{profile_label}: server did not register the manifest watcher"
    );

    project.write_file("/foundry.toml", "[profile.default]\nsrc = \"after\"\n");
    session
        .notify(
            "workspace/didChangeWatchedFiles",
            json!({
                "changes": [{ "uri": manifest_uri, "type": 2 }],
            }),
        )
        .await;
    wait_for_workspace_symbols(&mut session, "After", "Before", &profile_label).await;

    session.shutdown().await;
    session.exit().await;
}

#[tokio::test(flavor = "current_thread")]
async fn configuration_change_reloads_workspace_symbols_on_the_wire() {
    let profile = client_profile("Minimal LSP client");
    let profile_label = profile.label();
    let project = TestProject::from_fixture(PROJECT_FIXTURE);
    let root_uri = Url::from_file_path(project.root()).unwrap();
    let mut session = RawSession::start();
    let capabilities = profile.capabilities();

    session.initialize(profile, &root_uri, &capabilities).await;
    session.notify("initialized", json!({})).await;

    project.write_file("/foundry.toml", "[profile.default]\nsrc = \"after\"\n");
    session
        .notify(
            "workspace/didChangeConfiguration",
            json!({
                "settings": {},
            }),
        )
        .await;
    wait_for_workspace_symbols(&mut session, "After", "Before", &profile_label).await;

    session.shutdown().await;
    session.exit().await;
}

#[tokio::test(flavor = "current_thread")]
async fn push_diagnostic_clients_publish_and_clear_without_pull() {
    for (profile, document_pull) in [("legacy push", false), ("pull without refresh", true)] {
        let project = TestProject::new();
        let root_uri = Url::from_file_path(project.root()).unwrap();
        let document_uri = Url::from_file_path(project.path("/Diagnostics.sol")).unwrap();
        let capabilities = diagnostic_client_capabilities(document_pull, false, false);
        let mut session = RawSession::start();

        let initialize = session
            .initialize(client_profile("Minimal LSP client"), &root_uri, &capabilities)
            .await;
        assert!(
            initialize.pointer("/capabilities/diagnosticProvider").is_none(),
            "{profile}: push delivery must not advertise pull diagnostics: {initialize}"
        );

        session.notify("initialized", json!({})).await;
        session.open(&document_uri, DIAGNOSTIC_SOURCE).await;
        session.document_request("textDocument/documentSymbol", &document_uri).await;
        session.wait_for_server_message_count("textDocument/publishDiagnostics", 1).await;

        {
            let publications = session
                .server_messages("textDocument/publishDiagnostics")
                .into_iter()
                .filter(|message| {
                    message.pointer("/params/uri").and_then(Value::as_str)
                        == Some(document_uri.as_str())
                })
                .collect::<Vec<_>>();
            let [publication] = publications.as_slice() else {
                panic!("{profile}: expected one diagnostic publication, got {publications:?}");
            };
            assert_one_unresolved_diagnostic(
                &publication["params"]["diagnostics"],
                &document_uri,
                true,
                profile,
            );
        }

        session.replace_document(&document_uri, 2, CLEARED_DIAGNOSTIC_SOURCE).await;
        session.document_request("textDocument/documentSymbol", &document_uri).await;
        session.wait_for_server_message_count("textDocument/publishDiagnostics", 2).await;

        let publications = session
            .server_messages("textDocument/publishDiagnostics")
            .into_iter()
            .filter(|message| {
                message.pointer("/params/uri").and_then(Value::as_str)
                    == Some(document_uri.as_str())
            })
            .collect::<Vec<_>>();
        let [_, cleared] = publications.as_slice() else {
            panic!(
                "{profile}: expected diagnostic and clearing publications, got {publications:?}"
            );
        };
        assert_eq!(cleared["params"]["diagnostics"], json!([]));
        assert_eq!(session.server_message_count("workspace/diagnostic/refresh"), 0);

        session.shutdown().await;
        session.exit().await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn pull_diagnostic_client_refreshes_and_clears_without_push() {
    let project = TestProject::new();
    let root_uri = Url::from_file_path(project.root()).unwrap();
    let document_uri = Url::from_file_path(project.path("/Diagnostics.sol")).unwrap();
    let profile = client_profile("Zed");
    let profile_label = profile.label();
    let capabilities = profile.capabilities();
    let mut session = RawSession::start();

    let initialize = session.initialize(profile, &root_uri, &capabilities).await;
    assert_eq!(
        initialize.pointer("/capabilities/diagnosticProvider"),
        Some(&json!({
            "interFileDependencies": true,
            "workDoneProgress": true,
            "workspaceDiagnostics": true,
        })),
        "pull delivery must advertise the exact diagnostic provider"
    );

    session.notify("initialized", json!({})).await;
    session.open(&document_uri, DIAGNOSTIC_SOURCE).await;
    let initial = session.document_request("textDocument/diagnostic", &document_uri).await;
    assert_eq!(initial.get("kind").and_then(Value::as_str), Some("full"));
    assert_one_unresolved_diagnostic(&initial["items"], &document_uri, false, &profile_label);
    assert!(
        initial["items"][0].get("data").is_none(),
        "pull diagnostic data must follow pull dataSupport, not push dataSupport: {initial}"
    );
    let initial_result_id = initial
        .get("resultId")
        .and_then(Value::as_str)
        .expect("full diagnostic report should have a result ID")
        .to_owned();

    let diagnostic = initial["items"][0].clone();
    let code_actions = session
        .request(
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": document_uri },
                "range": diagnostic["range"],
                "context": { "diagnostics": [diagnostic] },
            }),
        )
        .await;
    assert_eq!(
        code_actions,
        json!([]),
        "the unresolved-symbol diagnostic should not produce a quick fix"
    );

    session.wait_for_server_message_count("workspace/diagnostic/refresh", 1).await;
    assert_eq!(session.server_message_count("textDocument/publishDiagnostics"), 0);

    session.replace_document(&document_uri, 2, CLEARED_DIAGNOSTIC_SOURCE).await;
    let cleared = session
        .request(
            "textDocument/diagnostic",
            json!({
                "textDocument": { "uri": document_uri },
                "previousResultId": initial_result_id,
            }),
        )
        .await;
    assert_eq!(cleared.get("kind").and_then(Value::as_str), Some("full"));
    assert_eq!(cleared.get("items"), Some(&json!([])));
    assert_eq!(session.server_message_count("workspace/diagnostic/refresh"), 1);
    assert_eq!(session.server_message_count("textDocument/publishDiagnostics"), 0);

    session.shutdown().await;
    session.exit().await;
}

#[tokio::test(flavor = "current_thread")]
async fn pull_diagnostic_data_support_is_used_on_the_wire() {
    let project = TestProject::new();
    let root_uri = Url::from_file_path(project.root()).unwrap();
    let document_uri = Url::from_file_path(project.path("/Diagnostics.sol")).unwrap();
    let capabilities = diagnostic_client_capabilities(true, true, true);
    let mut session = RawSession::start();

    let initialize =
        session.initialize(client_profile("Minimal LSP client"), &root_uri, &capabilities).await;
    assert!(initialize.pointer("/capabilities/diagnosticProvider").is_some());

    session.notify("initialized", json!({})).await;
    session.open(&document_uri, DIAGNOSTIC_SOURCE).await;
    let report = session.document_request("textDocument/diagnostic", &document_uri).await;
    assert_one_unresolved_diagnostic(&report["items"], &document_uri, true, "pull data");
    assert_eq!(session.server_message_count("textDocument/publishDiagnostics"), 0);

    session.shutdown().await;
    session.exit().await;
}

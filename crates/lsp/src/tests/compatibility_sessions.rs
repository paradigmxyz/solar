use crate::{
    LaunchConfig, new_server_service,
    test_support::{TestProject, read_lsp_frame, write_lsp_frame},
};
use async_lsp::ClientSocket;
use lsp_types::Url;
use serde_json::{Value, json};
use snapbox::{IntoData, assert_data_eq, str};
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
    src = "lib/before"

    //- /lib/before/Before.sol
    contract Before {}

    //- /lib/after/After.sol
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

struct RawSession {
    reader: BufReader<ReadHalf<DuplexStream>>,
    writer: WriteHalf<DuplexStream>,
    next_request_id: u64,
    server_messages: Vec<Value>,
    server_task: JoinHandle<async_lsp::Result<()>>,
    _client: ClientSocket,
}

impl RawSession {
    fn start() -> Self {
        let (main_loop, client) = async_lsp::MainLoop::new_server(|client| {
            new_server_service(client, LaunchConfig::default())
        });
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
            server_messages: Vec::new(),
            server_task,
            _client: client,
        }
    }

    async fn start_initialized(
        profile: &ClientProfile,
        project: &TestProject,
        capabilities: &Value,
    ) -> (Self, Value) {
        let mut session = Self::start();
        let root_uri = Url::from_file_path(project.root()).unwrap();
        let initialize = session.initialize(profile, &root_uri, capabilities).await;
        session.notify("initialized", json!({})).await;
        (session, initialize)
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
        write_lsp_frame(&mut self.writer, json!({ "jsonrpc": "2.0", "id": id, "result": result }))
            .await;
    }

    fn server_messages(&self, method: &str) -> Vec<&Value> {
        self.server_messages
            .iter()
            .filter(|message| message.get("method").and_then(Value::as_str) == Some(method))
            .collect()
    }

    fn server_message_count(&self, method: &str) -> usize {
        self.server_messages(method).len()
    }

    fn registered_watched_files(&self) -> bool {
        self.server_messages("client/registerCapability").iter().any(|message| {
            message["params"]["registrations"].as_array().is_some_and(|registrations| {
                registrations
                    .iter()
                    .any(|registration| registration["method"] == "workspace/didChangeWatchedFiles")
            })
        })
    }

    fn publications(&self, uri: &Url) -> Vec<&Value> {
        self.server_messages("textDocument/publishDiagnostics")
            .into_iter()
            .filter(|message| message["params"]["uri"] == uri.as_str())
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
    let mut capabilities =
        json!({ "textDocument": { "publishDiagnostics": { "dataSupport": true } } });
    if document_pull {
        capabilities["textDocument"]["diagnostic"] =
            if pull_data { json!({ "dataSupport": true }) } else { json!({}) };
    }
    if refresh {
        capabilities["workspace"] = json!({ "diagnostics": { "refreshSupport": true } });
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
        let document_uri = Url::from_file_path(project.path("/Session.sol")).unwrap();
        let client_capabilities = profile.capabilities();
        let expects_code_action_provider = client_capabilities
            .pointer("/textDocument/codeAction/codeActionLiteralSupport")
            .is_some();
        let expects_watched_files_registration = client_capabilities
            .pointer("/workspace/didChangeWatchedFiles/dynamicRegistration")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let (mut session, initialize) =
            RawSession::start_initialized(&profile, &project, &client_capabilities).await;
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
            session.registered_watched_files(),
            expects_watched_files_registration,
            "{profile_label}: watched-file registration did not match the client profile"
        );
        session.exit().await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn workspace_changes_reload_workspace_symbols_on_the_wire() {
    for (name, method) in [
        ("VS Code", "workspace/didChangeWatchedFiles"),
        ("Minimal LSP client", "workspace/didChangeConfiguration"),
    ] {
        let profile = client_profile(name);
        let profile_label = profile.label();
        let project = TestProject::from_fixture(PROJECT_FIXTURE);
        let (mut session, _) =
            RawSession::start_initialized(profile, &project, &profile.capabilities()).await;

        let params = if method == "workspace/didChangeWatchedFiles" {
            session.wait_for_server_message_count("client/registerCapability", 1).await;
            assert!(
                session.registered_watched_files(),
                "{profile_label}: server did not register the manifest watcher"
            );
            let manifest_uri = Url::from_file_path(project.path("/foundry.toml")).unwrap();
            json!({ "changes": [{ "uri": manifest_uri, "type": 2 }] })
        } else {
            json!({ "settings": {} })
        };
        project.write_file("/foundry.toml", "[profile.default]\nsrc = \"lib/after\"\n");
        session.notify(method, params).await;
        wait_for_workspace_symbols(&mut session, "After", "Before", &profile_label).await;

        session.shutdown().await;
        session.exit().await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn push_diagnostic_clients_publish_and_clear_without_pull() {
    for (profile, document_pull) in [("legacy push", false), ("pull without refresh", true)] {
        let project = TestProject::new();
        let document_uri = Url::from_file_path(project.path("/Diagnostics.sol")).unwrap();
        let capabilities = diagnostic_client_capabilities(document_pull, false, false);
        let (mut session, initialize) = RawSession::start_initialized(
            client_profile("Minimal LSP client"),
            &project,
            &capabilities,
        )
        .await;
        assert!(
            initialize.pointer("/capabilities/diagnosticProvider").is_none(),
            "{profile}: push delivery must not advertise pull diagnostics: {initialize}"
        );

        session.open(&document_uri, DIAGNOSTIC_SOURCE).await;
        session.document_request("textDocument/documentSymbol", &document_uri).await;
        session.wait_for_server_message_count("textDocument/publishDiagnostics", 1).await;

        let publications = session.publications(&document_uri);
        let [publication] = publications.as_slice() else {
            panic!("{profile}: expected one diagnostic publication, got {publications:?}");
        };
        assert_one_unresolved_diagnostic(
            &publication["params"]["diagnostics"],
            &document_uri,
            true,
            profile,
        );

        session.replace_document(&document_uri, 2, CLEARED_DIAGNOSTIC_SOURCE).await;
        session.document_request("textDocument/documentSymbol", &document_uri).await;
        session.wait_for_server_message_count("textDocument/publishDiagnostics", 2).await;

        let publications = session.publications(&document_uri);
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
    let document_uri = Url::from_file_path(project.path("/Diagnostics.sol")).unwrap();
    let profile = client_profile("Zed");
    let profile_label = profile.label();
    let (mut session, initialize) =
        RawSession::start_initialized(profile, &project, &profile.capabilities()).await;
    assert_eq!(
        initialize.pointer("/capabilities/diagnosticProvider"),
        Some(&json!({
            "interFileDependencies": true,
            "workDoneProgress": true,
            "workspaceDiagnostics": true,
        })),
        "pull delivery must advertise the exact diagnostic provider"
    );

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
    let document_uri = Url::from_file_path(project.path("/Diagnostics.sol")).unwrap();
    let capabilities = diagnostic_client_capabilities(true, true, true);
    let (mut session, initialize) = RawSession::start_initialized(
        client_profile("Minimal LSP client"),
        &project,
        &capabilities,
    )
    .await;
    assert!(initialize.pointer("/capabilities/diagnosticProvider").is_some());

    session.open(&document_uri, DIAGNOSTIC_SOURCE).await;
    let report = session.document_request("textDocument/diagnostic", &document_uri).await;
    assert_one_unresolved_diagnostic(&report["items"], &document_uri, true, "pull data");
    assert_eq!(session.server_message_count("textDocument/publishDiagnostics"), 0);

    session.shutdown().await;
    session.exit().await;
}

/// Returns the only native diagnostic for `source`, one field per line.
async fn native_diagnostic_on_the_wire(source: &str, pull: bool) -> String {
    let project = TestProject::new();
    let document_uri = Url::from_file_path(project.path("/Details.sol")).unwrap();
    let mut capabilities = diagnostic_client_capabilities(pull, pull, false);
    capabilities["textDocument"]["publishDiagnostics"] = json!({
        "relatedInformation": true,
        "tagSupport": { "valueSet": [1, 2] },
    });
    let (mut session, initialize) = RawSession::start_initialized(
        client_profile("Minimal LSP client"),
        &project,
        &capabilities,
    )
    .await;
    assert_eq!(initialize.pointer("/capabilities/diagnosticProvider").is_some(), pull);
    session.open(&document_uri, source).await;

    let diagnostics = if pull {
        let report = session.document_request("textDocument/diagnostic", &document_uri).await;
        assert_eq!(report["kind"], "full");
        assert_eq!(session.server_message_count("textDocument/publishDiagnostics"), 0);
        report["items"].clone()
    } else {
        session.document_request("textDocument/documentSymbol", &document_uri).await;
        session.wait_for_server_message_count("textDocument/publishDiagnostics", 1).await;
        let publications = session.server_messages("textDocument/publishDiagnostics");
        assert_eq!(publications.len(), 1);
        assert_eq!(publications[0]["params"]["uri"], document_uri.as_str());
        publications[0]["params"]["diagnostics"].clone()
    };
    let diagnostics = diagnostics.as_array().unwrap();
    let [diagnostic] = diagnostics.as_slice() else {
        panic!("expected one diagnostic, got {diagnostics:?}");
    };
    let rendered = diagnostic
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| format!("{key}: {value}\n"))
        .collect::<String>()
        .replace(document_uri.as_str(), "[URI]");
    session.shutdown().await;
    session.exit().await;
    rendered
}

#[tokio::test(flavor = "current_thread")]
async fn did_open_before_initialize_is_not_observable() {
    let project = TestProject::new();
    let root_uri = Url::from_file_path(project.root()).unwrap();
    let document_uri = Url::from_file_path(project.path("/Ghost.sol")).unwrap();
    let mut session = RawSession::start();

    session
        .notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": document_uri,
                    "languageId": "solidity",
                    "version": 1,
                    "text": "contract Ghost {}\n",
                },
            }),
        )
        .await;
    session.initialize(client_profile("Minimal LSP client"), &root_uri, &json!({})).await;
    session.notify("initialized", json!({})).await;

    let symbols = session.document_request("textDocument/documentSymbol", &document_uri).await;
    assert!(
        symbol_names(&symbols).is_empty(),
        "pre-initialize document remained observable: {symbols}"
    );

    session.shutdown().await;
    session.exit().await;
}

#[tokio::test(flavor = "current_thread")]
async fn diagnostic_details_are_preserved_on_the_wire() {
    for (source, expected) in [
        (
            "contract Details { uint8 x = 300; }\n",
            str![[r#"
message: "mismatched types\nexpected `uint8`, found `int_literal[9]`"
range: {"end":{"character":32,"line":0},"start":{"character":29,"line":0}}
relatedInformation: []
severity: 1
source: "solar"

"#]],
        ),
        (
            "contract Base { function f() public {} }\ncontract Derived is Base { function f() public override {} }\n",
            str![[r#"
code: "4334"
message: "cannot override non-virtual function\nhelp: add `virtual` to the base function to allow overriding"
range: {"end":{"character":38,"line":0},"start":{"character":16,"line":0}}
relatedInformation: [{"location":{"range":{"end":{"character":58,"line":1},"start":{"character":27,"line":1}},"uri":"[URI]"},"message":"overriding function is here"}]
severity: 1
source: "solar"

"#]],
        ),
        (
            "contract Details { function f() public view returns (uint256) { return block.difficulty; } }\n",
            str![[r#"
code: "8417"
message: "since Paris, `block.difficulty` was replaced by `block.prevrandao`, which returns a random number from the beacon chain"
range: {"end":{"character":87,"line":0},"start":{"character":71,"line":0}}
relatedInformation: []
severity: 2
source: "solar"
tags: [2]

"#]],
        ),
    ] {
        for pull in [false, true] {
            let diagnostic = native_diagnostic_on_the_wire(source, pull).await;
            assert_data_eq!(diagnostic, expected.clone().raw());
        }
    }
}

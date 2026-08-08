use crate::{
    new_server_service,
    test_support::{TestProject, read_lsp_frame, write_lsp_frame},
};
use async_lsp::ClientSocket;
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::{
    io::{AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf},
    task::JoinHandle,
};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

const SESSION_TIMEOUT: Duration = Duration::from_secs(5);
const SESSION_SOURCE: &str = "/*😀*/ contract Before { function ping() external {} }\n";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientProfile {
    name: String,
    version: String,
    source: String,
    expects_watched_files_registration: bool,
    expects_code_action_provider: bool,
    capabilities: Value,
}

impl ClientProfile {
    fn label(&self) -> String {
        format!("{} {} ({})", self.name, self.version, self.source)
    }
}

struct RawSession {
    reader: BufReader<ReadHalf<DuplexStream>>,
    writer: WriteHalf<DuplexStream>,
    next_request_id: u64,
    server_requests: Vec<Value>,
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
            server_requests: Vec::new(),
            server_task,
            _client: client,
        }
    }

    async fn notify(&mut self, method: &str, params: Value) {
        self.write_message(json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
        .await;
    }

    async fn request(&mut self, method: &str, params: Value) -> Value {
        let id = Value::from(self.next_request_id);
        self.next_request_id += 1;
        self.write_message(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
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

    fn registered_watched_files(&self) -> bool {
        self.server_requests.iter().any(|request| {
            request.get("method").and_then(Value::as_str) == Some("client/registerCapability")
                && request.pointer("/params/registrations").and_then(Value::as_array).is_some_and(
                    |registrations| {
                        registrations.iter().any(|registration| {
                            registration.get("method").and_then(Value::as_str)
                                == Some("workspace/didChangeWatchedFiles")
                        })
                    },
                )
        })
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
        let Some(id) = message.get("id").cloned() else { return };
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .expect("server request should have a method");
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
        self.server_requests.push(message);
        self.write_message(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }))
        .await;
    }

    async fn write_message(&mut self, message: Value) {
        write_lsp_frame(&mut self.writer, message).await;
    }

    async fn read_message(&mut self) -> Value {
        tokio::time::timeout(SESSION_TIMEOUT, read_lsp_frame(&mut self.reader))
            .await
            .expect("LSP message should arrive")
    }
}

fn client_profiles() -> Vec<ClientProfile> {
    serde_json::from_str(include_str!("fixtures/client_profiles.json")).unwrap()
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
            let response = session
                .request(
                    "workspace/symbol",
                    json!({
                        "query": "",
                    }),
                )
                .await;
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
    let profiles = client_profiles();
    assert_eq!(profiles.len(), 4, "compatibility suite should exercise four client profiles");

    for profile in profiles {
        let profile_label = profile.label();
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "before"

            //- /before/Before.sol
            contract Before {}

            //- /after/After.sol
            contract After {}
            "#,
        );
        let root_uri = lsp_types::Url::from_file_path(project.root()).unwrap();
        let document_uri = lsp_types::Url::from_file_path(project.path("/Session.sol")).unwrap();
        let mut session = RawSession::start();

        let initialize = session
            .request(
                "initialize",
                json!({
                    "processId": null,
                    "clientInfo": {
                        "name": profile.name,
                        "version": profile.version,
                    },
                    "rootUri": root_uri,
                    "capabilities": profile.capabilities,
                    "workspaceFolders": [{
                        "uri": root_uri,
                        "name": "compatibility-session",
                    }],
                }),
            )
            .await;
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
            profile.expects_code_action_provider,
            "{profile_label}: CodeAction provider did not match the client profile"
        );

        session.notify("initialized", json!({})).await;
        session
            .notify(
                "textDocument/didOpen",
                json!({
                    "textDocument": {
                        "uri": document_uri,
                        "languageId": "solidity",
                        "version": 1,
                        "text": SESSION_SOURCE,
                    },
                }),
            )
            .await;
        let symbols = session
            .request(
                "textDocument/documentSymbol",
                json!({
                    "textDocument": { "uri": document_uri },
                }),
            )
            .await;
        assert_symbol_replaced(&symbols, "Before", "After", &profile_label);
        assert_eq!(
            symbol_start_character(&symbols, "Before"),
            Some(7),
            "{profile_label}: symbol range did not use UTF-16 columns"
        );

        session
            .notify(
                "textDocument/didChange",
                json!({
                    "textDocument": {
                        "uri": document_uri,
                        "version": 2,
                    },
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
        let symbols = session
            .request(
                "textDocument/documentSymbol",
                json!({
                    "textDocument": { "uri": document_uri },
                }),
            )
            .await;
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

        session
            .notify(
                "textDocument/didSave",
                json!({
                    "textDocument": { "uri": document_uri },
                }),
            )
            .await;
        session
            .notify(
                "textDocument/didClose",
                json!({
                    "textDocument": { "uri": document_uri },
                }),
            )
            .await;
        wait_for_workspace_symbols(&mut session, "Before", "After", &profile_label).await;

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
        assert_eq!(
            session.registered_watched_files(),
            profile.expects_watched_files_registration,
            "{profile_label}: watched-file registration did not match the client profile"
        );
        session.exit().await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn did_open_before_initialize_is_not_observable() {
    let project = TestProject::new();
    let root_uri = lsp_types::Url::from_file_path(project.root()).unwrap();
    let document_uri = lsp_types::Url::from_file_path(project.path("/Ghost.sol")).unwrap();
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
    session
        .request(
            "initialize",
            json!({
                "processId": null,
                "rootUri": root_uri,
                "capabilities": {},
                "workspaceFolders": [{
                    "uri": root_uri,
                    "name": "lifecycle-session",
                }],
            }),
        )
        .await;
    session.notify("initialized", json!({})).await;

    let symbols = session
        .request(
            "textDocument/documentSymbol",
            json!({
                "textDocument": { "uri": document_uri },
            }),
        )
        .await;
    assert!(
        symbol_names(&symbols).is_empty(),
        "pre-initialize document remained observable: {symbols}"
    );

    session.shutdown().await;
    session.exit().await;
}

use lsp_types::{GotoDefinitionResponse, Hover, HoverContents, MarkupKind, Position, Range, Url};
use serde_json::Value;
use snapbox::{assert_data_eq, str};
use solar_interface::source_map::{FileLoader, RealFileLoader};
use std::{
    fmt::Write as _,
    fs,
    io::{self, BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tempfile::TempDir;

const SOLAR: &str = env!("CARGO_BIN_EXE_solar");
const TIMEOUT: Duration = Duration::from_secs(5);

struct LspProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    messages: Receiver<io::Result<Value>>,
    reader: Option<JoinHandle<()>>,
    _workspace: TempDir,
}

impl LspProcess {
    fn spawn() -> Self {
        let workspace = tempfile::tempdir().expect("create temporary LSP workspace");
        let mut child = Command::new(SOLAR)
            .arg("lsp")
            .current_dir(workspace.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn solar LSP server");
        let stdin = child.stdin.take().expect("piped LSP stdin");
        let stdout = child.stdout.take().expect("piped LSP stdout");
        let (message_tx, messages) = mpsc::channel();
        let reader = thread::spawn(move || {
            let mut stdout = BufReader::new(stdout);
            loop {
                match read_message(&mut stdout) {
                    Ok(Some(message)) => {
                        if message_tx.send(Ok(message)).is_err() {
                            return;
                        }
                    }
                    Ok(None) => return,
                    Err(error) => {
                        let _ = message_tx.send(Err(error));
                        return;
                    }
                }
            }
        });

        Self { child, stdin: Some(stdin), messages, reader: Some(reader), _workspace: workspace }
    }

    fn send(&mut self, message: Value) {
        let body = serde_json::to_vec(&message).expect("serialize LSP message");
        let stdin = self.stdin.as_mut().expect("LSP stdin should remain open");
        write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).expect("write LSP header");
        stdin.write_all(&body).expect("write LSP body");
        stdin.flush().expect("flush LSP message");
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(serde_json::json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    fn request(&mut self, id: i64, method: &str, params: Value) -> Value {
        self.send(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        self.receive_response(id)
    }

    fn exit(&mut self) -> ExitStatus {
        self.send(serde_json::json!({ "jsonrpc": "2.0", "method": "exit" }));
        self.wait_for_exit()
    }

    fn wait_for_exit(&mut self) -> ExitStatus {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().expect("poll solar LSP server") {
                self.stdin.take();
                self.join_reader();
                return status;
            }
            if Instant::now() >= deadline {
                self.terminate();
                panic!("solar LSP server did not exit within {TIMEOUT:?}");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn receive_response(&mut self, id: i64) -> Value {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let message = match self.messages.recv_timeout(remaining) {
                Ok(Ok(message)) => message,
                Ok(Err(error)) => panic!("failed to read LSP response: {error}"),
                Err(RecvTimeoutError::Timeout) => {
                    panic!("LSP response {id} did not arrive within {TIMEOUT:?}")
                }
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("LSP server closed stdout before response {id}")
                }
            };

            if message.get("id") == Some(&Value::from(id))
                && (message.get("result").is_some() || message.get("error").is_some())
            {
                return message;
            }
            if let (Some(request_id), Some(_)) = (message.get("id").cloned(), message.get("method"))
            {
                self.send(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": null,
                }));
            }
        }
    }

    fn terminate(&mut self) {
        self.stdin.take();
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        self.join_reader();
    }

    fn join_reader(&mut self) {
        if let Some(reader) = self.reader.take() {
            reader.join().expect("LSP frame reader should not panic");
        }
    }
}

impl Drop for LspProcess {
    fn drop(&mut self) {
        self.terminate();
    }
}

fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return if content_length.is_none() {
                Ok(None)
            } else {
                Err(io::Error::from(io::ErrorKind::UnexpectedEof))
            };
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "malformed LSP header"));
        };
        if name.eq_ignore_ascii_case("Content-Length") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            );
        }
    }

    let content_length = content_length
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length"))?;
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[test]
fn exit_without_shutdown_returns_failure_status() {
    let mut server = LspProcess::spawn();

    let status = server.exit();
    assert_eq!(status.code(), Some(1));
}

#[test]
fn shutdown_then_exit_returns_success_status() {
    let mut server = LspProcess::spawn();
    let initialize = server.request(
        1,
        "initialize",
        serde_json::json!({
            "processId": null,
            "rootUri": null,
            "workspaceFolders": null,
            "capabilities": {},
        }),
    );
    assert!(initialize.get("result").is_some(), "initialize failed: {initialize}");

    server.notify("initialized", serde_json::json!({}));
    let shutdown = server.request(2, "shutdown", Value::Null);
    assert!(shutdown.get("result").is_some(), "shutdown failed: {shutdown}");

    let status = server.exit();
    assert_eq!(status.code(), Some(0));
}

#[test]
fn failed_initialize_does_not_allow_graceful_exit() {
    let mut server = LspProcess::spawn();
    let initialize = server.request(1, "initialize", serde_json::json!({ "capabilities": [] }));
    assert_eq!(initialize["error"]["code"], -32602);

    server.notify("initialized", serde_json::json!({}));
    let shutdown = server.request(2, "shutdown", Value::Null);
    assert_eq!(shutdown["error"]["code"], -32002);

    let status = server.exit();
    assert_eq!(status.code(), Some(1));
}

#[test]
fn builtin_hover_has_documentation_without_source_navigation() {
    let source = "\
type Price is uint256;
contract Builtins {
    function f() external {}
    function inspect(bytes memory data) external view returns (bytes32, uint256, bytes4, address) {
        Price price = Price.wrap(1);
        bytes memory encoded = abi.encode(data);
        return (keccak256(encoded), Price.unwrap(price), this.f.selector, msg.sender);
    }
}
";
    let mut server = LspProcess::spawn();
    let workspace = RealFileLoader.canonicalize_path(server._workspace.path()).unwrap();
    let path = workspace.join("Builtins.sol");
    fs::write(&path, source).unwrap();
    let uri = Url::from_file_path(&path).unwrap();
    let initialize = server.request(
        1,
        "initialize",
        serde_json::json!({
            "processId": null,
            "rootUri": Url::from_directory_path(&workspace).unwrap(),
            "capabilities": {},
        }),
    );
    assert!(initialize.get("result").is_some(), "initialize failed: {initialize}");
    server.notify("initialized", serde_json::json!({}));
    server.notify(
        "textDocument/didOpen",
        serde_json::json!({
            "textDocument": { "uri": uri, "languageId": "solidity", "version": 1, "text": source },
        }),
    );

    let mut output = String::new();
    for (index, (needle, token, offset)) in [
        ("this.f.selector", "selector", 7),
        ("msg.sender", "msg", 0),
        ("msg.sender", "sender", 4),
        ("keccak256(encoded)", "keccak256", 0),
        ("abi.encode", "abi", 0),
        ("abi.encode", "encode", 4),
        ("Price.wrap", "wrap", 6),
        ("Price.unwrap", "unwrap", 6),
        ("this.f.selector", "this", 0),
    ]
    .into_iter()
    .enumerate()
    {
        let position = source_position(source, source.find(needle).unwrap() + offset);
        let params = serde_json::json!({ "textDocument": { "uri": uri }, "position": position });
        let id = 10 + index as i64 * 3;
        let hover = server.request(id, "textDocument/hover", params.clone());
        let hover: Hover = serde_json::from_value(hover["result"].clone())
            .unwrap_or_else(|error| panic!("{token}: {error}: {hover}"));
        assert_eq!(selected_line_text(source, hover.range.unwrap()), token);
        let HoverContents::Markup(hover) = hover.contents else {
            panic!("{token}: expected Markdown hover");
        };
        assert_eq!(hover.kind, MarkupKind::Markdown);
        let definition = server.request(id + 1, "textDocument/definition", params.clone());
        let declaration = server.request(id + 2, "textDocument/declaration", params);
        assert_eq!(definition.get("result"), Some(&Value::Null), "{token}: {definition}");
        assert_eq!(declaration.get("result"), Some(&Value::Null), "{token}: {declaration}");
        let body = hover.value.strip_prefix("```solidity\n").unwrap();
        let (signature, description) = body.split_once("\n```\n\n").unwrap();
        writeln!(output, "{token}: {signature}\n{description}").unwrap();
    }
    assert_data_eq!(
        output,
        str![[r#"
selector: bytes4 function.selector
The four-byte selector of a function or custom error, derived from its canonical ABI signature.

*Compiler-provided builtin; no Solidity source declaration.*

[Solidity documentation](https://docs.soliditylang.org/en/latest/types.html#function-types)
msg: namespace msg
Provides information about the current message call.

*Compiler-provided builtin; no Solidity source declaration.*

[Solidity documentation](https://docs.soliditylang.org/en/latest/units-and-global-variables.html#block-and-transaction-properties)
sender: address msg.sender
The address of the sender of the current message call.

*Compiler-provided builtin; no Solidity source declaration.*

[Solidity documentation](https://docs.soliditylang.org/en/latest/units-and-global-variables.html#block-and-transaction-properties)
keccak256: function keccak256(bytes memory) pure returns (bytes32)
Computes the Keccak-256 hash of the input bytes.

*Compiler-provided builtin; no Solidity source declaration.*

[Solidity documentation](https://docs.soliditylang.org/en/latest/units-and-global-variables.html#mathematical-and-cryptographic-functions)
abi: namespace abi
Provides ABI encoding and decoding functions.

*Compiler-provided builtin; no Solidity source declaration.*

[Solidity documentation](https://docs.soliditylang.org/en/latest/units-and-global-variables.html#abi-encoding-and-decoding-functions)
encode: function abi.encode(...) pure returns (bytes memory)
ABI-encodes the arguments into a byte array.

*Compiler-provided builtin; no Solidity source declaration.*

[Solidity documentation](https://docs.soliditylang.org/en/latest/units-and-global-variables.html#abi-encoding-and-decoding-functions)
wrap: function Price.wrap(uint256) pure returns (Price)
Converts a value of the underlying type into the user-defined value type without changing its representation.

*Compiler-provided builtin; no Solidity source declaration.*

[Solidity documentation](https://docs.soliditylang.org/en/latest/types.html#user-defined-value-types)
unwrap: function Price.unwrap(Price) pure returns (uint256)
Converts a user-defined value type into its underlying type without changing its representation.

*Compiler-provided builtin; no Solidity source declaration.*

[Solidity documentation](https://docs.soliditylang.org/en/latest/types.html#user-defined-value-types)
this: contract Builtins this
Refers to the current contract as an externally callable contract value.

*Compiler-provided builtin; no Solidity source declaration.*

[Solidity documentation](https://docs.soliditylang.org/en/latest/units-and-global-variables.html#contract-related)

"#]]
    );

    // User declarations in the same connection still navigate to the Solidity source.
    for (index, (needle, token, offset, declaration)) in
        [("Price.wrap", "Price", 0, "Price is"), ("this.f.selector", "f", 5, "f()")]
            .into_iter()
            .enumerate()
    {
        let position = source_position(source, source.find(needle).unwrap() + offset);
        let params = serde_json::json!({ "textDocument": { "uri": uri }, "position": position });
        let id = 50 + index as i64 * 3;
        let hover = server.request(id, "textDocument/hover", params.clone());
        let hover: Hover = serde_json::from_value(hover["result"].clone()).unwrap();
        assert_eq!(selected_line_text(source, hover.range.unwrap()), token);
        let definition = server.request(id + 1, "textDocument/definition", params.clone());
        let declaration_response = server.request(id + 2, "textDocument/declaration", params);
        assert_eq!(definition["result"], declaration_response["result"]);
        let response: GotoDefinitionResponse =
            serde_json::from_value(definition["result"].clone()).unwrap();
        let GotoDefinitionResponse::Array(locations) = response else {
            panic!("{token}: expected source navigation");
        };
        let [location] = locations.as_slice() else { panic!("expected one source declaration") };
        assert_eq!(location.uri, uri);
        assert_eq!(
            location.range.start,
            source_position(source, source.find(declaration).unwrap())
        );
        assert_eq!(selected_line_text(source, location.range), token);
    }

    let position = source_position(source, source.find("this.f.selector").unwrap());
    let params = serde_json::json!({ "textDocument": { "uri": uri }, "position": position });
    let response = server.request(75, "textDocument/typeDefinition", params);
    let response: GotoDefinitionResponse =
        serde_json::from_value(response["result"].clone()).unwrap();
    let GotoDefinitionResponse::Array(locations) = response else {
        panic!("this: expected contract type navigation");
    };
    let [location] = locations.as_slice() else { panic!("expected one contract type declaration") };
    assert_eq!(location.uri, uri);
    assert_eq!(location.range.start, source_position(source, source.find("Builtins {").unwrap()));
    assert_eq!(selected_line_text(source, location.range), "Builtins");

    let shutdown = server.request(100, "shutdown", Value::Null);
    assert!(shutdown.get("result").is_some(), "shutdown failed: {shutdown}");
    assert_eq!(server.exit().code(), Some(0));
}

fn source_position(source: &str, offset: usize) -> Position {
    let before = &source[..offset];
    let line = before.bytes().filter(|&byte| byte == b'\n').count() as u32;
    let character = before.rsplit('\n').next().unwrap().encode_utf16().count() as u32;
    Position::new(line, character)
}

fn selected_line_text(source: &str, range: Range) -> String {
    assert_eq!(range.start.line, range.end.line);
    let line = source.lines().nth(range.start.line as usize).unwrap();
    let selected = line
        .encode_utf16()
        .skip(range.start.character as usize)
        .take((range.end.character - range.start.character) as usize)
        .collect::<Vec<_>>();
    String::from_utf16(&selected).unwrap()
}

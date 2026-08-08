use serde_json::Value;
use std::{
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
    server.send(serde_json::json!({ "jsonrpc": "2.0", "method": "exit" }));

    let status = server.wait_for_exit();
    assert_eq!(status.code(), Some(1));
}

#[test]
fn shutdown_then_exit_returns_success_status() {
    let mut server = LspProcess::spawn();
    server.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "processId": null,
            "rootUri": null,
            "workspaceFolders": null,
            "capabilities": {},
        },
    }));
    let initialize = server.receive_response(1);
    assert!(initialize.get("result").is_some(), "initialize failed: {initialize}");

    server.send(serde_json::json!({
        "jsonrpc": "2.0",
        "method": "initialized",
        "params": {},
    }));
    server.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "shutdown",
        "params": null,
    }));
    let shutdown = server.receive_response(2);
    assert!(shutdown.get("result").is_some(), "shutdown failed: {shutdown}");

    server.send(serde_json::json!({ "jsonrpc": "2.0", "method": "exit" }));
    let status = server.wait_for_exit();
    assert_eq!(status.code(), Some(0));
}

#[test]
fn failed_initialize_does_not_allow_graceful_exit() {
    let mut server = LspProcess::spawn();
    server.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "capabilities": [] },
    }));
    let initialize = server.receive_response(1);
    assert_eq!(initialize["error"]["code"], -32602);

    server.send(serde_json::json!({
        "jsonrpc": "2.0",
        "method": "initialized",
        "params": {},
    }));
    server.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "shutdown",
        "params": null,
    }));
    let shutdown = server.receive_response(2);
    assert_eq!(shutdown["error"]["code"], -32002);

    server.send(serde_json::json!({ "jsonrpc": "2.0", "method": "exit" }));
    let status = server.wait_for_exit();
    assert_eq!(status.code(), Some(1));
}

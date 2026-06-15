#![allow(unused_crate_dependencies)]

use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

const CMD: &str = env!("CARGO_BIN_EXE_solar");

#[test]
fn lsp_lifecycle_over_stdio() {
    let mut child = Command::new(CMD)
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let mut stdin = child.stdin.take().unwrap();
        write_messages(
            &mut stdin,
            [
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {
                        "processId": null,
                        "rootUri": null,
                        "capabilities": {},
                    },
                }),
                json!({
                    "jsonrpc": "2.0",
                    "method": "initialized",
                    "params": {},
                }),
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "shutdown",
                    "params": null,
                }),
                json!({
                    "jsonrpc": "2.0",
                    "method": "exit",
                }),
            ],
        );
    }

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:#?}");
    assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));

    let responses = read_all_messages(&output.stdout);
    assert_eq!(
        responses,
        [
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "capabilities": {},
                    "serverInfo": {
                        "name": "solar",
                        "version": solar::config::version::short_version(),
                    },
                },
            }),
            json!({"jsonrpc": "2.0", "id": 2, "result": null}),
        ]
    );
}

fn write_messages(output: &mut impl Write, messages: impl IntoIterator<Item = Value>) {
    for value in messages {
        let body = serde_json::to_vec(&value).unwrap();
        write!(output, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
        output.write_all(&body).unwrap();
    }
}

fn read_all_messages(bytes: &[u8]) -> Vec<Value> {
    let mut input = BufReader::new(bytes);
    std::iter::from_fn(|| read_message(&mut input).unwrap()).collect()
}

fn read_message(input: &mut impl BufRead) -> std::io::Result<Option<Value>> {
    let mut content_length = None;
    let mut line = String::new();

    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }

        if let Some((name, value)) = trimmed.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = Some(value.trim().parse::<usize>().unwrap());
        }
    }

    let Some(content_length) = content_length else {
        return Ok(None);
    };

    let mut body = vec![0; content_length];
    input.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

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
        stdin
            .write_all(
                &[
                    frame(&json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": "initialize",
                        "params": {
                            "processId": null,
                            "rootUri": null,
                            "capabilities": {},
                        },
                    })),
                    frame(&json!({
                        "jsonrpc": "2.0",
                        "method": "initialized",
                        "params": {},
                    })),
                    frame(&json!({
                        "jsonrpc": "2.0",
                        "id": 2,
                        "method": "shutdown",
                        "params": null,
                    })),
                    frame(&json!({
                        "jsonrpc": "2.0",
                        "method": "exit",
                    })),
                ]
                .concat(),
            )
            .unwrap();
    }

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:#?}");
    assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));

    let responses = read_all_messages(&output.stdout);
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["id"], json!(1));
    assert_eq!(responses[0]["result"]["serverInfo"]["name"], json!("solar"));
    assert_eq!(responses[0]["result"]["capabilities"], json!({}));
    assert_eq!(responses[1], json!({"jsonrpc": "2.0", "id": 2, "result": null}));
}

fn frame(value: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(value).unwrap();
    let mut frame = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    frame.extend(body);
    frame
}

fn read_all_messages(bytes: &[u8]) -> Vec<Value> {
    let mut input = BufReader::new(bytes);
    let mut messages = Vec::new();
    while let Some(message) = read_message(&mut input).unwrap() {
        messages.push(message);
    }
    messages
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

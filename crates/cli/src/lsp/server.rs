use serde_json::{Value, json};
use std::io::{self, BufReader, Read, Write};

use super::transport;

pub(super) fn run(input: impl Read, output: impl Write) -> io::Result<()> {
    let mut input = BufReader::new(input);
    let mut output = output;

    while let Some(message) = transport::read_message(&mut input)? {
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            continue;
        };

        match method {
            "initialize" => respond(&mut output, request_id(&message), initialize_result())?,
            "shutdown" => respond(&mut output, request_id(&message), Value::Null)?,
            "exit" => break,
            _ if message.get("id").is_some() => {
                respond_error(&mut output, request_id(&message), -32601, "method not found")?;
            }
            _ => {}
        }
    }

    Ok(())
}

fn initialize_result() -> Value {
    json!({
        "capabilities": {},
        "serverInfo": {
            "name": "solar",
            "version": solar_config::version::short_version(),
        },
    })
}

fn respond(output: &mut impl Write, id: Value, result: Value) -> io::Result<()> {
    transport::write_message(
        output,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }),
    )
}

fn respond_error(output: &mut impl Write, id: Value, code: i64, message: &str) -> io::Result<()> {
    transport::write_message(
        output,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": code,
                "message": message,
            },
        }),
    )
}

fn request_id(message: &Value) -> Value {
    message.get("id").cloned().unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use std::io::BufReader;

    use super::*;

    #[test]
    fn lifecycle_handshake() {
        let input = [
            transport::frame(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "processId": null,
                    "rootUri": null,
                    "capabilities": {},
                },
            })),
            transport::frame(&json!({
                "jsonrpc": "2.0",
                "method": "initialized",
                "params": {},
            })),
            transport::frame(&json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "shutdown",
                "params": null,
            })),
            transport::frame(&json!({
                "jsonrpc": "2.0",
                "method": "exit",
            })),
        ]
        .concat();

        let mut output = Vec::new();
        run(input.as_slice(), &mut output).unwrap();

        let responses = read_all_messages(&output);
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
                            "version": solar_config::version::short_version(),
                        },
                    },
                }),
                json!({"jsonrpc": "2.0", "id": 2, "result": null}),
            ]
        );
    }

    #[test]
    fn unknown_request_gets_method_not_found() {
        let input = [
            transport::frame(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "solar/unknown",
            })),
            transport::frame(&json!({
                "jsonrpc": "2.0",
                "method": "exit",
            })),
        ]
        .concat();

        let mut output = Vec::new();
        run(input.as_slice(), &mut output).unwrap();

        let responses = read_all_messages(&output);
        assert_eq!(
            responses,
            [json!({
                "jsonrpc": "2.0",
                "id": 1,
                "error": {
                    "code": -32601,
                    "message": "method not found",
                },
            })]
        );
    }

    fn read_all_messages(bytes: &[u8]) -> Vec<Value> {
        let mut input = BufReader::new(bytes);
        std::iter::from_fn(|| transport::read_message(&mut input).unwrap()).collect()
    }
}

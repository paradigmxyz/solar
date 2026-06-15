use std::io::{self, BufReader, Read, Write};

use lsp_types::InitializeParams;
use serde_json::Value;

use super::{protocol, state::State, transport};

pub(super) fn run(input: impl Read, output: impl Write) -> io::Result<()> {
    let mut server = Server::new(input, output);
    server.run()
}

struct Server<R, W> {
    input: BufReader<R>,
    output: W,
    state: State,
}

impl<R: Read, W: Write> Server<R, W> {
    fn new(input: R, output: W) -> Self {
        Self { input: BufReader::new(input), output, state: State::new() }
    }

    fn run(&mut self) -> io::Result<()> {
        while let Some(message) = transport::read_message(&mut self.input)? {
            let Some(method) = message.get("method").and_then(Value::as_str) else {
                continue;
            };

            match method {
                "initialize" => {
                    let id = request_id(&message);
                    let Ok(params) = protocol::params::<InitializeParams>(&message) else {
                        self.respond_error(
                            id,
                            protocol::INVALID_PARAMS,
                            "invalid initialize params",
                        )?;
                        continue;
                    };

                    let result = self.state.initialize(params);
                    self.respond(id, result)?;
                }
                "shutdown" => self.respond(request_id(&message), Value::Null)?,
                "exit" => break,
                _ if message.get("id").is_some() => {
                    self.respond_error(
                        request_id(&message),
                        protocol::METHOD_NOT_FOUND,
                        "method not found",
                    )?;
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn respond<T: serde::Serialize>(&mut self, id: Value, result: T) -> io::Result<()> {
        transport::write_message(&mut self.output, &protocol::response(id, result)?)
    }

    fn respond_error(&mut self, id: Value, code: i64, message: &str) -> io::Result<()> {
        transport::write_message(&mut self.output, &protocol::error_response(id, code, message))
    }
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

    #[test]
    fn initialize_with_invalid_params_gets_invalid_params() {
        let input = [
            transport::frame(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": [],
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
                    "code": -32602,
                    "message": "invalid initialize params",
                },
            })]
        );
    }

    fn read_all_messages(bytes: &[u8]) -> Vec<Value> {
        let mut input = BufReader::new(bytes);
        std::iter::from_fn(|| transport::read_message(&mut input).unwrap()).collect()
    }
}

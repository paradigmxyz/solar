use serde_json::{Value, json};
use std::io::{self, BufReader, Read, Write};

use super::transport;

pub(super) fn run(input: impl Read, output: impl Write) -> io::Result<()> {
    Server::new(input, output).run()
}

struct Server<R, W> {
    input: BufReader<R>,
    output: W,
}

impl<R: Read, W: Write> Server<R, W> {
    fn new(input: R, output: W) -> Self {
        Self { input: BufReader::new(input), output }
    }

    fn run(&mut self) -> io::Result<()> {
        while let Some(message) = transport::read_message(&mut self.input)? {
            let Some(method) = message.get("method").and_then(Value::as_str) else {
                continue;
            };

            match method {
                "initialize" => self.initialize(request_id(&message))?,
                "shutdown" => self.respond(request_id(&message), Value::Null)?,
                "exit" => break,
                _ if message.get("id").is_some() => {
                    self.respond_error(request_id(&message), -32601, "method not found")?;
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn initialize(&mut self, id: Value) -> io::Result<()> {
        self.respond(
            id,
            json!({
                "capabilities": {},
                "serverInfo": {
                    "name": "solar",
                    "version": solar_config::version::short_version(),
                },
            }),
        )
    }

    fn respond(&mut self, id: Value, result: Value) -> io::Result<()> {
        transport::write_message(
            &mut self.output,
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result,
            }),
        )
    }

    fn respond_error(&mut self, id: Value, code: i64, message: &str) -> io::Result<()> {
        transport::write_message(
            &mut self.output,
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
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0]["id"], json!(1));
        assert_eq!(responses[0]["result"]["serverInfo"]["name"], json!("solar"));
        assert_eq!(responses[0]["result"]["capabilities"], json!({}));
        assert_eq!(responses[1], json!({"jsonrpc": "2.0", "id": 2, "result": null}));
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
        let mut messages = Vec::new();
        let mut input = BufReader::new(bytes);
        while let Some(message) = transport::read_message(&mut input).unwrap() {
            messages.push(message);
        }
        messages
    }
}

use serde_json::Value;
use std::io::{self, BufRead, Write};

pub(super) fn read_message(input: &mut impl BufRead) -> io::Result<Option<Value>> {
    let Some(content_length) = read_content_length(input)? else {
        return Ok(None);
    };

    let mut body = vec![0; content_length];
    input.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("invalid LSP JSON: {e}")))
}

pub(super) fn write_message(output: &mut impl Write, value: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(value)?;
    write!(output, "Content-Length: {}\r\n\r\n", body.len())?;
    output.write_all(&body)?;
    output.flush()
}

fn read_content_length(input: &mut impl BufRead) -> io::Result<Option<usize>> {
    let mut content_length = None;
    let mut line = String::new();

    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }

        let header = line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            break;
        }

        let Some((name, value)) = header.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            content_length = Some(value.trim().parse::<usize>().map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid LSP Content-Length header: {e}"),
                )
            })?);
        }
    }

    let Some(content_length) = content_length else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "missing LSP Content-Length header",
        ));
    };

    Ok(Some(content_length))
}

#[cfg(test)]
pub(super) fn frame(value: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(value).unwrap();
    let mut frame = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    frame.extend(body);
    frame
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use std::io::BufReader;

    use super::*;

    #[test]
    fn reads_framed_message() {
        let message = json!({"jsonrpc": "2.0", "method": "exit"});
        let bytes = frame(&message);
        let mut input = BufReader::new(bytes.as_slice());

        assert_eq!(read_message(&mut input).unwrap(), Some(message));
        assert_eq!(read_message(&mut input).unwrap(), None);
    }

    #[test]
    fn writes_framed_message() {
        let message = json!({"jsonrpc": "2.0", "result": null});
        let mut output = Vec::new();

        write_message(&mut output, &message).unwrap();

        let mut input = BufReader::new(output.as_slice());
        assert_eq!(read_message(&mut input).unwrap(), Some(message));
    }

    #[test]
    fn requires_content_length() {
        let mut input = BufReader::new(b"X-Test: 1\r\n\r\n{}".as_slice());

        let error = read_message(&mut input).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("missing LSP Content-Length header"));
    }
}

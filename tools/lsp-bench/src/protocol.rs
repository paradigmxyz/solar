use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::io::{BufRead, Write};

pub(crate) fn write_message(mut writer: impl Write, message: &Value) -> Result<()> {
    let body = serde_json::to_vec(message).context("failed to serialize LSP message")?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn read_message(mut reader: impl BufRead) -> Result<Option<Value>> {
    read_message_limited(&mut reader, usize::MAX)
}

pub(crate) fn read_message_limited(
    mut reader: impl BufRead,
    max_body_bytes: usize,
) -> Result<Option<Value>> {
    let mut content_length = None;
    let mut line = String::new();

    loop {
        line.clear();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            return if content_length.is_none() {
                Ok(None)
            } else {
                Err(anyhow::anyhow!("unexpected EOF in LSP headers"))
            };
        }
        if line == "\r\n" || line == "\n" {
            break;
        }

        let Some((name, value)) = line.split_once(':') else {
            bail!("malformed LSP header: {line:?}")
        };
        if name.eq_ignore_ascii_case("Content-Length") {
            content_length = Some(value.trim().parse::<usize>().context("invalid Content-Length")?);
        }
    }

    let Some(content_length) = content_length else {
        bail!("LSP message is missing Content-Length")
    };
    if content_length > max_body_bytes {
        bail!("LSP message body is too large: {content_length} bytes (limit {max_body_bytes})")
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body).context("failed to decode LSP message").map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    #[test]
    fn lsp_frames_round_trip() {
        let message = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"});
        let mut bytes = Vec::new();
        write_message(&mut bytes, &message).unwrap();

        let decoded = read_message(&mut Cursor::new(bytes)).unwrap().unwrap();

        assert_eq!(decoded, message);
    }
}

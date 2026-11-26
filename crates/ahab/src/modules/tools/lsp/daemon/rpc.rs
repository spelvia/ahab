//! JSON-RPC over a language server's standard streams: messages framed
//! with `Content-Length` headers (LSP's base protocol).

use serde_json::Value;
use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

/// The largest message read; a server sending more is broken.
const MAX_MESSAGE: usize = 64 << 20;

/// `value` framed for the wire.
pub fn frame(value: &Value) -> Vec<u8> {
    let body = value.to_string();
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(body.as_bytes());
    out
}

/// Writes one message.
pub async fn write<W: AsyncWrite + Unpin>(writer: &mut W, value: &Value) -> std::io::Result<()> {
    writer.write_all(&frame(value)).await?;
    writer.flush().await
}

/// Reads one message; `None` at the end of the stream.
pub async fn read<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
) -> std::io::Result<Option<Value>> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            if length.is_some() {
                break;
            }
            continue;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let Some(length) = length.filter(|&length| length <= MAX_MESSAGE) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "a message without a usable Content-Length",
        ));
    };
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

/// What a message is.
pub enum Message {
    /// An answer to one of ours.
    Response {
        id: i64,
        result: Result<Value, Value>,
    },
    /// The server asks something of us.
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
}

/// Sorts a message read from the server.
pub fn classify(mut value: Value) -> Option<Message> {
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let params = value.get_mut("params").map_or(Value::Null, Value::take);
    match (method, value.get("id").cloned()) {
        (Some(method), Some(id)) => Some(Message::Request { id, method, params }),
        (Some(method), None) => Some(Message::Notification { method, params }),
        (None, Some(id)) => {
            let id = id.as_i64()?;
            let result = match value.get_mut("error") {
                Some(error) if !error.is_null() => Err(error.take()),
                _ => Ok(value.get_mut("result").map_or(Value::Null, Value::take)),
            };
            Some(Message::Response { id, result })
        }
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn messages_survive_framing() {
        let first = json!({ "jsonrpc": "2.0", "id": 1, "result": { "é": "ü" } });
        let second = json!({ "jsonrpc": "2.0", "method": "window/logMessage", "params": {} });
        let mut bytes = frame(&first);
        bytes.extend(b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\n");
        bytes.extend(frame(&second));
        let mut reader = tokio::io::BufReader::new(&bytes[..]);
        assert_eq!(read(&mut reader).await.unwrap(), Some(first));
        assert_eq!(read(&mut reader).await.unwrap(), Some(second));
        assert_eq!(read(&mut reader).await.unwrap(), None);
    }

    #[test]
    fn messages_are_told_apart() {
        assert!(matches!(
            classify(json!({ "id": 3, "error": { "code": -32601 } })),
            Some(Message::Response {
                id: 3,
                result: Err(_)
            })
        ));
        assert!(matches!(
            classify(json!({ "id": "a", "method": "workspace/configuration", "params": {} })),
            Some(Message::Request { .. })
        ));
        assert!(matches!(
            classify(json!({ "method": "textDocument/publishDiagnostics", "params": {} })),
            Some(Message::Notification { .. })
        ));
    }
}

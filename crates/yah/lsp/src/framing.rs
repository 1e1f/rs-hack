//! @arch:layer(lsp)
//! @arch:role(framing)
//! @arch:thread(async_io)
//!
//! LSP Content-Length framing.
//!
//! The Language Server Protocol wraps each JSON-RPC message in HTTP-style
//! headers terminated by a blank line, then a fixed-length JSON body:
//!
//! ```text
//! Content-Length: 142\r\n
//! \r\n
//! {"jsonrpc":"2.0","id":1,...}
//! ```
//!
//! `yah serve --stdio` speaks line-delimited JSON on its own wire (one
//! frame per line — see `.yah/arch/authored/yah-files-tab.md` "LSP
//! multiplexing"); this module bridges that line-shape to the
//! Content-Length shape that real language servers expect.

use std::io;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Hard cap on a single LSP message body. Protects against runaway
/// `Content-Length` values from a misbehaving child. rust-analyzer's
/// largest realistic frame (workspace symbol responses on a huge tree)
/// is comfortably under 16MB; we double that for headroom.
pub const MAX_FRAME_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum FramingError {
    #[error("eof reading frame")]
    Eof,
    #[error("malformed header line: {0}")]
    BadHeader(String),
    #[error("missing Content-Length header")]
    MissingLength,
    #[error("Content-Length {0} exceeds max ({1})")]
    FrameTooLarge(usize, usize),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

/// Read one LSP-framed message into a `Vec<u8>` (the JSON body).
///
/// Reads headers line-by-line until a blank line separator, then exactly
/// `Content-Length` bytes. Headers other than `Content-Length` and
/// `Content-Type` are tolerated (some servers emit `X-Trace-Id` etc.).
///
/// Returns `Ok(None)` only on a clean EOF *before any header byte was
/// read* — that's the normal way the channel closes when the server
/// exits cleanly. EOF mid-frame is an error.
pub async fn read_message<R>(mut reader: R) -> Result<Option<Vec<u8>>, FramingError>
where
    R: AsyncRead + Unpin,
{
    let mut content_length: Option<usize> = None;
    let mut header_started = false;
    loop {
        let line = match read_header_line(&mut reader, header_started).await? {
            Some(line) => line,
            None => return Ok(None), // clean EOF before headers
        };
        header_started = true;
        if line.is_empty() {
            break;
        }
        // RFC 7230-style header. We're permissive about whitespace and
        // case — lsp servers vary in how strictly they format headers.
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| FramingError::BadHeader(line.clone()))?;
        if name.eq_ignore_ascii_case("content-length") {
            let n: usize = value
                .trim()
                .parse()
                .map_err(|_| FramingError::BadHeader(line.clone()))?;
            content_length = Some(n);
        }
        // Other headers (Content-Type, etc.) are silently accepted.
    }

    let len = content_length.ok_or(FramingError::MissingLength)?;
    if len > MAX_FRAME_BYTES {
        return Err(FramingError::FrameTooLarge(len, MAX_FRAME_BYTES));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).await?;
    Ok(Some(body))
}

/// Read one CRLF-terminated header line. `\r\n` is the spec-mandated
/// terminator but real-world servers occasionally emit bare `\n`; accept
/// both. Returns `Ok(None)` only on EOF *before any byte was read* and
/// only when `header_started` is false (so a partial frame surfaces as
/// `Eof`).
async fn read_header_line<R>(
    reader: &mut R,
    header_started: bool,
) -> Result<Option<String>, FramingError>
where
    R: AsyncRead + Unpin,
{
    let mut buf = Vec::with_capacity(64);
    loop {
        let mut byte = [0u8; 1];
        match reader.read_exact(&mut byte).await {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                if buf.is_empty() && !header_started {
                    return Ok(None);
                }
                return Err(FramingError::Eof);
            }
            Err(e) => return Err(e.into()),
        }
        match byte[0] {
            b'\n' => {
                // Strip trailing `\r` if present (the spec form).
                if buf.last() == Some(&b'\r') {
                    buf.pop();
                }
                let s = String::from_utf8(buf)
                    .map_err(|e| FramingError::BadHeader(format!("non-utf8 header: {e}")))?;
                return Ok(Some(s));
            }
            b => buf.push(b),
        }
        if buf.len() > 8 * 1024 {
            return Err(FramingError::BadHeader(
                "header line exceeded 8KiB".to_string(),
            ));
        }
    }
}

/// Write one LSP frame: `Content-Length: N\r\n\r\n<body>`.
///
/// The body is written exactly as given — callers pass a serialized
/// JSON-RPC envelope. The writer is **not** flushed; flush is the
/// caller's responsibility (so multiple writes can batch).
pub async fn write_message<W>(mut writer: W, body: &[u8]) -> Result<(), FramingError>
where
    W: AsyncWrite + Unpin,
{
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(body).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{duplex, AsyncWriteExt};

    #[tokio::test]
    async fn read_then_write_round_trip() {
        let (a, b) = duplex(64 * 1024);
        let (a_r, mut a_w) = tokio::io::split(a);
        let (b_r, b_w) = tokio::io::split(b);

        // a → b
        let body = br#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#;
        write_message(&mut a_w, body).await.unwrap();
        a_w.flush().await.unwrap();
        drop(a_w);

        let received = read_message(b_r).await.unwrap().unwrap();
        assert_eq!(received, body);

        drop(a_r);
        drop(b_w);
    }

    #[tokio::test]
    async fn read_returns_none_on_clean_eof_before_headers() {
        let (a, b) = duplex(64);
        drop(a); // close immediately
        let result = read_message(b).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn read_tolerates_extra_headers() {
        let (mut a, b) = duplex(64 * 1024);
        let payload = b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\n\
                        Content-Length: 18\r\n\
                        \r\n\
                        {\"hello\":\"world\"}\n";
        a.write_all(payload).await.unwrap();
        a.flush().await.unwrap();
        drop(a);
        let body = read_message(b).await.unwrap().unwrap();
        assert_eq!(body, b"{\"hello\":\"world\"}\n");
    }

    #[tokio::test]
    async fn read_tolerates_bare_lf_terminators() {
        // No \r — just \n. Some servers (and most of our hand-written
        // tests) produce this shape.
        let (mut a, b) = duplex(64 * 1024);
        let payload = b"Content-Length: 2\n\n{}";
        a.write_all(payload).await.unwrap();
        a.flush().await.unwrap();
        drop(a);
        let body = read_message(b).await.unwrap().unwrap();
        assert_eq!(body, b"{}");
    }

    #[tokio::test]
    async fn read_partial_header_is_eof_error() {
        let (mut a, b) = duplex(64);
        a.write_all(b"Content-Length: 5\r\n").await.unwrap();
        // No blank line, no body — just close.
        drop(a);
        let err = read_message(b).await.unwrap_err();
        assert!(matches!(err, FramingError::Eof));
    }

    #[tokio::test]
    async fn missing_content_length_errors() {
        let (mut a, b) = duplex(64);
        a.write_all(b"X-Other: 1\r\n\r\n{}").await.unwrap();
        drop(a);
        let err = read_message(b).await.unwrap_err();
        assert!(matches!(err, FramingError::MissingLength));
    }

    #[tokio::test]
    async fn frame_too_large_is_rejected() {
        let (mut a, b) = duplex(64);
        let bogus = format!(
            "Content-Length: {}\r\n\r\n",
            super::MAX_FRAME_BYTES + 1
        );
        a.write_all(bogus.as_bytes()).await.unwrap();
        drop(a);
        let err = read_message(b).await.unwrap_err();
        assert!(matches!(err, FramingError::FrameTooLarge(_, _)));
    }
}

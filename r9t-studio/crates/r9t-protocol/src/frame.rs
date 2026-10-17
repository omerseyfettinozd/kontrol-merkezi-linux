//! NDJSON framing with a hard per-line limit. Runtime-agnostic: callers feed bytes.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FrameError {
    #[error("line exceeds {limit} bytes")]
    TooLarge { limit: usize },
}

/// Accumulates bytes and yields complete lines (without `\n`, trailing `\r` trimmed).
///
/// After a `TooLarge` error the decoder discards input until the next newline, so a
/// server may answer with an error and keep the connection, or simply close it.
#[derive(Debug)]
pub struct LineDecoder {
    limit: usize,
    buf: Vec<u8>,
    discarding: bool,
}

impl LineDecoder {
    pub fn new(limit: usize) -> Self {
        Self { limit, buf: Vec::new(), discarding: false }
    }

    /// Appends `data` and returns every complete line or error found, in order.
    pub fn push(&mut self, data: &[u8]) -> Vec<Result<Vec<u8>, FrameError>> {
        let mut out = Vec::new();
        for chunk in data.split_inclusive(|&b| b == b'\n') {
            let complete = chunk.last() == Some(&b'\n');
            let body = if complete { &chunk[..chunk.len() - 1] } else { chunk };
            if self.discarding {
                if complete {
                    self.discarding = false;
                }
                continue;
            }
            if self.buf.len() + body.len() > self.limit {
                self.buf.clear();
                out.push(Err(FrameError::TooLarge { limit: self.limit }));
                self.discarding = !complete;
                continue;
            }
            self.buf.extend_from_slice(body);
            if complete {
                let mut line = std::mem::take(&mut self.buf);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                out.push(Ok(line));
            }
        }
        out
    }

    /// Bytes buffered for an incomplete line.
    pub fn pending(&self) -> usize {
        self.buf.len()
    }
}

/// Serializes `value` as one NDJSON line, refusing to exceed `limit`.
pub fn encode_line<T: serde::Serialize>(value: &T, limit: usize) -> Result<Vec<u8>, EncodeError> {
    let mut line = serde_json::to_vec(value)?;
    if line.len() > limit {
        return Err(EncodeError::TooLarge { size: line.len(), limit });
    }
    line.push(b'\n');
    Ok(line)
}

#[derive(Debug, Error)]
pub enum EncodeError {
    #[error("serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("message of {size} bytes exceeds {limit}")]
    TooLarge { size: usize, limit: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_lines_across_chunks() {
        let mut d = LineDecoder::new(16);
        assert!(d.push(b"{\"a\"").is_empty());
        let out = d.push(b":1}\r\n{}\n{");
        assert_eq!(out, vec![Ok(b"{\"a\":1}".to_vec()), Ok(b"{}".to_vec())]);
        assert_eq!(d.pending(), 1);
    }

    #[test]
    fn oversize_line_is_rejected_and_skipped() {
        let mut d = LineDecoder::new(4);
        let out = d.push(b"123456");
        assert_eq!(out, vec![Err(FrameError::TooLarge { limit: 4 })]);
        // Rest of the oversize line is discarded; the next line is intact.
        let out = d.push(b"78\nok\n");
        assert_eq!(out, vec![Ok(b"ok".to_vec())]);
    }

    #[test]
    fn exact_limit_is_accepted() {
        let mut d = LineDecoder::new(4);
        assert_eq!(d.push(b"1234\n"), vec![Ok(b"1234".to_vec())]);
    }

    #[test]
    fn encode_refuses_oversize() {
        assert!(encode_line(&"x".repeat(10), 5).is_err());
        assert_eq!(encode_line(&1, 5).unwrap(), b"1\n");
    }
}

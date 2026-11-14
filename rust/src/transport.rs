/// Where a stream comes from: the API, or `ASK_CLAUDE_REPLAY` (a file,
/// or a directory whose `1.sse, 2.sse, ...` are the turns in order).
/// `ASK_CLAUDE_REQUEST_LOG` appends each body that would have been sent.
pub trait Transport {
    /// Send a body; the reply body as a reader, read as it arrives (KTD3),
    /// so one loop prints each `data:` line and keeps it for the fold.
    fn send(&mut self, body: &str) -> io::Result<Reply>;
}

/// A reply body: the SSE of a stream, or an error body, as raw bytes.
pub type Reply = Box<dyn Read + Send>;

use std::io::{self, Read};
use std::path::PathBuf;

/// POST /v1/messages. An error status is not an `Err`: its body (the
/// API's `{"type":"error",..}`) comes back for the caller to report.
pub struct Http {
    base_url: String,
    api_key: String,
    betas: String,
}

impl Http {
    pub fn new(base_url: String, api_key: String, betas: String) -> Self {
        Self {
            base_url,
            api_key,
            betas,
        }
    }
}

impl Transport for Http {
    fn send(&mut self, body: &str) -> io::Result<Reply> {
        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
        let response = ureq::post(&url)
            .config()
            .http_status_as_error(false)
            .build()
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2026-10-21")
            .header("anthropic-beta", &self.betas)
            .header("content-type", "application/json")
            .send(body)
            .map_err(io::Error::other)?;
        Ok(Box::new(response.into_body().into_reader()))
    }
}

/// `ASK_CLAUDE_REPLAY`: a recorded stream instead of the API.
pub struct Replay {
    path: PathBuf,
    turn: usize,
}

impl Replay {
    pub fn new(path: PathBuf) -> Self {
        Self { path, turn: 0 }
    }
}

impl Transport for Replay {
    fn send(&mut self, body: &str) -> io::Result<Reply> {
        if let Some(log) = std::env::var_os("ASK_CLAUDE_REQUEST_LOG") {
            use io::Write;
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log)?;
            writeln!(f, "{body}")?;
        }
        self.turn += 1;
        let file = if self.path.is_dir() {
            std::fs::File::open(self.path.join(format!("{}.sse", self.turn)))?
        } else {
            std::fs::File::open(&self.path)?
        };
        Ok(Box::new(file))
    }
}

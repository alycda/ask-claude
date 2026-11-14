/// Where a stream comes from: the API, or `ASK_CLAUDE_REPLAY` (a file,
/// or a directory whose `1.sse, 2.sse, ...` are the turns in order).
/// `ASK_CLAUDE_REQUEST_LOG` appends each body that would have been sent.
pub trait Transport {
    /// Send a body, get the raw SSE of the reply.
    fn send(&mut self, body: &str) -> std::io::Result<String>;
}

use std::io;
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
    fn send(&mut self, body: &str) -> io::Result<String> {
        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
        ureq::post(&url)
            .config()
            .http_status_as_error(false)
            .build()
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2026-10-21")
            .header("anthropic-beta", &self.betas)
            .header("content-type", "application/json")
            .send(body)
            .map_err(io::Error::other)?
            .body_mut()
            .read_to_string()
            .map_err(io::Error::other)
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
    fn send(&mut self, body: &str) -> io::Result<String> {
        if let Some(log) = std::env::var_os("ASK_CLAUDE_REQUEST_LOG") {
            use io::Write;
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log)?;
            writeln!(f, "{body}")?;
        }
        self.turn += 1;
        if self.path.is_dir() {
            std::fs::read_to_string(self.path.join(format!("{}.sse", self.turn)))
        } else {
            std::fs::read_to_string(&self.path)
        }
    }
}

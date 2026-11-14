/// Where a stream comes from: the API ([`Http`]), or `ASK_CLAUDE_REPLAY`
/// ([`Replay`]: a file, or a directory whose `1.sse, 2.sse, ...` are the
/// sends in order), with [`Logged`] around either for the test logs. The
/// twin of `transport` in `lib/messages.bash`.
pub trait Transport {
    /// Send a body with its `anthropic-beta` value; the reply body as a
    /// reader, read as it arrives (KTD3), so one loop prints each `data:`
    /// line and keeps it for the fold. An `Err`, or a reply with no bytes,
    /// is the bash "no response" path.
    fn send(&mut self, body: &str, betas: &str) -> io::Result<Reply>;
}

/// A reply body: the SSE of a stream, or an error body, as raw bytes.
pub type Reply = Box<dyn Read + Send>;

use std::fs::{File, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// POST /v1/messages with the headers `headers` in `lib/common.bash`
/// sends. The agent (KTD3): an error status is not an `Err`, its body (the
/// API's `{"type":"error",..}`) comes back for the caller to report as the
/// bash `API error`; no redirect is followed, so the key never goes where
/// one points, as curl without `-L`; 1800 s for the whole request, as
/// `curl --max-time 1800`, and 30 s to connect.
pub struct Http {
    agent: ureq::Agent,
    url: String,
    api_key: String,
}

impl Http {
    /// `base_url` is the checked `--base-url`; one trailing slash is
    /// dropped, as `${BASE_URL%/}` drops it.
    pub fn new(base_url: &str, api_key: String) -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(1800)))
            .timeout_connect(Some(Duration::from_secs(30)))
            .build();
        let root = base_url.strip_suffix('/').unwrap_or(base_url);
        Self {
            agent: ureq::Agent::new_with_config(config),
            url: format!("{root}/v1/messages"),
            api_key,
        }
    }
}

impl Transport for Http {
    fn send(&mut self, body: &str, betas: &str) -> io::Result<Reply> {
        let response = self
            .agent
            .post(&self.url)
            .header("content-type", "application/json")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2026-10-21")
            .header("anthropic-beta", betas)
            .send(body)
            .map_err(io::Error::other)?;
        Ok(Box::new(response.into_body().into_reader()))
    }
}

/// `ASK_CLAUDE_REPLAY`: a recorded stream instead of the API. A file
/// answers every send; a directory answers send N with `N.sse`, the
/// counter advancing on every send, continuations included, and a missing
/// turn file is an `Err`, the "no response" path.
pub struct Replay {
    path: PathBuf,
    sent: usize,
}

impl Replay {
    pub fn new(path: PathBuf) -> Self {
        Self { path, sent: 0 }
    }
}

impl Transport for Replay {
    fn send(&mut self, _body: &str, _betas: &str) -> io::Result<Reply> {
        self.sent += 1;
        let file = if self.path.is_dir() {
            self.path.join(format!("{}.sse", self.sent))
        } else {
            self.path.clone()
        };
        match File::open(&file) {
            Ok(f) => Ok(Box::new(f)),
            Err(e) => Err(io::Error::new(e.kind(), format!("{}: {e}", file.display()))),
        }
    }
}

/// The test logs around another transport, written before it sends:
/// `ASK_CLAUDE_BETA_LOG` gets each request's `anthropic-beta` value (a
/// replay never shows the header), `ASK_CLAUDE_REQUEST_LOG` each body,
/// one line each. A request log that cannot be written fails the send
/// with nothing sent, as in bash; one another account owns or can read is
/// refused, since it holds request text at rest, as private as a chat
/// (KTD3; the script has no such check).
pub struct Logged {
    inner: Box<dyn Transport + Send>,
    requests: Option<PathBuf>,
    betas: Option<PathBuf>,
}

impl Logged {
    /// `Err` is the message for a request log that exists and is not
    /// private, in the shape of the state directory's.
    pub fn new(
        inner: Box<dyn Transport + Send>,
        requests: Option<PathBuf>,
        betas: Option<PathBuf>,
    ) -> Result<Self, String> {
        if let Some(log) = &requests
            && let Ok(meta) = std::fs::metadata(log)
        {
            private(log, &meta)?;
        }
        Ok(Self {
            inner,
            requests,
            betas,
        })
    }
}

impl Transport for Logged {
    fn send(&mut self, body: &str, betas: &str) -> io::Result<Reply> {
        if let Some(log) = &self.betas
            && open_log(log)
                .and_then(|mut f| writeln!(f, "{betas}"))
                .is_err()
        {
            // bash reports the failed redirect and sends anyway
            eprintln!(
                "ask-claude: the beta log {} cannot be written",
                log.display()
            );
        }
        if let Some(log) = &self.requests {
            let cannot = |_| {
                io::Error::other(format!(
                    "the request log {} cannot be written; nothing sent",
                    log.display()
                ))
            };
            let mut f = open_log(log).map_err(cannot)?;
            // the check at start-up says nothing about a file put in its
            // place since: the open file is the one that must be private
            private(log, &f.metadata().map_err(cannot)?).map_err(io::Error::other)?;
            writeln!(f, "{body}").map_err(cannot)?;
        }
        self.inner.send(body, betas)
    }
}

/// A log opened for appending, created 0600 when missing.
fn open_log(log: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    opts.open(log)
}

/// A log file this account owns and no other can read, or the refusal.
#[cfg(unix)]
fn private(log: &Path, meta: &Metadata) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    // SAFETY: geteuid(2) takes nothing and cannot fail
    let me = unsafe { libc::geteuid() };
    if meta.uid() != me {
        return Err(format!(
            "{} is owned by uid {}, not you; keep the request log in a file you own",
            log.display(),
            meta.uid()
        ));
    }
    if meta.mode() & 0o044 != 0 {
        return Err(format!(
            "{} is readable by other accounts; keep the request log private",
            log.display()
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn private(_log: &Path, _meta: &Metadata) -> Result<(), String> {
    Ok(())
}

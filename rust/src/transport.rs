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

/// The Managed Agents transport.
pub mod sessions;

use std::fs::{File, Metadata, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

/// POST /v1/messages with the headers `headers` in `lib/common.bash`
/// sends, or for `-p openai` POST /chat/completions with the key as a
/// Bearer token and no Anthropic header, as
/// `tests/providers/openai-compatible.hurl` sends it. The agent (KTD3): an
/// error status is not an `Err`, its body (the API's `{"type":"error",..}`,
/// or OpenAI's `{"error":{..}}`) comes back for the caller to report as the
/// bash `API error`; no redirect is followed, so the key never goes where
/// one points, as curl without `-L`; 1800 s for the whole request, as
/// `curl --max-time 1800`, and 30 s to connect.
pub struct Http {
    agent: ureq::Agent,
    url: String,
    api_key: String,
    bearer: bool,
}

impl Http {
    /// To the provider's checked root, one trailing slash dropped, as
    /// `${BASE_URL%/}` drops it.
    pub fn new(provider: &crate::config::Provider, api_key: String) -> Self {
        use crate::config::Provider;
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(1800)))
            .timeout_connect(Some(Duration::from_secs(30)))
            .build();
        let base_url = provider.base_url();
        let root = base_url.strip_suffix('/').unwrap_or(base_url);
        let (path, bearer) = match provider {
            Provider::Anthropic { .. } => ("v1/messages", false),
            Provider::OpenAiCompatible { .. } => ("chat/completions", true),
        };
        Self {
            agent: ureq::Agent::new_with_config(config),
            url: format!("{root}/{path}"),
            api_key,
            bearer,
        }
    }
}

impl Transport for Http {
    /// The request starts on the reply's first read, so it runs on the
    /// thread that reads the reply ([`lines`]) and an interrupt is seen
    /// while it connects and waits for the headers too, not only while the
    /// body streams.
    fn send(&mut self, body: &str, betas: &str) -> io::Result<Reply> {
        let agent = self.agent.clone();
        let (url, key) = (self.url.clone(), self.api_key.clone());
        let (body, betas) = (body.to_string(), betas.to_string());
        let bearer = self.bearer;
        Ok(Box::new(Deferred::Pending(Box::new(move || {
            let request = agent.post(&url).header("content-type", "application/json");
            let request = if bearer {
                request.header("authorization", &format!("Bearer {key}"))
            } else {
                request
                    .header("x-api-key", &key)
                    .header("anthropic-version", "2026-10-21")
                    .header("anthropic-beta", &betas)
            };
            let response = request.send(&body).map_err(io::Error::other)?;
            Ok(Box::new(response.into_body().into_reader()) as Reply)
        }))))
    }
}

/// A reply whose request has not started: the first read starts it.
enum Deferred {
    Pending(Box<dyn FnOnce() -> io::Result<Reply> + Send>),
    Started(Reply),
    Done,
}

impl Read for Deferred {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Deferred::Pending(_) = self
            && let Deferred::Pending(start) = std::mem::replace(self, Deferred::Done)
        {
            *self = Deferred::Started(start()?);
        }
        match self {
            Deferred::Started(reply) => reply.read(buf),
            _ => Ok(0),
        }
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
        refuse_shared(requests.as_deref())?;
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
            log_request(log, body)?;
        }
        self.inner.send(body, betas)
    }
}

/// `Err` is the refusal of a request log that exists and is not private,
/// in the shape of the state directory's.
fn refuse_shared(requests: Option<&Path>) -> Result<(), String> {
    if let Some(log) = requests
        && let Ok(meta) = std::fs::metadata(log)
    {
        private(log, &meta)?;
    }
    Ok(())
}

/// Append `body` to the request log as one line, before it is sent; an
/// `Err` means nothing may be sent.
fn log_request(log: &Path, body: &str) -> io::Result<()> {
    let cannot = |_| {
        io::Error::other(format!(
            "the request log {} cannot be written; nothing sent",
            log.display()
        ))
    };
    let mut f = open_log(log).map_err(cannot)?;
    // the check at start-up says nothing about a file put in its place
    // since: the open file is the one that must be private
    private(log, &f.metadata().map_err(cannot)?).map_err(io::Error::other)?;
    writeln!(f, "{body}").map_err(cannot)
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

/// Set by the signal handler (KTD11): SIGINT, SIGTERM and SIGHUP do
/// nothing else, so no work stops halfway inside a handler; the main
/// thread looks at the flag between waits for the next line and exits 130.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn on_signal(_: libc::c_int) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

/// Install the handler for SIGINT, SIGTERM and SIGHUP. Reads restart after
/// it runs (`SA_RESTART`), so the worker's blocking read is not cut short:
/// the main thread decides what an interrupt means.
#[cfg(unix)]
pub fn catch_signals() {
    // SAFETY: the handler only stores to an atomic, which is
    // async-signal-safe; the sigaction struct is zeroed then filled.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
        action.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut action.sa_mask);
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            libc::sigaction(signal, &action, std::ptr::null_mut());
        }
    }
}

#[cfg(not(unix))]
pub fn catch_signals() {}

/// Whether a signal asked this process to stop.
pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

/// What [`Lines::next`] waited for.
#[derive(Debug)]
pub enum Next {
    /// one line of the body, its newline included (the last may lack one)
    Line(Vec<u8>),
    /// the read failed; what came before it stands
    Failed(io::Error),
    /// the body ended
    End,
    /// a signal set the flag while waiting
    Interrupted,
}

/// The lines of a reply as a worker thread reads them (KTD3). A signal
/// cannot wake a blocking socket read on every platform, so the read
/// happens off the main thread, which waits on the channel a tick at a
/// time and looks at the interrupt flag between ticks. On an interrupt the
/// worker is abandoned to process exit.
pub struct Lines {
    rx: mpsc::Receiver<io::Result<Vec<u8>>>,
}

/// How long the main thread waits for a line before it looks at the flag.
const TICK: Duration = Duration::from_millis(50);

/// Start reading `reply` on a worker thread.
pub fn lines(reply: Reply) -> Lines {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(reply);
        loop {
            let mut line = Vec::new();
            let item = match reader.read_until(b'\n', &mut line) {
                Ok(0) => break,
                Ok(_) => Ok(line),
                Err(e) => Err(e),
            };
            let failed = item.is_err();
            if tx.send(item).is_err() || failed {
                break;
            }
        }
    });
    Lines { rx }
}

impl Lines {
    /// The next line, the end, a failed read, or an interrupt.
    pub fn next(&self) -> Next {
        loop {
            if let Some(next) = self.tick() {
                return next;
            }
        }
    }

    /// [`Lines::next`], or None once `deadline` has passed with nothing to
    /// hand over: a session stream waited on for its connected comment.
    pub fn next_until(&self, deadline: std::time::Instant) -> Option<Next> {
        while std::time::Instant::now() < deadline {
            if let Some(next) = self.tick() {
                return Some(next);
            }
        }
        None
    }

    /// One wait of at most a tick, after a look at the flag.
    fn tick(&self) -> Option<Next> {
        if interrupted() {
            return Some(Next::Interrupted);
        }
        match self.rx.recv_timeout(TICK) {
            Ok(Ok(line)) => Some(Next::Line(line)),
            Ok(Err(e)) => Some(Next::Failed(e)),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => Some(Next::End),
        }
    }
}

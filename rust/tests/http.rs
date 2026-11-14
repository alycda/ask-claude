//! The Http transport (R5), end to end against a localhost mock: what the
//! replay seam cannot see. Replay hands the binary a recording and never
//! speaks HTTP, so the headers, the path, an error status, a redirect and
//! one request per pause_turn round are only checked here (and live, by
//! `just test`). The mock serves the recorded fixtures, never a stream made
//! up for the test, so the oracle rule holds: every byte it sends is one the
//! API sent, or the hand-derived error.sse the bats suite already uses.
//!
//! `ASK_CLAUDE_BIN=<path> cargo test --test http` runs them against another
//! binary, as the bats suite does; on the bash script they are green.
//!
//! The mock is std only: a listener thread that answers one request per
//! connection, in order, and keeps each request it read. Adapted from the
//! claude-rs prototype's tests/common/mod.rs.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use assert_cmd::prelude::*;
use serde_json::{Value, json};

const KEY: &str = "sk-ant-test-not-a-key";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(repo().join("tests/fixtures/sse").join(name)).expect("fixture")
}

/// One request as the mock read it; header names lower-cased.
#[derive(Debug, Clone)]
struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Value,
}

impl Request {
    fn header(&self, name: &str) -> &str {
        self.headers.get(name).map(String::as_str).unwrap_or("")
    }
}

/// One canned response.
struct Reply {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
    /// send `body[..at]`, then the rest once the receiver yields or its
    /// sender is dropped: a reply still streaming
    hold: Option<(usize, mpsc::Receiver<()>)>,
}

impl Reply {
    fn sse(name: &str) -> Reply {
        Reply {
            status: 200,
            headers: vec![("content-type", "text/event-stream".into())],
            body: fixture(name),
            hold: None,
        }
    }
    /// The fixture's first `at` bytes now, the rest when the returned
    /// sender sends or is dropped.
    fn held(name: &str, at: usize) -> (Reply, mpsc::Sender<()>) {
        let (release, wait) = mpsc::channel();
        let reply = Reply {
            hold: Some((at, wait)),
            ..Reply::sse(name)
        };
        (reply, release)
    }
    /// error.sse is a bare JSON error body, the one the API sends with a 404
    /// for an unknown model.
    fn error(status: u16, name: &str) -> Reply {
        Reply {
            status,
            headers: vec![("content-type", "application/json".into())],
            body: fixture(name),
            hold: None,
        }
    }
    fn redirect(to: &str) -> Reply {
        Reply {
            status: 307,
            headers: vec![("location", to.to_string())],
            body: Vec::new(),
            hold: None,
        }
    }
}

struct Mock {
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
}

impl Mock {
    /// Answer `replies.len()` requests, one per connection, in order. A
    /// request is recorded before its reply is written, so once the binary
    /// has exited every request it got an answer to is in `requests()`.
    fn start(replies: Vec<Reply>) -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&requests);
        thread::spawn(move || {
            for reply in replies {
                let Ok((stream, _)) = listener.accept() else {
                    return;
                };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                if let Some(req) = read_request(&stream) {
                    log.lock().unwrap().push(req);
                }
                write_reply(stream, &reply);
            }
        });
        Mock { url, requests }
    }

    fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }
}

/// Read one HTTP/1.1 request: a content-length body or a chunked one, since
/// the test must not care which the client picks.
fn read_request(stream: &TcpStream) -> Option<Request> {
    let mut r = BufReader::new(stream);
    let mut line = String::new();
    r.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let mut headers = HashMap::new();
    loop {
        line.clear();
        r.read_line(&mut line).ok()?;
        let l = line.trim_end();
        if l.is_empty() {
            break;
        }
        if let Some((k, v)) = l.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    let mut body = Vec::new();
    if headers
        .get("transfer-encoding")
        .is_some_and(|v| v.eq_ignore_ascii_case("chunked"))
    {
        loop {
            line.clear();
            r.read_line(&mut line).ok()?;
            let size = usize::from_str_radix(line.trim().split(';').next()?, 16).ok()?;
            let mut chunk = vec![0; size + 2]; // the chunk and its CRLF
            r.read_exact(&mut chunk).ok()?;
            if size == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..size]);
        }
    } else if let Some(n) = headers.get("content-length").and_then(|v| v.parse().ok()) {
        body.resize(n, 0);
        r.read_exact(&mut body).ok()?;
    }
    let body = serde_json::from_slice(&body).unwrap_or(Value::Null);
    Some(Request {
        method,
        path,
        headers,
        body,
    })
}

fn write_reply(mut stream: TcpStream, reply: &Reply) {
    let mut head = format!(
        "HTTP/1.1 {} X\r\ncontent-length: {}\r\nconnection: close\r\n",
        reply.status,
        reply.body.len()
    );
    for (k, v) in &reply.headers {
        head += &format!("{k}: {v}\r\n");
    }
    head += "\r\n";
    let _ = stream.write_all(head.as_bytes());
    match &reply.hold {
        // the first part, then nothing until the test lets the rest go
        Some((at, release)) => {
            let _ = stream.write_all(&reply.body[..*at]);
            let _ = stream.flush();
            let _ = release.recv();
            let _ = stream.write_all(&reply.body[*at..]);
        }
        None => {
            let _ = stream.write_all(&reply.body);
        }
    }
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

/// The binary against `base_url` with `args`, no replay, no request log,
/// state in `state`, no stdin.
fn command(base_url: &str, args: &[&str], state: &Path) -> Command {
    // ASK_CLAUDE_BIN runs the same scenarios against another binary, the bash
    // script above all: it is the oracle, so these tests pass on it first
    let mut cmd = match std::env::var_os("ASK_CLAUDE_BIN") {
        Some(bin) => Command::new(bin),
        None => Command::cargo_bin("ask-claude").expect("the binary builds"),
    };
    // Nothing the binary reads comes from the shell that runs the suite: a
    // developer's ASK_CLAUDE_REPLAY would bypass the mock, ASK_CLAUDE_BETA_LOG
    // would have every request append to a real file, and an
    // ASK_CLAUDE_MCP_TOKEN would be sent to the mock. Dropped by prefix,
    // so a knob added later is covered. HOME stays: under mise, `usage` on
    // PATH is a shim that finds its install through it.
    for (name, _) in std::env::vars_os() {
        let name_str = name.to_string_lossy();
        if ["ASK_CLAUDE_", "ANTHROPIC_", "OPENAI_"]
            .iter()
            .any(|prefix| name_str.starts_with(prefix))
            || name_str == "GITHUB_TOKEN"
        {
            cmd.env_remove(&name);
        }
    }
    cmd.arg("--base-url")
        .arg(base_url)
        .args(args)
        .env("ANTHROPIC_API_KEY", KEY)
        .env("XDG_STATE_HOME", state.join("state"))
        .stdin(Stdio::null());
    cmd
}

/// The one shot against `base_url`, state in a temp dir. Returns the output
/// and the state dir (kept alive by the caller).
fn one_shot(base_url: &str, args: &[&str]) -> (Output, tempfile::TempDir) {
    let state = tempfile::tempdir().unwrap();
    let out = command(base_url, args, state.path()).output().unwrap();
    (out, state)
}

/// The one shot started with its stdout and stderr piped, and a thread
/// that hands over everything stdout has printed so far each time more
/// arrives.
fn spawn(
    base_url: &str,
    args: &[&str],
    group: bool,
) -> (Child, mpsc::Receiver<Vec<u8>>, tempfile::TempDir) {
    let state = tempfile::tempdir().unwrap();
    let mut cmd = command(base_url, args, state.path());
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    if group {
        // its own process group, so a signal can go to all of it at once,
        // the way the terminal sends Ctrl-C to the foreground job
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    }
    let mut child = cmd.spawn().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut seen = Vec::new();
        let mut buf = [0; 256];
        while let Ok(n @ 1..) = stdout.read(&mut buf) {
            seen.extend_from_slice(&buf[..n]);
            if tx.send(seen.clone()).is_err() {
                break;
            }
        }
    });
    (child, rx, state)
}

/// Wait up to `limit` for stdout to have printed `text`.
fn printed(stdout: &mpsc::Receiver<Vec<u8>>, text: &str, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match stdout.recv_timeout(left) {
            Ok(seen) if String::from_utf8_lossy(&seen).contains(text) => return true,
            Ok(_) => {}
            Err(_) => return false,
        }
    }
    false
}

/// The offset in a fixture just after the event that carries `text`, so a
/// held reply sends that event and stops.
fn after(name: &str, text: &str) -> usize {
    let body = fixture(name);
    let at = body
        .windows(text.len())
        .position(|w| w == text.as_bytes())
        .expect("the fixture carries the text");
    let end = body[at..]
        .windows(2)
        .position(|w| w == b"\n\n")
        .expect("the event ends");
    at + end + 2
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn chats(state: &Path) -> Vec<Value> {
    let Ok(dir) = std::fs::read_dir(state.join("state/ask-claude")) else {
        return Vec::new();
    };
    dir.filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .map(|p| serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap())
        .collect()
}

/// The request bash `transport()` and `headers()` send: POST to
/// /v1/messages, the key in x-api-key and nowhere else, the API version, the
/// one beta a plain turn needs, and the body the request builder made.
#[test]
fn http_one_shot_sends_the_documented_request() {
    let mock = Mock::start(vec![Reply::sse("text.sse")]);
    let (out, _state) = one_shot(&mock.url, &["ping"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "pong\n");

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1, "{reqs:?}");
    let r = &reqs[0];
    assert_eq!(r.method, "POST");
    assert_eq!(r.path, "/v1/messages");
    assert_eq!(r.header("x-api-key"), KEY);
    assert_eq!(r.header("anthropic-version"), "2026-10-21");
    assert_eq!(
        r.header("anthropic-beta"),
        "server-side-fallback-2026-07-01"
    );
    assert!(
        r.header("content-type").starts_with("application/json"),
        "{}",
        r.header("content-type")
    );
    assert!(!r.path.contains(KEY), "the key never goes in the URL");
    assert_eq!(r.body["model"], "claude-opus-5-5");
    assert_eq!(r.body["stream"], true);
    assert_eq!(
        r.body["messages"],
        json!([{"role": "user", "content": "ping"}])
    );
}

/// Skills add the code-execution beta after the default one, comma-joined.
#[test]
fn http_skills_add_the_code_execution_beta() {
    let mock = Mock::start(vec![Reply::sse("code-execution.sse")]);
    let (out, _state) = one_shot(&mock.url, &["-s", "pptx", "run"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        mock.requests()[0].header("anthropic-beta"),
        "server-side-fallback-2026-07-01,code-execution-2025-08-25"
    );
}

/// A trailing slash on --base-url is stripped (bash strips it too), so the
/// path is /v1/messages, not //v1/messages.
#[test]
fn http_a_trailing_slash_on_the_base_url_is_dropped() {
    let mock = Mock::start(vec![Reply::sse("text.sse")]);
    let (out, _state) = one_shot(&format!("{}/", mock.url), &["ping"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(mock.requests()[0].path, "/v1/messages");
}

/// An error status comes back as its body, which run() reads as an API
/// error: exit 1, the API's own message, and no user turn kept. With ureq's
/// default (status as Err) this would be "no response" instead.
#[test]
fn http_an_error_status_is_an_api_error_with_its_message() {
    let mock = Mock::start(vec![Reply::error(404, "error.sse")]);
    let (out, state) = one_shot(&mock.url, &["ping"]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("API error: model: claude-does-not-exist"),
        "{}",
        stderr(&out)
    );
    for chat in chats(state.path()) {
        assert_eq!(chat.as_array().map(Vec::len), Some(0), "{chat}");
    }
}

/// A redirect is not followed, so the key never reaches the host it points
/// at: curl without -L does the same. The empty 307 body is the bash "no
/// response" path, exit 1.
#[test]
fn http_a_redirect_is_not_followed() {
    let elsewhere = Mock::start(vec![Reply::sse("text.sse")]);
    let mock = Mock::start(vec![Reply::redirect(&format!(
        "{}/v1/messages",
        elsewhere.url
    ))]);
    let (out, _state) = one_shot(&mock.url, &["ping"]);
    assert_eq!(mock.requests().len(), 1);
    assert!(
        elsewhere.requests().is_empty(),
        "the redirect was followed: {:?}",
        elsewhere.requests()
    );
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(stderr(&out).contains("no response"), "{}", stderr(&out));
}

/// Each pause_turn round is its own request, with the same headers, and the
/// history grows by the paused reply each time: round k sends k + 1
/// messages. Eight continuations, so nine requests.
#[test]
fn http_pause_turn_rounds_are_separate_requests() {
    let mock = Mock::start((0..9).map(|_| Reply::sse("pause-turn.sse")).collect());
    let (out, _state) = one_shot(&mock.url, &["ping"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 9, "{}", stderr(&out));
    for (k, r) in reqs.iter().enumerate() {
        assert_eq!(r.header("x-api-key"), KEY, "round {k}");
        assert_eq!(r.header("anthropic-version"), "2026-10-21", "round {k}");
        let n = r.body["messages"].as_array().map(Vec::len);
        assert_eq!(n, Some(k + 1), "round {k}");
    }
}

/// The reply is printed as it arrives, not once the body has ended: with
/// the stream held after the pong delta, "pong" is on stdout while the
/// rest (message_stop, the usage) has not been sent. curl -N and jq
/// --unbuffered do this in the script; a transport that reads the whole
/// body first prints nothing until the release.
#[test]
fn http_the_reply_prints_as_it_arrives() {
    let (reply, release) = Reply::held("text.sse", after("text.sse", "\"pong\""));
    let mock = Mock::start(vec![reply]);
    let (child, stdout, _state) = spawn(&mock.url, &["ping"], false);
    let early = printed(&stdout, "pong", Duration::from_secs(5));
    drop(release);
    let out = child.wait_with_output().unwrap();
    assert!(early, "pong was not printed while the stream was held");
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(printed(&stdout, "pong\n", Duration::from_secs(5)));
}

/// Ctrl-C while a reply streams exits 130 within a second (KTD11): the
/// signal only sets a flag, and the main loop, which waits for the next
/// line with a short timeout, sees it even though the socket read never
/// returns. The signal goes to the whole process group, as the terminal
/// sends it. The script dies of the signal instead, which its shell
/// reports as 130 but which is not an exit code, so against it this test
/// fails by design; the scaffold dies the same way.
#[cfg(unix)]
#[test]
fn http_ctrl_c_while_a_reply_streams_exits_130() {
    let (reply, release) = Reply::held("text.sse", after("text.sse", "\"pong\""));
    let mock = Mock::start(vec![reply]);
    let (mut child, stdout, _state) = spawn(&mock.url, &["ping"], true);
    assert!(
        printed(&stdout, "pong", Duration::from_secs(5)),
        "the reply never started"
    );
    let pid = libc::pid_t::try_from(child.id()).unwrap();
    // SAFETY: kill(2) on the process group the child leads; no memory involved
    assert_eq!(unsafe { libc::kill(-pid, libc::SIGINT) }, 0);
    let sent = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if sent.elapsed() > Duration::from_secs(1) {
            break None;
        }
        thread::sleep(Duration::from_millis(10));
    };
    drop(release);
    let Some(status) = status else {
        let _ = child.kill();
        panic!("still running a second after Ctrl-C");
    };
    assert_eq!(status.code(), Some(130), "{status:?}");
}

//! The Http transport (R5), end to end against a localhost mock: what the
//! replay seam cannot see. Replay hands the binary a recording and never
//! speaks HTTP, so the headers, the path, an error status, a redirect and
//! one request per pause_turn round are only checked here (and live, by
//! `just test`). The mock serves the recorded fixtures, never a stream made
//! up for the test, so the oracle rule holds: every byte it sends is one the
//! API sent, or the hand-derived error.sse the bats suite already uses.
//! The exception is `-p openai` (R7), which no oracle speaks: its streams
//! under `tests/fixtures/openai/` are written by hand from the shape
//! `tests/providers/openai-compatible.hurl` asserts, until a live run
//! records one.
//!
//! `ASK_CLAUDE_BIN=<path> cargo test --test http` runs them against another
//! binary, as the bats suite does; on the bash script they are green but
//! the `http_openai_*` ones, since bash refuses `-p openai`, and
//! `http_agent_ctrl_c_*`, since an interrupted bash session turn dies of
//! the signal where the plan (R6) asks for user.interrupt and 130: run it
//! there with `-- --skip http_openai --skip http_agent_ctrl_c`. The
//! sessions mock answers each connection on its own thread
//! ([`Mock::route`]), since a session stream stays open while its events
//! are posted; its canned create reply is the replay seam's own default.
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

/// A Messages stream under `tests/fixtures/sse/`, or, for a name with a
/// directory (`openai/text.sse`), the file under `tests/fixtures/`.
fn fixture(name: &str) -> Vec<u8> {
    let dir = if name.contains('/') {
        "tests/fixtures"
    } else {
        "tests/fixtures/sse"
    };
    std::fs::read(repo().join(dir).join(name)).expect("fixture")
}

/// One request as the mock read it; header names lower-cased.
#[derive(Debug, Clone)]
struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Value,
    /// the body as it came, for one that is not JSON (a multipart upload)
    raw: Vec<u8>,
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
    Some(Request {
        method,
        path,
        headers,
        body: serde_json::from_slice(&body).unwrap_or(Value::Null),
        raw: body,
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
    let mut cmd = binary(state);
    cmd.arg("--base-url").arg(base_url).args(args);
    cmd
}

/// The binary with the test key, state in `state`, no stdin, nothing from
/// the shell that runs the suite, and no arguments yet.
fn binary(state: &Path) -> Command {
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
    cmd.env("ANTHROPIC_API_KEY", KEY)
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

/// ANTHROPIC_BASE_URL is the environment twin of --base-url (R22): without
/// the flag the request goes to the root it names, and one stderr line names
/// the variable and the host. The flag wins over it and says nothing; the
/// variable then names a port nothing listens on, so a request sent there
/// would fail the one shot.
#[test]
fn http_anthropic_base_url_routes_the_request_and_the_flag_wins() {
    let announced = |o: &Output| -> Vec<String> {
        stderr(o)
            .lines()
            .filter(|l| l.contains("ANTHROPIC_BASE_URL"))
            .map(String::from)
            .collect()
    };

    let mock = Mock::start(vec![Reply::sse("text.sse")]);
    let state = tempfile::tempdir().unwrap();
    let out = binary(state.path())
        .env("ANTHROPIC_BASE_URL", &mock.url)
        .arg("ping")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "pong\n");
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1, "{reqs:?}");
    assert_eq!(reqs[0].path, "/v1/messages");
    let host = mock.url.strip_prefix("http://").unwrap();
    assert_eq!(
        announced(&out),
        [format!(
            "ask-claude: ANTHROPIC_BASE_URL is set; requests go to {host}"
        )]
    );

    let mock = Mock::start(vec![Reply::sse("text.sse")]);
    let state = tempfile::tempdir().unwrap();
    let out = command(&mock.url, &["ping"], state.path())
        .env("ANTHROPIC_BASE_URL", "http://127.0.0.1:9")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(mock.requests().len(), 1);
    assert_eq!(announced(&out), Vec::<String>::new());
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

/// The chat loop against `base_url`: `stdin` piped in, state in a temp dir.
fn ask(base_url: &str, args: &[&str], stdin: &str) -> (Output, tempfile::TempDir) {
    let state = tempfile::tempdir().unwrap();
    let mut child = command(base_url, args, state.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    (child.wait_with_output().unwrap(), state)
}

/// An API error in the chat loop is reported and the loop goes on: the
/// failed line is dropped with its error (bash rolls back to before the
/// ask), the next line is answered, and the chat holds only that exchange.
/// Replay cannot stage this in bash, where one file answers every send, so
/// it lives here. From the claude-rs prototype's REPL tests.
#[test]
fn http_an_api_error_mid_chat_is_not_fatal() {
    let mock = Mock::start(vec![Reply::error(404, "error.sse"), Reply::sse("text.sse")]);
    let (out, state) = ask(&mock.url, &[], "first\nsecond\n/quit\n");
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("API error: model: claude-does-not-exist"),
        "{}",
        stderr(&out)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("pong"));
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 2, "/quit is not sent");
    assert_eq!(
        reqs[1].body["messages"],
        json!([{"role": "user", "content": "second"}]),
        "the failed turn is not sent again"
    );
    let chats = chats(state.path());
    assert_eq!(chats.len(), 1, "{chats:?}");
    assert_eq!(chats[0].as_array().map(Vec::len), Some(2), "{}", chats[0]);
    assert_eq!(chats[0][0]["content"], "second");
}

// ---- -p openai: the chat-completions shape (R7) ----------------------------

const OPENAI_KEY: &str = "sk-openai-test-not-a-key";

/// The one shot with `-p openai` against `base_url`: the OpenAI key in its
/// variable, state in a temp dir.
fn openai(base_url: &str, args: &[&str]) -> (Output, tempfile::TempDir) {
    let state = tempfile::tempdir().unwrap();
    let out = command(base_url, &[&["-p", "openai"], args].concat(), state.path())
        .env("OPENAI_API_KEY", OPENAI_KEY)
        .output()
        .unwrap();
    (out, state)
}

/// The request the hurl contract sends: POST to /chat/completions under the
/// base URL, the key as a Bearer token from OPENAI_API_KEY and in no other
/// header or the URL (not the Anthropic key, no Anthropic headers), and the
/// streaming body with usage asked for. The streamed chunks print as the
/// Messages stream does.
#[test]
fn http_openai_one_shot_sends_the_chat_completions_request() {
    let mock = Mock::start(vec![Reply::sse("openai/text.sse")]);
    let (out, _state) = openai(&format!("{}/v1", mock.url), &["-m", "gpt-4.1-mini", "ping"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "pong\n");

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1, "{reqs:?}");
    let r = &reqs[0];
    assert_eq!(r.method, "POST");
    assert_eq!(r.path, "/v1/chat/completions");
    assert_eq!(r.header("authorization"), format!("Bearer {OPENAI_KEY}"));
    assert_eq!(r.header("x-api-key"), "");
    assert_eq!(r.header("anthropic-version"), "");
    assert_eq!(r.header("anthropic-beta"), "");
    assert!(
        r.header("content-type").starts_with("application/json"),
        "{}",
        r.header("content-type")
    );
    assert!(
        !r.path.contains(OPENAI_KEY),
        "the key never goes in the URL"
    );
    assert_eq!(
        r.body,
        json!({
            "model": "gpt-4.1-mini",
            "stream": true,
            "max_completion_tokens": 16000,
            "stream_options": {"include_usage": true},
            "messages": [{"role": "user", "content": "ping"}],
        })
    );
}

/// The reply folds into the message the Messages path keeps: the chat on
/// disk is `{role, content}` with the assistant's content a text block, and
/// the usage line has the Messages line's shape, the cache counters at the
/// `// 0` default.
#[test]
fn http_openai_usage_line_and_saved_chat_take_the_messages_shape() {
    let mock = Mock::start(vec![Reply::sse("openai/text.sse")]);
    let (out, state) = openai(&mock.url, &["-m", "gpt-4.1-mini", "ping"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let line = stderr(&out)
        .lines()
        .find(|l| l.starts_with('['))
        .map(String::from)
        .unwrap_or_default();
    assert!(
        line.starts_with("[gpt-4.1-mini in=14 cache_read=0 cache_write=0 out=2 ")
            && line.ends_with("s]"),
        "{}",
        stderr(&out)
    );
    assert_eq!(
        chats(state.path()),
        [json!([
            {"role": "user", "content": "ping"},
            {"role": "assistant", "content": [{"type": "text", "text": "pong"}]},
        ])]
    );
}

/// A stream with no usage chunk (a server that ignores `include_usage`)
/// prints the counts as null, as the script's line prints a missing field;
/// `finish_reason: length` is the max_tokens notice.
#[test]
fn http_openai_without_usage_prints_null_counts_and_the_length_stop() {
    let mock = Mock::start(vec![Reply::sse("openai/length.sse")]);
    let (out, _state) = openai(&mock.url, &["-m", "gpt-4.1-mini", "ping"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "po\n");
    let err = stderr(&out);
    assert!(
        err.contains("[gpt-4.1-mini in=null cache_read=0 cache_write=0 out=null "),
        "{err}"
    );
    assert!(
        err.contains("[stopped: max_tokens reached, raise --max-tokens]"),
        "{err}"
    );
}

/// An error status is the API's message, exit 1, and the user turn rolled
/// back, as on the Messages path.
#[test]
fn http_openai_an_error_status_is_an_api_error_with_its_message() {
    let mock = Mock::start(vec![Reply::error(404, "openai/error.json")]);
    let (out, state) = openai(&mock.url, &["-m", "gpt-nope", "ping"]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains(
            "ask-claude: API error: The model `gpt-nope` does not exist or you do not have access to it."
        ),
        "{}",
        stderr(&out)
    );
    for chat in chats(state.path()) {
        assert_eq!(chat.as_array().map(Vec::len), Some(0), "{chat}");
    }
}

/// The key is OPENAI_API_KEY's: without it the one shot stops before any
/// request, even with the Anthropic key set.
#[test]
fn http_openai_needs_openai_api_key() {
    let mock = Mock::start(vec![Reply::sse("openai/text.sse")]);
    let state = tempfile::tempdir().unwrap();
    let out = command(
        &mock.url,
        &["-p", "openai", "-m", "gpt-4.1-mini", "ping"],
        state.path(),
    )
    .output()
    .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert_eq!(stderr(&out), "ask-claude: no key: export OPENAI_API_KEY\n");
    assert!(mock.requests().is_empty());
}

/// OPENAI_BASE_URL is the API root for `-p openai`, and ANTHROPIC_BASE_URL
/// is not (KTD17): with both set the request goes where the OpenAI one
/// points, and nothing names the Anthropic one. The flag wins over
/// OPENAI_BASE_URL. Each unused root names a port nothing listens on.
#[test]
fn http_openai_base_url_is_openai_base_url_and_the_flag_wins() {
    let mock = Mock::start(vec![Reply::sse("openai/text.sse")]);
    let state = tempfile::tempdir().unwrap();
    let out = binary(state.path())
        .env("OPENAI_API_KEY", OPENAI_KEY)
        .env("ANTHROPIC_BASE_URL", "http://127.0.0.1:9")
        .env("OPENAI_BASE_URL", format!("{}/v1", mock.url))
        .args(["-p", "openai", "-m", "gpt-4.1-mini", "ping"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "pong\n");
    assert!(
        !stderr(&out).contains("ANTHROPIC_BASE_URL"),
        "{}",
        stderr(&out)
    );
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1, "{reqs:?}");
    assert_eq!(reqs[0].path, "/v1/chat/completions");

    let mock = Mock::start(vec![Reply::sse("openai/text.sse")]);
    let state = tempfile::tempdir().unwrap();
    let out = command(
        &mock.url,
        &["-p", "openai", "-m", "gpt-4.1-mini", "ping"],
        state.path(),
    )
    .env("OPENAI_API_KEY", OPENAI_KEY)
    .env("OPENAI_BASE_URL", "http://127.0.0.1:9")
    .output()
    .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(mock.requests().len(), 1);
}

/// The chunks print as they arrive: with the stream held after the "po"
/// chunk, "po" is on stdout before the rest is sent.
#[test]
fn http_openai_the_reply_prints_as_it_arrives() {
    let name = "openai/text.sse";
    let (reply, release) = Reply::held(name, after(name, "\"po\""));
    let mock = Mock::start(vec![reply]);
    let state = tempfile::tempdir().unwrap();
    let mut cmd = command(
        &mock.url,
        &["-p", "openai", "-m", "gpt-4.1-mini", "ping"],
        state.path(),
    );
    cmd.env("OPENAI_API_KEY", OPENAI_KEY)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
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
    let early = printed(&rx, "po", Duration::from_secs(5));
    drop(release);
    let out = child.wait_with_output().unwrap();
    assert!(early, "po was not printed while the stream was held");
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(printed(&rx, "pong\n", Duration::from_secs(5)));
}

// ---- Managed Agents sessions (R6): the live paths of `ma_call` and the stream

/// A session fixture under `tests/fixtures/sessions/`.
fn session_fixture(name: &str) -> Vec<u8> {
    std::fs::read(repo().join("tests/fixtures/sessions").join(name)).expect("fixture")
}

/// What `route` answers one request with.
type Route = Box<dyn Fn(&Request) -> Reply + Send + Sync>;

impl Mock {
    /// Answer every request on its own thread with `route`'s reply, so a
    /// session's stream can stay open while its events are posted. Each
    /// request is recorded, in the order the requests arrived, before its
    /// reply is chosen.
    fn route(route: Route) -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&requests);
        let route = Arc::new(route);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let (log, route) = (Arc::clone(&log), Arc::clone(&route));
                thread::spawn(move || {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                    let Some(req) = read_request(&stream) else {
                        return;
                    };
                    log.lock().unwrap().push(req.clone());
                    write_reply(stream, &route(&req));
                });
            }
        });
        Mock { url, requests }
    }
}

impl Reply {
    fn json(status: u16, body: &[u8]) -> Reply {
        Reply {
            status,
            headers: vec![("content-type", "application/json".into())],
            body: body.to_vec(),
            hold: None,
        }
    }
    /// A session stream: `: connected` at once, the rest once `release`
    /// yields or is dropped.
    fn session_stream(name: &str, release: mpsc::Receiver<()>) -> Reply {
        let body = session_fixture(name);
        let at = body
            .windows(2)
            .position(|w| w == b"\n\n")
            .map(|i| i + 2)
            .expect("the connected comment");
        assert!(body.starts_with(b": connected"), "{name}");
        Reply {
            status: 200,
            headers: vec![("content-type", "text/event-stream".into())],
            body,
            hold: Some((at, release)),
        }
    }
}

/// What the replay seam answers POST /v1/sessions with when no create.json
/// is given (`ma_call`): the session the fixtures are recorded under.
const CREATED: &[u8] = br#"{"id":"sesn_FIXTURE01","status":"running"}"#;

/// The binary against `base_url` with the agent and environment ids, `args`
/// after `agent`, `stdin` piped in, state in a temp dir.
fn agent(base_url: &str, args: &[&str], stdin: &str) -> (Output, tempfile::TempDir) {
    let state = tempfile::tempdir().unwrap();
    let mut child = command(base_url, &[&["agent"], args].concat(), state.path())
        .env("ASK_CLAUDE_AGENT", "agent_FIXTURE")
        .env("ASK_CLAUDE_ENVIRONMENT", "env_FIXTURE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    (child.wait_with_output().unwrap(), state)
}

/// The headers `ma_headers` sends on every sessions request: the key in
/// x-api-key and nowhere else, the API version, the Managed Agents beta.
fn assert_ma_headers(r: &Request) {
    assert_eq!(r.header("x-api-key"), KEY, "{r:?}");
    assert_eq!(r.header("anthropic-version"), "2026-10-21", "{r:?}");
    assert_eq!(
        r.header("anthropic-beta"),
        "managed-agents-2026-04-01",
        "{r:?}"
    );
    assert!(!r.path.contains(KEY), "the key never goes in the URL");
}

/// The one shot: POST /v1/sessions with the create body (the same golden
/// the bats suite holds the replayed body to), then the session's stream,
/// printed to the first idle; the usage line names the session.
#[test]
fn http_agent_one_shot_creates_the_session_then_reads_its_stream() {
    let mock = Mock::route(Box::new(|r: &Request| {
        match (r.method.as_str(), r.path.as_str()) {
            ("POST", "/v1/sessions") => Reply::json(200, CREATED),
            ("GET", "/v1/sessions/sesn_FIXTURE01/events/stream") => Reply {
                status: 200,
                headers: vec![("content-type", "text/event-stream".into())],
                body: session_fixture("text.sse"),
                hold: None,
            },
            _ => Reply::json(
                404,
                br#"{"error":{"type":"not_found_error","message":"no route"}}"#,
            ),
        }
    }));
    let (out, _state) = agent(&mock.url, &["ping"], "");
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "pong\n");
    assert!(
        stderr(&out).contains("[sesn_FIXTURE01 in=4 out=4 "),
        "{}",
        stderr(&out)
    );

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 2, "{reqs:?}");
    let create = &reqs[0];
    assert_eq!(
        (create.method.as_str(), create.path.as_str()),
        ("POST", "/v1/sessions")
    );
    assert_ma_headers(create);
    assert!(
        create
            .header("content-type")
            .starts_with("application/json"),
        "{create:?}"
    );
    let golden: Value = serde_json::from_slice(
        &std::fs::read(repo().join("tests/fixtures/agents/create-plain.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(create.body, golden);
    assert_eq!(reqs[1].method, "GET");
    assert_ma_headers(&reqs[1]);
}

/// A turn on a session that exists (KTD3): the stream is opened, and the
/// events are posted only once it said it is connected, so nothing the
/// session emits for them is missed. The mock holds the stream after its
/// connected comment until the events arrive.
#[test]
fn http_agent_turn_opens_the_stream_before_it_posts_the_events() {
    let (release, wait) = mpsc::channel();
    let release = Mutex::new(Some(release));
    let wait = Mutex::new(Some(wait));
    let mock = Mock::route(Box::new(move |r: &Request| {
        match (r.method.as_str(), r.path.as_str()) {
            ("GET", "/v1/sessions/sesn_FIXTURE01/events/stream") => {
                Reply::session_stream("chat/2.sse", wait.lock().unwrap().take().unwrap())
            }
            ("POST", "/v1/sessions/sesn_FIXTURE01/events") => {
                drop(release.lock().unwrap().take());
                Reply::json(200, b"{}")
            }
            ("GET", p) if p.starts_with("/v1/sessions/sesn_FIXTURE01/events?") => {
                Reply::json(200, &session_fixture("chat/events.json"))
            }
            _ => Reply::json(
                404,
                br#"{"error":{"type":"not_found_error","message":"no route"}}"#,
            ),
        }
    }));
    let (out, _state) = agent(&mock.url, &["resume", "sesn_FIXTURE01"], "again\n/quit\n");
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("pong"),
        "{}",
        stderr(&out)
    );
    assert!(
        stderr(&out).contains("you: Reply with the single word: pong"),
        "the history: {}",
        stderr(&out)
    );

    let reqs = mock.requests();
    let at = |method: &str, path: &str| {
        reqs.iter()
            .position(|r| r.method == method && r.path == path)
            .unwrap_or_else(|| panic!("no {method} {path}: {reqs:?}"))
    };
    let stream = at("GET", "/v1/sessions/sesn_FIXTURE01/events/stream");
    let post = at("POST", "/v1/sessions/sesn_FIXTURE01/events");
    assert!(stream < post, "the stream opens first: {reqs:?}");
    assert_eq!(
        reqs[post].body,
        json!({"events": [{"type": "user.message", "content": [{"type": "text", "text": "again"}]}]})
    );
    // curl -g sends the brackets as they are; either spelling is the query
    let raw = "/v1/sessions/sesn_FIXTURE01/events?types[]=user.message&types[]=agent.message";
    let history = &reqs[0];
    assert!(
        history.path == raw || history.path == raw.replace("[]", "%5B%5D"),
        "{history:?}"
    );
    reqs.iter().for_each(assert_ma_headers);
}

/// `agent list`: GET /v1/sessions newest first, one line per session.
#[test]
fn http_agent_list_gets_the_sessions_newest_first() {
    let mock = Mock::route(Box::new(|r: &Request| match r.path.as_str() {
        "/v1/sessions?order=desc&limit=50" => {
            Reply::json(200, &session_fixture("chat/sessions.json"))
        }
        _ => Reply::json(
            404,
            br#"{"error":{"type":"not_found_error","message":"no route"}}"#,
        ),
    }));
    let (out, _state) = agent(&mock.url, &["list"], "");
    assert!(out.status.success(), "{}", stderr(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout.lines().next(),
        Some("sesn_FIXTURE01  idle  $0.00  fixture custom tool"),
        "{stdout}"
    );
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1, "{reqs:?}");
    assert_eq!(reqs[0].method, "GET");
    assert_ma_headers(&reqs[0]);
}

/// A stream the API refuses (an error status with a JSON body) is the
/// script's "no response from the stream", with the body in it, exit 1.
#[test]
fn http_agent_a_refused_stream_is_no_response_with_its_body() {
    let mock = Mock::route(Box::new(|r: &Request| match r.method.as_str() {
        "POST" => Reply::json(200, CREATED),
        _ => Reply::json(401, &session_fixture("not-sse/1.sse")),
    }));
    let (out, state) = agent(&mock.url, &["ping"], "");
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("no response from the stream"),
        "{}",
        stderr(&out)
    );
    assert!(
        stderr(&out).contains("invalid x-api-key"),
        "{}",
        stderr(&out)
    );
    assert!(
        !state
            .path()
            .join("state/ask-claude/sessions/sesn_FIXTURE01")
            .exists(),
        "a failed one shot is not remembered"
    );
}

/// Ctrl-C while the live stream waits for its next event: the main thread
/// sees the flag between waits (KTD3, KTD11), posts `user.interrupt` to the
/// session and exits 130 within a second.
#[test]
fn http_agent_ctrl_c_while_the_stream_is_held_posts_user_interrupt_and_exits_130() {
    let (_release, wait) = mpsc::channel::<()>();
    let wait = Mutex::new(Some(wait));
    let mock = Mock::route(Box::new(move |r: &Request| {
        match (r.method.as_str(), r.path.as_str()) {
            ("POST", "/v1/sessions") => Reply::json(200, CREATED),
            ("GET", _) => {
                let name = "text.sse";
                let body = session_fixture(name);
                let at = body
                    .windows(b"\"pong\"".len())
                    .position(|w| w == b"\"pong\"")
                    .map(|i| i + body[i..].windows(2).position(|w| w == b"\n\n").unwrap() + 2)
                    .unwrap();
                Reply {
                    status: 200,
                    headers: vec![("content-type", "text/event-stream".into())],
                    body,
                    hold: Some((at, wait.lock().unwrap().take().unwrap())),
                }
            }
            _ => Reply::json(200, b"{}"),
        }
    }));
    let state = tempfile::tempdir().unwrap();
    let mut cmd = command(&mock.url, &["agent", "ping"], state.path());
    cmd.env("ASK_CLAUDE_AGENT", "agent_FIXTURE")
        .env("ASK_CLAUDE_ENVIRONMENT", "env_FIXTURE")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
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
    assert!(
        printed(&rx, "pong", Duration::from_secs(5)),
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
    let Some(status) = status else {
        let _ = child.kill();
        panic!("still running a second after Ctrl-C");
    };
    assert_eq!(status.code(), Some(130), "{status:?}");
    let reqs = mock.requests();
    let last = reqs.last().unwrap();
    assert_eq!(
        (last.method.as_str(), last.path.as_str()),
        ("POST", "/v1/sessions/sesn_FIXTURE01/events"),
        "{reqs:?}"
    );
    assert_eq!(last.body, json!({"events": [{"type": "user.interrupt"}]}));
}

// ---- the Files API: `agent files` downloads and `agent -f` uploads, which
// the replay seam stands in for without a request

/// What the mock answers GET /v1/files/<id>/content with: the bytes the
/// replay seam serves for that file.
fn file_content(name: &str) -> Reply {
    Reply {
        status: 200,
        headers: vec![("content-type", "application/octet-stream".into())],
        body: session_fixture(&format!("files/files/{name}")),
        hold: None,
    }
}

/// The Files API of the `files` fixture session: its list page, and
/// result.txt's content when `content` is true, else a 404.
fn files_mock(content: bool) -> Mock {
    Mock::route(Box::new(move |r: &Request| {
        match (r.method.as_str(), r.path.as_str()) {
            ("GET", "/v1/files?scope_id=sesn_FIXTURE01") => {
                Reply::json(200, &session_fixture("files/files.json"))
            }
            ("GET", "/v1/files/file_FIXTURE02/content") if content => file_content("result.txt"),
            _ => Reply::json(
                404,
                br#"{"error":{"type":"not_found_error","message":"no route"}}"#,
            ),
        }
    }))
}

/// `agent files <args>` against `base_url`, run in `cwd`.
fn files_in(base_url: &str, cwd: &Path, args: &[&str]) -> Output {
    let state = tempfile::tempdir().unwrap();
    command(
        base_url,
        &[&["agent", "files"], args].concat(),
        state.path(),
    )
    .current_dir(cwd)
    .output()
    .unwrap()
}

/// The names in `dir`, sorted.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// A download: the list page, then the file's content, both with the
/// session headers; the bytes land under the name, private, and no
/// temporary file is left beside it.
#[test]
fn http_agent_files_downloads_the_named_file_through_a_temp_file() {
    use std::os::unix::fs::PermissionsExt;
    let mock = files_mock(true);
    let dir = tempfile::tempdir().unwrap();
    let out = files_in(&mock.url, dir.path(), &["sesn_FIXTURE01", "result.txt"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("wrote result.txt (4 bytes)"),
        "{}",
        stderr(&out)
    );
    let path = dir.path().join("result.txt");
    assert_eq!(std::fs::read(&path).unwrap(), b"OUT\n");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    assert_eq!(names(dir.path()), ["result.txt"]);

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 2, "{reqs:?}");
    assert_eq!(
        (reqs[1].method.as_str(), reqs[1].path.as_str()),
        ("GET", "/v1/files/file_FIXTURE02/content")
    );
    reqs.iter().for_each(assert_ma_headers);
}

/// A download the API refuses (curl --fail): exit 1 with the script's
/// line, and nothing left in the directory, no partial file, no temp file.
#[test]
fn http_agent_files_a_refused_download_exits_1_and_leaves_nothing() {
    let mock = files_mock(false);
    let dir = tempfile::tempdir().unwrap();
    let out = files_in(&mock.url, dir.path(), &["sesn_FIXTURE01", "result.txt"]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("download of result.txt failed"),
        "{}",
        stderr(&out)
    );
    assert!(names(dir.path()).is_empty(), "{:?}", names(dir.path()));
}

/// --force replaces a regular file with a new one (a rename, never a write
/// through the old name, so a hard link to it keeps the old bytes), and
/// refuses a symlink before the content is asked for.
#[test]
fn http_agent_files_force_replaces_a_regular_file_and_never_a_symlink() {
    let mock = files_mock(true);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("result.txt");
    std::fs::write(&path, "mine\n").unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let linked = elsewhere.path().join("hard");
    std::fs::hard_link(&path, &linked).unwrap();
    let out = files_in(
        &mock.url,
        dir.path(),
        &["--force", "sesn_FIXTURE01", "result.txt"],
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(std::fs::read(&path).unwrap(), b"OUT\n");
    assert_eq!(std::fs::read(&linked).unwrap(), b"mine\n");

    std::fs::remove_file(&path).unwrap();
    let victim = elsewhere.path().join("victim.txt");
    std::fs::write(&victim, "precious\n").unwrap();
    std::os::unix::fs::symlink(&victim, &path).unwrap();
    let before = mock.requests().len();
    let out = files_in(
        &mock.url,
        dir.path(),
        &["--force", "sesn_FIXTURE01", "result.txt"],
    );
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert_eq!(std::fs::read(&victim).unwrap(), b"precious\n");
    let reqs = mock.requests();
    assert!(
        reqs[before..].iter().all(|r| !r.path.ends_with("/content")),
        "{reqs:?}"
    );
}

/// The Files API's answer to an upload: the new file, as the API sends it.
const UPLOADED: &[u8] =
    br#"{"id":"file_FIXTURE01","type":"file","filename":"notes.md","size_bytes":9}"#;

/// The upload, the create and the stream of a one shot with `-f`.
fn upload_mock() -> Mock {
    Mock::route(Box::new(|r: &Request| {
        match (r.method.as_str(), r.path.as_str()) {
            ("POST", "/v1/files") => Reply::json(200, UPLOADED),
            ("POST", "/v1/sessions") => Reply::json(200, CREATED),
            ("GET", "/v1/sessions/sesn_FIXTURE01/events/stream") => Reply {
                status: 200,
                headers: vec![("content-type", "text/event-stream".into())],
                body: session_fixture("text.sse"),
                hold: None,
            },
            _ => Reply::json(
                404,
                br#"{"error":{"type":"not_found_error","message":"no route"}}"#,
            ),
        }
    }))
}

/// The multipart body `curl -F file=@<path>` sends for one file: its
/// boundary from the content type, the part named `file` with the file's
/// name and a content type from its extension, the bytes, the closing
/// boundary.
fn form_part(r: &Request, filename: &str, content_type: &str, bytes: &[u8]) {
    let ct = r.header("content-type");
    let boundary = ct
        .strip_prefix("multipart/form-data; boundary=")
        .unwrap_or_else(|| panic!("not a form upload: {r:?}"));
    let mut want = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
    )
    .into_bytes();
    want.extend_from_slice(bytes);
    want.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    assert_eq!(
        String::from_utf8_lossy(&r.raw),
        String::from_utf8_lossy(&want)
    );
}

/// `agent -f`: the file goes to POST /v1/files as `curl -F` sends it, with
/// the key and the API version but not the Managed Agents beta (the Files
/// API is out of beta), and its id is attached to the session the create
/// makes, the prompt not prefixed (the same golden the bats suite holds
/// the replayed create to).
#[test]
fn http_agent_f_uploads_the_file_as_a_form_then_attaches_it() {
    let mock = upload_mock();
    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join("notes.md");
    std::fs::write(&notes, "line one\n").unwrap();
    let (out, _state) = agent(&mock.url, &["-f", notes.to_str().unwrap(), "summarize"], "");
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "pong\n");

    let reqs = mock.requests();
    assert_eq!(reqs.len(), 3, "{reqs:?}");
    let upload = &reqs[0];
    assert_eq!(
        (upload.method.as_str(), upload.path.as_str()),
        ("POST", "/v1/files")
    );
    assert_eq!(upload.header("x-api-key"), KEY);
    assert_eq!(upload.header("anthropic-version"), "2026-10-21");
    assert!(
        !upload.headers.contains_key("anthropic-beta"),
        "no beta header on the Files API: {upload:?}"
    );
    form_part(
        upload,
        "notes.md",
        "application/octet-stream",
        b"line one\n",
    );

    let golden: Value = serde_json::from_slice(
        &std::fs::read(repo().join("tests/fixtures/agents/create-file.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(reqs[1].path, "/v1/sessions");
    assert_eq!(reqs[1].body, golden);
}

/// The part's name and type as curl writes them: a double quote, a
/// carriage return and a newline in the name percent-encoded (curl's form
/// escaping), the type from the extension whatever its case.
#[test]
fn http_agent_f_names_and_types_the_part_as_curl_does() {
    let mock = upload_mock();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a\"b\nc.TXT");
    std::fs::write(&file, "x").unwrap();
    let (_out, _state) = agent(&mock.url, &["-f", file.to_str().unwrap(), "summarize"], "");
    let reqs = mock.requests();
    assert_eq!(reqs[0].path, "/v1/files", "{reqs:?}");
    form_part(&reqs[0], "a%22b%0Ac.TXT", "text/plain", b"x");
}

/// An upload the API refuses: its message after `upload failed`, exit 1,
/// and no session created.
#[test]
fn http_agent_f_a_refused_upload_exits_1_before_the_create() {
    let mock = Mock::route(Box::new(|_: &Request| {
        Reply::json(
            400,
            br#"{"type":"error","error":{"type":"invalid_request_error","message":"file too large"}}"#,
        )
    }));
    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join("notes.md");
    std::fs::write(&notes, "line one\n").unwrap();
    let (out, _state) = agent(&mock.url, &["-f", notes.to_str().unwrap(), "summarize"], "");
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("upload failed: file too large"),
        "{}",
        stderr(&out)
    );
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1, "{reqs:?}");
}

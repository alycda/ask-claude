//! The harness every protocol test uses: an MCP client that knows nothing
//! of rmcp. It speaks newline-delimited JSON-RPC to a child process, the
//! way Claude Code and the script's `--local-mcp` do, so the tests pin the
//! wire and leave the server's types to the server. tests/harness.rs shows
//! it working on the script's own fake server before any server exists.
//!
//! Every child runs with a cleared environment: PATH, HOME and
//! XDG_STATE_HOME only. No key, no MISE_*, nothing the server could read by
//! accident. The working directory is the repository root, so mise's shims
//! (jq, usage) resolve from mise.toml.

#![allow(dead_code)] // each test file uses its own part of the harness

use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Value, json};
use tempfile::TempDir;

/// How long any one answer, or the exit after stdin closes, may take.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// The protocol version the handshake offers: the one the script's
/// `--local-mcp` client sends, so the two clients agree.
pub const PROTOCOL: &str = "2025-06-18";

pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("mcp/ sits in the repository")
        .to_path_buf()
}

pub fn fixture(rel: &str) -> PathBuf {
    repo().join("tests/fixtures").join(rel)
}

pub fn read_json(path: &Path) -> Value {
    let text =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{} is not JSON: {e}", path.display()))
}

/// `../VERSION`, trimmed: what the server must report as its version.
pub fn version() -> String {
    fs::read_to_string(repo().join("VERSION"))
        .expect("VERSION")
        .trim()
        .to_string()
}

/// The server under test: this crate's binary, or ASK_CLAUDE_MCP_BIN (a
/// release build, or a stand-in when checking the tests themselves), the
/// variable tests/unit/mcp.bats reads too.
pub fn server_bin() -> PathBuf {
    std::env::var_os("ASK_CLAUDE_MCP_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_ask-claude-mcp")))
}

/// The five reports, in the order the README asks them.
pub const REPORTS: [&str; 5] = [
    "tokens_by_chat",
    "cache_hit_by_model",
    "seconds_by_model",
    "refusals_by_category",
    "advisor_spend",
];

/// tests/fixtures/mcp/reports/<name>.json, or <name>.since.json with a
/// suffix "since": what jq prints for the events fixture.
pub fn golden(name: &str, suffix: Option<&str>) -> Value {
    let file = match suffix {
        Some(s) => format!("mcp/reports/{name}.{s}.json"),
        None => format!("mcp/reports/{name}.json"),
    };
    read_json(&fixture(&file))
}

/// Equal the way jq sees JSON: a number is its value, so jq's `6` and the
/// `6.0` an f64 serializes to are the same; object keys in any order;
/// arrays in order. serde_json's own `==` tells 6 from 6.0.
pub fn jq_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| jq_eq(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| jq_eq(v, w)))
        }
        _ => a == b,
    }
}

#[track_caller]
pub fn assert_jq_eq(actual: &Value, expected: &Value, what: &str) {
    assert!(
        jq_eq(actual, expected),
        "{what}\n--- expected (jq) ---\n{}\n--- actual ---\n{}",
        serde_json::to_string_pretty(expected).unwrap(),
        serde_json::to_string_pretty(actual).unwrap()
    );
}

/// The names of a tools/list or resources/list array, in order.
pub fn names(items: &[Value]) -> Vec<String> {
    items
        .iter()
        .map(|t| {
            t["name"]
                .as_str()
                .expect("every item has a name")
                .to_string()
        })
        .collect()
}

/// One test's state: XDG_STATE_HOME is `<tmp>/state`, so the script's
/// STATE is `<tmp>/state/ask-claude`, and HOME is `<tmp>/home`. Nothing
/// under `state/` exists until a test makes it.
pub struct StateHome {
    tmp: TempDir,
}

impl Default for StateHome {
    fn default() -> Self {
        Self::new()
    }
}

impl StateHome {
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().expect("a temp dir");
        fs::create_dir(tmp.path().join("home")).expect("HOME");
        StateHome { tmp }
    }

    pub fn xdg(&self) -> PathBuf {
        self.tmp.path().join("state")
    }

    pub fn home(&self) -> PathBuf {
        self.tmp.path().join("home")
    }

    /// The script's STATE for this test.
    pub fn dir(&self) -> PathBuf {
        self.xdg().join("ask-claude")
    }

    /// STATE made the way the script makes it: mode 700.
    pub fn create(&self) -> PathBuf {
        let dir = self.dir();
        fs::create_dir_all(&dir).expect("STATE");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).expect("chmod 700");
        dir
    }

    /// A file directly under STATE, mode 600 as the script's umask leaves it.
    pub fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.create().join(name);
        fs::write(&path, contents).expect("a state file");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("chmod 600");
        path
    }

    pub fn events(&self, contents: &str) -> PathBuf {
        self.write("events.jsonl", contents)
    }

    /// tests/fixtures/mcp/events.jsonl as this test's event log.
    pub fn events_fixture(&self) -> PathBuf {
        let text = fs::read_to_string(fixture("mcp/events.jsonl")).expect("the events fixture");
        self.events(&text)
    }

    /// A chat file `<name>.json`, last written `age` seconds ago, so tests
    /// can order chats the way `ls -t` does.
    pub fn chat(&self, name: &str, turns: &str, age: u64) -> PathBuf {
        let path = self.write(&format!("{name}.json"), turns);
        let when = SystemTime::now() - Duration::from_secs(age);
        fs::File::options()
            .write(true)
            .open(&path)
            .and_then(|f| f.set_modified(when))
            .expect("set the chat's mtime");
        path
    }

    /// The environment every child gets, besides PATH.
    pub fn env(&self) -> Vec<(&'static str, OsString)> {
        vec![
            ("HOME", self.home().into_os_string()),
            ("XDG_STATE_HOME", self.xdg().into_os_string()),
        ]
    }

    /// The server under test, on this state.
    pub fn start(&self) -> Server {
        Server::spawn(&server_bin(), &self.env())
    }

    /// The server under test, handshake done.
    pub fn connect(&self) -> Server {
        let mut server = self.start();
        server.initialize();
        server
    }
}

/// A child process spoken to over stdio.
pub struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    stderr: Arc<Mutex<String>>,
    next_id: u64,
    what: String,
}

impl Server {
    pub fn spawn(program: &Path, env: &[(&str, OsString)]) -> Server {
        let mut cmd = Command::new(program);
        cmd.env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .current_dir(repo())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd
            .spawn()
            .unwrap_or_else(|e| panic!("cannot start {}: {e}", program.display()));

        let (tx, lines) = mpsc::channel();
        let stdout = child.stdout.take().expect("stdout");
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let stderr = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&stderr);
        let mut err = child.stderr.take().expect("stderr");
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = err.read(&mut buf) {
                if n == 0 {
                    break;
                }
                sink.lock()
                    .unwrap()
                    .push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        });
        Server {
            stdin: child.stdin.take(),
            child,
            lines,
            stderr,
            next_id: 1,
            what: program.display().to_string(),
        }
    }

    /// What the child wrote to stderr so far.
    pub fn stderr(&self) -> String {
        self.stderr.lock().unwrap().clone()
    }

    fn fail(&self, why: &str) -> ! {
        panic!(
            "{}: {why}\n--- its stderr ---\n{}",
            self.what,
            self.stderr()
        )
    }

    /// One message, one line.
    pub fn send(&mut self, message: &Value) {
        let line = format!("{message}\n");
        let ok = self
            .stdin
            .as_mut()
            .map(|s| s.write_all(line.as_bytes()).and_then(|_| s.flush()).is_ok())
            .unwrap_or(false);
        if !ok {
            self.fail("stdin is closed: the process exited before reading the request");
        }
    }

    pub fn notify(&mut self, method: &str, params: Value) {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    /// The whole response to one request: `result` or `error`. Notifications
    /// on the way are skipped; a request from the server gets
    /// method-not-found, since this client offers no capabilities. A line
    /// that is not JSON fails the test: stdout is the protocol's alone.
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = match self.lines.recv_timeout(left) {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => {
                    self.fail(&format!("no answer to {method} in {TIMEOUT:?}"))
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.fail(&format!("stdout closed before an answer to {method}"))
                }
            };
            let msg: Value = serde_json::from_str(&line).unwrap_or_else(|_| {
                self.fail(&format!(
                    "stdout carries JSON-RPC only, and this line is not JSON: {line}"
                ))
            });
            if msg.get("method").is_some() {
                if let Some(their_id) = msg.get("id") {
                    let reply = json!({"jsonrpc": "2.0", "id": their_id,
                        "error": {"code": -32601, "message": "method not found"}});
                    self.send(&reply);
                }
                continue;
            }
            if msg["id"] == json!(id) {
                return msg;
            }
        }
    }

    /// The `result` of a request that must succeed.
    pub fn result(&mut self, method: &str, params: Value) -> Value {
        let response = self.request(method, params);
        match response.get("result") {
            Some(r) => r.clone(),
            None => self.fail(&format!("{method} failed: {response}")),
        }
    }

    /// The handshake as the script's client does it: initialize at
    /// [`PROTOCOL`], then notifications/initialized. Returns the result.
    pub fn initialize(&mut self) -> Value {
        let result = self.result(
            "initialize",
            json!({"protocolVersion": PROTOCOL, "capabilities": {},
                   "clientInfo": {"name": "ask-claude-mcp-tests", "version": "0"}}),
        );
        self.notify("notifications/initialized", json!({}));
        result
    }

    /// Every tool, following nextCursor.
    pub fn tools(&mut self) -> Vec<Value> {
        self.list("tools/list", "tools")
    }

    /// Every resource, following nextCursor.
    pub fn resources(&mut self) -> Vec<Value> {
        self.list("resources/list", "resources")
    }

    fn list(&mut self, method: &str, key: &str) -> Vec<Value> {
        let mut all = Vec::new();
        let mut params = json!({});
        for _ in 0..20 {
            let page = self.result(method, params);
            let items = page[key]
                .as_array()
                .unwrap_or_else(|| self.fail(&format!("{method} has no {key} array: {page}")));
            all.extend(items.iter().cloned());
            match page.get("nextCursor").and_then(Value::as_str) {
                Some(cursor) => params = json!({"cursor": cursor}),
                None => return all,
            }
        }
        self.fail(&format!("{method} still had a cursor after 20 pages"))
    }

    /// The `result` of tools/call: content, and isError on a tool error.
    pub fn call(&mut self, name: &str, arguments: Value) -> Value {
        self.result("tools/call", json!({"name": name, "arguments": arguments}))
    }

    /// A report: the call must succeed, and its first content block is
    /// text holding the report as JSON, which this returns parsed.
    pub fn report(&mut self, name: &str, arguments: Value) -> Value {
        let result = self.call(name, arguments);
        if result["isError"] == json!(true) {
            self.fail(&format!("{name} is a tool error: {result}"));
        }
        let block = &result["content"][0];
        if block["type"] != "text" {
            self.fail(&format!(
                "{name}: the first content block is not text: {result}"
            ));
        }
        let text = block["text"].as_str().unwrap_or_default();
        serde_json::from_str(text)
            .unwrap_or_else(|_| self.fail(&format!("{name}: the text is not JSON: {text}")))
    }

    /// A tool error: a result with isError true (not a JSON-RPC error, so
    /// the model sees why), whose text this returns.
    pub fn tool_error(&mut self, name: &str, arguments: Value) -> String {
        let result = self.call(name, arguments);
        if result["isError"] != json!(true) {
            self.fail(&format!("{name} should be a tool error: {result}"));
        }
        result["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    /// The whole response to resources/read.
    pub fn read(&mut self, uri: &str) -> Value {
        self.request("resources/read", json!({"uri": uri}))
    }

    /// Close stdin and wait for the exit; a process still running after
    /// [`TIMEOUT`] is killed and fails the test.
    pub fn close(mut self) -> ExitStatus {
        drop(self.stdin.take());
        let deadline = Instant::now() + TIMEOUT;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return status,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
                _ => {
                    let _ = self.child.kill();
                    self.fail(&format!("still running {TIMEOUT:?} after stdin closed"))
                }
            }
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

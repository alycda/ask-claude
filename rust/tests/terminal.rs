//! The chat loop on a terminal (R5, KTD5): line editing where one paste is
//! one turn, Ctrl-C exits 130 without the epilogue, Ctrl-D runs it, and no
//! history file is written. The bats suite has no terminal, so these drive
//! the binary through util-linux `script`, which gives it a pty and passes
//! the bytes written here as typed keys. Linux only, and skipped where
//! `script` is missing; on macOS they are a manual check, the last step of
//! the phase 5 gate in docs/plan.md.
//!
//! `ASK_CLAUDE_BIN=<path>` runs them against another binary. The bash
//! script this binary replaced failed the paste test by design: `read -e`
//! kept only the first line of a paste and dropped the rest.

#![cfg(target_os = "linux")]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const WAIT: Duration = Duration::from_secs(10);

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The binary on a pty, what it has printed so far, and its keyboard.
struct Terminal {
    child: Child,
    keys: ChildStdin,
    seen: Arc<Mutex<Vec<u8>>>,
    reader: thread::JoinHandle<()>,
}

impl Terminal {
    /// The binary with `args` on a pty, replaying text.sse, its state and
    /// HOME in `dir`; None where `script` is not installed.
    fn start(dir: &Path, args: &[&str]) -> Option<Self> {
        Self::start_with(dir, args, None)
    }

    /// [`Terminal::start`], with stdout redirected to `stdout` when given,
    /// so only stderr and the keys are on the pty.
    fn start_with(dir: &Path, args: &[&str], stdout: Option<&Path>) -> Option<Self> {
        Self::start_env(dir, args, stdout, &[])
    }

    /// [`Terminal::start_with`], with `env` set over the defaults (another
    /// replay, the agent and environment ids).
    fn start_env(
        dir: &Path,
        args: &[&str],
        stdout: Option<&Path>,
        env: &[(&str, &std::ffi::OsStr)],
    ) -> Option<Self> {
        Self::start_redirected(dir, args, stdout, None, env)
    }

    /// [`Terminal::start_env`], with stderr redirected to `stderr` when
    /// given (`ask-claude 2> errors.txt`), so the pty carries stdout and
    /// the keys, and whatever the binary writes to the terminal itself.
    fn start_redirected(
        dir: &Path,
        args: &[&str],
        stdout: Option<&Path>,
        stderr: Option<&Path>,
        env: &[(&str, &std::ffi::OsStr)],
    ) -> Option<Self> {
        if Command::new("script").arg("--version").output().is_err() {
            eprintln!("skipped: no util-linux script to give the binary a terminal");
            return None;
        }
        let bin = match std::env::var_os("ASK_CLAUDE_BIN") {
            Some(bin) => PathBuf::from(bin),
            None => PathBuf::from(env!("CARGO_BIN_EXE_ask-claude")),
        };
        let mut line = shell_quote(&bin.to_string_lossy());
        for a in args {
            line.push(' ');
            line.push_str(&shell_quote(a));
        }
        if let Some(file) = stdout {
            line.push_str(" > ");
            line.push_str(&shell_quote(&file.to_string_lossy()));
        }
        if let Some(file) = stderr {
            line.push_str(" 2> ");
            line.push_str(&shell_quote(&file.to_string_lossy()));
        }
        let home = dir.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let mut cmd = Command::new("script");
        cmd.args(["-q", "-f", "-e", "-c", &line, "/dev/null"]);
        for (name, _) in std::env::vars_os() {
            let n = name.to_string_lossy();
            if n.starts_with("ASK_CLAUDE_") || n.starts_with("ANTHROPIC_") {
                cmd.env_remove(&name);
            }
        }
        cmd.env("TERM", "xterm-256color")
            .env("HOME", &home)
            .env("XDG_STATE_HOME", dir.join("state"))
            .env("ANTHROPIC_API_KEY", "sk-ant-test-not-a-key")
            .env(
                "ASK_CLAUDE_REPLAY",
                repo().join("tests/fixtures/sse/text.sse"),
            )
            .env("ASK_CLAUDE_REQUEST_LOG", dir.join("requests.jsonl"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (name, value) in env {
            cmd.env(name, value);
        }
        let mut child = cmd.spawn().unwrap();
        let keys = child.stdin.take().unwrap();
        let mut out = child.stdout.take().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let reader = thread::spawn(move || {
            let mut buf = [0; 512];
            while let Ok(n @ 1..) = out.read(&mut buf) {
                sink.lock().unwrap().extend_from_slice(&buf[..n]);
            }
        });
        Some(Self {
            child,
            keys,
            seen,
            reader,
        })
    }

    /// Everything printed so far, as text.
    fn screen(&self) -> String {
        String::from_utf8_lossy(&self.seen.lock().unwrap()).into_owned()
    }

    /// Wait until `text` has been printed after the first `after` (from
    /// the start when `after` is empty): the prompt that follows a reply,
    /// not the one before it.
    fn wait_for(&self, after: &str, text: &str) {
        let deadline = Instant::now() + WAIT;
        loop {
            let screen = self.screen();
            if let Some(at) = screen.find(after)
                && screen[at + after.len()..].contains(text)
            {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{text:?} not printed after {after:?}: {screen:?}"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn type_keys(&mut self, bytes: &[u8]) {
        self.keys.write_all(bytes).unwrap();
        self.keys.flush().unwrap();
    }

    /// The exit status, waiting at most WAIT, and all that was printed.
    fn exit(mut self) -> (ExitStatus, String) {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                // the pty closes with the process, so the reader ends
                // once it has everything
                let _ = self.reader.join();
                let screen = String::from_utf8_lossy(&self.seen.lock().unwrap()).into_owned();
                return (status, screen);
            }
            if Instant::now() > deadline {
                let _ = self.child.kill();
                panic!("still running: {:?}", self.screen());
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn requests(dir: &Path) -> Vec<Value> {
    std::fs::read_to_string(dir.join("requests.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn chats(dir: &Path) -> Vec<Value> {
    let Ok(entries) = std::fs::read_dir(dir.join("state/ask-claude")) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .map(|p| serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap())
        .collect()
}

/// A paste of two lines, in the bracketed-paste markers a terminal sends
/// around it, then Enter: one turn holding both lines, not two turns and
/// not the first line alone. Ctrl-D at the next prompt runs the epilogue
/// and exits 0, and HOME holds no history file afterwards: the line
/// history is in memory only, since a file would keep the typed lines of a
/// Chat that leaves none.
#[test]
fn terminal_one_paste_is_one_turn_and_ctrl_d_quits() {
    let dir = tempfile::tempdir().unwrap();
    let Some(mut term) = Terminal::start(dir.path(), &[]) else {
        return;
    };
    term.wait_for("", "> ");
    term.type_keys(b"\x1b[200~line one\nline two\x1b[201~");
    thread::sleep(Duration::from_millis(200));
    term.type_keys(b"\r");
    term.wait_for("", "pong");
    term.wait_for("pong", "> ");
    term.type_keys(b"\x04");
    let (status, screen) = term.exit();
    assert_eq!(status.code(), Some(0), "{screen}");
    assert!(screen.contains("resume with:"), "{screen}");
    let reqs = requests(dir.path());
    assert_eq!(reqs.len(), 1, "{reqs:?}");
    assert_eq!(
        reqs[0]["messages"],
        json!([{"role": "user", "content": "line one\nline two"}])
    );
    let home: Vec<_> = std::fs::read_dir(dir.path().join("home"))
        .unwrap()
        .collect();
    assert!(home.is_empty(), "a file in HOME: {home:?}");
}

/// Ctrl-C at the prompt exits 130 at once, without the epilogue: the
/// Chat stays as it was, its turns saved, and nothing says how to resume.
#[test]
fn terminal_ctrl_c_at_the_prompt_exits_130_without_the_epilogue() {
    let dir = tempfile::tempdir().unwrap();
    let Some(mut term) = Terminal::start(dir.path(), &[]) else {
        return;
    };
    term.wait_for("", "> ");
    term.type_keys(b"hi\r");
    term.wait_for("", "pong");
    term.wait_for("pong", "> ");
    term.type_keys(b"\x03");
    let (status, screen) = term.exit();
    assert_eq!(status.code(), Some(130), "{screen}");
    assert!(!screen.contains("resume with"), "{screen}");
    let chats = chats(dir.path());
    assert_eq!(chats.len(), 1, "{chats:?}");
    assert_eq!(chats[0].as_array().map(Vec::len), Some(2));
}

/// SIGTERM or SIGHUP from another process while the editor waits
/// at the prompt exits 130 then, without the epilogue, as off a pipe:
/// not at the next key, and the Enter that follows saves no turn.
#[test]
fn terminal_a_signal_at_the_prompt_exits_130_without_the_epilogue() {
    // kill -INT takes the SIGTERM path: the same handler, the same flag
    for signal in [libc::SIGTERM, libc::SIGHUP] {
        let dir = tempfile::tempdir().unwrap();
        let Some(mut term) = Terminal::start(dir.path(), &[]) else {
            return;
        };
        term.wait_for("", "> ");
        term.type_keys(b"hi\r");
        term.wait_for("", "pong");
        term.wait_for("pong", "> ");
        let pid = binary_pid(term.child.id());
        // SAFETY: kill(2) of the binary this test started, still running
        assert_eq!(unsafe { libc::kill(pid, signal) }, 0);
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut exited = false;
        while !exited && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
            exited = term.child.try_wait().unwrap().is_some();
        }
        if !exited {
            // still at the prompt: this Enter would save a dangling turn
            term.type_keys(b"again\r");
        }
        let (status, screen) = term.exit();
        assert!(exited, "signal {signal}: still waiting for a key: {screen}");
        assert_eq!(status.code(), Some(130), "signal {signal}: {screen}");
        assert!(!screen.contains("resume with"), "{screen}");
        let chats = chats(dir.path());
        assert_eq!(chats.len(), 1, "{chats:?}");
        assert_eq!(chats[0].as_array().map(Vec::len), Some(2), "{chats:?}");
    }
}

/// The pid of the binary under the `script` whose pid is `script`: the
/// process below it whose executable is the binary.
fn binary_pid(script: u32) -> libc::pid_t {
    let bin = match std::env::var_os("ASK_CLAUDE_BIN") {
        Some(bin) => PathBuf::from(bin),
        None => PathBuf::from(env!("CARGO_BIN_EXE_ask-claude")),
    };
    let bin = std::fs::canonicalize(bin).unwrap();
    let parent = |pid: &str| -> Option<String> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let after = &stat[stat.rfind(')')? + 2..];
        after.split(' ').nth(1).map(str::to_string)
    };
    let pids: Vec<String> = std::fs::read_dir("/proc")
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.bytes().all(|b| b.is_ascii_digit()))
        .collect();
    let mut below = vec![script.to_string()];
    let mut at = 0;
    while at < below.len() {
        let pid = below[at].clone();
        if std::fs::read_link(format!("/proc/{pid}/exe")).is_ok_and(|exe| exe == bin) {
            return pid.parse().unwrap();
        }
        below.extend(
            pids.iter()
                .filter(|p| parent(p).as_deref() == Some(&pid))
                .cloned(),
        );
        at += 1;
    }
    panic!("no {} below script {script}", bin.display());
}

/// The keys and the prompt on the terminal, the replies redirected to a
/// file (`ask-claude > answers.txt`): the file holds the reply and the
/// epilogue, and none of the codes the line editor sends the terminal
/// (bracketed paste on and off, a cursor query). The script's `read -e`
/// writes those to the terminal only.
#[test]
fn terminal_a_redirected_stdout_gets_no_terminal_codes() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.txt");
    let Some(mut term) = Terminal::start_with(dir.path(), &[], Some(&out)) else {
        return;
    };
    term.wait_for("", "> ");
    term.type_keys(b"hi\r");
    // the usage line is on stderr, the terminal; the next prompt follows it
    term.wait_for("[claude-opus-5-5", "> ");
    term.type_keys(b"\x04");
    let (status, screen) = term.exit();
    assert_eq!(status.code(), Some(0), "{screen}");
    let file = std::fs::read_to_string(&out).unwrap();
    assert!(file.contains("pong"), "{file:?}");
    assert!(file.contains("claude: pong"), "{file:?}");
    assert!(!file.contains('\x1b'), "{file:?}");
}

// ---- the tool confirmation of a session, asked on /dev/tty (KTD9) --------

/// `agent ping` on a pty over the session replay `replay`.
fn agent_on_terminal(dir: &Path, replay: &Path) -> Option<Terminal> {
    let replay = replay.as_os_str();
    Terminal::start_env(
        dir,
        &["agent", "ping"],
        None,
        &[
            ("ASK_CLAUDE_REPLAY", replay),
            ("ASK_CLAUDE_AGENT", "agent_FIXTURE".as_ref()),
            ("ASK_CLAUDE_ENVIRONMENT", "env_FIXTURE".as_ref()),
        ],
    )
}

/// A replay directory whose turns ask for a bash call each, `calls` of
/// them one stream after another, then end; agent-confirm.bats's
/// confirm_fixture, one ask per stream.
fn asking(dir: &Path, calls: usize) -> PathBuf {
    let replay = dir.join("replay");
    std::fs::create_dir_all(&replay).unwrap();
    let usage = r#"data: {"type":"session.usage","usage":{"input_tokens":1,"output_tokens":1}}"#;
    for n in 1..=calls {
        let ask = format!(
            r#"data: {{"type":"agent.tool_use","id":"sevt_T{n}","name":"bash","input":{{"command":"echo {n}"}}}}"#
        );
        let idle = format!(
            r#"data: {{"type":"session.status_idle","stop_reason":{{"type":"requires_action","event_ids":["sevt_T{n}"]}}}}"#
        );
        std::fs::write(
            replay.join(format!("{n}.sse")),
            format!(": connected\n\n{ask}\n\n{usage}\n\n{idle}\n\n"),
        )
        .unwrap();
    }
    let end = r#"data: {"type":"session.status_idle","stop_reason":{"type":"end_turn"}}"#;
    std::fs::write(
        replay.join(format!("{}.sse", calls + 1)),
        format!(": connected\n\n{usage}\n\n{end}\n\n"),
    )
    .unwrap();
    replay
}

/// The confirmation results the request log holds, in order.
fn results(dir: &Path) -> Vec<String> {
    requests(dir)
        .iter()
        .flat_map(|r| r["events"].as_array().cloned().unwrap_or_default())
        .filter(|e| e["type"] == "user.tool_confirmation")
        .map(|e| e["result"].as_str().unwrap_or("").to_string())
        .collect()
}

/// On a terminal the ask is a prompt in the script's words, the input
/// encoded once: `y` allows, and the next ask is asked again; Enter alone
/// denies.
#[test]
fn terminal_agent_asks_on_the_terminal_y_allows_enter_denies() {
    let dir = tempfile::tempdir().unwrap();
    let replay = asking(dir.path(), 2);
    let Some(mut term) = agent_on_terminal(dir.path(), &replay) else {
        return;
    };
    term.wait_for("", r#"allow bash {"command":"echo 1"}? [y/N/a(lways)] "#);
    term.type_keys(b"y\r");
    term.wait_for("", r#"allow bash {"command":"echo 2"}? [y/N/a(lways)] "#);
    term.type_keys(b"\r");
    let (status, screen) = term.exit();
    assert_eq!(status.code(), Some(0), "{screen}");
    assert!(screen.contains("ask-claude: allowed bash"), "{screen}");
    assert_eq!(results(dir.path()), ["allow", "deny"], "{screen}");
}

/// A line typed before a prompt was shown is not an answer to it. The
/// first `y` answers the first ask; the second `y`, sent in the same
/// write, is in the terminal's queue before the second prompt exists, so
/// whoever typed it had not seen that call's name or input. The second
/// prompt must still wait, and Enter alone then denies. Security-audit
/// run 4 (lead 5) queued `y` ahead of a prompt and the call was allowed.
/// Two asks, so the order is fixed by the test and not by how fast the
/// binary reaches its first prompt.
#[test]
fn terminal_agent_does_not_take_type_ahead_as_the_answer() {
    let dir = tempfile::tempdir().unwrap();
    let replay = asking(dir.path(), 2);
    let Some(mut term) = agent_on_terminal(dir.path(), &replay) else {
        return;
    };
    term.wait_for("", r#"allow bash {"command":"echo 1"}? [y/N/a(lways)] "#);
    term.type_keys(b"y\ry\r");
    term.wait_for("", r#"allow bash {"command":"echo 2"}? [y/N/a(lways)] "#);
    // not type_keys: where the queued line answered, the binary has
    // exited and the pipe is closed; the assertion below reports that
    let _ = term.keys.write_all(b"\r");
    let _ = term.keys.flush();
    let (status, screen) = term.exit();
    assert_eq!(status.code(), Some(0), "{screen}");
    assert_eq!(results(dir.path()), ["allow", "deny"], "{screen}");
}

/// `a` allows this call and every later call of that tool in this
/// process, without asking again.
#[test]
fn terminal_agent_always_answers_the_rest_of_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let replay = asking(dir.path(), 2);
    let Some(mut term) = agent_on_terminal(dir.path(), &replay) else {
        return;
    };
    term.wait_for("", r#"allow bash {"command":"echo 1"}? [y/N/a(lways)] "#);
    term.type_keys(b"a\r");
    let (status, screen) = term.exit();
    assert_eq!(status.code(), Some(0), "{screen}");
    assert_eq!(results(dir.path()), ["allow", "allow"], "{screen}");
    assert_eq!(screen.matches("[y/N/a(lways)]").count(), 1, "{screen}");
}

/// `agent ping 2> errors.txt`: stdin is still the terminal, so the gate
/// asks, and the answer is read from /dev/tty. The question has to be on
/// that same terminal: an answer typed there approves the name and input
/// the question shows, and a question that went to a file showed the person
/// answering nothing. Security-audit run 4 (lead 4) typed `yes` on such a
/// terminal and the call was allowed, the prompt only in the file. The
/// script asks with `read -p`, on stderr too, and fails this the same way.
#[test]
fn terminal_agent_asks_on_the_terminal_with_stderr_redirected() {
    let dir = tempfile::tempdir().unwrap();
    let replay = asking(dir.path(), 1);
    let errors = dir.path().join("errors.txt");
    let Some(mut term) = Terminal::start_redirected(
        dir.path(),
        &["agent", "ping"],
        None,
        Some(&errors),
        &[
            ("ASK_CLAUDE_REPLAY", replay.as_os_str()),
            ("ASK_CLAUDE_AGENT", "agent_FIXTURE".as_ref()),
            ("ASK_CLAUDE_ENVIRONMENT", "env_FIXTURE".as_ref()),
        ],
    ) else {
        return;
    };
    term.wait_for("", r#"allow bash {"command":"echo 1"}? [y/N/a(lways)] "#);
    term.type_keys(b"y\r");
    let (status, screen) = term.exit();
    assert_eq!(status.code(), Some(0), "{screen}");
    assert_eq!(results(dir.path()), ["allow"], "{screen}");
}

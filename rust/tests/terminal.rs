//! The chat loop on a terminal (R5, KTD5): line editing where one paste is
//! one turn, Ctrl-C exits 130 without the epilogue, Ctrl-D runs it, and no
//! history file is written. The bats suite has no terminal, so these drive
//! the binary through util-linux `script`, which gives it a pty and passes
//! the bytes written here as typed keys. Linux only, and skipped where
//! `script` is missing; on macOS they are the manual check `just pty-test`
//! names.
//!
//! `ASK_CLAUDE_BIN=<path>` runs them against another binary. The script
//! fails the paste test by design: `read -e` keeps only the first line of
//! a paste and drops the rest, the bug this branch exists to fix.

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

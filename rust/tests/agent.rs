//! `ask-claude agent` through the replay seam where bats cannot follow:
//! Ctrl-C in the middle of a session turn (R6, plan U6). The bats suite
//! (`tests/unit/agent*.bats`) holds every other session scenario; these
//! are the rust-only ones, since the bash script exits 1 after an
//! interrupted one shot where the plan asks for 130.
//!
//! A replayed stream that never ends is a FIFO in the replay directory:
//! the test writes the first events and keeps its end open, so the reader
//! blocks after them the way a live stream waits for the next event. One
//! nobody writes to blocks the reader before the stream has even opened.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use assert_cmd::prelude::*;

const INTERRUPT: &str = r#"{"events":[{"type":"user.interrupt"}]}"#;

/// The first events of a turn: connected, running, and the start of a reply.
const FIRST: &str = concat!(
    ": connected\n\n",
    "event: message\n",
    r#"data: {"id":"sevt_T1","type":"session.status_running"}"#,
    "\n\n",
    "event: message\n",
    r#"data: {"content":[{"text":"po","type":"text"}],"id":"sevt_T2","type":"agent.message"}"#,
    "\n\n",
);

/// `<dir>/<n>.sse` as a FIFO.
fn fifo(dir: &Path, n: u32) -> PathBuf {
    let path = dir.join(format!("{n}.sse"));
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: mkfifo(3) with a NUL-terminated path this test owns
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
    path
}

/// Open the FIFO for writing once the binary opens it for reading, write
/// `first`, and keep it open until the returned sender sends or is dropped.
fn feed(path: PathBuf, first: &'static str) -> mpsc::Sender<()> {
    let (done, wait) = mpsc::channel::<()>();
    thread::spawn(move || {
        let mut w = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        w.write_all(first.as_bytes()).unwrap();
        w.flush().unwrap();
        let _ = wait.recv();
    });
    done
}

/// The binary on the replay directory `dir` with the request log, `args`
/// after `agent`, its own process group (so a signal reaches it as Ctrl-C
/// reaches the foreground job), and stdout read by a thread that hands
/// over everything printed so far.
fn spawn(
    dir: &Path,
    state: &Path,
    args: &[&str],
    stdin: Option<&str>,
) -> (Child, mpsc::Receiver<Vec<u8>>) {
    let mut cmd = Command::cargo_bin("ask-claude").expect("the binary builds");
    for (name, _) in std::env::vars_os() {
        let n = name.to_string_lossy();
        if ["ASK_CLAUDE_", "ANTHROPIC_", "OPENAI_"]
            .iter()
            .any(|p| n.starts_with(p))
        {
            cmd.env_remove(&name);
        }
    }
    cmd.env("ANTHROPIC_API_KEY", "sk-ant-test-not-a-key")
        .env("ASK_CLAUDE_AGENT", "agent_FIXTURE")
        .env("ASK_CLAUDE_ENVIRONMENT", "env_FIXTURE")
        .env("ASK_CLAUDE_REPLAY", dir)
        .env("ASK_CLAUDE_REQUEST_LOG", state.join("requests.jsonl"))
        .env("XDG_STATE_HOME", state.join("state"))
        .arg("agent")
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let mut child = cmd.spawn().unwrap();
    if let Some(text) = stdin {
        // kept open: the loop is still reading lines when the signal comes
        let mut pipe = child.stdin.take().unwrap();
        pipe.write_all(text.as_bytes()).unwrap();
        std::mem::forget(pipe);
    }
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
    (child, rx)
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

/// Wait up to `limit` for `path` to hold `lines` lines.
fn logged(path: &Path, lines: usize, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if std::fs::read_to_string(path).is_ok_and(|s| s.lines().count() >= lines) {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    false
}

/// SIGINT to the child's group, then its exit status and stderr, or a
/// panic when it still runs a second later.
fn interrupt(mut child: Child) -> (std::process::ExitStatus, String) {
    let pid = libc::pid_t::try_from(child.id()).unwrap();
    // SAFETY: kill(2) on the process group the child leads; no memory involved
    assert_eq!(unsafe { libc::kill(-pid, libc::SIGINT) }, 0);
    let sent = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if sent.elapsed() > Duration::from_secs(1) {
            let _ = child.kill();
            panic!("still running a second after Ctrl-C");
        }
        thread::sleep(Duration::from_millis(10));
    };
    let mut err = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    (status, err)
}

fn requests(state: &Path) -> Vec<String> {
    std::fs::read_to_string(state.join("requests.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// Ctrl-C while the stream waits for its next event: `user.interrupt` is
/// posted, in the request log after the create body, and the exit is 130.
#[test]
fn agent_ctrl_c_while_the_replay_blocks_after_the_first_event_interrupts_and_exits_130() {
    let dir = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let done = feed(fifo(dir.path(), 1), FIRST);
    let (child, stdout) = spawn(dir.path(), state.path(), &["ping"], None);
    assert!(
        printed(&stdout, "po", Duration::from_secs(5)),
        "the reply never started"
    );
    let (status, err) = interrupt(child);
    drop(done);
    assert_eq!(status.code(), Some(130), "{err}");
    assert!(
        err.contains("[interrupted: sending user.interrupt]"),
        "{err}"
    );
    let reqs = requests(state.path());
    assert_eq!(reqs.len(), 2, "{reqs:?}");
    assert!(reqs[0].contains("initial_events"), "{reqs:?}");
    assert_eq!(reqs[1], INTERRUPT);
    assert!(
        !state.path().join("state/ask-claude/sessions").exists()
            || std::fs::read_dir(state.path().join("state/ask-claude/sessions"))
                .unwrap()
                .next()
                .is_none(),
        "an interrupted one shot is not remembered"
    );
}

/// Ctrl-C before the stream has opened at all (the replay file is a FIFO
/// nobody writes to, so its open blocks on the reader thread): the main
/// thread still sees the signal, posts `user.interrupt` and exits 130.
#[test]
fn agent_ctrl_c_before_the_replay_stream_opens_interrupts_and_exits_130() {
    let dir = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    fifo(dir.path(), 1);
    let (child, _stdout) = spawn(dir.path(), state.path(), &["ping"], None);
    let log = state.path().join("requests.jsonl");
    assert!(
        logged(&log, 1, Duration::from_secs(5)),
        "the session was never created"
    );
    thread::sleep(Duration::from_millis(200));
    let (status, err) = interrupt(child);
    assert_eq!(status.code(), Some(130), "{err}");
    assert_eq!(
        requests(state.path()).last().map(String::as_str),
        Some(INTERRUPT)
    );
}

/// Ctrl-C mid-turn in a resumed session: the line went out as a
/// `user.message`, then `user.interrupt` follows it, and the exit is 130
/// with no epilogue.
#[test]
fn agent_ctrl_c_mid_turn_of_a_resumed_session_interrupts_after_the_message() {
    let dir = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let done = feed(fifo(dir.path(), 1), FIRST);
    let (child, stdout) = spawn(
        dir.path(),
        state.path(),
        &["resume", "sesn_FIXTURE01"],
        Some("again\n"),
    );
    assert!(
        printed(&stdout, "po", Duration::from_secs(5)),
        "the reply never started"
    );
    let (status, err) = interrupt(child);
    drop(done);
    assert_eq!(status.code(), Some(130), "{err}");
    assert!(!err.contains("resume with"), "{err}");
    let reqs = requests(state.path());
    assert_eq!(reqs.len(), 2, "{reqs:?}");
    assert_eq!(
        reqs[0],
        r#"{"events":[{"type":"user.message","content":[{"type":"text","text":"again"}]}]}"#
    );
    assert_eq!(reqs[1], INTERRUPT);
}

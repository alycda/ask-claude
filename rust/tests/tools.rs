//! U10: the Tool runner in process (KTD9, KTD10): what a bash command may
//! reach and how long it may run, what an editor command may touch, what a
//! result may carry back, and when the gate asks. Red on the scaffold's
//! `todo!()` until U10's feat commit; `src/tools.rs` names the shape.
//!
//! Each test gets its own repository, a sibling directory outside it, and a
//! state directory, under one temp dir. The child's environment, the
//! secrets, the bounds and the interrupt come in through `Config`, so no
//! test touches this process's own.
//!
//! Driving the gate through the binary (piped stdin, two chat lines around
//! a command that reads stdin) needs `--tools`, which U11 adds; those
//! scenarios are `tests/unit/tools.bats`.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use ask_claude::session::Gate;
use ask_claude::tools::{BASH, Config, EDITOR, Limits, Outcome, Runner};
use serde_json::{Value, json};
use tempfile::TempDir;

/// A key this process holds; it must never come back in a result.
const KEY: &str = "sk-ant-test-runner-secret";

/// One test's directories: `root` is the repository, `outside` a sibling
/// holding `secret.txt`, `state` the state directory, `stop` the interrupt.
struct Dirs {
    _tmp: TempDir,
    root: PathBuf,
    outside: PathBuf,
    state: PathBuf,
    stop: Arc<AtomicBool>,
}

fn dirs() -> Dirs {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let (root, outside, state) = (base.join("repo"), base.join("outside"), base.join("state"));
    for d in [&root, &outside, &state] {
        fs::create_dir(d).unwrap();
    }
    fs::write(outside.join("secret.txt"), "OUTSIDE-CONTENT\n").unwrap();
    Dirs {
        _tmp: tmp,
        root,
        outside,
        state,
        stop: Arc::new(AtomicBool::new(false)),
    }
}

impl Dirs {
    /// A config with `PATH` and the key in the parent environment, the key
    /// as a secret, `$HOME` outside the root, and short bounds.
    fn config(&self) -> Config {
        let stop = Arc::clone(&self.stop);
        Config {
            root: self.root.clone(),
            home: self.outside.clone(),
            state: self.state.clone(),
            env: vec![
                ("PATH".into(), std::env::var("PATH").unwrap_or_default()),
                ("ANTHROPIC_API_KEY".into(), KEY.into()),
            ],
            secrets: vec![KEY.into()],
            protected: vec![],
            limits: Limits {
                timeout: Duration::from_secs(10),
                grace: Duration::from_millis(200),
                cap: 30_000,
            },
            interrupted: Arc::new(move || stop.load(Ordering::SeqCst)),
        }
    }

    fn runner(&self) -> Runner {
        Runner::new(self.config()).unwrap()
    }
}

/// `call` under `ASK_CLAUDE_CONFIRM=allow` with no terminal.
fn allowed(r: &mut Runner, name: &str, input: &Value) -> Outcome {
    let mut gate = Gate::new(Some("allow")).unwrap();
    r.run(
        name,
        input,
        &mut gate,
        false,
        &mut |_| None,
        &mut std::io::sink(),
    )
}

fn bash(command: &str) -> Value {
    json!({ "command": command })
}

fn view(path: &Path) -> Value {
    json!({ "command": "view", "path": path.to_string_lossy() })
}

fn create(path: &Path, text: &str) -> Value {
    json!({ "command": "create", "path": path.to_string_lossy(), "file_text": text })
}

fn replace(path: &Path, old: &str, new: &str) -> Value {
    json!({ "command": "str_replace", "path": path.to_string_lossy(), "old_str": old, "new_str": new })
}

/// Whether a process with this pid still exists.
fn alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid.trim()])
        .status()
        .is_ok_and(|s| s.success())
}

/// Wait up to `limit` for `pid` to be gone; whether it went.
fn gone_within(pid: &str, limit: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if !alive(pid) {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    !alive(pid)
}

/// A command that starts a grandchild sleeping, writes its pid to `pid` in
/// the root, and waits on it: a group kill has to reach past the shell.
const SLEEPER: &str = "sleep 30 & echo $! > pid; wait";

fn entries(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn echo_pong_returns_pong_and_the_child_never_sees_the_key() {
    let d = dirs();
    let out = allowed(&mut d.runner(), BASH, &bash("echo pong; env"));
    assert!(!out.is_error, "{out:?}");
    assert!(out.content.lines().any(|l| l == "pong"), "{out:?}");
    assert!(!out.content.contains("ANTHROPIC_API_KEY"), "{out:?}");
}

#[test]
fn a_key_a_command_prints_comes_back_redacted() {
    let d = dirs();
    fs::write(d.root.join("leak.txt"), KEY).unwrap();
    let out = allowed(&mut d.runner(), BASH, &bash("cat leak.txt"));
    assert!(out.content.contains("[redacted]"), "{out:?}");
    assert!(!out.content.contains(KEY), "{out:?}");
}

#[test]
fn a_command_that_reads_stdin_gets_nothing_and_the_next_line_runs() {
    let d = dirs();
    let start = Instant::now();
    let out = allowed(&mut d.runner(), BASH, &bash("cat\necho after"));
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "cat waited on stdin"
    );
    assert!(!out.is_error, "{out:?}");
    assert_eq!(out.content.trim(), "after");
}

#[test]
fn bash_runs_in_the_repository_root() {
    let d = dirs();
    let out = allowed(&mut d.runner(), BASH, &bash("pwd -P"));
    assert_eq!(out.content.trim(), d.root.to_string_lossy());
}

#[test]
fn view_outside_the_root_is_refused_without_the_content() {
    let d = dirs();
    let secret = d.outside.join("secret.txt");
    symlink(&secret, d.root.join("link")).unwrap();
    let mut r = d.runner();
    for path in [
        PathBuf::from("../outside/secret.txt"),
        d.root.join("../outside/secret.txt"),
        secret.clone(),
        d.root.join("link"),
    ] {
        let out = allowed(&mut r, EDITOR, &view(&path));
        assert!(out.is_error, "{}: {out:?}", path.display());
        assert!(
            !out.content.contains("OUTSIDE-CONTENT"),
            "{}: {out:?}",
            path.display()
        );
    }
}

#[test]
fn a_command_past_the_timeout_is_stopped_with_its_whole_group() {
    let d = dirs();
    let mut config = d.config();
    config.limits.timeout = Duration::from_secs(1);
    let mut r = Runner::new(config).unwrap();
    let start = Instant::now();
    let out = allowed(&mut r, BASH, &bash(SLEEPER));
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "the timeout did not fire"
    );
    assert!(out.is_error, "{out:?}");
    let pid = fs::read_to_string(d.root.join("pid")).unwrap();
    assert!(
        gone_within(&pid, Duration::from_secs(3)),
        "the grandchild survived"
    );
}

#[test]
fn a_flood_is_cut_to_its_head_and_tail() {
    let d = dirs();
    let mut config = d.config();
    config.limits.cap = 1000;
    let mut r = Runner::new(config).unwrap();
    let out = allowed(
        &mut r,
        BASH,
        &bash("echo HEAD-MARKER; yes filler | head -c 5000000; echo; echo TAIL-MARKER"),
    );
    assert!(out.content.len() < 1500, "{} bytes kept", out.content.len());
    assert!(out.content.contains("HEAD-MARKER"), "{out:?}");
    assert!(out.content.contains("TAIL-MARKER"), "{out:?}");
}

#[test]
fn an_interrupt_while_a_command_sleeps_leaves_no_process_and_no_temp_file() {
    let d = dirs();
    let mut r = d.runner();
    let (pid_file, stop) = (d.root.join("pid"), Arc::clone(&d.stop));
    let interrupter = thread::spawn(move || {
        let start = Instant::now();
        while !pid_file.exists() && start.elapsed() < Duration::from_secs(5) {
            thread::sleep(Duration::from_millis(20));
        }
        stop.store(true, Ordering::SeqCst);
    });
    let start = Instant::now();
    let out = allowed(&mut r, BASH, &bash(SLEEPER));
    interrupter.join().unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(8),
        "the interrupt was not seen"
    );
    assert!(out.is_error, "{out:?}");
    let pid = fs::read_to_string(d.root.join("pid")).unwrap();
    assert!(
        gone_within(&pid, Duration::from_secs(3)),
        "the grandchild survived"
    );
    assert_eq!(
        entries(&d.root),
        ["pid"],
        "a temp file was left in the root"
    );
    assert!(
        entries(&d.state).is_empty(),
        "a temp file was left in the state directory"
    );
}

#[test]
fn create_outside_the_root_through_a_symlink_or_over_a_file_is_refused() {
    let d = dirs();
    fs::write(d.root.join("exists.txt"), "ORIGINAL").unwrap();
    symlink(d.outside.join("target.txt"), d.root.join("dangling")).unwrap();
    let mut r = d.runner();
    for path in [
        PathBuf::from("../outside/new.txt"),
        d.root.join("../outside/new.txt"),
        d.root.join("dangling"),
        d.root.join("exists.txt"),
    ] {
        let out = allowed(&mut r, EDITOR, &create(&path, "NEW"));
        assert!(out.is_error, "{}: {out:?}", path.display());
    }
    assert_eq!(
        fs::read_to_string(d.root.join("exists.txt")).unwrap(),
        "ORIGINAL"
    );
    assert!(!d.outside.join("new.txt").exists());
    assert!(!d.outside.join("target.txt").exists());
}

#[test]
fn str_replace_refuses_two_matches_and_rewrites_one_keeping_the_mode() {
    let d = dirs();
    let file = d.root.join("f.txt");
    fs::write(&file, "aa\nbb\naa\n").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
    let mut r = d.runner();

    let out = allowed(&mut r, EDITOR, &replace(&file, "aa", "cc"));
    assert!(out.is_error, "{out:?}");
    assert_eq!(fs::read_to_string(&file).unwrap(), "aa\nbb\naa\n");

    let out = allowed(&mut r, EDITOR, &replace(&file, "bb", "dd"));
    assert!(!out.is_error, "{out:?}");
    assert_eq!(fs::read_to_string(&file).unwrap(), "aa\ndd\naa\n");
    let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o640, "the mode was {mode:o}");
}

#[test]
fn view_of_a_directory_lists_two_levels_and_no_hidden_entry() {
    let d = dirs();
    fs::create_dir_all(d.root.join("lvl-one/lvl-two/lvl-three")).unwrap();
    fs::create_dir(d.root.join(".lvl-hidden")).unwrap();
    let out = allowed(&mut d.runner(), EDITOR, &view(&d.root));
    assert!(!out.is_error, "{out:?}");
    assert!(out.content.contains("lvl-one"), "{out:?}");
    assert!(out.content.contains("lvl-two"), "{out:?}");
    assert!(!out.content.contains("lvl-three"), "{out:?}");
    assert!(!out.content.contains("lvl-hidden"), "{out:?}");
}

#[test]
fn a_write_to_a_chat_under_the_root_prompts_under_always_and_is_denied_unattended() {
    let d = dirs();
    let state = d.root.join("state");
    fs::create_dir(&state).unwrap();
    let chat = state.join("2026-11-15T00-00-00.json");
    fs::write(&chat, "[]").unwrap();
    let mut config = d.config();
    config.state = state;
    let mut r = Runner::new(config).unwrap();

    let mut gate = Gate::new(None).unwrap();
    let mut asked = 0;
    let mut answers = ["a", "n"].into_iter();
    let mut ask = |_: &str| {
        asked += 1;
        answers.next().map(str::to_string)
    };
    let ok = r.run(
        EDITOR,
        &create(&d.root.join("ok.txt"), "x"),
        &mut gate,
        true,
        &mut ask,
        &mut std::io::sink(),
    );
    assert!(!ok.is_error, "{ok:?}");
    let out = r.run(
        EDITOR,
        &replace(&chat, "[]", "[1]"),
        &mut gate,
        true,
        &mut ask,
        &mut std::io::sink(),
    );
    assert_eq!(asked, 2, "the chat write did not prompt under always");
    assert!(out.is_error, "{out:?}");
    assert_eq!(fs::read_to_string(&chat).unwrap(), "[]");

    let out = allowed(&mut r, EDITOR, &replace(&chat, "[]", "[1]"));
    assert!(out.is_error, "{out:?}");
    assert_eq!(fs::read_to_string(&chat).unwrap(), "[]");
}

#[test]
fn with_no_terminal_and_no_policy_a_call_is_denied_and_nothing_starts() {
    let d = dirs();
    let mut gate = Gate::new(None).unwrap();
    let mut err = Vec::new();
    let out = d.runner().run(
        BASH,
        &bash("touch ran"),
        &mut gate,
        false,
        &mut |_| None,
        &mut err,
    );
    assert!(out.is_error, "{out:?}");
    assert!(!d.root.join("ran").exists(), "the command ran");
    let err = String::from_utf8_lossy(&err);
    assert!(err.contains("stdin is not a terminal"), "{err}");
}

#[test]
fn always_for_bash_warns_once_and_covers_neither_the_editor_nor_a_protected_path() {
    let d = dirs();
    let plugin = d.root.join("plugins/p");
    fs::create_dir_all(&plugin).unwrap();
    let mut config = d.config();
    config.protected = vec![plugin.clone()];
    let mut r = Runner::new(config).unwrap();
    let mut gate = Gate::new(None).unwrap();
    let mut err = Vec::new();
    let mut prompts: Vec<String> = Vec::new();
    let mut answers = ["a", "a", "n", "n"].into_iter();
    let mut ask = |p: &str| {
        prompts.push(p.to_string());
        answers.next().map(str::to_string)
    };

    for command in ["echo one", "echo two"] {
        let out = r.run(BASH, &bash(command), &mut gate, true, &mut ask, &mut err);
        assert!(!out.is_error, "{out:?}");
    }
    let warned = String::from_utf8_lossy(&err).lines().count();
    assert_eq!(warned, 1, "{}", String::from_utf8_lossy(&err));

    let out = r.run(
        EDITOR,
        &create(&d.root.join("new.txt"), "x"),
        &mut gate,
        true,
        &mut ask,
        &mut err,
    );
    assert!(!out.is_error, "{out:?}");
    let out = r.run(
        EDITOR,
        &create(&d.root.join("Cargo.toml"), "x"),
        &mut gate,
        true,
        &mut ask,
        &mut err,
    );
    assert!(out.is_error, "{out:?}");
    assert!(!d.root.join("Cargo.toml").exists());
    let out = r.run(
        EDITOR,
        &create(&plugin.join("x.txt"), "x"),
        &mut gate,
        true,
        &mut ask,
        &mut err,
    );
    assert!(out.is_error, "{out:?}");
    assert!(!plugin.join("x.txt").exists());
    drop(ask);
    assert_eq!(prompts.len(), 4, "{prompts:?}");
}

#[test]
fn a_root_at_home_or_at_slash_is_refused() {
    let d = dirs();
    let mut config = d.config();
    config.root = d.outside.clone();
    assert!(Runner::new(config).is_err());
    let mut config = d.config();
    config.root = PathBuf::from("/");
    assert!(Runner::new(config).is_err());
}

#[test]
fn restart_is_answered_without_running_anything() {
    let d = dirs();
    let out = allowed(&mut d.runner(), BASH, &json!({ "restart": true }));
    assert_eq!(out.content, "Bash session restarted");
    assert!(!out.is_error);
}

#[test]
fn without_bash_on_path_the_call_is_an_error_not_a_dialect_change() {
    let d = dirs();
    let mut config = d.config();
    config.env = vec![("PATH".into(), "/nonexistent".into())];
    let out = allowed(&mut Runner::new(config).unwrap(), BASH, &bash("echo x"));
    assert!(out.is_error, "{out:?}");
    assert!(out.content.contains("bash not found"), "{out:?}");
}

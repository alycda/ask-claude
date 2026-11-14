//! The binary: its interface (R0), the bytes at its edges (R1: a stream
//! line is bytes, decoded the way jq decodes it) and the first end-to-end
//! turn through the replay seam (R5). The rest of the behaviour is the bats
//! suite, run against this binary by `just test-rust`.

use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

fn bin() -> Command {
    Command::cargo_bin("ask-claude").expect("the binary builds")
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn r0_usage_spec_is_the_bash_spec() {
    let out = bin().arg("--usage-spec").output().unwrap();
    assert!(out.status.success());
    let spec = String::from_utf8(out.stdout).unwrap();
    assert!(spec.contains("bin ask-claude"), "{spec}");
    assert!(spec.contains("cmd agent"), "{spec}");

    // usage diff against the script itself, when the usage CLI is here
    if which::which_usage().is_none() {
        eprintln!("usage not on PATH: the diff is left to `just parity`");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let kdl = dir.path().join("rust.kdl");
    std::fs::write(&kdl, &spec).unwrap();
    let diff = Command::new("usage")
        .arg("diff")
        .arg(repo().join("ask-claude"))
        .arg(&kdl)
        .output()
        .unwrap();
    let report =
        String::from_utf8_lossy(&diff.stdout).to_string() + &String::from_utf8_lossy(&diff.stderr);
    // newer usage says "No interface changes." when the specs agree
    assert!(
        report.contains("Found 0 breaking") || report.contains("No interface changes"),
        "{report}"
    );
}

/// `-V` prints the repository's `VERSION`, the number `just bump` moves and
/// the bats `version:` test reads, not a second copy in `Cargo.toml`; run
/// from a directory with no `.env`, since the binary never reads one.
#[test]
fn r0_version_flag_prints_the_version_file() {
    let version = std::fs::read_to_string(repo().join("VERSION")).unwrap();
    let cwd = tempfile::tempdir().unwrap();
    bin()
        .arg("-V")
        .current_dir(cwd.path())
        .assert()
        .success()
        .stdout(format!("ask-claude {}\n", version.trim()));
}

/// The key comes from the environment only: a `.env` in the cwd holding
/// one is `just`'s business, not the binary's. The message is the bash
/// script's `need_key`, word for word.
#[test]
fn r0_a_dotenv_key_is_not_read() {
    let cwd = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    std::fs::write(
        cwd.path().join(".env"),
        "ANTHROPIC_API_KEY=sk-ant-from-dotenv\n",
    )
    .unwrap();
    bin()
        .arg("ping")
        .current_dir(cwd.path())
        .env_remove("ANTHROPIC_API_KEY")
        .env("XDG_STATE_HOME", state.path())
        .env(
            "ASK_CLAUDE_REPLAY",
            repo().join("tests/fixtures/sse/text.sse"),
        )
        .assert()
        .code(2)
        .stdout("")
        .stderr("ask-claude: no key: export ANTHROPIC_API_KEY\n");
}

/// `ask-claude --usage-spec | head -1`: a reader that went away is SIGPIPE,
/// the way curl and jq end, not a Rust panic about a broken pipe (KTD4).
/// The pipe's read end is closed before the binary writes, so the write
/// always raises the signal; a binary that ignored it and exited 0 would
/// have swallowed the broken pipe, so only the signal passes.
#[cfg(unix)]
#[test]
fn r0_a_closed_stdout_is_sigpipe_not_a_panic() {
    use std::os::unix::process::ExitStatusExt;
    for flag in ["--usage-spec", "-V"] {
        let (reader, writer) = std::io::pipe().unwrap();
        drop(reader);
        let out = bin().arg(flag).stdout(writer).output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr.contains("panicked") && !stderr.contains("Broken pipe"),
            "{flag}: {stderr}"
        );
        assert_eq!(
            out.status.signal(),
            Some(libc::SIGPIPE),
            "{flag}: {:?}",
            out.status
        );
    }
}

/// `umask 077` at the top, as the script has it: a file the binary creates
/// is private to the user whatever the umask it was started under. The
/// request log is opened with no mode of its own, so only the umask keeps
/// it from 0644 under a shell's usual 022. Oracle: `umask 022;
/// ASK_CLAUDE_REQUEST_LOG=<new file> ./ask-claude ping` leaves it 0600.
#[cfg(unix)]
#[test]
fn r0_a_file_it_creates_is_private_whatever_the_umask() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("requests.jsonl");
    Command::new("sh")
        .arg("-c")
        .arg(r#"umask 022 && exec "$0" ping"#)
        .arg(bin().get_program())
        .env("ANTHROPIC_API_KEY", "sk-ant-test-not-a-key")
        .env("XDG_STATE_HOME", dir.path().join("state"))
        .env(
            "ASK_CLAUDE_REPLAY",
            repo().join("tests/fixtures/sse/text.sse"),
        )
        .env("ASK_CLAUDE_REQUEST_LOG", &log)
        .assert()
        .success()
        .stdout("pong\n");
    let mode = std::fs::metadata(&log).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "{mode:o}");
}

#[test]
fn r0_unknown_flag_is_an_error() {
    bin()
        .arg("--nope")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--nope"));
}

/// R5: the one shot through `ASK_CLAUDE_REPLAY`, the way `tests/unit/oneshot.bats`
/// drives the bash script. Green means `just test-rust` is worth running.
#[test]
fn r5_one_shot_through_replay_prints_pong() {
    let state = tempfile::tempdir().unwrap();
    bin()
        .arg("ping")
        .env("ANTHROPIC_API_KEY", "sk-ant-test-not-a-key")
        .env("XDG_STATE_HOME", state.path())
        .env(
            "ASK_CLAUDE_REPLAY",
            repo().join("tests/fixtures/sse/text.sse"),
        )
        .assert()
        .success()
        .stdout("pong\n")
        .stderr(
            predicate::str::is_match(
                r"\[claude-opus-5-5 in=\d+ cache_read=\d+ cache_write=\d+ out=\d+ \d+s\]",
            )
            .unwrap(),
        );
}

/// A stream line with invalid UTF-8 prints what the bash script prints: jq
/// replaces each bad sequence with U+FFFD as it parses the line, and its
/// rule is not Rust's `from_utf8_lossy`. An overlong form, a surrogate and a
/// code point past U+10FFFF are one U+FFFD each where Rust gives one per
/// byte, and a lead byte that the string ends before its sequence could
/// takes the rest of the string with it (the `g`).
///
/// The expected bytes are the oracle's, not this binary's:
/// `ASK_CLAUDE_REPLAY=<this stream> ./ask-claude ping` with jq 1.8.2, the same
/// as `printf '<the delta line>' | jq -rj -f lib/stream.jq`.
#[test]
fn r1_invalid_utf8_in_a_line_is_replaced_the_way_jq_replaces_it() {
    let text_sse = std::fs::read(repo().join("tests/fixtures/sse/text.sse")).unwrap();
    let pong = b"\"text\":\"pong\"";
    let at = text_sse
        .windows(pong.len())
        .position(|w| w == pong)
        .expect("text.sse carries the pong delta");
    let mut crafted = text_sse[..at].to_vec();
    crafted.extend_from_slice(
        b"\"text\":\"a\xffb\xe0\x80\x80c\xed\xa0\x80d\xf4\x90\x80\x80e\xe2\x82Af\xf0g\"",
    );
    crafted.extend_from_slice(&text_sse[at + pong.len()..]);
    let dir = tempfile::tempdir().unwrap();
    let sse = dir.path().join("crafted.sse");
    std::fs::write(&sse, &crafted).unwrap();

    bin()
        .arg("ping")
        .env("ANTHROPIC_API_KEY", "sk-ant-test-not-a-key")
        .env("XDG_STATE_HOME", dir.path())
        .env("ASK_CLAUDE_REPLAY", &sse)
        .assert()
        .success()
        .stdout("a\u{FFFD}b\u{FFFD}c\u{FFFD}d\u{FFFD}e\u{FFFD}Af\u{FFFD}\n");
}

/// jq replaces after it has undone the escapes, so "the rest of the string"
/// is counted in decoded bytes: `x\xf0é` is x, F0, C3 A9, three bytes
/// where F0 promises four, and é goes with the F0. Replacing on the raw
/// line instead would see six bytes, a `\` where a continuation should be,
/// and keep the é. Oracle as above: the bash script on this stream prints
/// `x` U+FFFD.
#[test]
fn r1_invalid_utf8_is_replaced_after_the_escapes_are_undone() {
    let text_sse = std::fs::read(repo().join("tests/fixtures/sse/text.sse")).unwrap();
    let pong = b"\"text\":\"pong\"";
    let at = text_sse
        .windows(pong.len())
        .position(|w| w == pong)
        .expect("text.sse carries the pong delta");
    let mut crafted = text_sse[..at].to_vec();
    crafted.extend_from_slice(b"\"text\":\"x\xf0\\u00e9\"");
    crafted.extend_from_slice(&text_sse[at + pong.len()..]);
    let dir = tempfile::tempdir().unwrap();
    let sse = dir.path().join("crafted.sse");
    std::fs::write(&sse, &crafted).unwrap();

    bin()
        .arg("ping")
        .env("ANTHROPIC_API_KEY", "sk-ant-test-not-a-key")
        .env("XDG_STATE_HOME", dir.path())
        .env("ASK_CLAUDE_REPLAY", &sse)
        .assert()
        .success()
        .stdout("x\u{FFFD}\n");
}

/// A tool input that is not JSON when its block stops is a reply jq cannot
/// fold, and `call` in lib/messages.bash says so as an API error and
/// exits 1, under `max_tokens` and `end_turn` alike: no usage line, no
/// stop notice, only the newline the printer gave `message_stop`.
/// Oracle: `ASK_CLAUDE_REPLAY=<this stream> ./ask-claude ping` prints
/// "\n", ends stderr with the line below (after jq's own complaint) and
/// exits 1.
#[test]
fn r2_a_tool_input_that_is_not_json_is_the_bash_parse_error() {
    for stop_reason in ["max_tokens", "end_turn"] {
        let dir = tempfile::tempdir().unwrap();
        let sse = dir.path().join("cut.sse");
        let events = [
            r#"{"type":"message_start","message":{"model":"claude-opus-5-5","usage":{"input_tokens":1,"output_tokens":0}}}"#,
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_FIXTURE01","name":"bash","input":{}}}"#,
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"command\": \"ls"}}"#,
            r#"{"type":"content_block_stop","index":0}"#,
            &format!(
                r#"{{"type":"message_delta","delta":{{"stop_reason":"{stop_reason}","stop_sequence":null}},"usage":{{"output_tokens":3}}}}"#
            ),
            r#"{"type":"message_stop"}"#,
        ];
        let body: String = events.iter().map(|e| format!("data: {e}\n\n")).collect();
        std::fs::write(&sse, body).unwrap();

        bin()
            .arg("ping")
            .env("ANTHROPIC_API_KEY", "sk-ant-test-not-a-key")
            .env("XDG_STATE_HOME", dir.path().join("state"))
            .env("ASK_CLAUDE_REPLAY", &sse)
            .assert()
            .code(1)
            .stdout("\n")
            .stderr(
                predicate::str::ends_with("ask-claude: API error: the reply could not be parsed\n")
                    .and(predicate::str::contains("in=").not())
                    .and(predicate::str::contains("[stopped").not()),
            );
    }
}

/// The one shot the way tests/unit/oneshot.bats drives the script: replay,
/// request log and state directory from the environment.
fn one_shot(state: &Path, replay: &Path) -> Command {
    let mut cmd = bin();
    cmd.env("ANTHROPIC_API_KEY", "sk-ant-test-not-a-key")
        .env("XDG_STATE_HOME", state.join("state"))
        .env("ASK_CLAUDE_REPLAY", replay)
        .env("ASK_CLAUDE_REQUEST_LOG", state.join("requests.jsonl"))
        .env_remove("ASK_CLAUDE_BETA_LOG");
    cmd
}

fn chat_files(state: &Path) -> Vec<PathBuf> {
    let dir = state.join("state/ask-claude");
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|d| {
            d.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// `text.sse` with the pong delta's string replaced by `with` (raw bytes).
fn text_sse_with(dir: &Path, with: &[u8]) -> PathBuf {
    let text_sse = std::fs::read(repo().join("tests/fixtures/sse/text.sse")).unwrap();
    let pong = b"\"text\":\"pong\"";
    let at = text_sse
        .windows(pong.len())
        .position(|w| w == pong)
        .expect("text.sse carries the pong delta");
    let mut crafted = text_sse[..at].to_vec();
    crafted.extend_from_slice(with);
    crafted.extend_from_slice(&text_sse[at + pong.len()..]);
    let sse = dir.join("crafted.sse");
    std::fs::write(&sse, &crafted).unwrap();
    sse
}

/// An API error body exits 1 with the bash message, and the user's turn is
/// not kept: a chat that starts with an unanswered prompt is not a chat.
#[test]
fn an_api_error_exits_1_and_keeps_no_user_turn() {
    let dir = tempfile::tempdir().unwrap();
    one_shot(dir.path(), &repo().join("tests/fixtures/sse/error.sse"))
        .arg("ping")
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "API error: model: claude-does-not-exist",
        ));
    for f in chat_files(dir.path()) {
        let turns: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        assert_eq!(turns.as_array().map(Vec::len), Some(0), "{}", f.display());
    }
}

/// pause_turn is continued at most eight times, each round announced on
/// stderr, then the stop is reported, exactly as bash does.
#[test]
fn pause_turn_is_continued_eight_times_then_reported() {
    let dir = tempfile::tempdir().unwrap();
    let out = one_shot(
        dir.path(),
        &repo().join("tests/fixtures/sse/pause-turn.sse"),
    )
    .arg("ping")
    .output()
    .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    let rounds = stderr.matches("[pause_turn: continuing, round ").count();
    assert_eq!(rounds, 8, "{stderr}");
    assert!(
        stderr.contains("[pause_turn: continuing, round 1]"),
        "{stderr}"
    );
    assert!(stderr.contains("[stopped: pause_turn]"), "{stderr}");
}

/// A replay directory serves 1.sse, 2.sse, ... one per send, continuations
/// included; when the turns run out the next send is the bash "no response"
/// path, exit 1. The request log holds one line per send, in order.
#[test]
fn a_replay_directory_serves_one_file_per_send_then_no_response() {
    let dir = tempfile::tempdir().unwrap();
    let turns = dir.path().join("turns");
    std::fs::create_dir(&turns).unwrap();
    std::fs::copy(
        repo().join("tests/fixtures/sse/pause-turn.sse"),
        turns.join("1.sse"),
    )
    .unwrap();
    std::fs::copy(
        repo().join("tests/fixtures/sse/text.sse"),
        turns.join("2.sse"),
    )
    .unwrap();
    one_shot(dir.path(), &turns)
        .arg("ping")
        .assert()
        .success()
        .stdout(predicate::str::ends_with("pong\n"))
        .stderr(predicate::str::contains(
            "[pause_turn: continuing, round 1]",
        ));
    let log = std::fs::read_to_string(dir.path().join("requests.jsonl")).unwrap();
    assert_eq!(log.lines().count(), 2, "{log}");
    for line in log.lines() {
        let body: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(body["messages"][0]["content"], "ping");
    }

    let short = dir.path().join("short");
    std::fs::create_dir(&short).unwrap();
    std::fs::copy(
        repo().join("tests/fixtures/sse/pause-turn.sse"),
        short.join("1.sse"),
    )
    .unwrap();
    one_shot(dir.path(), &short)
        .arg("ping")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("no response"));
}

/// A request log another account can read is refused before anything is
/// sent, with the shape of the state-directory message: a log is request
/// text at rest, as private as a chat.
#[test]
fn a_group_readable_request_log_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("requests.jsonl");
    std::fs::write(&log, "").unwrap();
    std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o640)).unwrap();
    one_shot(dir.path(), &repo().join("tests/fixtures/sse/text.sse"))
        .arg("ping")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("other accounts"));
    assert_eq!(
        std::fs::read_to_string(&log).unwrap(),
        "",
        "nothing was appended"
    );
    assert!(chat_files(dir.path()).is_empty(), "no chat was written");
}

/// The chat a one shot leaves: one file under the state directory, named
/// by its start time and the process id as `new_chat` in lib/chat.bash
/// names it, holding the user turn and the reply, private to the user
/// (mode 0600 in a 0700 directory) as the script's `umask 077` makes it.
#[test]
fn a_one_shot_saves_a_private_chat_of_two_turns() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    one_shot(dir.path(), &repo().join("tests/fixtures/sse/text.sse"))
        .arg("ping")
        .assert()
        .success()
        .stdout("pong\n");
    let chats = chat_files(dir.path());
    assert_eq!(chats.len(), 1, "{chats:?}");
    let name = chats[0].file_name().unwrap().to_string_lossy().into_owned();
    let (stamp, rest) = name.split_once('-').expect("<time>-<pid>.json");
    assert!(
        stamp.len() == 15
            && stamp.as_bytes()[8] == b'T'
            && stamp.bytes().filter(u8::is_ascii_digit).count() == 14,
        "{name}"
    );
    let pid = rest.strip_suffix(".json").expect(".json");
    assert!(pid.parse::<u32>().is_ok(), "{name}");
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&chats[0]), 0o600, "{name}");
    assert_eq!(mode(&dir.path().join("state/ask-claude")), 0o700);
    let turns: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&chats[0]).unwrap()).unwrap();
    assert_eq!(
        turns[0],
        serde_json::json!({"role": "user", "content": "ping"})
    );
    assert_eq!(turns[1]["role"], "assistant");
    assert_eq!(turns[1]["content"][0]["text"], "pong");
    assert_eq!(turns.as_array().map(Vec::len), Some(2));
    let leftovers: Vec<_> = std::fs::read_dir(dir.path().join("state/ask-claude"))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.file_name()))
        .filter(|n| n.to_string_lossy().contains("tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

/// The request log line is written before the send, so it is there even
/// when the send fails: a replay file that does not exist is the bash "no
/// response" path, exit 1, the user turn rolled back and the empty chat
/// removed.
#[test]
fn the_request_log_line_is_written_before_the_send() {
    let dir = tempfile::tempdir().unwrap();
    one_shot(dir.path(), &dir.path().join("missing.sse"))
        .arg("ping")
        .assert()
        .code(1)
        .stdout("")
        .stderr(predicate::str::contains("no response"));
    let log = std::fs::read_to_string(dir.path().join("requests.jsonl")).unwrap();
    assert_eq!(log.lines().count(), 1, "{log}");
    let body: serde_json::Value = serde_json::from_str(log.trim_end()).unwrap();
    assert_eq!(body["messages"][0]["content"], "ping");
    assert!(chat_files(dir.path()).is_empty(), "no chat is left");
}

/// A `data:` line that is not JSON ends the printer where jq's printer
/// dies (on that line, after what came before it) and makes the fold fail
/// as jq -s fails: an API error, exit 1, no usage line. Dropping the line
/// and carrying on, as a lenient parser would, prints a reply bash does
/// not. Oracle: the bash script on this stream prints "pong" (no newline,
/// message_stop never reached the printer) and ends stderr with the line
/// below, after jq's own complaint.
#[test]
fn a_data_line_that_is_not_json_is_the_bash_parse_error() {
    let dir = tempfile::tempdir().unwrap();
    let sse = text_sse_with(
        dir.path(),
        b"\"text\":\"pong\"}}\n\ndata: {not json\n\nx: {",
    );
    one_shot(dir.path(), &sse)
        .arg("ping")
        .assert()
        .code(1)
        .stdout("pong")
        .stderr(
            predicate::str::ends_with("ask-claude: API error: the reply could not be parsed\n")
                .and(predicate::str::contains("in=").not()),
        );
    assert!(chat_files(dir.path()).is_empty(), "no chat is left");
}

/// A lone low surrogate escape in a `data:` line is U+FFFD to jq, which
/// prints and keeps the reply; serde_json alone refuses the line. Oracle:
/// the bash script on this stream prints "p\u{FFFD}ng\n" and exits 0.
#[test]
fn a_lone_low_surrogate_in_a_data_line_is_the_replacement_character() {
    let dir = tempfile::tempdir().unwrap();
    let sse = text_sse_with(dir.path(), b"\"text\":\"p\\udc00ng\"");
    one_shot(dir.path(), &sse)
        .arg("ping")
        .assert()
        .success()
        .stdout("p\u{FFFD}ng\n");
}

/// A body that is not SSE is an HTTP error body: its `.error.message`, or
/// "unreadable response" when it is not JSON or carries none (bash
/// `api_error`). Oracle: the bash script on each body below.
#[test]
fn a_body_that_is_not_sse_is_an_api_error() {
    for body in ["Bad Gateway\n", "{\"type\":\"ok\"}\n"] {
        let dir = tempfile::tempdir().unwrap();
        let sse = dir.path().join("body.sse");
        std::fs::write(&sse, body).unwrap();
        one_shot(dir.path(), &sse)
            .arg("ping")
            .assert()
            .code(1)
            .stdout("")
            .stderr("ask-claude: API error: unreadable response\n");
        assert!(chat_files(dir.path()).is_empty(), "{body}: no chat is left");
    }
}

/// Ctrl-C that comes as an error body ends is still an interrupt (KTD11):
/// exit 130 with the user turn left dangling, as the killed script leaves
/// it, not exit 1 with the turn rolled back. The replay is a FIFO: its one
/// line is read, then the signal goes and the writer closes at once, so
/// the end of the body is what the wait for the next line sees. Against
/// the script, which dies of the signal, this fails by design, as the
/// Ctrl-C test in tests/http.rs does.
#[cfg(unix)]
#[test]
fn ctrl_c_as_an_error_body_ends_exits_130_with_the_turn_dangling() {
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("reply.sse");
    let name = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    // SAFETY: mkfifo(3) on a NUL-terminated path this frame owns
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let mut child = one_shot(dir.path(), &fifo)
        .arg("ping")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // blocks until the binary opens the replay for its send
    let mut writer = std::fs::OpenOptions::new().write(true).open(&fifo).unwrap();
    let mut body = std::fs::read(repo().join("tests/fixtures/sse/error.sse")).unwrap();
    body.push(b'\n');
    writer.write_all(&body).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let pid = libc::pid_t::try_from(child.id()).unwrap();
    // SAFETY: kill(2) on the child's pid; no memory involved
    assert_eq!(unsafe { libc::kill(pid, libc::SIGINT) }, 0);
    drop(writer);
    let status = child.wait().unwrap();
    let mut stderr = String::new();
    std::io::Read::read_to_string(&mut child.stderr.take().unwrap(), &mut stderr).unwrap();
    assert_eq!(status.code(), Some(130), "{stderr}");
    let chats = chat_files(dir.path());
    assert_eq!(chats.len(), 1, "{chats:?}");
    let turns: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&chats[0]).unwrap()).unwrap();
    assert_eq!(
        turns,
        serde_json::json!([{"role": "user", "content": "ping"}])
    );
}

mod which {
    /// `usage` on PATH, or None.
    pub fn which_usage() -> Option<std::path::PathBuf> {
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|d| d.join("usage"))
                .find(|p| p.is_file())
        })
    }
}

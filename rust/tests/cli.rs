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
/// the way curl and jq end, not a Rust panic about a broken pipe. The pipe's
/// read end is closed before the binary writes, so the write always fails.
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
        assert!(
            out.status.success() || out.status.signal() == Some(13),
            "{flag}: {:?}",
            out.status
        );
    }
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

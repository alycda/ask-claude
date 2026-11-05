//! The binary: its interface (R0, green from the scaffold) and the first
//! end-to-end turn through the replay seam (R5, red until the transport
//! and the one shot exist). The rest of the behaviour is the bats suite,
//! run against this binary by `just test-rust`.

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
        .arg(repo().join("ask-claude.sh"))
        .arg(&kdl)
        .output()
        .unwrap();
    let report =
        String::from_utf8_lossy(&diff.stdout).to_string() + &String::from_utf8_lossy(&diff.stderr);
    assert!(report.contains("Found 0 breaking"), "{report}");
}

#[test]
fn r0_version_flag() {
    bin()
        .arg("-V")
        .assert()
        .success()
        .stdout(format!("ask-claude {}\n", env!("CARGO_PKG_VERSION")));
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

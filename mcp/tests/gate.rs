//! U5: the script's state-dir gate, read side. The script refuses a STATE
//! that is a symlink, belongs to another account or other accounts can
//! write, because whoever controls it controls what the script finds
//! there. A server that read such a directory would hand a planted chat or
//! event log to the model as this user's own. So the server refuses the
//! same directories, on every call, and never repairs one (no chmod: the
//! script does that, when it runs).
//!
//! Pinned here: what every version of the gate refuses (a symlink, a
//! directory any account can write) and what it accepts (700, 750).
//! Group-writable is left out on purpose: 0.3.0 refuses it, and the audit
//! run 2 line accepts a directory writable only by the user's own primary
//! group. Another account's directory needs a second account to test.

mod common;

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;

use ask_claude_mcp::state;
use common::{REPORTS, StateHome, assert_jq_eq, golden};
use serde_json::json;

fn chmod(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777
}

/// STATE as a symlink to a private directory that holds the fixture log.
fn symlinked(state: &StateHome) {
    let real = state.home().join("elsewhere");
    fs::create_dir(&real).unwrap();
    chmod(&real, 0o700);
    fs::copy(
        common::fixture("mcp/events.jsonl"),
        real.join("events.jsonl"),
    )
    .unwrap();
    fs::create_dir_all(state.xdg()).unwrap();
    symlink(&real, state.dir()).unwrap();
}

#[test]
fn u5_check_a_missing_directory_is_nothing_to_read() {
    let state = StateHome::new();
    assert_eq!(state::check(&state.dir()), Ok(false));
}

#[test]
fn u5_check_a_private_directory_is_read() {
    let state = StateHome::new();
    let dir = state.create();
    assert_eq!(state::check(&dir), Ok(true));
    chmod(&dir, 0o750);
    assert_eq!(state::check(&dir), Ok(true), "others may read, not write");
}

#[test]
fn u5_check_refuses_a_symlink() {
    let state = StateHome::new();
    symlinked(&state);
    let err = state::check(&state.dir()).expect_err("a symlink");
    assert!(err.contains("symlink"), "{err}");
}

#[test]
fn u5_check_refuses_a_directory_any_account_can_write() {
    for m in [0o777, 0o702, 0o703] {
        let state = StateHome::new();
        let dir = state.create();
        chmod(&dir, m);
        let err = state::check(&dir).expect_err(&format!("mode {m:o}"));
        assert!(err.contains("writable"), "mode {m:o}: {err}");
        assert_eq!(mode(&dir), m, "the check changed the mode");
    }
}

#[test]
fn u5_check_refuses_a_file_where_the_directory_should_be() {
    let state = StateHome::new();
    fs::create_dir_all(state.xdg()).unwrap();
    fs::write(state.dir(), "").unwrap();
    assert!(state::check(&state.dir()).is_err());
}

#[test]
fn u5_every_report_refuses_a_symlinked_state() {
    let state = StateHome::new();
    symlinked(&state);
    let mut server = state.connect();
    for name in REPORTS {
        let text = server.tool_error(name, json!({}));
        assert!(text.contains("symlink"), "{name}: {text}");
    }
}

#[test]
fn u5_every_report_refuses_a_state_any_account_can_write() {
    let state = StateHome::new();
    let dir = state.dir();
    state.events_fixture();
    chmod(&dir, 0o777);
    let mut server = state.connect();
    for name in REPORTS {
        let text = server.tool_error(name, json!({}));
        assert!(text.contains("writable"), "{name}: {text}");
    }
    assert_eq!(mode(&dir), 0o777, "the server changed the mode");
}

#[test]
fn u5_the_gate_runs_on_every_call() {
    let state = StateHome::new();
    state.events_fixture();
    let mut server = state.connect();
    let first = server.report("tokens_by_chat", json!({}));
    assert_jq_eq(&first, &golden("tokens_by_chat", None), "while private");
    chmod(&state.dir(), 0o777);
    let text = server.tool_error("tokens_by_chat", json!({}));
    assert!(text.contains("writable"), "{text}");
}

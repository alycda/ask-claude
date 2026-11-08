//! U4: the event log as it is found, not as it should be. No state
//! directory yet (a new machine, ASK_CLAUDE_EVENTS=off), an empty log, a
//! last line cut short by a full disk or a kill mid-write, a line some
//! other tool appended: the reports still answer, over the lines that are
//! events. Where jq -s would refuse the whole file, the server skips the
//! line. Rust-only behaviour, so no golden: the expected values are jq's
//! over the good lines.

mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use ask_claude_mcp::{events, reports};
use common::{REPORTS, StateHome, assert_jq_eq, fixture, golden};
use serde_json::{Value, json};

fn fixture_text() -> String {
    fs::read_to_string(fixture("mcp/events.jsonl")).unwrap()
}

fn fixture_events() -> Vec<Value> {
    fixture_text()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

const TORN: &str = r#"{"v":1,"ask_claude":"0.3.0","ts":"2026-11-08T07:0"#;

#[test]
fn u4_every_report_over_no_events_is_empty() {
    for name in REPORTS {
        assert_eq!(reports::run(name, &[]), Some(json!([])), "{name}");
    }
}

#[test]
fn u4_parse_of_nothing_is_nothing() {
    assert_eq!(events::parse(""), Vec::<Value>::new());
}

#[test]
fn u4_parse_skips_a_torn_last_line() {
    let text = fixture_text() + TORN;
    assert_eq!(events::parse(&text), fixture_events());
}

#[test]
fn u4_parse_skips_every_line_that_is_not_an_object() {
    let mut lines: Vec<String> = fixture_text().lines().map(String::from).collect();
    lines.insert(3, String::new());
    lines.insert(5, "not json at all".into());
    lines.insert(7, "[1, 2]".into());
    lines.insert(8, r#""a string""#.into());
    lines.insert(9, "42".into());
    lines.insert(10, "null".into());
    let text = lines.join("\n") + "\n";
    assert_eq!(events::parse(&text), fixture_events());
}

#[test]
fn u4_no_state_directory_reports_nothing_and_creates_nothing() {
    let state = StateHome::new();
    let mut server = state.connect();
    for name in REPORTS {
        assert_eq!(server.report(name, json!({})), json!([]), "{name}");
    }
    assert!(
        !state.xdg().exists(),
        "{} was created",
        state.xdg().display()
    );
}

#[test]
fn u4_no_event_log_reports_nothing_and_creates_nothing() {
    let state = StateHome::new();
    let dir = state.create();
    let mut server = state.connect();
    for name in REPORTS {
        assert_eq!(server.report(name, json!({})), json!([]), "{name}");
    }
    assert!(
        !dir.join("events.jsonl").exists(),
        "events.jsonl was created"
    );
}

#[test]
fn u4_an_empty_event_log_reports_nothing() {
    let state = StateHome::new();
    state.events("");
    let mut server = state.connect();
    for name in REPORTS {
        assert_eq!(server.report(name, json!({})), json!([]), "{name}");
    }
}

#[test]
fn u4_a_torn_last_line_is_skipped_by_the_server_too() {
    let state = StateHome::new();
    state.events(&(fixture_text() + TORN));
    let mut server = state.connect();
    for name in REPORTS {
        assert_jq_eq(&server.report(name, json!({})), &golden(name, None), name);
    }
}

#[test]
fn u4_an_unreadable_log_is_a_tool_error_and_the_server_goes_on() {
    let state = StateHome::new();
    let log = state.events_fixture();
    fs::set_permissions(&log, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&log).is_ok() {
        eprintln!("skipped: this account reads a mode 000 file (root?)");
        return;
    }
    let mut server = state.connect();
    let text = server.tool_error("tokens_by_chat", json!({}));
    assert!(text.contains("events.jsonl"), "names the file: {text}");
    // still serving
    fs::set_permissions(&log, fs::Permissions::from_mode(0o600)).unwrap();
    let report = server.report("tokens_by_chat", json!({}));
    assert_jq_eq(&report, &golden("tokens_by_chat", None), "after chmod 600");
}

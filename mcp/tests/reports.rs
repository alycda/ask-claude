//! U2: the five reports. jq is the oracle: each golden under
//! tests/fixtures/mcp/reports/ is what the README's query prints for
//! tests/fixtures/mcp/events.jsonl (tests/unit/mcp-reports.bats holds the
//! goldens and the README to each other). The library's reports must equal
//! them as jq sees JSON, and so must the server's answers.

mod common;

use std::fs;

use ask_claude_mcp::{events, reports, state};
use common::{REPORTS, StateHome, assert_jq_eq, fixture, golden};
use serde_json::{Value, json};

/// The events fixture, read here without the library.
fn fixture_events() -> Vec<Value> {
    fs::read_to_string(fixture("mcp/events.jsonl"))
        .expect("the events fixture")
        .lines()
        .map(|l| serde_json::from_str(l).expect("every fixture line is JSON"))
        .collect()
}

#[test]
fn u2_state_dir_is_the_scripts() {
    assert_eq!(
        state::dir(Some("/x/state"), Some("/home/a")),
        Some("/x/state/ask-claude".into())
    );
    assert_eq!(
        state::dir(None, Some("/home/a")),
        Some("/home/a/.local/state/ask-claude".into())
    );
    // ${XDG_STATE_HOME:-...}: set but empty counts as unset
    assert_eq!(
        state::dir(Some(""), Some("/home/a")),
        Some("/home/a/.local/state/ask-claude".into())
    );
    assert_eq!(state::dir(None, None), None);
}

#[test]
fn u2_parse_reads_every_line_of_a_well_formed_log() {
    let text = fs::read_to_string(fixture("mcp/events.jsonl")).unwrap();
    assert_eq!(events::parse(&text), fixture_events());
}

#[test]
fn u2_each_report_equals_its_jq_golden() {
    let events = fixture_events();
    for name in REPORTS {
        let report = reports::run(name, &events).unwrap_or_else(|| panic!("{name} is a report"));
        assert_jq_eq(&report, &golden(name, None), name);
    }
}

#[test]
fn u2_a_name_that_is_not_a_report_is_none() {
    assert_eq!(reports::run("no_such_report", &fixture_events()), None);
}

#[test]
fn u2_each_report_through_the_server() {
    let state = StateHome::new();
    state.events_fixture();
    let mut server = state.connect();
    for name in REPORTS {
        let report = server.report(name, json!({}));
        assert_jq_eq(&report, &golden(name, None), name);
    }
}

#[test]
fn u2_a_report_reads_the_log_as_it_is_now() {
    // the script appends while the server runs: no read at startup only
    let state = StateHome::new();
    let log = state.events_fixture();
    let mut server = state.connect();
    let before = server.report("refusals_by_category", json!({}));
    assert_jq_eq(&before, &golden("refusals_by_category", None), "before");

    let mut text = fs::read_to_string(&log).unwrap();
    let line = r#"{"v":1,"ask_claude":"0.3.0","ts":"2026-11-08T09:00:00Z","kind":"messages","chat":"20261108T090000-400","host":"api.anthropic.com","model":"claude-opus-5-5","round":0,"seconds":1,"stop_reason":"refusal","stop_category":"cyber","usage_scope":"request","input_tokens":10,"output_tokens":0,"cache_read":0,"cache_write":0,"list_cost":null,"advisor":null,"error":null}"#;
    text.push_str(line);
    text.push('\n');
    fs::write(&log, text).unwrap();

    let after = server.report("refusals_by_category", json!({}));
    assert_jq_eq(
        &after,
        &json!([{"category": null, "refusals": 1}, {"category": "cyber", "refusals": 2}]),
        "after one more refusal",
    );
}

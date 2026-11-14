//! U3: `since`, every report's one argument. "Advisor spend this week" is
//! advisor_spend with since set to Monday. The goldens are jq's
//! `map(select(.ts >= $since))` before the README query (<name>.since.json,
//! since 2026-11-05); ts is UTC ISO 8601, so string order is time order.

mod common;

use std::fs;

use ask_claude_mcp::{events, reports};
use common::{REPORTS, StateHome, assert_jq_eq, fixture, golden};
use serde_json::{Value, json};

fn fixture_events() -> Vec<Value> {
    fs::read_to_string(fixture("mcp/events.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn u3_since_a_date_keeps_that_day_and_after() {
    let kept = events::since(fixture_events(), "2026-11-05").expect("a date is a since");
    assert_eq!(kept.len(), 9);
    assert!(
        kept.iter()
            .all(|e| e["ts"].as_str().unwrap() >= "2026-11-05")
    );
}

#[test]
fn u3_since_an_instant_keeps_that_second_and_after() {
    let kept = events::since(fixture_events(), "2026-11-05T12:01:00Z").expect("an instant");
    assert_eq!(kept.len(), 8, "the event at exactly 12:01:00 is kept");
    assert_eq!(kept[0]["ts"], "2026-11-05T12:01:00Z");
}

#[test]
fn u3_since_after_the_last_event_keeps_none() {
    assert_eq!(events::since(fixture_events(), "2026-12-01"), Ok(vec![]));
}

#[test]
fn u3_since_is_a_date_or_a_utc_instant_and_nothing_else() {
    for bad in [
        "",
        "2026-11",
        "yesterday",
        "2026-11-05T12:00",
        "2026-11-05 12:00:00Z",
        "2026-11-05T12:00:00+01:00",
        "2026-11-05T12:00:00.5Z",
        " 2026-11-05",
        "2026-11-05\n",
    ] {
        let err = events::since(fixture_events(), bad).expect_err(bad);
        assert!(err.contains("YYYY-MM-DD"), "{bad:?}: {err}");
    }
}

#[test]
fn u3_each_report_since_equals_its_jq_golden() {
    for name in REPORTS {
        let kept = events::since(fixture_events(), "2026-11-05").unwrap();
        let report = reports::run(name, &kept).unwrap();
        assert_jq_eq(&report, &golden(name, Some("since")), name);
    }
}

#[test]
fn u3_each_report_since_through_the_server() {
    let state = StateHome::new();
    state.events_fixture();
    let mut server = state.connect();
    for name in REPORTS {
        let report = server.report(name, json!({"since": "2026-11-05"}));
        assert_jq_eq(&report, &golden(name, Some("since")), name);
    }
}

#[test]
fn u3_a_null_since_is_no_since() {
    let state = StateHome::new();
    state.events_fixture();
    let mut server = state.connect();
    let report = server.report("advisor_spend", json!({"since": null}));
    assert_jq_eq(&report, &golden("advisor_spend", None), "since null");
}

#[test]
fn u3_a_bad_since_is_a_tool_error_that_says_what_since_takes() {
    // a tool error, not a protocol error: the model reads it and retries
    let state = StateHome::new();
    state.events_fixture();
    let mut server = state.connect();
    let text = server.tool_error("tokens_by_chat", json!({"since": "last week"}));
    assert!(text.contains("since"), "{text}");
    assert!(text.contains("YYYY-MM-DD"), "{text}");
}

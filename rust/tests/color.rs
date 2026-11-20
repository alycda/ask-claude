//! `color::enabled`, the rule for one stream, as its whole truth table
//! (docs/plan.md, K3): 3 values of `--color` x a terminal or not x
//! `NO_COLOR` unset, empty and set x `TERM` dumb or not, 36 rows. Each row
//! is written out with its answer; none is computed here.

use ask_claude::color::{self, When};

/// (when, the stream is a terminal, NO_COLOR, TERM, colour)
#[rustfmt::skip]
const TABLE: [(When, bool, Option<&str>, &str, bool); 36] = [
    (When::Auto, true, None, "xterm-256color", true),
    (When::Auto, true, None, "dumb", false),
    (When::Auto, true, Some(""), "xterm-256color", true),
    (When::Auto, true, Some(""), "dumb", false),
    (When::Auto, true, Some("1"), "xterm-256color", false),
    (When::Auto, true, Some("1"), "dumb", false),
    (When::Auto, false, None, "xterm-256color", false),
    (When::Auto, false, None, "dumb", false),
    (When::Auto, false, Some(""), "xterm-256color", false),
    (When::Auto, false, Some(""), "dumb", false),
    (When::Auto, false, Some("1"), "xterm-256color", false),
    (When::Auto, false, Some("1"), "dumb", false),
    (When::Always, true, None, "xterm-256color", true),
    (When::Always, true, None, "dumb", true),
    (When::Always, true, Some(""), "xterm-256color", true),
    (When::Always, true, Some(""), "dumb", true),
    (When::Always, true, Some("1"), "xterm-256color", true),
    (When::Always, true, Some("1"), "dumb", true),
    (When::Always, false, None, "xterm-256color", true),
    (When::Always, false, None, "dumb", true),
    (When::Always, false, Some(""), "xterm-256color", true),
    (When::Always, false, Some(""), "dumb", true),
    (When::Always, false, Some("1"), "xterm-256color", true),
    (When::Always, false, Some("1"), "dumb", true),
    (When::Never, true, None, "xterm-256color", false),
    (When::Never, true, None, "dumb", false),
    (When::Never, true, Some(""), "xterm-256color", false),
    (When::Never, true, Some(""), "dumb", false),
    (When::Never, true, Some("1"), "xterm-256color", false),
    (When::Never, true, Some("1"), "dumb", false),
    (When::Never, false, None, "xterm-256color", false),
    (When::Never, false, None, "dumb", false),
    (When::Never, false, Some(""), "xterm-256color", false),
    (When::Never, false, Some(""), "dumb", false),
    (When::Never, false, Some("1"), "xterm-256color", false),
    (When::Never, false, Some("1"), "dumb", false),
];

/// `always` is on and `never` is off, whatever the stream and the
/// environment: 24 of the rows.
#[test]
fn k3_always_and_never_beat_the_environment() {
    let mut n = 0;
    for (when, terminal, no_color, term, want) in TABLE {
        if when == When::Auto {
            continue;
        }
        assert_eq!(
            color::enabled(when, terminal, no_color, Some(term)),
            want,
            "{when:?}, terminal {terminal}, NO_COLOR {no_color:?}, TERM {term}"
        );
        n += 1;
    }
    assert_eq!(n, 24);
}

/// `auto` is on for a terminal with `NO_COLOR` unset or empty and a `TERM`
/// that is not `dumb`, and off in every other row: 2 on, 10 off.
#[test]
fn k3_auto_follows_the_stream_and_the_environment() {
    let mut on = 0;
    for (when, terminal, no_color, term, want) in TABLE {
        if when != When::Auto {
            continue;
        }
        assert_eq!(
            color::enabled(when, terminal, no_color, Some(term)),
            want,
            "auto, terminal {terminal}, NO_COLOR {no_color:?}, TERM {term}"
        );
        on += usize::from(want);
    }
    assert_eq!(on, 2);
}

/// The table itself: every combination once, so a row cannot be missing
/// or written twice.
#[test]
fn k3_the_table_has_every_combination_once() {
    let mut seen = std::collections::HashSet::new();
    for (when, terminal, no_color, term, _) in TABLE {
        assert!(seen.insert((format!("{when:?}"), terminal, no_color, term)));
    }
    assert_eq!(seen.len(), 3 * 2 * 3 * 2);
}

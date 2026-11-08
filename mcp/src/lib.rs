//! ask-claude-mcp, the library: the pure parts of the server, with the
//! logic left out. The protocol is the binary's (main.rs); what it reads
//! is here, so the reports can be held to their goldens without a server.
//!
//! The oracle is jq, as for the Rust port: each report is a query from the
//! README's "Events on disk", kept verbatim in
//! `../tests/fixtures/mcp/reports/<name>.jq`, and its golden is what jq
//! prints for `../tests/fixtures/mcp/events.jsonl`. These signatures are a
//! starting shape, not a promise: tests/reports.rs and tests/state.rs call
//! them, and may change with them in a commit that says why.

/// Where the script keeps its state, and whether it is safe to read.
pub mod state {
    use std::path::{Path, PathBuf};

    /// The script's `STATE`: `${XDG_STATE_HOME:-$HOME/.local/state}/ask-claude`.
    /// An empty `XDG_STATE_HOME` counts as unset, as `:-` does; `None` when
    /// neither is set.
    pub fn dir(xdg_state_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
        // ${XDG_STATE_HOME:-...}: keep it only when it is non-empty
        let xdg_state_home = xdg_state_home.filter(|s| !s.is_empty());

        let base = match xdg_state_home {
            Some(xdg_state_home) => PathBuf::from(xdg_state_home),
            // no HOME either: `?` returns None
            None => PathBuf::from(home?).join(".local/state"),
        };
        Some(base.join("ask-claude"))
    }

    /// The script's state-dir gate, read side, run before every read:
    /// `Ok(true)` to read, `Ok(false)` when there is no state directory yet
    /// (nothing to report; the server never creates it), `Err` with the
    /// reason to refuse: a symlink, another account's directory, one other
    /// accounts can write. It never changes a mode: that is the script's.
    pub fn check(_dir: &Path) -> Result<bool, String> {
        todo!("U5: the script's gate, read side")
    }

    #[cfg(test)]
    mod tests {
        use super::dir;
        use rstest::rstest;
        use std::path::PathBuf;

        #[rstest]
        #[case::xdg_state_home_wins_when_set(
            Some("/x/state"),
            Some("/home/a"),
            Some("/x/state/ask-claude")
        )]
        #[case::unset_falls_back_to_home(
            None,
            Some("/home/a"),
            Some("/home/a/.local/state/ask-claude")
        )]
        #[case::set_but_empty_counts_as_unset(
            Some(""),
            Some("/home/a"),
            Some("/home/a/.local/state/ask-claude")
        )]
        #[case::neither_is_none(None, None, None)]
        fn dir_is_the_scripts_state_line(
            #[case] xdg_state_home: Option<&str>,
            #[case] home: Option<&str>,
            #[case] expected: Option<&str>,
        ) {
            assert_eq!(dir(xdg_state_home, home), expected.map(PathBuf::from));
        }
    }
}

/// `events.jsonl`, one JSON object per line (README, "Events on disk").
pub mod events {
    use serde_json::Value;

    /// Every line that is a JSON object, in file order. A line that is not
    /// one is skipped: a torn last line from a write cut short, a blank, a
    /// line some other tool appended. (jq -s would refuse the whole file.)
    pub fn parse(text: &str) -> Vec<Value> {
        text.lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(Value::is_object)
            .collect()
    }

    /// The events at or after `since`, which is `YYYY-MM-DD` or
    /// `YYYY-MM-DDTHH:MM:SSZ`; compared with `ts` as strings, the way
    /// `map(select(.ts >= $since))` does. `Err` names the accepted forms.
    pub fn since(_events: Vec<Value>, _since: &str) -> Result<Vec<Value>, String> {
        todo!("U3: the since filter")
    }
}

/// The README's five questions, one function each behind one name.
pub mod reports {
    use std::collections::BTreeMap;

    use serde::Serialize;
    use serde_json::Value;

    // One row of each report, named and typed as its README query prints
    // it. A group key is an Option: jq groups a missing or null key as
    // null, which prints first.

    #[derive(Debug, Serialize)]
    pub struct ChatTokens {
        pub chat: Option<String>,
        pub kind: Option<String>,
        pub input_tokens: u64,
        pub output_tokens: u64,
        pub cache_read: u64,
        pub cache_write: u64,
    }

    #[derive(Debug, Serialize)]
    pub struct CacheHit {
        pub model: Option<String>,
        pub cache_hit: f64,
    }

    #[derive(Debug, Serialize)]
    pub struct ModelSeconds {
        pub model: Option<String>,
        pub requests: usize,
        pub mean_seconds: f64,
        pub max_seconds: f64,
    }

    #[derive(Debug, Serialize)]
    pub struct Refusals {
        pub category: Option<String>,
        pub refusals: usize,
    }

    #[derive(Debug, Serialize)]
    pub struct AdvisorSpend {
        pub advisor: Option<String>,
        pub calls: u64,
        pub input_tokens: u64,
        pub output_tokens: u64,
    }

    /// jq's `group_by(key)`: groups in key order, with null (`None`) before
    /// every string, and each group's events in file order, so `.[0]` is
    /// the group's first event.
    ///
    /// `events` is a slice or any iterator of `&Value`, so a report can
    /// filter first: `group_by(events.iter().filter(...), key)`.
    fn group_by<'a>(
        events: impl IntoIterator<Item = &'a Value>,
        key: impl Fn(&'a Value) -> Option<&'a str>,
    ) -> BTreeMap<Option<&'a str>, Vec<&'a Value>> {
        let mut groups: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for event in events {
            groups.entry(key(event)).or_default().push(event);
        }
        groups
    }

    /// The report `name` over `events` (already filtered), equal as JSON to
    /// what its README query prints; `None` for a name that is not one of
    /// tokens_by_chat, cache_hit_by_model, seconds_by_model,
    /// refusals_by_category, advisor_spend.
    pub fn run(name: &str, events: &[Value]) -> Option<Value> {
        // each report has its own row type, so each arm serializes its own
        let report = match name {
            "tokens_by_chat" => serde_json::to_value(tokens_by_chat(events)),
            "cache_hit_by_model" => serde_json::to_value(cache_hit_by_model(events)),
            "seconds_by_model" => serde_json::to_value(seconds_by_model(events)),
            "refusals_by_category" => serde_json::to_value(refusals_by_category(events)),
            "advisor_spend" => serde_json::to_value(advisor_spend(events)),
            _ => return None,
        };
        Some(report.expect("report rows are strings and numbers"))
    }

    /// tests/fixtures/mcp/reports/tokens_by_chat.jq
    fn tokens_by_chat(events: &[Value]) -> Vec<ChatTokens> {
        group_by(events, |e| e["chat"].as_str())
            .into_iter()
            .map(|(chat, group)| {
                let first = group[0];
                let session = first["usage_scope"] == "session";
                // def total(f): map(f // 0) | if $scope == "session" then max else add end
                let total = |field: &str| {
                    let values = group.iter().map(|e| e[field].as_u64().unwrap_or(0));
                    if session {
                        values.max().unwrap_or(0)
                    } else {
                        values.sum()
                    }
                };
                ChatTokens {
                    chat: chat.map(String::from),
                    kind: first["kind"].as_str().map(String::from),
                    input_tokens: total("input_tokens"),
                    output_tokens: total("output_tokens"),
                    cache_read: total("cache_read"),
                    cache_write: total("cache_write"),
                }
            })
            .collect()
    }

    /// tests/fixtures/mcp/reports/cache_hit_by_model.jq
    fn cache_hit_by_model(_events: &[Value]) -> Vec<CacheHit> {
        todo!("U2: tests/fixtures/mcp/reports/cache_hit_by_model.jq")
    }

    /// tests/fixtures/mcp/reports/seconds_by_model.jq
    fn seconds_by_model(_events: &[Value]) -> Vec<ModelSeconds> {
        todo!("U2: tests/fixtures/mcp/reports/seconds_by_model.jq")
    }

    /// tests/fixtures/mcp/reports/refusals_by_category.jq
    fn refusals_by_category(_events: &[Value]) -> Vec<Refusals> {
        todo!("U2: tests/fixtures/mcp/reports/refusals_by_category.jq")
    }

    /// tests/fixtures/mcp/reports/advisor_spend.jq
    fn advisor_spend(_events: &[Value]) -> Vec<AdvisorSpend> {
        todo!("U2: tests/fixtures/mcp/reports/advisor_spend.jq")
    }
}

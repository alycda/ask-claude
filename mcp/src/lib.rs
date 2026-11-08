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
    pub fn dir(_xdg_state_home: Option<&str>, _home: Option<&str>) -> Option<PathBuf> {
        todo!("U2: the script's STATE line")
    }

    /// The script's state-dir gate, read side, run before every read:
    /// `Ok(true)` to read, `Ok(false)` when there is no state directory yet
    /// (nothing to report; the server never creates it), `Err` with the
    /// reason to refuse: a symlink, another account's directory, one other
    /// accounts can write. It never changes a mode: that is the script's.
    pub fn check(_dir: &Path) -> Result<bool, String> {
        todo!("U5: the script's gate, read side")
    }
}

/// `events.jsonl`, one JSON object per line (README, "Events on disk").
pub mod events {
    use serde_json::Value;

    /// Every line that is a JSON object, in file order. A line that is not
    /// one is skipped: a torn last line from a write cut short, a blank, a
    /// line some other tool appended. (jq -s would refuse the whole file.)
    pub fn parse(_text: &str) -> Vec<Value> {
        todo!("U2: one object per line; U4: the rest skipped")
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
    use serde_json::Value;

    /// The report `name` over `events` (already filtered), equal as JSON to
    /// what its README query prints; `None` for a name that is not one of
    /// tokens_by_chat, cache_hit_by_model, seconds_by_model,
    /// refusals_by_category, advisor_spend.
    pub fn run(_name: &str, _events: &[Value]) -> Option<Value> {
        todo!("U2: the five README queries")
    }
}

//! The event log, `$STATE/events.jsonl`: one JSON line per request, built
//! as `lib/event.jq` builds it and written as `event` in
//! `lib/common.bash` writes it. Tokens, timing, outcome and the tool set
//! only, never prompt, reply or error text: every string that came from
//! the endpoint is kept only when it looks like an identifier.

use serde_json::Value;

/// What `event` passes `lib/event.jq` besides its input, under the same
/// names (`--arg`, `--argjson`).
#[derive(Debug, Clone, Default)]
pub struct Args {
    /// `date -u +%Y-%m-%dT%H:%M:%SZ`
    pub ts: String,
    pub version: String,
    /// `messages` or `session`
    pub kind: String,
    /// the Chat's name or the session id; "" is none
    pub chat: String,
    pub host: String,
    /// the pause_turn round (messages) or custom-tool rounds (session)
    pub round: u64,
    pub seconds: u64,
    /// the failure's category; "" is none
    pub error: String,
    /// the request's `.tools` (messages), or null
    pub tools: Value,
}

/// The line `lib/event.jq` prints for `input` (the reassembled message,
/// the session fold, or null), compact and in its key order; None where
/// jq prints nothing.
pub fn line(input: &Value, args: &Args) -> Option<String> {
    let _ = (input, args);
    todo!("U5: the event log")
}

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
/// jq prints nothing or fails, as with a `server_tool_use` that is not an
/// object, or an MCP toolset whose `configs` is a string.
pub fn line(input: &Value, args: &Args) -> Option<String> {
    use serde_json::{Map, json};
    let null = &Value::Null;
    let input = if input.is_object() { input } else { null };
    let session = args.kind == "session";
    let u = match or(&input["usage"]) {
        u @ Value::Object(_) => u,
        _ => null,
    };
    let advisors: Vec<&Value> = values(&u["iterations"])
        .filter(|i| i.is_object() && i["type"] == "advisor_message")
        .collect();
    let cache = |session_key: &str, request_key: &str| {
        if u.is_null() {
            Value::Null
        } else if session {
            num(&u[session_key])
        } else {
            // `$u.cache_read_input_tokens // 0 | num`
            match or(&u[request_key]) {
                Value::Null => Value::from(0),
                v => num(v),
            }
        }
    };
    // `.stop_details.category?`: an error on a stop_details that is not an
    // object or null is caught, and the default applies
    let category = match &input["stop_details"] {
        Value::Object(o) => o.get("category").unwrap_or(null),
        _ => null,
    };
    let advisor = if advisors.is_empty() {
        Value::Null
    } else {
        let sum = |key: &str| add(advisors.iter().map(|a| or_zero(num(&a[key]))));
        json!({
            "model": ident(&advisors[0]["model"]),
            "calls": advisors.len(),
            "input_tokens": sum("input_tokens"),
            "output_tokens": sum("output_tokens"),
        })
    };
    let tool_calls = if session || input.is_null() {
        Value::Null
    } else {
        let server = match &u["server_tool_use"] {
            s @ (Value::Object(_) | Value::Null) => s,
            // `.web_search_requests?` on anything else is empty, and an
            // empty value empties the whole object: jq prints nothing
            _ => return None,
        };
        let mcp = values(&input["content"])
            .filter(|b| b.is_object() && b["type"] == "mcp_tool_use")
            .count();
        json!({
            "web_search": num(&server["web_search_requests"]),
            "web_fetch": num(&server["web_fetch_requests"]),
            "mcp": mcp,
        })
    };

    let mut o = Map::new();
    o.insert("v".into(), json!(1));
    o.insert("ask_claude".into(), ident(&json!(args.version)));
    o.insert("ts".into(), ident(&json!(args.ts)));
    o.insert(
        "kind".into(),
        json!(if session { "session" } else { "messages" }),
    );
    o.insert(
        "chat".into(),
        if args.chat.is_empty() {
            Value::Null
        } else {
            ident(&json!(args.chat))
        },
    );
    o.insert("host".into(), hostname(&args.host));
    o.insert(
        "model".into(),
        if session {
            Value::Null
        } else {
            ident(or(&input["model"]))
        },
    );
    o.insert("round".into(), json!(args.round));
    o.insert("seconds".into(), json!(args.seconds));
    o.insert("stop_reason".into(), ident(or(&input["stop_reason"])));
    o.insert("stop_category".into(), ident(or(category)));
    o.insert(
        "usage_scope".into(),
        json!(if session { "session" } else { "request" }),
    );
    o.insert("input_tokens".into(), num(&u["input_tokens"]));
    o.insert("output_tokens".into(), num(&u["output_tokens"]));
    o.insert(
        "cache_read".into(),
        cache("cache_read", "cache_read_input_tokens"),
    );
    o.insert(
        "cache_write".into(),
        cache("cache_write", "cache_creation_input_tokens"),
    );
    o.insert(
        "list_cost".into(),
        if session {
            cents(&u["list_cost"])
        } else {
            Value::Null
        },
    );
    o.insert("advisor".into(), advisor);
    o.insert(
        "error".into(),
        if args.error.is_empty() {
            Value::Null
        } else {
            ident(&json!(args.error))
        },
    );
    o.insert("tools".into(), tools(&args.tools)?);
    o.insert("tool_calls".into(), tool_calls);
    Some(crate::tojson(&Value::Object(o)))
}

/// jq's `$v // null`: null for null and false.
fn or(v: &Value) -> &Value {
    crate::or(v, &Value::Null)
}

/// jq's `.[]?`: an array's elements, an object's values, else nothing.
fn values(v: &Value) -> Box<dyn Iterator<Item = &Value> + '_> {
    match v {
        Value::Array(a) => Box::new(a.iter()),
        Value::Object(o) => Box::new(o.values()),
        _ => Box::new(std::iter::empty()),
    }
}

/// `num`: a number as it is, anything else null.
fn num(v: &Value) -> Value {
    if v.is_number() {
        v.clone()
    } else {
        Value::Null
    }
}

/// `num // 0`.
fn or_zero(v: Value) -> Value {
    if v.is_null() { Value::from(0) } else { v }
}

/// jq's `add` over numbers: one term as it is (`null + x` is `x`), a sum
/// whole while every term is, else a float as jq prints it ([`sum`]).
fn add(terms: impl Iterator<Item = Value>) -> Value {
    let terms: Vec<Value> = terms.collect();
    if let [only] = terms.as_slice() {
        return only.clone();
    }
    let whole: Option<i64> = terms
        .iter()
        .try_fold(0i64, |sum, t| t.as_i64().and_then(|n| sum.checked_add(n)));
    match whole {
        Some(sum) => Value::from(sum),
        None => sum(terms.iter().filter_map(Value::as_f64).sum()),
    }
}

/// A sum jq computed in doubles, as it prints one: a whole sum below 1e16
/// with no fraction (`2.0 + 3` is `5`, where serde writes `5.0`), any
/// other as serde writes it, which is jq's text for the sums token counts
/// make (not for exponents: serde writes `1e16`, jq `1e+16`).
pub(crate) fn sum(f: f64) -> Value {
    if f.fract() == 0.0 && f.abs() < 1e16 {
        Value::from(f as i64)
    } else {
        serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number)
    }
}

/// A whole-string match of `\A[<class>]{1,max}\z` over ASCII.
fn shaped(s: &str, max: usize, class: impl Fn(u8) -> bool) -> bool {
    (1..=max).contains(&s.len()) && s.bytes().all(class)
}

fn ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"._:-".contains(&b)
}

/// `ident`: null stays null, an identifier-shaped string stays, anything
/// else is "unknown".
fn ident(v: &Value) -> Value {
    match v {
        Value::Null => Value::Null,
        Value::String(s) if shaped(s, 64, ident_byte) => v.clone(),
        _ => Value::from("unknown"),
    }
}

/// `hostname`.
fn hostname(host: &str) -> Value {
    let ok = shaped(host, 255, |b| {
        b.is_ascii_alphanumeric() || b".:[]-".contains(&b)
    });
    Value::from(if ok { host } else { "unknown" })
}

/// `toolname`.
fn toolname(name: &Value) -> Value {
    match name {
        Value::String(s) if shaped(s, 255, ident_byte) => name.clone(),
        _ => Value::from("unknown"),
    }
}

/// `cents`: the API's cents as a string, a number made into one.
fn cents(v: &Value) -> Value {
    match v {
        Value::Null => Value::Null,
        Value::Number(n) => Value::from(n.to_string()),
        Value::String(s) => {
            let (whole, frac) = match s.split_once('.') {
                Some((w, f)) => (w, Some(f)),
                None => (s.as_str(), None),
            };
            let digits = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
            if digits(whole) && frac.is_none_or(digits) {
                v.clone()
            } else {
                Value::from("unknown")
            }
        }
        _ => Value::from("unknown"),
    }
}

/// The `tools` field: what the request let the model reach, each server
/// tool by its versioned type, an MCP server as `mcp:<server>` or one
/// `mcp:<server>:<tool>` per tool of an allowlist, through `toolname`;
/// null without an array. None where jq fails (`to_entries` of a string).
fn tools(given: &Value) -> Option<Value> {
    let Value::Array(entries) = or(given) else {
        return Some(Value::Null);
    };
    let mut out = Vec::new();
    for t in entries.iter().filter(|t| t.is_object()) {
        if t["type"] != "mcp_toolset" {
            let name = match or(&t["type"]) {
                Value::Null => &t["name"],
                ty => ty,
            };
            out.push(toolname(name));
            continue;
        }
        let server = match or(&t["mcp_server_name"]) {
            Value::Null => "?".to_string(),
            s => crate::text(s),
        };
        // `.default_config.enabled? == false`: on a default_config that is
        // neither an object nor null the test is empty, and so is the entry
        let enabled = match &t["default_config"] {
            Value::Object(c) => c.get("enabled").unwrap_or(&Value::Null),
            Value::Null => &Value::Null,
            _ => continue,
        };
        if *enabled != Value::Bool(false) {
            out.push(toolname(&Value::from(format!("mcp:{server}"))));
            continue;
        }
        let configs: Vec<(String, &Value)> = match or(&t["configs"]) {
            Value::Null => Vec::new(),
            Value::Object(c) => c.iter().map(|(k, v)| (k.clone(), v)).collect(),
            Value::Array(a) => a
                .iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v))
                .collect(),
            _ => return None,
        };
        for (key, config) in configs {
            let on = match config {
                Value::Object(c) => c.get("enabled") == Some(&Value::Bool(true)),
                _ => false,
            };
            if on {
                out.push(toolname(&Value::from(format!("mcp:{server}:{key}"))));
            }
        }
    }
    Some(Value::Array(out))
}

/// Where the lines go, `$STATE/events.jsonl`, and the host they name;
/// `ASK_CLAUDE_EVENTS=off` turns it off.
#[derive(Debug, Clone)]
pub struct Log {
    file: Option<std::path::PathBuf>,
    host: String,
}

impl Log {
    /// The log under `state` for requests to `base_url`: its host, as
    /// `lib/config.bash` cuts it (after `://`, before the first `/`), so
    /// never the path or a query.
    pub fn new(state: &std::path::Path, base_url: &str) -> Self {
        let off = std::env::var_os("ASK_CLAUDE_EVENTS").is_some_and(|v| v == "off");
        let rest = base_url
            .split_once("://")
            .map_or(base_url, |(_, rest)| rest);
        Self {
            file: (!off).then(|| state.join("events.jsonl")),
            host: rest[..rest.find('/').unwrap_or(rest.len())].to_string(),
        }
    }

    /// Append one line, `event` in `lib/common.bash`: `kind`, the input
    /// (message, fold or null), the round, seconds, the failure's category
    /// ("" for none), the Chat or session ("" for none), the request's
    /// tools (null for none). A log that cannot be written is skipped in
    /// silence, as the script's is.
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &self,
        kind: &str,
        input: &Value,
        round: u64,
        seconds: u64,
        error: &str,
        chat: &str,
        tools: &Value,
    ) {
        let Some(file) = &self.file else {
            return;
        };
        let args = Args {
            ts: utc_now(),
            version: crate::version().to_string(),
            kind: kind.to_string(),
            chat: chat.to_string(),
            host: self.host.clone(),
            round,
            seconds,
            error: error.to_string(),
            tools: tools.clone(),
        };
        let Some(mut text) = line(input, &args) else {
            return;
        };
        text.push('\n');
        let _ = append(file, text.as_bytes());
    }
}

/// Append `bytes` in one write, the file created 0600 when missing and
/// never opened through a symlink. SIGXFSZ is ignored while it writes, so
/// a log at the `ulimit -f` limit is a write error (EFBIG), skipped like
/// any other, and not the end of this process: the script appends from a
/// subshell for the same reason.
fn append(file: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut log = opts.open(file)?;
    // SAFETY: signal(2) only sets this process's disposition of SIGXFSZ
    #[cfg(unix)]
    let before = unsafe { libc::signal(libc::SIGXFSZ, libc::SIG_IGN) };
    let written = log.write_all(bytes);
    #[cfg(unix)]
    if before != libc::SIG_ERR {
        // SAFETY: the disposition signal(2) returned above, put back
        unsafe { libc::signal(libc::SIGXFSZ, before) };
    }
    written
}

/// `date -u +%Y-%m-%dT%H:%M:%SZ`, from the days since the epoch by the
/// civil-from-days algorithm (Howard Hinnant's), so no time zone is read.
fn utc_now() -> String {
    utc(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()))
}

/// [`utc_now`] for `secs` since the epoch.
fn utc(secs: u64) -> String {
    let (days, rest) = (secs / 86_400, secs % 86_400);
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    /// Against `date -u -d @<secs> +%Y-%m-%dT%H:%M:%SZ`: the epoch, a leap
    /// day, a century that is not a leap year, and this year.
    #[test]
    fn utc_is_dates_form() {
        assert_eq!(super::utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(super::utc(951_825_599), "2000-02-29T11:59:59Z");
        assert_eq!(super::utc(4_107_542_400), "2100-03-01T00:00:00Z");
        assert_eq!(super::utc(1_794_700_800), "2026-11-15T00:00:00Z");
    }

    /// Against jq 1.8.2: `[2.0, 3] | add` prints 5, `[2.5, 3] | add` 5.5,
    /// `[2.0] | add` 2.0 (one term is never added); `-5.0 + 0` prints -5.
    #[test]
    fn add_prints_a_whole_sum_as_jq_does() {
        use serde_json::{Value, json};
        let add =
            |terms: Value| crate::tojson(&super::add(terms.as_array().unwrap().iter().cloned()));
        assert_eq!(add(json!([2.0, 3])), "5");
        assert_eq!(add(json!([2.5, 3])), "5.5");
        assert_eq!(add(json!([2.0])), "2.0");
        assert_eq!(add(json!([2, 3])), "5");
        assert_eq!(crate::tojson(&super::sum(-5.0)), "-5");
    }

    /// The session fold's cache_write, `lib/session-fold.jq` under jq
    /// 1.8.2: ephemeral counts of 2.0 and 3 make 5, of 2.5 and 3 make 5.5.
    #[test]
    fn a_session_cache_write_sum_prints_as_jq_does() {
        use serde_json::json;
        let cache_write = |five: f64| {
            let usage = json!({"type": "session.usage", "usage": {"cache_creation": {
                "ephemeral_5m_input_tokens": five, "ephemeral_1h_input_tokens": 3}}});
            crate::tojson(&crate::session::fold(&[usage])["usage"]["cache_write"])
        };
        assert_eq!(cache_write(2.0), "5");
        assert_eq!(cache_write(2.5), "5.5");
    }

    /// The one `"\(x)"` / `tostring` for the crate: jq 1.8.2 prints
    /// `{"a":"\u007f"}` for `{"a": "\u007f"} | tostring`, DEL escaped.
    #[test]
    fn text_escapes_del_as_jq_tostring_does() {
        let v = serde_json::json!({"a": "\u{7f}"});
        assert_eq!(crate::text(&v), r#"{"a":"\u007f"}"#);
        assert_eq!(crate::text(&serde_json::json!("\u{7f}")), "\u{7f}");
    }
}

use serde_json::Value;

/// What one session event prints (`lib/session.jq`).
pub fn line(event: &Value) -> String {
    use crate::{or, text};
    let name = |default: &str| text(or(&event["name"], &Value::from(default)));
    let out = match event["type"].as_str().unwrap_or("") {
        "agent.message" => texts(event, ""),
        "agent.thinking" => "(thinking) ".to_string(),
        "agent.tool_use" if event["name"] == "web_fetch" && event["input"]["url"].is_string() => {
            let url = event["input"]["url"].as_str().unwrap_or("");
            format!(
                "(web_fetch {}) ",
                crate::url_host(url, &['/']).unwrap_or(url)
            )
        }
        "agent.tool_use" | "agent.custom_tool_use" => format!("({}) ", name("tool")),
        "agent.mcp_tool_use" => format!("(mcp {}) ", name("?")),
        "session.status_idle" | "session.status_terminated" => "\n".to_string(),
        _ => String::new(),
    };
    crate::stream::safe(&out)
}

/// The `text` blocks of an event's content, joined with `sep`.
fn texts(event: &Value, sep: &str) -> String {
    event["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["type"] == "text")
        .map(|b| crate::text(&b["text"]))
        .collect::<Vec<_>>()
        .join(sep)
}

/// One stream of a session, folded for the shell's decisions
/// (`lib/session-fold.jq`): stop_reason, event_ids, asks, custom, usage,
/// error, in that key order.
pub fn fold(events: &[Value]) -> Value {
    use crate::or;
    use serde_json::json;

    let last = |kinds: &[&str]| {
        events
            .iter()
            .rev()
            .find(|e| kinds.iter().any(|k| e["type"] == *k))
    };
    let null = Value::Null;
    let idle = last(&["session.status_idle", "session.status_terminated"]).unwrap_or(&null);
    let ids = or(&idle["stop_reason"]["event_ids"], &json!([])).clone();
    let waiting = |e: &Value| ids.as_array().is_some_and(|ids| ids.contains(&e["id"]));
    let picked = |kind: &str| -> Vec<Value> {
        events
            .iter()
            .filter(|e| e["type"] == kind && waiting(e))
            .map(|e| json!({"id": e["id"], "name": e["name"], "input": e["input"]}))
            .collect()
    };
    let stop_reason = if idle["type"] == "session.status_terminated" {
        json!("terminated")
    } else {
        idle["stop_reason"]["type"].clone()
    };

    let empty = json!({});
    let u = last(&["session.usage"]).map_or(&null, |e| &e["usage"]);
    let u = or(u, &empty);
    let zero = json!(0);
    let n = |v: &Value| or(v, &zero).clone();
    let cw = &u["cache_creation"];
    let (a, b) = (
        n(&cw["ephemeral_5m_input_tokens"]),
        n(&cw["ephemeral_1h_input_tokens"]),
    );
    let cache_write = match a
        .as_i64()
        .zip(b.as_i64())
        .and_then(|(a, b)| a.checked_add(b))
    {
        Some(sum) => json!(sum),
        None => json!(a.as_f64().unwrap_or(0.0) + b.as_f64().unwrap_or(0.0)),
    };
    let error = last(&["session.error"])
        .map_or(&null, |e| &e["error"])
        .clone();

    json!({
        "stop_reason": stop_reason,
        "event_ids": ids,
        "asks": picked("agent.tool_use"),
        "custom": picked("agent.custom_tool_use"),
        "usage": {
            "input_tokens": n(&u["input_tokens"]),
            "output_tokens": n(&u["output_tokens"]),
            "cache_read": n(&u["cache_read_input_tokens"]),
            "cache_write": cache_write,
            "list_cost": or(&u["list_cost"]["amount"], &json!("0")),
            "active_seconds": n(&u["active_seconds"]),
        },
        "error": error,
    })
}

/// The body of POST /v1/sessions (`lib/session-create.jq`).
#[derive(Debug, Clone, Default)]
pub struct CreateArgs {
    pub advisor: String,
    pub agent: String,
    pub environment: String,
    /// whole cents as a string; "0" means no budget
    pub budget: String,
    pub title: String,
    pub prompt: String,
    pub file_id: String,
    pub file_name: String,
    pub vaults: Vec<String>,
    /// owner/name[@branch] or a GitHub URL
    pub repo: String,
    /// GITHUB_TOKEN, read by the caller; "" with a repo is an error
    pub github_token: String,
    /// memstore ids, `:ro` for read-only
    pub memories: Vec<String>,
}

pub fn create_body(args: &CreateArgs) -> Value {
    use serde_json::{Map, json};

    let mut b = Map::new();
    b.insert("agent".into(), json!({"type": "agent", "id": args.agent}));
    b.insert("environment_id".into(), json!(args.environment));
    if !args.title.is_empty() {
        b.insert("title".into(), json!(args.title));
    }
    if args.budget != "0" {
        b.insert(
            "budget".into(),
            json!({"type": "limit", "max_list_cost": {"amount": args.budget, "currency": "USD"}}),
        );
    }
    let vaults: Vec<&str> = words(&args.vaults).collect();
    if !vaults.is_empty() {
        b.insert("vault_ids".into(), json!(vaults));
    }

    let mut resources = Vec::new();
    if !args.file_id.is_empty() {
        resources.push(json!({
            "type": "file", "file_id": args.file_id,
            "mount_path": format!("/mnt/session/uploads/{}", args.file_name)
        }));
    }
    if !args.repo.is_empty()
        && let Some(r) = repo_resource(&args.repo, &args.github_token)
    {
        resources.push(r);
    }
    for m in words(&args.memories) {
        let mut parts = m.split(':');
        let mut r = json!({"type": "memory_store", "memory_store_id": parts.next().unwrap_or("")});
        if parts.next() == Some("ro") {
            r["access"] = json!("read_only");
        }
        resources.push(r);
    }
    if !resources.is_empty() {
        b.insert("resources".into(), Value::Array(resources));
    }

    b.insert(
        "initial_events".into(),
        json!([{"type": "user.message", "content": [{"type": "text", "text": args.prompt}]}]),
    );
    Value::Object(b)
}

/// Each entry split on spaces, empties dropped: jq's `words` over the
/// space-joined list the bash passes.
fn words(list: &[String]) -> impl Iterator<Item = &str> {
    list.iter()
        .flat_map(|s| s.split(' '))
        .filter(|w| !w.is_empty())
}

/// `owner/name[@branch]` or `https://github.com/owner/name[@branch]`;
/// None when it is neither.
fn repo_resource(repo: &str, token: &str) -> Option<Value> {
    use serde_json::json;
    const GH: &str = "https://github.com/";
    let (path, branch) = match repo.split_once('@') {
        Some((p, b)) if !b.is_empty() => (p, Some(b)),
        Some(_) => return None,
        None => (repo, None),
    };
    let bare = path.strip_prefix(GH).unwrap_or(path);
    let (owner, name) = bare.split_once('/')?;
    if owner.is_empty() || name.is_empty() || name.contains('/') {
        return None;
    }
    let mut r = json!({
        "type": "github_repository",
        "url": if path.starts_with("https://") { path.to_string() } else { format!("{GH}{path}") },
        "mount_path": format!("/workspace/{name}"),
        "authorization_token": token,
    });
    if let Some(branch) = branch {
        r["checkout"] = json!({"type": "branch", "name": branch});
    }
    Some(r)
}

/// A session's messages as `you:` / `agent:` text (`lib/session-history.jq`).
pub fn history(page: &Value) -> String {
    page["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let who = match e["type"].as_str()? {
                "user.message" => "you",
                "agent.message" => "agent",
                _ => return None,
            };
            // the golden is `jq -r`'s output: its own newline after the string's
            Some(crate::stream::safe(&format!("{who}: {}\n", texts(e, "\n"))) + "\n")
        })
        .collect()
}

/// `session_id_ok` in `lib/agent-api.bash`: the shape of a session id the
/// API hands out, `sesn_` and 1 to 128 of letters, digits, `_` or `-`. An
/// id names a file under `sessions/`, a URL path segment and a terminal
/// line, so nothing else passes (CODING_STANDARDS.md, values the API
/// controls).
pub fn id_ok(id: &str) -> bool {
    let _ = id;
    todo!("U5: session pure parts")
}

/// `session_remember`: the stub `agent list` and `agent resume` read,
/// `<sessions>/<id>`, written private through a temp file and a rename.
/// The id is checked here, at the sink, whatever the caller checked.
pub fn remember(
    sessions: &std::path::Path,
    id: &str,
    agent: &str,
    environment: &str,
) -> std::io::Result<()> {
    let _ = (sessions, id, agent, environment);
    todo!("U5: session pure parts")
}

/// `session_find`: the session `arg` names, a unique prefix (with or
/// without `sesn_`) of one started here, or `arg` itself when it is a
/// session id; `Err` is the message for exit 2.
pub fn find(sessions: &std::path::Path, arg: &str) -> Result<String, String> {
    let _ = (sessions, arg);
    todo!("U5: session pure parts")
}

/// The confirmation gate (`session_confirm`, KTD9): `ASK_CLAUDE_CONFIRM`
/// as a policy, and the "always" answers this process gave, by tool name,
/// in memory and never in a file, so no other process can plant or reuse
/// one.
#[derive(Debug)]
pub struct Gate {
    _todo: (),
}

impl Gate {
    /// The gate for `ASK_CLAUDE_CONFIRM`'s value (unset or empty is ask);
    /// `Err` is the message for exit 2.
    pub fn new(value: Option<&str>) -> Result<Self, String> {
        let _ = value;
        todo!("U5: session pure parts")
    }

    /// Whether the call of `name` with `input` (both as shown: through
    /// safe, the input encoded once) may run. The order is KTD9's: the
    /// policy, then no terminal means deny (said on `err`), then an
    /// "always" for this name, then `ask` with the script's prompt, which
    /// returns the line typed or None.
    pub fn confirm(
        &mut self,
        name: &str,
        input: &str,
        tty: bool,
        ask: &mut dyn FnMut(&str) -> Option<String>,
        err: &mut dyn std::io::Write,
    ) -> bool {
        let _ = (name, input, tty, ask, err);
        todo!("U5: session pure parts")
    }
}

/// The tools a `--local-mcp` server offers, for [`answer`]'s custom tool
/// calls (`local_mcp_has` and `local_mcp_call` in `lib/local-mcp.bash`).
pub trait LocalTools {
    /// Whether the server lists a tool of this name.
    fn has(&self, name: &str) -> bool;
    /// Run the call; the `user.custom_tool_result` event.
    fn call(&mut self, tool_use: &Value) -> Value;
}

/// `session_answer`: the events that answer what a fold waits on, as
/// `{"events": [...]}`: a `user.tool_confirmation` per ask, through the
/// gate, then a `user.custom_tool_result` per custom tool call, run on
/// `local` when it lists the tool and the gate allows it, else an error
/// result. What it says goes to `err`.
pub fn answer(
    fold: &Value,
    gate: &mut Gate,
    tty: bool,
    local: Option<&mut dyn LocalTools>,
    ask: &mut dyn FnMut(&str) -> Option<String>,
    err: &mut dyn std::io::Write,
) -> Value {
    let _ = (fold, gate, tty, local, ask, err);
    todo!("U5: session pure parts")
}

//! ask-claude, the library: the shape of the Rust port with the logic left
//! out (docs/plan.md, phase 4). Every function here has a bash or jq twin
//! and a golden under `../tests/fixtures/`; `tests/goldens.rs` holds them to
//! those goldens byte for byte, and `benches/goldens.rs` times them.
//!
//! The contract is the jq one: JSON in, JSON or text out, the same inputs
//! the bash `ask-claude` passes to `lib/*.jq`. How each module represents things
//! inside is the port's business; the typed SSE model in [`sse`] is a
//! starting shape, not a promise.

pub mod cli;

/// Run the parsed CLI; the exit code is the bash script's (0 reply, 1 API
/// error, 2 bad input, 130 after Ctrl-C).
pub fn run(cli: cli::Cli) -> i32 {
    if cli.command.is_some() {
        todo!("R6, R7: list, resume, agent");
    }
    let Some(prompt) = cli.prompt.clone() else {
        todo!("R7: the chat loop");
    };

    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => {
            eprintln!("ask-claude: set ANTHROPIC_API_KEY (https://platform.claude.com/settings/keys)");
            return 2;
        }
    };

    let r = &cli.request;
    let Ok(max_tokens) = r.max_tokens.parse::<u64>() else {
        eprintln!("ask-claude: --max-tokens wants a whole number, not {}", r.max_tokens);
        return 2;
    };
    let skills = match r.skills.as_deref() {
        Some(list) if !list.is_empty() => request::skills(list).to_string(),
        _ => String::new(),
    };
    let args = request::Args {
        advisor: r.advisor.clone().unwrap_or_default(),
        mcp_tool: r.mcp_tool.join(" "),
        model: r.model.clone(),
        max_tokens,
        system: r.system.clone().unwrap_or_default(),
        effort: format!("{:?}", r.effort).to_lowercase(),
        skills: skills.clone(),
        container_id: String::new(),
        web: r.web,
        mcp: r.mcp.clone(),
        mcp_token: std::env::var("ASK_CLAUDE_MCP_TOKEN").unwrap_or_default(),
        messages: serde_json::json!([{ "role": "user", "content": prompt }]),
    };

    match std::env::var_os("ASK_CLAUDE_REPLAY") {
        Some(path) => one_shot(&args, &mut transport::Replay::new(path.into())),
        None => {
            let base_url = r
                .base_url
                .clone()
                .unwrap_or_else(|| "https://api.anthropic.com".to_string());
            let mut betas = String::from("server-side-fallback-2026-07-01");
            if !skills.is_empty() {
                betas.push_str(",code-execution-2025-08-25");
            }
            one_shot(&args, &mut transport::Http::new(base_url, api_key, betas))
        }
    }
}

/// R5: one request, its stream printed to stdout as `lib/stream.jq` does,
/// then the `[model in=.. ..]` line and any `[stopped: ..]` on stderr, the
/// way `call` in `lib/messages.bash` ends a turn. 0 on a reply, 1 on an
/// API or transport error.
pub fn one_shot<T: transport::Transport>(args: &request::Args, t: &mut T) -> i32 {
    use std::io::Write;
    use stream::safe;

    let t0 = std::time::Instant::now();
    let raw = match t.send(&request::body(args).to_string()) {
        Ok(raw) => raw,
        Err(e) => {
            eprintln!("ask-claude: {}", safe(&e.to_string()));
            return 1;
        }
    };

    let mut events: Vec<serde_json::Value> = sse::data_lines(&raw)
        .into_iter()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    if events.is_empty() {
        // not a stream: an error body (`{"type":"error","error":{..}}`), or nothing usable
        match serde_json::from_str::<serde_json::Value>(&raw) {
            Ok(v) if v["type"] == "error" => events.push(v),
            _ => {
                eprintln!("ask-claude: no response from the API");
                return 1;
            }
        }
    }

    let mut out = std::io::stdout().lock();
    for e in &events {
        let _ = out.write_all(stream::line(e).as_bytes());
        let _ = out.flush();
    }
    drop(out);

    let msg = reassemble::message(&events);
    if !msg["error"].is_null() {
        let kind = text(&msg["error"]["type"]);
        let message = text(&msg["error"]["message"]);
        eprintln!("ask-claude: {}", safe(&format!("{kind}: {message}")));
        return 1;
    }

    let advisors: Vec<&serde_json::Value> = msg["usage"]["iterations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|i| i["type"] == "advisor_message")
        .collect();
    let adv = if advisors.is_empty() {
        String::new()
    } else {
        let sum = |k: &str| -> u64 { advisors.iter().filter_map(|a| a[k].as_u64()).sum() };
        format!(
            " advisor={} in={} out={}",
            text(&advisors[0]["model"]),
            sum("input_tokens"),
            sum("output_tokens")
        )
    };
    let u = &msg["usage"];
    eprintln!(
        "{}",
        safe(&format!(
            "[{} in={} cache_read={} cache_write={} out={}{adv} {}s]",
            text(&msg["model"]),
            text(&u["input_tokens"]),
            text(or(&u["cache_read_input_tokens"], &0.into())),
            text(or(&u["cache_creation_input_tokens"], &0.into())),
            text(&u["output_tokens"]),
            t0.elapsed().as_secs()
        ))
    );

    match msg["stop_reason"].as_str() {
        Some("end_turn") => {}
        Some("refusal") => eprintln!(
            "[stopped: refusal {}]",
            safe(&text(or(&msg["stop_details"]["category"], &"".into())))
        ),
        Some("max_tokens") => eprintln!("[stopped: max_tokens reached, raise --max-tokens]"),
        _ => eprintln!("[stopped: {}]", safe(&text(&msg["stop_reason"]))),
    }
    0
}

/// jq's `$a // $b`: `$b` when `$a` is null or false.
fn or<'a>(a: &'a serde_json::Value, b: &'a serde_json::Value) -> &'a serde_json::Value {
    match a {
        serde_json::Value::Null | serde_json::Value::Bool(false) => b,
        _ => a,
    }
}

/// jq's `"\(x)"`: a string as itself, anything else as its JSON text.
fn text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The host of `<scheme>://<host>...` (scheme lowercase letters), the host
/// running up to the first of `stop`; jq's `capture("^[a-z]+://(?<h>[^/]+)")`.
fn url_host<'a>(url: &'a str, stop: &[char]) -> Option<&'a str> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme.is_empty() || !scheme.bytes().all(|b| b.is_ascii_lowercase()) {
        return None;
    }
    let host = &rest[..rest.find(stop).unwrap_or(rest.len())];
    (!host.is_empty()).then_some(host)
}

/// R1: the Messages API event stream, line by line.
pub mod sse {
    use serde::Deserialize;
    use serde_json::Value;

    /// One event of the stream. Every line of every `sse/*.sse` fixture
    /// deserializes, and nothing is swallowed: `kind()` is the JSON `type`.
    /// (A placeholder shape: the port may make this an enum, as long as
    /// `parse` and `kind` keep their meaning.)
    #[derive(Debug, Clone, Deserialize)]
    pub struct Event {
        #[serde(rename = "type")]
        pub kind: String,
        #[serde(flatten)]
        pub rest: Value,
    }

    impl Event {
        pub fn kind(&self) -> &str {
            &self.kind
        }
    }

    /// The JSON of every `data: ` line of a raw SSE body, one per line, the
    /// way the bash loop strips them.
    pub fn data_lines(raw: &str) -> Vec<&str> {
        raw.lines()
            .filter_map(|l| l.strip_prefix("data: "))
            .collect()
    }

    /// One `data:` payload into an [`Event`].
    pub fn parse(line: &str) -> Result<Event, serde_json::Error> {
        serde_json::from_str(line)
    }
}

/// R2: fold an event stream into one message (`lib/reassemble.jq`).
pub mod reassemble {
    use serde_json::Value;

    /// The events of one reply, in order, into the message the history
    /// keeps: `{model, stop_reason, stop_details, usage, container, error,
    /// content}` in that key order, with `content` the blocks in index order
    /// and tool inputs parsed from their `input_json_delta` partials.
    pub fn message(events: &[Value]) -> Value {
        use serde_json::{Map, json};
        use std::collections::BTreeMap;

        let mut blocks: BTreeMap<u64, Value> = BTreeMap::new();
        let mut model = Value::Null;
        let mut stop_reason = Value::Null;
        let mut stop_details = Value::Null;
        let mut usage = Value::Object(Map::new());
        let mut container = Value::Null;
        let mut error = Value::Null;

        /// jq's `.k += $s` on a string field that may be absent.
        fn append(block: &mut Value, key: &str, s: &Value) {
            let s = s.as_str().unwrap_or("");
            match block.get_mut(key) {
                Some(Value::String(cur)) => cur.push_str(s),
                _ => block[key] = Value::String(s.to_string()),
            }
        }

        for e in events {
            let index = e["index"].as_u64().unwrap_or(0);
            match e["type"].as_str().unwrap_or("") {
                "message_start" => {
                    model = e["message"]["model"].clone();
                    usage = e["message"]["usage"].clone();
                    container = e["message"]["container"].clone();
                }
                "content_block_start" => {
                    let mut b = e["content_block"].clone();
                    if matches!(
                        b["type"].as_str(),
                        Some("tool_use" | "server_tool_use" | "mcp_tool_use")
                    ) {
                        b["partial"] = json!("");
                    }
                    blocks.insert(index, b);
                }
                "content_block_delta" => {
                    let b = blocks.entry(index).or_insert_with(|| json!({}));
                    let d = &e["delta"];
                    match d["type"].as_str().unwrap_or("") {
                        "text_delta" => append(b, "text", &d["text"]),
                        "thinking_delta" => append(b, "thinking", &d["thinking"]),
                        "signature_delta" => b["signature"] = d["signature"].clone(),
                        "input_json_delta" => append(b, "partial", &d["partial_json"]),
                        _ => {}
                    }
                }
                "content_block_stop" => {
                    if let Some(Value::Object(b)) = blocks.get_mut(&index)
                        && let Some(partial) = b.remove("partial")
                    {
                        let p = partial.as_str().unwrap_or("");
                        let input = if p.is_empty() {
                            json!({})
                        } else {
                            serde_json::from_str(p).unwrap_or(Value::Null)
                        };
                        b.insert("input".into(), input);
                    }
                }
                "message_delta" => {
                    stop_reason = e["delta"]["stop_reason"].clone();
                    stop_details = e["delta"]["stop_details"].clone();
                    if let Some(add) = e["usage"].as_object() {
                        if !usage.is_object() {
                            usage = Value::Object(Map::new());
                        }
                        let u = usage.as_object_mut().unwrap();
                        for (k, v) in add {
                            u.insert(k.clone(), v.clone());
                        }
                    }
                    if !e["delta"]["container"].is_null() {
                        container = e["delta"]["container"].clone();
                    }
                }
                "error" => error = e["error"].clone(),
                _ => {}
            }
        }

        json!({
            "model": model,
            "stop_reason": stop_reason,
            "stop_details": stop_details,
            "usage": usage,
            "container": container,
            "error": error,
            "content": blocks.into_values().collect::<Vec<_>>(),
        })
    }
}

/// R3: build the request body (`lib/request.jq`, `lib/skills.jq`).
pub mod request {
    use serde_json::Value;

    /// Everything `lib/request.jq` takes as `--arg`, with the same meaning
    /// of "" for "not given".
    #[derive(Debug, Clone, Default)]
    pub struct Args {
        pub advisor: String,
        pub mcp_tool: String,
        pub model: String,
        pub max_tokens: u64,
        pub system: String,
        pub effort: String,
        /// the JSON text `skills` produces, or ""
        pub skills: String,
        pub container_id: String,
        pub web: bool,
        /// MCP server URLs
        pub mcp: Vec<String>,
        /// ASK_CLAUDE_MCP_TOKEN, read by the caller; "" sends none
        pub mcp_token: String,
        /// the history: the `messages` array
        pub messages: Value,
    }

    /// The body of POST /v1/messages, keys in the goldens' order.
    pub fn body(args: &Args) -> Value {
        use serde_json::{Map, json};

        let mut b = Map::new();
        b.insert("model".into(), json!(args.model));
        b.insert("max_tokens".into(), json!(args.max_tokens));
        b.insert("messages".into(), args.messages.clone());
        b.insert("fallbacks".into(), json!("default"));
        b.insert("stream".into(), json!(true));
        b.insert("cache_control".into(), json!({"type": "ephemeral"}));
        if !args.system.is_empty() {
            b.insert("system".into(), json!(args.system));
        }
        if !args.effort.is_empty() {
            b.insert("output_config".into(), json!({"effort": args.effort}));
        }

        let mcp: Vec<&str> = args
            .mcp
            .iter()
            .flat_map(|m| m.split(' '))
            .filter(|w| !w.is_empty())
            .collect();
        if !mcp.is_empty() {
            let servers: Vec<Value> = mcp
                .iter()
                .map(|url| {
                    let mut s = json!({"type": "url", "url": url, "name": host(url)});
                    let token = mcp_token(url, mcp.len(), &args.mcp_token);
                    if !token.is_empty() {
                        s["authorization_token"] = json!(token);
                    }
                    s
                })
                .collect();
            b.insert("mcp_servers".into(), Value::Array(servers));
        }

        let mut tools = Vec::new();
        if !args.skills.is_empty() {
            tools.push(json!({"type": "code_execution_20260521", "name": "code_execution"}));
        }
        if args.web {
            tools.push(json!({"type": "web_search_20260318", "name": "web_search", "max_uses": 5}));
            tools.push(json!({"type": "web_fetch_20260318", "name": "web_fetch", "max_uses": 5}));
        }
        for url in &mcp {
            tools.push(toolset(url, &args.mcp_tool));
        }
        let used = advisor_used(&args.messages);
        if !args.advisor.is_empty() && used < 3 {
            tools.push(json!({
                "type": "advisor_20260301", "name": "advisor",
                "model": args.advisor, "max_uses": 3 - used
            }));
        }
        if !tools.is_empty() {
            b.insert("tools".into(), Value::Array(tools));
        }

        if !args.skills.is_empty() {
            let skills: Value = serde_json::from_str(&args.skills).unwrap_or(Value::Null);
            let mut c = json!({"skills": skills});
            if !args.container_id.is_empty() {
                c["id"] = json!(args.container_id);
            }
            b.insert("container".into(), c);
        } else if !args.container_id.is_empty() {
            // a pause_turn continuation's container: the plain string form
            b.insert("container".into(), json!(args.container_id));
        }
        Value::Object(b)
    }

    /// An MCP server's name: its host with every character outside
    /// `[A-Za-z0-9_-]` made `-`.
    fn host(url: &str) -> String {
        crate::url_host(url, &['/', ':'])
            .unwrap_or("")
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '-' })
            .collect()
    }

    fn hostname(url: &str) -> String {
        crate::url_host(url, &['/', ':']).unwrap_or("").to_ascii_lowercase()
    }

    /// `ASK_CLAUDE_MCP_TOKEN_<HOST>` for this server, else the shared token
    /// when it is the only server.
    fn mcp_token(url: &str, servers: usize, shared: &str) -> String {
        let var = format!("ASK_CLAUDE_MCP_TOKEN_{}", host(url).to_ascii_uppercase().replace('-', "_"));
        match std::env::var(var) {
            Ok(own) if !own.is_empty() => own,
            _ if servers == 1 => shared.to_string(),
            _ => String::new(),
        }
    }

    /// The `mcp_toolset` of one server, with an allowlist when `--mcp-tool`
    /// names any of its tools (`<host>:<tool>`, host case-insensitive).
    fn toolset(url: &str, mcp_tool: &str) -> Value {
        use serde_json::{Map, json};
        let h = hostname(url);
        let allow: Vec<&str> = mcp_tool
            .split(' ')
            .filter(|w| !w.is_empty())
            .filter(|w| w.split(':').next().unwrap_or("").to_ascii_lowercase() == h)
            .map(|w| w.split_once(':').map_or(w, |(_, tool)| tool))
            .collect();
        let mut t = json!({"type": "mcp_toolset", "mcp_server_name": host(url)});
        if !allow.is_empty() {
            let configs: Map<String, Value> = allow
                .iter()
                .map(|tool| (tool.to_string(), json!({"enabled": true})))
                .collect();
            t["default_config"] = json!({"enabled": false});
            t["configs"] = Value::Object(configs);
        }
        t
    }

    /// Advisor calls since the last user turn: the cap is 3 a turn.
    fn advisor_used(messages: &Value) -> u64 {
        let Some(h) = messages.as_array() else { return 0 };
        let start = h.iter().rposition(|m| m["role"] == "user").map_or(0, |u| u + 1);
        h[start..]
            .iter()
            .filter_map(|m| m["content"].as_array())
            .flatten()
            .filter(|b| b["type"] == "server_tool_use" && b["name"] == "advisor")
            .count() as u64
    }

    /// `-s pptx,skill_abc@skver_1` into the `container.skills` array
    /// (`lib/skills.jq`); "" into `[]`.
    pub fn skills(list: &str) -> Value {
        list.split([',', '\n'])
            .filter(|s| !s.is_empty())
            .map(|s| {
                let mut parts = s.split('@');
                let id = parts.next().unwrap_or("");
                serde_json::json!({
                    "skill_id": id,
                    "version": parts.next().unwrap_or("latest"),
                    "type": if id.starts_with("skill_") { "custom" } else { "anthropic" },
                })
            })
            .collect()
    }
}

/// R4: print the stream as it arrives (`lib/stream.jq`).
pub mod stream {
    use serde_json::Value;

    /// What one event prints: text deltas, `(thinking) `, `(web_search) `,
    /// `(running code) `, a newline at `message_stop`, "" otherwise; every
    /// control character but tab and newline removed (`safe`).
    pub fn line(event: &Value) -> String {
        use crate::{or, text};
        let q = Value::from("?");
        let out = match event["type"].as_str().unwrap_or("") {
            "content_block_start" => {
                let b = &event["content_block"];
                let name = b["name"].as_str();
                match b["type"].as_str().unwrap_or("") {
                    "thinking" => "(thinking) ".to_string(),
                    "server_tool_use" => match name {
                        Some(n @ ("web_search" | "web_fetch")) => format!("({n}) "),
                        Some("advisor") => "(advisor) ".to_string(),
                        _ => "(running code) ".to_string(),
                    },
                    "advisor_tool_result"
                        if b["content"]["type"] == "advisor_tool_result_error" =>
                    {
                        format!("(advisor error: {}) ", text(or(&b["content"]["error_code"], &q)))
                    }
                    "mcp_tool_use" => format!("(mcp {}) ", text(or(&b["server_name"], &q))),
                    "bash_code_execution_tool_result" => {
                        let rc = &b["content"]["return_code"];
                        let rc = if rc.is_number() { rc.to_string() } else { "?".to_string() };
                        format!("(result rc={rc}) ")
                    }
                    _ => String::new(),
                }
            }
            "content_block_delta" if event["delta"]["type"] == "text_delta" => {
                text(&event["delta"]["text"])
            }
            "message_stop" => "\n".to_string(),
            _ => String::new(),
        };
        safe(&out)
    }

    /// The `safe` of every printer: keep tab and newline, drop every other
    /// C0 and C1 control character, so a reply cannot drive the terminal.
    pub fn safe(text: &str) -> String {
        text.chars()
            .filter(|&c| {
                let c = c as u32;
                c == 9 || c == 10 || (c >= 32 && c != 127 && !(128..=159).contains(&c))
            })
            .collect()
    }
}

/// R6: Managed Agents sessions (`lib/session*.jq`).
pub mod session {
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
                format!("(web_fetch {}) ", crate::url_host(url, &['/']).unwrap_or(url))
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
        let cache_write = match (n(&cw["ephemeral_5m_input_tokens"]), n(&cw["ephemeral_1h_input_tokens"])) {
            (a, b) if a.is_i64() && b.is_i64() => json!(a.as_i64().unwrap() + b.as_i64().unwrap()),
            (a, b) => json!(a.as_f64().unwrap_or(0.0) + b.as_f64().unwrap_or(0.0)),
        };
        let error = last(&["session.error"]).map_or(&null, |e| &e["error"]).clone();

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
        list.iter().flat_map(|s| s.split(' ')).filter(|w| !w.is_empty())
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
}

/// R5: the transport, with the replay seam the bats suite drives.
pub mod transport {
    /// Where a stream comes from: the API, or `ASK_CLAUDE_REPLAY` (a file,
    /// or a directory whose `1.sse, 2.sse, ...` are the turns in order).
    /// `ASK_CLAUDE_REQUEST_LOG` appends each body that would have been sent.
    pub trait Transport {
        /// Send a body, get the raw SSE of the reply.
        fn send(&mut self, body: &str) -> std::io::Result<String>;
    }

    use std::io;
    use std::path::PathBuf;

    /// POST /v1/messages. An error status is not an `Err`: its body (the
    /// API's `{"type":"error",..}`) comes back for the caller to report.
    pub struct Http {
        base_url: String,
        api_key: String,
        betas: String,
    }

    impl Http {
        pub fn new(base_url: String, api_key: String, betas: String) -> Self {
            Self { base_url, api_key, betas }
        }
    }

    impl Transport for Http {
        fn send(&mut self, body: &str) -> io::Result<String> {
            let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
            ureq::post(&url)
                .config()
                .http_status_as_error(false)
                .build()
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2026-10-21")
                .header("anthropic-beta", &self.betas)
                .header("content-type", "application/json")
                .send(body)
                .map_err(io::Error::other)?
                .body_mut()
                .read_to_string()
                .map_err(io::Error::other)
        }
    }

    /// `ASK_CLAUDE_REPLAY`: a recorded stream instead of the API.
    pub struct Replay {
        path: PathBuf,
        turn: usize,
    }

    impl Replay {
        pub fn new(path: PathBuf) -> Self {
            Self { path, turn: 0 }
        }
    }

    impl Transport for Replay {
        fn send(&mut self, body: &str) -> io::Result<String> {
            if let Some(log) = std::env::var_os("ASK_CLAUDE_REQUEST_LOG") {
                use io::Write;
                let mut f = std::fs::OpenOptions::new().create(true).append(true).open(log)?;
                writeln!(f, "{body}")?;
            }
            self.turn += 1;
            if self.path.is_dir() {
                std::fs::read_to_string(self.path.join(format!("{}.sse", self.turn)))
            } else {
                std::fs::read_to_string(&self.path)
            }
        }
    }
}

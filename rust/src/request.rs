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
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn hostname(url: &str) -> String {
    crate::url_host(url, &['/', ':'])
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// `ASK_CLAUDE_MCP_TOKEN_<HOST>` for this server, else the shared token
/// when it is the only server.
fn mcp_token(url: &str, servers: usize, shared: &str) -> String {
    let var = format!(
        "ASK_CLAUDE_MCP_TOKEN_{}",
        host(url).to_ascii_uppercase().replace('-', "_")
    );
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
    let Some(h) = messages.as_array() else {
        return 0;
    };
    let start = h
        .iter()
        .rposition(|m| m["role"] == "user")
        .map_or(0, |u| u + 1);
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

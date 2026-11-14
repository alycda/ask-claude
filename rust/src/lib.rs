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

/// the Messages API event stream, line by line.
pub mod sse;

/// fold an event stream into one message.
pub mod reassemble;

/// build the request body.
pub mod request;

/// print the stream as it arrives.
pub mod stream;

/// Managed Agents sessions.
pub mod session;

/// transport, with the replay seam the bats suite drives.
pub mod transport;

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
            eprintln!(
                "ask-claude: set ANTHROPIC_API_KEY (https://platform.claude.com/settings/keys)"
            );
            return 2;
        }
    };

    let r = &cli.request;
    let Ok(max_tokens) = r.max_tokens.parse::<u64>() else {
        eprintln!(
            "ask-claude: --max-tokens wants a whole number, not {}",
            r.max_tokens
        );
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

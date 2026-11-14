//! ask-claude, the library: the shape of the Rust port with the logic left
//! out (docs/plan.md, phase 4). Every function here has a bash or jq twin
//! and a golden under `../tests/fixtures/`; `tests/goldens.rs` holds them to
//! those goldens byte for byte, and `benches/goldens.rs` times them.
//!
//! The contract is the jq one: JSON in, JSON or text out, the same inputs
//! the bash `ask-claude` passes to `lib/*.jq`. How each module represents things
//! inside is the port's business; the typed SSE model in [`sse`] is a
//! starting shape, not a promise.

// An API or file input error is an exit code the bash script would give,
// never a panic (docs/plans, KTD4); tests may still unwrap.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod cli;

/// The repository's `VERSION`, the one number `just bump` moves and the
/// bash script copies into `ASK_CLAUDE_VERSION`; `Cargo.toml` carries none.
pub fn version() -> &'static str {
    include_str!("../../VERSION").trim()
}

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

/// the checks every form runs first (`lib/config.bash`).
pub mod config;

/// chats on disk under the state directory.
pub mod history;

/// Run the parsed CLI; the exit code is the bash script's (0 reply, 1 API
/// error, 2 bad input, 130 after Ctrl-C). The order is the script's: the
/// checks of `lib/config.bash` and the state directory for every form,
/// then the form; for the one shot, the key, `-f`, then the request.
pub fn run(cli: cli::Cli) -> i32 {
    let r = &cli.request;
    let cfg = match config::check(r) {
        Ok(cfg) => cfg,
        Err(refused) => {
            eprintln!("ask-claude: {refused}");
            return 2;
        }
    };
    if let Err(refused) = history::state_dir() {
        eprintln!("ask-claude: {refused}");
        return 2;
    }
    if cli.command.is_some() {
        todo!("R6, R7: list, resume, agent");
    }
    let Some(prompt) = cli.prompt.as_deref() else {
        todo!("R6: the chat loop");
    };

    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => {
            // `need_key` in lib/config.bash, word for word
            eprintln!("ask-claude: no key: export ANTHROPIC_API_KEY");
            return 2;
        }
    };
    // -f: the file's contents, trailing newlines dropped as `$(cat ..)`
    // drops them, then a blank line, then the prompt
    let mut text = String::new();
    if let Some(file) = &cli.file {
        match std::fs::read(file) {
            Ok(bytes) => {
                text.push_str(String::from_utf8_lossy(&bytes).trim_end_matches('\n'));
                text.push_str("\n\n");
            }
            Err(_) => {
                eprintln!("ask-claude: cannot read {file}");
                return 2;
            }
        }
    }
    text.push_str(prompt);

    let skills = match r.skills.as_deref() {
        Some(list) if !list.is_empty() => request::skills(list).to_string(),
        _ => String::new(),
    };
    let args = request::Args {
        advisor: r.advisor.clone().unwrap_or_default(),
        mcp_tool: r.mcp_tool.join(" "),
        model: r.model.clone(),
        max_tokens: cfg.max_tokens,
        system: r.system.clone().unwrap_or_default(),
        effort: format!("{:?}", r.effort).to_lowercase(),
        skills,
        container_id: String::new(),
        web: r.web,
        mcp: r.mcp.clone(),
        mcp_token: std::env::var("ASK_CLAUDE_MCP_TOKEN").unwrap_or_default(),
        messages: serde_json::json!([{ "role": "user", "content": text }]),
        tools: Vec::new(),
    };

    let env_path = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(std::path::PathBuf::from)
    };
    let inner: Box<dyn transport::Transport + Send> = match env_path("ASK_CLAUDE_REPLAY") {
        Some(path) => Box::new(transport::Replay::new(path)),
        None => Box::new(transport::Http::new(&cfg.base_url, api_key)),
    };
    let mut t = match transport::Logged::new(
        inner,
        env_path("ASK_CLAUDE_REQUEST_LOG"),
        env_path("ASK_CLAUDE_BETA_LOG"),
    ) {
        Ok(t) => t,
        Err(refused) => {
            eprintln!("ask-claude: {refused}");
            return 2;
        }
    };
    transport::catch_signals();
    one_shot(&args, &mut t)
}

/// The `anthropic-beta` value of a request, as `call` in
/// `lib/messages.bash` sets it once per user turn: skills run in the
/// code-execution sandbox, `--mcp` needs the MCP client, and the advisor
/// beta goes with `--advisor` and also with a history that holds advisor
/// blocks (a chat resumed without `--advisor`), which the API refuses
/// without it.
fn betas(args: &request::Args) -> String {
    let mut betas = String::from("server-side-fallback-2026-07-01");
    if !args.skills.is_empty() {
        betas.push_str(",code-execution-2025-08-25");
    }
    if !args.mcp.is_empty() {
        betas.push_str(",mcp-client-2025-11-20");
    }
    if !args.advisor.is_empty()
        || args
            .messages
            .to_string()
            .contains("\"advisor_tool_result\"")
    {
        betas.push_str(",advisor-tool-2026-03-01");
    }
    betas
}

/// What came back for one send, taken a line at a time as bash's pipeline
/// takes it: `transport | tee "$log" | sse_data | jq --unbuffered -f
/// lib/stream.jq`, then `jq -s -f lib/reassemble.jq` over the same lines.
#[derive(Debug)]
struct Received {
    /// bytes of the body; none is the "no response" path (`[ -s "$log" ]`)
    bytes: usize,
    /// the body as text while no `data: ` line has come: an error body
    body: String,
    /// a `data: ` line came (`grep -q '^data: '`): the body is a stream
    sse: bool,
    /// every event, in order, for the fold; None once a `data:` line was
    /// not JSON, where the printer's jq dies and the fold's fails
    events: Option<Vec<serde_json::Value>>,
}

impl Received {
    fn new() -> Self {
        Self {
            bytes: 0,
            body: String::new(),
            sse: false,
            events: Some(Vec::new()),
        }
    }

    /// One line of the body, its newline included: printed through the
    /// printer when it is a `data:` line, and kept.
    fn take(&mut self, raw: &[u8], out: &mut dyn std::io::Write) {
        self.bytes += raw.len();
        let text = sse::decode(raw);
        let Some(json) = text.strip_prefix("data: ") else {
            if !self.sse {
                self.body.push_str(&text);
            }
            return;
        };
        self.sse = true;
        self.body.clear();
        // `read -r` in sse_data drops a last line with no newline
        let Some(json) = json.strip_suffix('\n') else {
            return;
        };
        if let Some(events) = &mut self.events {
            match reassemble::fromjson(json) {
                Ok(event) => {
                    let _ = out.write_all(stream::line(&event).as_bytes());
                    let _ = out.flush();
                    events.push(event);
                }
                Err(_) => self.events = None,
            }
        }
    }
}

/// A signal stopped the wait for the reply (KTD11).
#[derive(Debug)]
struct Interrupted;

/// Send one request and take its reply line by line as it arrives, the
/// read on a worker thread (KTD3), or stop at an interrupt.
fn exchange(
    t: &mut dyn transport::Transport,
    body: &str,
    betas: &str,
) -> Result<Received, Interrupted> {
    use transport::Next;
    let mut got = Received::new();
    let mut out = std::io::stdout().lock();
    match t.send(body, betas) {
        Ok(reply) => {
            let lines = transport::lines(reply);
            loop {
                match lines.next() {
                    Next::Line(line) => got.take(&line, &mut out),
                    Next::Failed(e) => {
                        failed(&e);
                        break;
                    }
                    Next::End => break,
                    Next::Interrupted => return Err(Interrupted),
                }
            }
        }
        Err(e) => failed(&e),
    }
    Ok(got)
}

/// A transport failure, the way curl -sS reports one before the script's
/// own message.
fn failed(e: &std::io::Error) {
    eprintln!("ask-claude: {}", stream::safe(&e.to_string()));
}

/// The bash `api_error` of a body that is not a stream: its
/// `.error.message`, or "unreadable response" for a body that is not JSON
/// or carries none.
fn api_error(body: &str) -> String {
    use serde_json::Value;
    let message = match serde_json::from_str::<Value>(body) {
        Ok(Value::Null) => Some(String::new()),
        Ok(Value::Object(o)) => match o.get("error") {
            None | Some(Value::Null) => Some(String::new()),
            Some(Value::Object(e)) => Some(match e.get("message") {
                None | Some(Value::Null) | Some(Value::Bool(false)) => String::new(),
                Some(m) => stream::safe(&text(m)),
            }),
            Some(_) => None,
        },
        _ => None,
    };
    match message {
        Some(m) if !m.is_empty() => m,
        _ => "unreadable response".into(),
    }
}

/// R5: one request, its stream printed to stdout as `lib/stream.jq` does
/// while it arrives, then the `[model in=.. ..]` line and any `[stopped:
/// ..]` on stderr, the way `call` in `lib/messages.bash` ends a turn. 0 on
/// a reply, 1 on an API error or no response.
pub fn one_shot(args: &request::Args, t: &mut dyn transport::Transport) -> i32 {
    use stream::safe;

    let t0 = std::time::Instant::now();
    let Ok(got) = exchange(t, &request::body(args).to_string(), &betas(args)) else {
        // Ctrl-C: what was printed stands and nothing more is done, as a
        // killed script does nothing more
        return 130;
    };
    if got.bytes == 0 {
        eprintln!("ask-claude: no response (the request failed or timed out)");
        return 1;
    }
    // each arm is the script's: not SSE is an HTTP error body; a reply jq
    // cannot fold is an error, not an empty turn that vanishes
    let folded = match (&got.sse, &got.events) {
        (false, _) => Err(api_error(&got.body)),
        (true, None) => Err("the reply could not be parsed".to_string()),
        (true, Some(events)) => {
            reassemble::message(events).map_err(|_| "the reply could not be parsed".to_string())
        }
    };
    let msg = match folded {
        Ok(msg) => match or(&msg["error"]["message"], &serde_json::Value::Null) {
            serde_json::Value::Null => msg,
            m => {
                eprintln!("ask-claude: API error: {}", safe(&text(m)));
                return 1;
            }
        },
        Err(err) => {
            eprintln!("ask-claude: API error: {err}");
            return 1;
        }
    };

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
    // a signal during the fold or the notices: the script would be gone
    if transport::interrupted() {
        return 130;
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

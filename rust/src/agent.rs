//! `ask-claude agent`: a Managed Agents session (CONCEPTS.md, Session),
//! the twin of `lib/agent-turn.bash`, the requests of
//! `lib/agent-api.bash`, and the `agent` arms of the script. A turn opens
//! the session's stream, sends what the turn says, prints until the first
//! idle, folds what it read ([`session::fold`]) and answers what the
//! session waits on, until it waits on nothing more.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::cli::{AgentArgs, Cli};
use crate::stream::safe;
use crate::transport::sessions::{self, Sessions};
use crate::transport::{self, Next};
use crate::{config, event, session};

/// The custom-tool rounds a turn answers before it gives up on the
/// session (`session_turn`'s cap; confirmations are the user's and do not
/// count).
const ROUNDS: u64 = 8;

/// How long a turn waits for its stream to say it is connected before it
/// sends anyway (`session_stream_open`'s hundred tenths of a second).
const CONNECT: Duration = Duration::from_secs(10);

/// Run `ask-claude agent ...`: the exit code is the script's (0, 1 for an
/// API or session error, 2 for bad input), and 130 after Ctrl-C (R6).
pub fn run(
    args: &AgentArgs,
    cli: &Cli,
    cfg: &config::Config,
    state: &Path,
    log: &event::Log,
) -> i32 {
    if !matches!(cfg.provider, config::Provider::Anthropic { .. }) {
        return crate::refuse("agent runs Managed Agents sessions, which only -p anthropic has");
    }
    let sessions = state.join("sessions");
    match &args.command {
        None => {}
        Some(_) => todo!("U6: agent list, resume, files, budget, archive, trace and tools"),
    }
    let mut agent = match Agent::new(args, cfg, sessions, log) {
        Ok(agent) => agent,
        Err(code) => return code,
    };
    if cli.file.is_some() {
        todo!("U6 step 3: agent -f uploads the file and attaches it");
    }
    match &args.prompt {
        Some(prompt) => agent.one_shot(prompt),
        None => todo!("U6: the session chat"),
    }
}

/// What every request of an agent run needs, from the checks
/// `session_prereqs` runs first.
struct Agent<'a> {
    t: Box<dyn Sessions + Send>,
    gate: session::Gate,
    #[allow(dead_code)]
    log: &'a event::Log,
    /// `$STATE/sessions`, the stubs `list` and `resume` read
    sessions: PathBuf,
    agent: String,
    environment: String,
    /// whole cents, "0" for none
    budget: String,
}

impl<'a> Agent<'a> {
    /// `session_prereqs`, in its order, then the transport; `Err` is the
    /// exit code after the message. The resources (`--vault`, `--memory`,
    /// `--repo`) are checked first, before the ids.
    fn new(
        args: &AgentArgs,
        cfg: &config::Config,
        sessions: PathBuf,
        log: &'a event::Log,
    ) -> Result<Self, i32> {
        let given = |v: &Option<String>| v.clone().filter(|v| !v.is_empty());
        let Some(agent) = given(&args.agent) else {
            return Err(crate::refuse(
                "no agent id: --agent or ASK_CLAUDE_AGENT (the Console, Agents)",
            ));
        };
        let Some(environment) = given(&args.environment) else {
            return Err(crate::refuse(
                "no environment id: --environment or ASK_CLAUDE_ENVIRONMENT (the Console, Environments)",
            ));
        };
        let budget = &args.budget;
        if budget.is_empty() || !budget.bytes().all(|b| b.is_ascii_digit()) {
            return Err(crate::refuse(&format!(
                "--budget must be whole cents, 0 for none, got: {budget}"
            )));
        }
        let confirm = std::env::var("ASK_CLAUDE_CONFIRM").ok();
        let gate = session::Gate::new(confirm.as_deref()).map_err(|m| crate::refuse(&m))?;
        let api_key = crate::need_key(&cfg.provider).ok_or(2)?;
        let t = open(cfg, api_key).map_err(|m| crate::refuse(&m))?;
        transport::catch_signals();
        Ok(Self {
            t,
            gate,
            log,
            sessions,
            agent,
            environment,
            budget: budget.clone(),
        })
    }

    /// `ma_call`: the response body, or after a request that got none, the
    /// transport's complaint on stderr (as curl's) and the body the script
    /// stands in for it.
    fn call(&mut self, method: &str, path: &str, body: Option<&str>) -> String {
        self.t.call(method, path, body).unwrap_or_else(|e| {
            crate::failed(&e);
            r#"{"error":{"type":"no_response","message":"no response (curl failed or timed out)"}}"#
                .to_string()
        })
    }

    /// The one shot: a new session whose first event is the prompt, one
    /// turn, and the session remembered when the turn went well.
    fn one_shot(&mut self, prompt: &str) -> i32 {
        let Some(id) = self.create(prompt) else {
            return 1;
        };
        let rc = self.turn(&id, None);
        if rc == 0 {
            self.remember(&id);
        }
        rc
    }

    /// `session_create`: POST /v1/sessions with the prompt as its first
    /// event; the new session's id, or None after the API error. The id
    /// names a file and goes into URLs, so one that is not an id is
    /// refused here.
    fn create(&mut self, prompt: &str) -> Option<String> {
        let body = session::create_body(&session::CreateArgs {
            agent: self.agent.clone(),
            environment: self.environment.clone(),
            budget: self.budget.clone(),
            prompt: prompt.to_string(),
            ..Default::default()
        });
        let resp = self.call("POST", "/sessions", Some(&crate::tojson(&body)));
        let Some(id) = id_of(&resp) else {
            let err = api_error(&resp).unwrap_or_else(|| "unreadable response".into());
            eprintln!("ask-claude: API error: {err}");
            return None;
        };
        if !session::id_ok(&id) {
            eprintln!(
                "ask-claude: API error: the session id is not one (sesn_ and letters, digits, _ or -)"
            );
            return None;
        }
        Some(id)
    }

    /// `session_remember`; a stub that cannot be written is said, and the
    /// run goes on.
    fn remember(&self, id: &str) {
        if let Err(e) = session::remember(&self.sessions, id, &self.agent, &self.environment) {
            eprintln!("ask-claude: cannot remember {id}: {e}");
        }
    }

    /// `session_turn`: one user turn on session `id`, `events` the body
    /// that starts it (None when the create carried it), however many
    /// streams it takes. Prints the reply as it arrives and the usage line
    /// on stderr. 0 when the session is idle, 1 on a session error, a
    /// terminated session, no response, or a request the API refused; 130
    /// after a signal.
    fn turn(&mut self, id: &str, mut events: Option<String>) -> i32 {
        let t0 = Instant::now();
        let mut rounds: u64 = 0;
        let fold = loop {
            let mut got = Streamed::default();
            let lines = match self.t.stream(id) {
                Ok(reply) => Some(transport::lines(reply)),
                Err(e) => {
                    got.failure = e.to_string();
                    None
                }
            };
            // KTD3: what a turn sends goes out once the stream is
            // connected, so nothing the session emits for it is missed
            let mut interrupted = lines.as_ref().is_some_and(|l| got.connect(l));
            if !interrupted && let Some(body) = events.take() {
                let resp = self.call("POST", &format!("/sessions/{id}/events"), Some(&body));
                if let Some(err) = api_error(&resp) {
                    eprintln!("ask-claude: API error: {err}");
                    return 1;
                }
            }
            if !interrupted && let Some(lines) = &lines {
                interrupted = got.read(lines);
            }
            if interrupted || transport::interrupted() {
                return 130;
            }
            if !got.sse {
                eprintln!("ask-claude: no response from the stream ({})", got.detail());
                return 1;
            }
            let Some(evs) = &got.events else {
                eprintln!("ask-claude: the stream could not be parsed");
                return 1;
            };
            let fold = session::fold(evs);
            if let Some(err) = message(&fold) {
                eprintln!("ask-claude: session error: {err}");
                return 1;
            }
            if fold["stop_reason"] == "requires_action" {
                let count = |key: &str| fold[key].as_array().map_or(0, Vec::len);
                if count("asks") + count("custom") == 0 {
                    let ids: Vec<String> = fold["event_ids"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(tostring)
                        .collect();
                    eprintln!(
                        "ask-claude: the session waits on something this version cannot answer ({}); interrupting",
                        safe(&ids.join(" "))
                    );
                    self.post_interrupt(id);
                    return 1;
                }
                // custom tool calls are answered here, so they are
                // capped; confirmations are the user's, so they are not
                if count("asks") == 0 {
                    rounds += 1;
                }
                if rounds <= ROUNDS {
                    events = Some(crate::tojson(&self.answer(&fold)));
                    continue;
                }
            }
            break fold;
        };

        let seconds = t0.elapsed().as_secs();
        if let Some(line) = usage_line(id, &fold, seconds) {
            eprintln!("{line}");
        }
        let stop = safe(&tostring(crate::or(&fold["stop_reason"], &"".into())));
        match stop.as_str() {
            "end_turn" | "" => 0,
            "requires_action" => {
                eprintln!(
                    "[stopped: requires_action after {rounds} custom tool rounds; interrupting]"
                );
                self.post_interrupt(id);
                1
            }
            "terminated" => {
                eprintln!("[stopped: the session was terminated]");
                1
            }
            "budget_reached" => {
                eprintln!(
                    "[stopped: budget_reached, raise it with: ask-claude agent budget {id} <cents>]"
                );
                0
            }
            other => {
                eprintln!("[stopped: {other}]");
                0
            }
        }
    }

    /// `session_answer`: the events that answer what `fold` waits on,
    /// through the confirmation gate.
    fn answer(&mut self, fold: &Value) -> Value {
        let mut ask = |_: &str| None;
        session::answer(
            fold,
            &mut self.gate,
            false,
            None,
            &mut ask,
            &mut std::io::stderr(),
        )
    }

    /// Stop the session's work: POST `user.interrupt`, its answer unread.
    fn post_interrupt(&mut self, id: &str) {
        self.call(
            "POST",
            &format!("/sessions/{id}/events"),
            Some(r#"{"events":[{"type":"user.interrupt"}]}"#),
        );
    }
}

/// The sessions transport: the API or `ASK_CLAUDE_REPLAY`, inside the
/// request log; `Err` is a request log that is not private.
fn open(cfg: &config::Config, api_key: String) -> Result<Box<dyn Sessions + Send>, String> {
    let inner: Box<dyn Sessions + Send> = match sessions::replay_path() {
        Some(path) => Box::new(sessions::Replay::new(path)),
        None => Box::new(sessions::Http::new(cfg.provider.base_url(), api_key)),
    };
    let log = std::env::var_os("ASK_CLAUDE_REQUEST_LOG")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    Ok(Box::new(sessions::Logged::new(inner, log)?))
}

/// What one stream of a turn brought, taken a line at a time as the
/// script's `session_stream_read | sse_data | jq -f lib/session.jq` takes
/// it, and kept for `lib/session-fold.jq`.
#[derive(Debug, Default)]
struct Streamed {
    /// the first bytes read, for the no-response message
    head: Vec<u8>,
    /// a `data: ` line came (`grep -q '^data: '`)
    sse: bool,
    /// every event so far, None once a `data:` line was not JSON (the
    /// printer's jq dies there and the fold's fails)
    events: Option<Vec<Value>>,
    /// the first idle or termination was read, or the stream ended
    done: bool,
    /// what the transport said when the stream failed (curl's stderr)
    failure: String,
}

impl Streamed {
    /// Wait for the connected comment, at most [`CONNECT`], taking what
    /// comes before it; true on an interrupt.
    fn connect(&mut self, lines: &transport::Lines) -> bool {
        let deadline = Instant::now() + CONNECT;
        while !self.done {
            match lines.next_until(deadline) {
                Some(Next::Line(line)) => {
                    let connected = line.starts_with(b": connected");
                    self.take(&line);
                    if connected {
                        break;
                    }
                }
                Some(Next::Interrupted) => return true,
                Some(next) => self.end(next),
                None => break,
            }
        }
        false
    }

    /// Read to the first idle or termination, or the end; true on an
    /// interrupt.
    fn read(&mut self, lines: &transport::Lines) -> bool {
        while !self.done {
            match lines.next() {
                Next::Line(line) => self.take(&line),
                Next::Interrupted => return true,
                next => self.end(next),
            }
        }
        false
    }

    fn end(&mut self, next: Next) {
        if let Next::Failed(e) = next {
            self.failure = e.to_string();
        }
        self.done = true;
    }

    /// One line, its newline included: printed when it is a `data:` line,
    /// kept for the fold; the turn's stream ends after the first idle or
    /// termination (`sed` quitting on it).
    fn take(&mut self, raw: &[u8]) {
        if self.head.len() < 200 {
            self.head
                .extend_from_slice(&raw[..raw.len().min(200 - self.head.len())]);
        }
        let text = crate::sse::decode(raw);
        self.done = ends_turn(&text);
        let Some(json) = text.strip_prefix("data: ") else {
            return;
        };
        if !self.sse {
            self.sse = true;
            self.events = Some(Vec::new());
        }
        // `read -r` in sse_data drops a last line with no newline
        let Some(json) = json.strip_suffix('\n') else {
            return;
        };
        if let Some(events) = &mut self.events {
            match crate::reassemble::fromjson(json) {
                Ok(event) => {
                    let mut out = std::io::stdout().lock();
                    let _ = out.write_all(session::line(&event).as_bytes());
                    let _ = out.flush();
                    events.push(event);
                }
                Err(_) => self.events = None,
            }
        }
    }

    /// The start of what the stream said, the transport's complaint
    /// first, as `head -c 200 "$log.err" "$log" | tr -d '\n'`, through
    /// safe, cut at 200 characters.
    fn detail(&self) -> String {
        let failure = self.failure.as_bytes();
        let mut bytes = failure[..failure.len().min(200)].to_vec();
        bytes.extend_from_slice(&self.head);
        let text: String = String::from_utf8_lossy(&bytes)
            .chars()
            .filter(|&c| c != '\n')
            .collect();
        safe(&text).chars().take(200).collect()
    }
}

/// Whether a stream line is the turn's end: `sed`'s
/// `/"type":"session.status_idle"/q` and its `status_terminated` twin,
/// the dot any one character.
fn ends_turn(line: &str) -> bool {
    const AT: &str = "\"type\":\"session";
    line.match_indices(AT).any(|(i, _)| {
        let mut rest = line[i + AT.len()..].chars();
        rest.next().is_some()
            && ["status_idle\"", "status_terminated\""]
                .iter()
                .any(|end| rest.as_str().starts_with(end))
    })
}

/// `api_error` in `lib/common.bash` over a sessions response: the API's
/// `.error.message` through safe, "unreadable response" for a body that
/// is not JSON, and None when there is no error (no `.error`, a null
/// message, an empty body).
fn api_error(body: &str) -> Option<String> {
    let mut out = Vec::new();
    for value in serde_json::Deserializer::from_str(body).into_iter::<Value>() {
        let Ok(value) = value else {
            out.push("unreadable response".to_string());
            break;
        };
        let error = match &value {
            Value::Null => continue,
            Value::Object(o) => o.get("error").unwrap_or(&Value::Null),
            _ => {
                out.push("unreadable response".to_string());
                break;
            }
        };
        let m = match error {
            Value::Null => continue,
            Value::Object(e) => crate::or(e.get("message").unwrap_or(&Value::Null), &Value::Null),
            _ => {
                out.push("unreadable response".to_string());
                break;
            }
        };
        if !m.is_null() {
            out.push(safe(&crate::text(m)));
        }
    }
    let text = out.join("\n");
    (!text.is_empty()).then_some(text)
}

/// A fold's error message, as `api_error` reads it from the fold: None
/// when it has none.
fn message(fold: &Value) -> Option<String> {
    let m = crate::or(&fold["error"]["message"], &Value::Null);
    let text = if m.is_null() {
        String::new()
    } else {
        safe(&crate::text(m))
    };
    (!text.is_empty()).then_some(text)
}

/// `jq -r '.id // empty'` over a create response: the first value's id
/// as text.
fn id_of(body: &str) -> Option<String> {
    let value = serde_json::Deserializer::from_str(body)
        .into_iter::<Value>()
        .next()?
        .ok()?;
    let id = match &value {
        Value::Object(o) => crate::or(o.get("id").unwrap_or(&Value::Null), &Value::Null),
        _ => &Value::Null,
    };
    (!id.is_null()).then(|| crate::text(id))
}

/// jq's `tostring`: a string as itself, anything else as its JSON.
fn tostring(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => crate::tojson(other),
    }
}

/// `DOLLARS` in `lib/common.bash`: cents, as the API gives them, as
/// `$1.23`; None where jq's `tonumber` fails.
fn dollars(cents: &Value) -> Option<String> {
    let n = match cents {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => s.parse::<f64>().ok().filter(|n| n.is_finite())?,
        _ => return None,
    };
    let d = n / 100.0;
    let whole = d.floor();
    // jq's `%` truncates both sides to integers
    let cents = ((d * 100.0).round() as i64) % 100;
    let whole = if whole.abs() < 1e15 {
        format!("{}", whole as i64)
    } else {
        crate::text(&serde_json::json!(whole))
    };
    Some(format!("${whole}.{cents:02}"))
}

/// The line `session_turn` prints after a turn: the session, tokens, the
/// cost so far, seconds; None where jq would print nothing.
fn usage_line(id: &str, fold: &Value, seconds: u64) -> Option<String> {
    let u = &fold["usage"];
    let n = |k: &str| crate::text(&u[k]);
    Some(safe(&format!(
        "[{id} in={} out={} cache_read={} cache_write={} cost={} {seconds}s]",
        n("input_tokens"),
        n("output_tokens"),
        n("cache_read"),
        n("cache_write"),
        dollars(&u["list_cost"])?,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dollars_is_the_jq_definition() {
        assert_eq!(dollars(&json!("0")).as_deref(), Some("$0.00"));
        assert_eq!(dollars(&json!("250")).as_deref(), Some("$2.50"));
        assert_eq!(dollars(&json!("7")).as_deref(), Some("$0.07"));
        assert_eq!(dollars(&json!(1234)).as_deref(), Some("$12.34"));
        assert_eq!(dollars(&json!("12.5")).as_deref(), Some("$0.13"));
        assert_eq!(dollars(&json!("abc")), None);
        assert_eq!(dollars(&json!(null)), None);
    }

    #[test]
    fn api_error_reads_the_message_or_says_unreadable() {
        assert_eq!(api_error(""), None);
        assert_eq!(api_error("{}"), None);
        assert_eq!(api_error("null"), None);
        assert_eq!(api_error(r#"{"error":null}"#), None);
        assert_eq!(api_error(r#"{"error":{"type":"x"}}"#), None);
        assert_eq!(
            api_error(r#"{"error":{"message":"is archived"}}"#).as_deref(),
            Some("is archived")
        );
        assert_eq!(
            api_error("Bad Gateway").as_deref(),
            Some("unreadable response")
        );
        assert_eq!(api_error("[]").as_deref(), Some("unreadable response"));
        assert_eq!(
            api_error(r#"{"error":{"message":"a\u001b[2Jb"}}"#).as_deref(),
            Some("a[2Jb")
        );
    }

    #[test]
    fn a_turn_ends_at_the_session_idle_not_the_thread_idle() {
        assert!(ends_turn(
            r#"data: {"id":"x","type":"session.status_idle"}"#
        ));
        assert!(ends_turn(r#"data: {"type":"session.status_terminated"}"#));
        assert!(!ends_turn(r#"data: {"type":"session.thread_status_idle"}"#));
        assert!(!ends_turn(r#"data: {"type":"session.status_running"}"#));
    }
}

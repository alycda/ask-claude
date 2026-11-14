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

use crate::cli::{AgentArgs, AgentCommand, Cli};
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
        Some(AgentCommand::List) => return list(cfg),
        Some(AgentCommand::Resume { session }) => {
            // the session first, as `agent resume` finds it before the checks
            let id = match session::find(&sessions, session) {
                Ok(id) => id,
                Err(refused) => return crate::refuse(&refused),
            };
            let mut agent = match Agent::new(args, cfg, sessions, log) {
                Ok(agent) => agent,
                Err(code) => return code,
            };
            eprintln!("resuming {id}");
            if let Some(text) = agent.history(&id) {
                eprint!("{text}");
            }
            return agent.chat(Some(id));
        }
        Some(AgentCommand::Budget { session, cents }) => {
            return budget(cfg, &sessions, session, cents);
        }
        Some(AgentCommand::Archive { session }) => return archive(cfg, &sessions, session),
        Some(AgentCommand::Trace { session }) => return trace(&sessions, session),
        Some(_) => todo!("U6 step 3: agent files; agent tools"),
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
        None => agent.chat(None),
    }
}

/// `agent list`, `session_list`: the API's sessions, newest first, one
/// line each, `id  status  cost  title`, through safe; 0 whatever the API
/// said, as the script's `exit 0` after it.
fn list(cfg: &config::Config) -> i32 {
    let mut t = match connect(cfg) {
        Ok(t) => t,
        Err(code) => return code,
    };
    let page = request(&mut *t, "GET", "/sessions?order=desc&limit=50", None);
    if let Some(err) = api_error(&page) {
        eprintln!("ask-claude: API error: {err}");
        return 0;
    }
    let Ok(page) = serde_json::from_str::<Value>(&page) else {
        return 0;
    };
    let cell = |v: &Value| match v {
        Value::Null => String::new(),
        v => crate::text(v),
    };
    for s in each(&page["data"]) {
        let empty = Value::from("");
        let cost = crate::or(&s["usage"]["list_cost"]["amount"], &"0".into()).clone();
        // jq stops at the row whose cost it cannot read
        let Some(cost) = dollars(&cost) else {
            break;
        };
        let row = [
            cell(&s["id"]),
            cell(&s["status"]),
            cost,
            cell(crate::or(&s["title"], &empty)),
        ];
        crate::chat::out(&(safe(&row.join("  ")) + "\n"));
    }
    0
}

/// jq's `.[]?`: an array's elements, an object's values, else nothing.
fn each(v: &Value) -> Vec<&Value> {
    match v {
        Value::Array(a) => a.iter().collect(),
        Value::Object(o) => o.values().collect(),
        _ => Vec::new(),
    }
}

/// The session an `agent <command> <session>` names (`session_find`), or
/// the exit code after its refusal.
fn named(sessions: &Path, session: &str) -> Result<String, i32> {
    session::find(sessions, session).map_err(|refused| crate::refuse(&refused))
}

/// `agent budget`, `session_budget`: a new spend cap in whole cents, 0 to
/// remove it (for good, the API says), checked before the key; the cap the
/// API answers with, as `budget: $2.50` or `budget: none`. 1 after an API
/// error or an answer whose cap cannot be read.
fn budget(cfg: &config::Config, sessions: &Path, session: &str, cents: &str) -> i32 {
    let id = match named(sessions, session) {
        Ok(id) => id,
        Err(code) => return code,
    };
    if cents.is_empty() || !cents.bytes().all(|b| b.is_ascii_digit()) {
        return crate::refuse(&format!(
            "budget takes whole cents, 0 to remove the cap, got: {cents}"
        ));
    }
    let mut t = match connect(cfg) {
        Ok(t) => t,
        Err(code) => return code,
    };
    let body = if cents == "0" {
        r#"{"budget":null}"#.to_string()
    } else {
        crate::tojson(&serde_json::json!({
            "budget": {"type": "limit", "max_list_cost": {"amount": cents, "currency": "USD"}}
        }))
    };
    let resp = request(&mut *t, "POST", &format!("/sessions/{id}"), Some(&body));
    if let Some(err) = api_error(&resp) {
        eprintln!("ask-claude: API error: {err}");
        return 1;
    }
    // jq over an empty answer prints nothing and succeeds
    let Some(Ok(answer)) = serde_json::Deserializer::from_str(&resp)
        .into_iter::<Value>()
        .next()
    else {
        return 0;
    };
    let cap = if answer["budget"].is_null() {
        "none".to_string()
    } else {
        match dollars(&answer["budget"]["max_list_cost"]["amount"]) {
            Some(cap) => cap,
            None => return 1,
        }
    };
    crate::chat::out(&(safe(&format!("budget: {cap}")) + "\n"));
    0
}

/// `agent archive`, `session_archive`: archive the session (the API
/// refuses a running one) and forget its stub here.
fn archive(cfg: &config::Config, sessions: &Path, session: &str) -> i32 {
    let id = match named(sessions, session) {
        Ok(id) => id,
        Err(code) => return code,
    };
    let mut t = match connect(cfg) {
        Ok(t) => t,
        Err(code) => return code,
    };
    let resp = request(&mut *t, "POST", &format!("/sessions/{id}/archive"), None);
    if let Some(err) = api_error(&resp) {
        eprintln!("ask-claude: API error: {err}");
        return 1;
    }
    // `rm -f`: a session not started here has no stub to remove
    let _ = std::fs::remove_file(sessions.join(&id));
    crate::chat::out(&format!("archived {id}\n"));
    0
}

/// `agent trace`: the Console URL of the session, in the workspace
/// `ASK_CLAUDE_WORKSPACE` names (`default` when unset or empty); no key,
/// no request.
fn trace(sessions: &Path, session: &str) -> i32 {
    let id = match named(sessions, session) {
        Ok(id) => id,
        Err(code) => return code,
    };
    let workspace = std::env::var("ASK_CLAUDE_WORKSPACE")
        .ok()
        .filter(|w| !w.is_empty())
        .unwrap_or_else(|| "default".into());
    crate::chat::out(&format!(
        "https://platform.claude.com/workspaces/{workspace}/sessions/{id}\n"
    ));
    0
}

/// What every request of an agent run needs, from the checks
/// `session_prereqs` runs first.
struct Agent<'a> {
    t: Box<dyn Sessions + Send>,
    gate: session::Gate,
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
        request(&mut *self.t, method, path, body)
    }

    /// `session_history`: the session's messages so far, from the API,
    /// as `you:` and `agent:` lines; None after the API error.
    fn history(&mut self, id: &str) -> Option<String> {
        let path = format!("/sessions/{id}/events?types[]=user.message&types[]=agent.message");
        let page = self.call("GET", &path, None);
        if let Some(err) = api_error(&page) {
            eprintln!("ask-claude: API error: {err}");
            return None;
        }
        Some(session::history(
            &serde_json::from_str(&page).unwrap_or_default(),
        ))
    }

    /// `session_loop`: one turn per line until `/quit`, `/exit` or the end
    /// of input. With no session yet the first line creates one, so
    /// quitting first leaves nothing; `/new` drops the session and the
    /// next line creates another; `/history` prints it. On the way out,
    /// the command that resumes it. 0, or 130 after a signal, with no
    /// epilogue (R6).
    fn chat(&mut self, mut id: Option<String>) -> i32 {
        use crate::chat::{Input, Read, out};
        eprintln!(
            "ask-claude: agent {}  (/new starts another session, /quit exits)",
            self.agent
        );
        let mut input = Input::new();
        loop {
            let line = match input.read() {
                Read::Line(line) => line,
                Read::End => break,
                Read::Interrupted => return 130,
            };
            match line.as_str() {
                "" => continue,
                "/quit" | "/exit" => break,
                "/history" => {
                    if let Some(text) = id.clone().and_then(|id| self.history(&id)) {
                        out(&text);
                    }
                    continue;
                }
                "/new" => {
                    id = None;
                    eprintln!("new session on the next line");
                    continue;
                }
                _ => {}
            }
            let rc = match id.clone() {
                Some(id) => self.turn(&id, Some(user_message(&line))),
                None => {
                    // a create that failed was said; the next line tries again
                    let Some(new) = self.create(&line) else {
                        continue;
                    };
                    let rc = self.turn(&new, None);
                    if rc == 0 {
                        self.remember(&new);
                    }
                    id = Some(new);
                    rc
                }
            };
            if rc == 130 {
                return 130;
            }
            out("\n");
        }
        if let Some(id) = id {
            eprintln!("resume with: ask-claude agent resume {id}");
        }
        0
    }

    /// The one shot: a new session whose first event is the prompt, one
    /// turn, and the session remembered when the turn went well.
    fn one_shot(&mut self, prompt: &str) -> i32 {
        let Some(id) = self.create(prompt) else {
            // a signal during the create: there is no session to stop
            return if transport::interrupted() { 130 } else { 1 };
        };
        let rc = self.turn(&id, None);
        if rc == 0 {
            self.remember(&id);
        }
        rc
    }

    /// `session_create`: POST /v1/sessions with the prompt as its first
    /// event; the new session's id, or None after the API error and its
    /// line in the event log. The id names a file and goes into URLs, so
    /// one that is not an id is refused here.
    fn create(&mut self, prompt: &str) -> Option<String> {
        let t0 = Instant::now();
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
            self.event(&Value::Null, 0, t0, &crate::error_kind(&resp), "");
            return None;
        };
        if !session::id_ok(&id) {
            eprintln!(
                "ask-claude: API error: the session id is not one (sesn_ and letters, digits, _ or -)"
            );
            self.event(&Value::Null, 0, t0, "unreadable", "");
            return None;
        }
        Some(id)
    }

    /// `event session ...`: one line in the event log for a session
    /// request, its input the fold or null, `chat` the session or "".
    fn event(&self, input: &Value, round: u64, t0: Instant, error: &str, chat: &str) {
        let seconds = t0.elapsed().as_secs();
        self.log
            .record("session", input, round, seconds, error, chat, &Value::Null);
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
    /// after a signal. One line in the event log, whatever the outcome.
    fn turn(&mut self, id: &str, mut events: Option<String>) -> i32 {
        let t0 = Instant::now();
        let mut rounds: u64 = 0;
        let null = Value::Null;
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
                    // ma_call's own body for no response says no_response
                    self.event(&null, rounds, t0, &crate::error_kind(&resp), id);
                    return 1;
                }
            }
            if !interrupted && let Some(lines) = &lines {
                interrupted = got.read(lines);
            }
            if interrupted || transport::interrupted() {
                return self.interrupt(id, rounds, t0);
            }
            if !got.sse {
                eprintln!("ask-claude: no response from the stream ({})", got.detail());
                self.event(&null, rounds, t0, "no_response", id);
                return 1;
            }
            let Some(evs) = &got.events else {
                eprintln!("ask-claude: the stream could not be parsed");
                self.event(&null, rounds, t0, "parse", id);
                return 1;
            };
            let fold = session::fold(evs);
            if let Some(err) = message(&fold) {
                eprintln!("ask-claude: session error: {err}");
                let kind = crate::text(crate::or(&fold["error"]["type"], &"unknown".into()));
                self.event(&fold, rounds, t0, &kind, id);
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
                    self.event(&fold, rounds, t0, "unanswerable", id);
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
                    // a signal while the terminal was asked
                    if transport::interrupted() {
                        return self.interrupt(id, rounds, t0);
                    }
                    continue;
                }
            }
            break fold;
        };

        let seconds = t0.elapsed().as_secs();
        if let Some(line) = usage_line(id, &fold, seconds) {
            eprintln!("{line}");
        }
        // at the cap the last round was counted but not answered
        self.event(&fold, rounds.min(ROUNDS), t0, "", id);
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
    /// through the confirmation gate, which asks on the terminal when
    /// stdin is one (`session_tty`) and `/dev/tty` can be read.
    fn answer(&mut self, fold: &Value) -> Value {
        use std::io::IsTerminal;
        let tty = std::io::stdin().is_terminal() && std::fs::File::open("/dev/tty").is_ok();
        session::answer(
            fold,
            &mut self.gate,
            tty,
            None,
            &mut ask_tty,
            &mut std::io::stderr(),
        )
    }

    /// After Ctrl-C (or SIGTERM, SIGHUP) in a turn: an `interrupted` line
    /// in the event log, the session told to stop too, then 130 (R6,
    /// KTD11). The reader thread is left to the process exit.
    fn interrupt(&mut self, id: &str, rounds: u64, t0: Instant) -> i32 {
        eprintln!("[interrupted: sending user.interrupt]");
        self.event(&Value::Null, rounds, t0, "interrupted", id);
        self.post_interrupt(id);
        130
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

/// `read -r -p <prompt> </dev/tty` in `session_confirm`: the prompt on
/// stderr, then one line from the terminal, its newline dropped. The read
/// is on a thread, one byte at a time so it takes no more than the line,
/// and the main thread looks at the interrupt flag while it waits (KTD3);
/// None after a signal, or when the terminal cannot be read, which the
/// gate takes as no.
fn ask_tty(prompt: &str) -> Option<String> {
    use std::io::Read;
    let mut tty = std::fs::File::open("/dev/tty").ok()?;
    eprint!("{prompt}");
    let _ = std::io::stderr().flush();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        while let Ok(1) = tty.read(&mut byte) {
            if byte[0] == b'\n' {
                break;
            }
            line.push(byte[0]);
        }
        let _ = tx.send(String::from_utf8_lossy(&line).into_owned());
    });
    loop {
        if transport::interrupted() {
            return None;
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => return Some(line),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return None,
        }
    }
}

/// The body `ma_call` stands in for a request that got no response.
const NO_RESPONSE: &str =
    r#"{"error":{"type":"no_response","message":"no response (curl failed or timed out)"}}"#;

/// The events of a line typed in a session chat.
fn user_message(line: &str) -> String {
    crate::tojson(&serde_json::json!({
        "events": [{"type": "user.message", "content": [{"type": "text", "text": line}]}]
    }))
}

/// `ma_call`: the response body, or after a request that got none, the
/// transport's complaint on stderr (as curl's) and the body the script
/// stands in for it.
fn request(t: &mut dyn Sessions, method: &str, path: &str, body: Option<&str>) -> String {
    t.call(method, path, body).unwrap_or_else(|e| {
        crate::failed(&e);
        NO_RESPONSE.to_string()
    })
}

/// The key (`need_key`), then the transport, for the commands that run no
/// session turn; `Err` is the exit code after the message.
fn connect(cfg: &config::Config) -> Result<Box<dyn Sessions + Send>, i32> {
    let api_key = crate::need_key(&cfg.provider).ok_or(2)?;
    open(cfg, api_key).map_err(|refused| crate::refuse(&refused))
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

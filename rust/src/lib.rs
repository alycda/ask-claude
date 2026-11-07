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
pub fn run(_cli: cli::Cli) -> i32 {
    todo!("R5..R7: the one shot, the chat, history, sessions")
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
    pub fn message(_events: &[Value]) -> Value {
        todo!("R2: the fold of lib/reassemble.jq")
    }
}

/// R3: build the request body (`lib/request.jq`, `lib/skills.jq`).
pub mod request {
    use serde_json::Value;

    /// Everything `lib/request.jq` takes as `--arg`, with the same meaning
    /// of "" for "not given".
    #[derive(Debug, Clone, Default)]
    pub struct Args {
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
    pub fn body(_args: &Args) -> Value {
        todo!("R3: lib/request.jq")
    }

    /// `-s pptx,skill_abc@skver_1` into the `container.skills` array
    /// (`lib/skills.jq`); "" into `[]`.
    pub fn skills(_list: &str) -> Value {
        todo!("R3: lib/skills.jq")
    }
}

/// R4: print the stream as it arrives (`lib/stream.jq`).
pub mod stream {
    use serde_json::Value;

    /// What one event prints: text deltas, `(thinking) `, `(web_search) `,
    /// `(running code) `, a newline at `message_stop`, "" otherwise; every
    /// control character but tab and newline removed (`safe`).
    pub fn line(_event: &Value) -> String {
        todo!("R4: lib/stream.jq")
    }

    /// The `safe` of every printer: keep tab and newline, drop every other
    /// C0 and C1 control character, so a reply cannot drive the terminal.
    pub fn safe(_text: &str) -> String {
        todo!("R4: safe, shared by every printer")
    }
}

/// R6: Managed Agents sessions (`lib/session*.jq`).
pub mod session {
    use serde_json::Value;

    /// What one session event prints (`lib/session.jq`).
    pub fn line(_event: &Value) -> String {
        todo!("R6: lib/session.jq")
    }

    /// One stream of a session, folded for the shell's decisions
    /// (`lib/session-fold.jq`): stop_reason, event_ids, asks, custom, usage,
    /// error, in that key order.
    pub fn fold(_events: &[Value]) -> Value {
        todo!("R6: lib/session-fold.jq")
    }

    /// The body of POST /v1/sessions (`lib/session-create.jq`).
    #[derive(Debug, Clone, Default)]
    pub struct CreateArgs {
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

    pub fn create_body(_args: &CreateArgs) -> Value {
        todo!("R6: lib/session-create.jq")
    }

    /// A session's messages as `you:` / `agent:` text (`lib/session-history.jq`).
    pub fn history(_page: &Value) -> String {
        todo!("R6: lib/session-history.jq")
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
}

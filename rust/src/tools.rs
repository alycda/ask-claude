//! Local tools (CONCEPTS.md, Local tool and Tool runner): Anthropic's bash
//! and text editor tools run on this machine, behind the confirmation gate,
//! with the bounds KTD10 sets. U10 of
//! `docs/plans/2026-11-06-0143-feat-rust-parity-and-local-tools-plan.md`.
//!
//! Scaffold: the shape `tests/tools.rs` drives, with the logic left out. The
//! names are a starting shape for U10's feat commit, not a promise; a change
//! to them is a change to the tests, in its own commit.
//!
//! Everything the runner reads from outside comes in through [`Config`], so
//! the tests set the environment, the secrets, the bounds and the interrupt
//! without touching the process's own.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::session::Gate;

/// The bash tool's name, as the request declares it (`bash_20250124`).
pub const BASH: &str = "bash";

/// The text editor tool's name (`text_editor_20250728`).
pub const EDITOR: &str = "str_replace_based_edit_tool";

/// What a call returns to the model: a `tool_result`'s content and flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// the result text, capped, then with every secret redacted
    pub content: String,
    /// the `tool_result`'s `is_error`
    pub is_error: bool,
}

/// KTD10's bounds. The defaults are the plan's; the tests shorten them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// how long a bash command runs before its process group gets TERM
    pub timeout: Duration,
    /// how long after TERM before the group gets KILL
    pub grace: Duration,
    /// bytes of output kept, as head and tail around an omission marker
    pub cap: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(120),
            grace: Duration::from_secs(2),
            cap: 30_000,
        }
    }
}

/// What a runner is given. The binary fills it from the process: the cwd,
/// `$HOME`, the state directory, the environment, the keys it holds, and
/// [`crate::transport::interrupted`].
pub struct Config {
    /// the repository root, canonicalized by [`Runner::new`]
    pub root: PathBuf,
    /// `$HOME`: a root here, or at `/`, is refused
    pub home: PathBuf,
    /// the state directory: bash's temp files are written here, and an
    /// editor write inside it is protected when it lies under the root
    pub state: PathBuf,
    /// the environment the child's is built from, by KTD10's allowlist
    pub env: Vec<(String, String)>,
    /// the values replaced by `[redacted]` in what a call returns
    pub secrets: Vec<String>,
    /// directories protected beyond KTD10's list: an enabled plugin's (U14)
    pub protected: Vec<PathBuf>,
    /// the bounds on a bash command
    pub limits: Limits,
    /// whether a signal asked this process to stop
    pub interrupted: Arc<dyn Fn() -> bool + Send + Sync>,
}

/// The Tool runner: one function from a tool call to its [`Outcome`].
pub struct Runner {
    /// what it was given, the root canonicalized
    pub config: Config,
    /// whether the line saying an "always" removed bash's prompt was printed
    pub warned: bool,
}

impl Runner {
    /// The runner for `config`, its root canonicalized. `Err` is the exit-2
    /// message: a root at `$HOME` or at `/`, or one that does not exist.
    pub fn new(config: Config) -> Result<Self, String> {
        let _ = config;
        todo!("U10: Runner::new")
    }

    /// One call, `name` and `input` as the `tool_use` block carries them:
    /// the gate first (KTD9, with a protected path prompting even under
    /// "always" or `allow`, and denied without a terminal), then the tool.
    /// `tty`, `ask` and `err` are [`Gate::confirm`]'s.
    pub fn run(
        &mut self,
        name: &str,
        input: &Value,
        gate: &mut Gate,
        tty: bool,
        ask: &mut dyn FnMut(&str) -> Option<String>,
        err: &mut dyn Write,
    ) -> Outcome {
        let _ = (name, input, gate, tty, ask, err);
        todo!("U10: Runner::run")
    }
}

//! The CLI: one clap definition that mirrors the `#USAGE` block of
//! the bash `ask-claude` flag for flag. `--usage-spec` (main.rs) prints it as a usage
//! spec so `usage diff ../ask-claude <(ask-claude --usage-spec)` can say
//! whether the two binaries still agree (docs/plan.md R0, the parity gate).

use clap::parser::ValueSource;
use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};

/// chat loop; /new resets, /quit exits
#[derive(Parser, Debug)]
#[command(
    name = "ask-claude",
    bin_name = "ask-claude",
    disable_version_flag = true,
    disable_help_subcommand = true
)]
pub struct Cli {
    /// one shot, prints the response
    #[arg(value_name = "prompt")]
    pub prompt: Option<String>,

    #[command(flatten)]
    pub request: RequestFlags,

    /// prepend this file's contents to the prompt
    #[arg(short = 'f', long, global = true, value_name = "file")]
    pub file: Option<String>,

    /// print the version
    #[arg(short = 'V', long)]
    pub version: bool,

    /// print the usage spec of this binary and exit (for `usage diff`)
    #[arg(long, hide = true)]
    pub usage_spec: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Cli {
    /// `Cli::parse`, and what only the matches know: which of `-m` and
    /// `--base-url` the command line gave, rather than a default or
    /// `ANTHROPIC_BASE_URL` (clap's `value_source`; a global flag's source
    /// is the deepest command's, propagated to the root).
    pub fn parse_args() -> Self {
        let matches = Self::command().get_matches();
        let mut cli = Self::from_arg_matches(&matches)
            .map_err(|e| e.format(&mut Self::command()))
            .unwrap_or_else(|e| e.exit());
        let given = |id| matches.value_source(id) == Some(ValueSource::CommandLine);
        cli.request.model_given = given("model");
        cli.request.base_url_given = given("base_url");
        // The script tests `[ -z "${usage_prompt:-}" ]`, so an empty prompt
        // is no prompt: the chat, not a turn the API refuses as empty.
        cli.prompt = cli.prompt.filter(|p| !p.is_empty());
        if let Some(Command::Agent(agent)) = cli.command.as_mut() {
            agent.prompt = agent.prompt.take().filter(|p| !p.is_empty());
        }
        cli
    }
}

/// The flags every form of a request takes; global, so they bind under the
/// subcommands too (the same `global=#true` the bash spec carries).
#[derive(Args, Debug)]
pub struct RequestFlags {
    /// a full model id (see https://platform.claude.com/docs/en/api/cli/models/list)
    #[arg(
        short = 'm',
        long,
        global = true,
        value_name = "model",
        default_value = "claude-opus-5-5"
    )]
    pub model: String,

    /// how hard the model thinks
    #[arg(
        short = 'e',
        long,
        global = true,
        value_name = "effort",
        default_value = "medium"
    )]
    pub effort: Effort,

    /// system prompt
    #[arg(short = 'S', long, global = true, value_name = "system")]
    pub system: Option<String>,

    /// comma-separated <skill_id>[@<version>]
    #[arg(short = 's', long, global = true, value_name = "skills")]
    pub skills: Option<String>,

    /// anthropic|openai; only anthropic is implemented in bash
    #[arg(
        short = 'p',
        long,
        global = true,
        value_name = "provider",
        default_value = "anthropic"
    )]
    pub provider: String,

    /// API root, e.g. https://api.venice.ai/api/v1 for Venice; defaults per provider
    // clap fills it from the variable whatever the provider; config::check
    // keeps the variable to -p anthropic (KTD17)
    #[arg(long, global = true, value_name = "url", env = "ANTHROPIC_BASE_URL")]
    pub base_url: Option<String>,

    /// cap on output tokens per reply
    #[arg(long, global = true, value_name = "n", default_value = "16000")]
    pub max_tokens: String,

    /// let the model search and fetch the web (server tools; searches cost extra)
    #[arg(long, global = true)]
    pub web: bool,

    /// an MCP server (https) the model may call; repeatable; its token in ASK_CLAUDE_MCP_TOKEN_<HOST>, or ASK_CLAUDE_MCP_TOKEN for a lone server
    #[arg(long, global = true, value_name = "url")]
    pub mcp: Vec<String>,

    /// a model the reply may consult mid-turn (server tool, at most 3 calls a turn, billed at that model's rates); must be at least as capable as -m
    #[arg(long, global = true, value_name = "model")]
    pub advisor: Option<String>,

    /// only these tools of that --mcp server, e.g. mcp.example.com:search; repeatable; a server not named keeps all its tools
    #[arg(long, global = true, value_name = "mcp-tool")]
    pub mcp_tool: Vec<String>,

    /// `-m` came from the command line, not its default; filled by
    /// [`Cli::parse_args`]. `-p openai` takes `OPENAI_MODEL` otherwise
    #[arg(skip)]
    pub model_given: bool,

    /// `--base-url` came from the command line, not `ANTHROPIC_BASE_URL`;
    /// filled by [`Cli::parse_args`]. `-p openai` drops the variable's
    /// value (KTD17)
    #[arg(skip)]
    pub base_url_given: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// chats on disk, newest first
    List,
    /// continue a chat
    Resume {
        /// chat name or unique prefix; newest chat if omitted
        #[arg(value_name = "chat")]
        chat: Option<String>,
    },
    /// a Managed Agents session: one shot with a prompt, a chat without
    Agent(Box<AgentArgs>),
}

#[derive(Args, Debug)]
pub struct AgentArgs {
    /// one shot in a new session
    #[arg(value_name = "prompt")]
    pub prompt: Option<String>,

    /// agent id from the Console
    #[arg(long, global = true, value_name = "id", env = "ASK_CLAUDE_AGENT")]
    pub agent: Option<String>,

    /// environment id from the Console
    #[arg(long, global = true, value_name = "id", env = "ASK_CLAUDE_ENVIRONMENT")]
    pub environment: Option<String>,

    /// spend cap for the session at list price; 0 for none
    // `--budget -5` is a bad budget, which the usage parser hands to the
    // script, not an unknown flag; a negative number only, since any value
    // with a dash (allow_hyphen_values) changes the spec's double_dash
    #[arg(
        long,
        global = true,
        value_name = "cents",
        default_value = "100",
        allow_negative_numbers = true
    )]
    pub budget: String,

    /// vault of credentials to attach; repeatable
    #[arg(long, global = true, value_name = "id")]
    pub vault: Vec<String>,

    /// GitHub repository to mount, owner/name[@branch]; GITHUB_TOKEN
    #[arg(long, global = true, value_name = "url")]
    pub repo: Option<String>,

    /// memory store to attach, <id>[:ro]; repeatable, up to 8
    #[arg(long, global = true, value_name = "id")]
    pub memory: Vec<String>,

    /// files: overwrite what exists
    #[arg(long, global = true)]
    pub force: bool,

    /// a stdio MCP server on this machine that answers the agent's custom tools, e.g. 'tb --mcp'
    #[arg(
        long,
        global = true,
        value_name = "command",
        env = "ASK_CLAUDE_LOCAL_MCP"
    )]
    pub local_mcp: Option<String>,

    #[command(subcommand)]
    pub command: Option<AgentCommand>,
}

#[derive(Subcommand, Debug)]
pub enum AgentCommand {
    /// sessions, newest first
    List,
    /// the --local-mcp server's tools as custom tool definitions, for the agent
    Tools,
    /// continue a session
    Resume {
        /// session id or unique prefix
        #[arg(value_name = "session")]
        session: String,
    },
    /// what the agent wrote; download the named ones
    Files {
        #[arg(value_name = "session")]
        session: String,
        /// files to download into the current directory
        // zero or more, as the bash spec's `"[name]..."` says; clap's default of
        // 1.. for a positional Vec reads as `var_min=1` to usage-lib 7
        #[arg(value_name = "name", num_args = 0..)]
        name: Vec<String>,
    },
    /// change the spend cap of a session
    Budget {
        #[arg(value_name = "session")]
        session: String,
        /// new cap at list price; 0 removes it, for good
        #[arg(value_name = "cents")]
        cents: String,
    },
    /// archive an idle session
    Archive {
        #[arg(value_name = "session")]
        session: String,
    },
    /// print the Console URL of a session
    Trace {
        #[arg(value_name = "session")]
        session: String,
    },
}

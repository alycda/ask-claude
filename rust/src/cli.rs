//! The CLI: one clap definition that mirrors the `#USAGE` block of
//! the bash `ask-claude` flag for flag. `--usage-spec` (main.rs) prints it as a usage
//! spec so `usage diff ../ask-claude <(ask-claude --usage-spec)` can say
//! whether the two binaries still agree (docs/plan.md R0, the parity gate).

use clap::{Args, Parser, Subcommand, ValueEnum};

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

/// The flags every form of a request takes; global, so they bind under the
/// subcommands too (the same `global=#true` the bash spec carries).
#[derive(Args, Debug)]
pub struct RequestFlags {
    /// haiku, sonnet, opus, fable, or a full model id
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
    #[arg(long, global = true, value_name = "url")]
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
    #[arg(long, global = true, value_name = "cents", default_value = "100")]
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
        #[arg(value_name = "name")]
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

//! ask-claude-mcp, the binary: an MCP server on stdio, newline-delimited
//! JSON-RPC in on stdin and out on stdout, anything else on stderr. It
//! exits 0 when stdin closes. The tools and resources it serves are in
//! docs/plans/2026-11-08-002-feat-mcp-server-plan.md; tests/protocol.rs
//! and the files after it hold it to them.

use ask_claude_mcp::{events, reports, state};
use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use tokio::io::{stdin, stdout};

use rmcp::handler::server::wrapper::Parameters;

#[derive(serde::Deserialize, rmcp::schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct Since {
    /// only count activity on or after this date
    since: Option<String>,
}

struct AskClaude;

#[tool_router]
impl AskClaude {
    fn events(
        &self,
        Parameters(Since { since }): Parameters<Since>,
    ) -> Result<Vec<serde_json::Value>, String> {
        let Some(dir) = state::dir(
            std::env::var("XDG_STATE_HOME").ok().as_deref(),
            std::env::var("HOME").ok().as_deref(),
        ) else {
            return Err("neither XDG_STATE_HOME nor HOME is set".into());
        };
        // self._helper();

        // read on every call: the script appends while the server runs
        let text = match std::fs::read_to_string(dir.join("events.jsonl")) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(format!("events.jsonl: {e}")),
        };
        let mut events = events::parse(&text);

        if let Some(since) = since {
            events = match events::since(events, &since) {
                Ok(events) => events,
                Err(why) => return Err(why),
            };
        }

        Ok(events)
    }

    #[tool(
        description = "tokens per chat; sessions take their last total",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn tokens_by_chat(&self, params: Parameters<Since>) -> Result<CallToolResult, McpError> {
        let report = reports::run(
            "tokens_by_chat",
            &self.events(params).expect("tokens_by_chat is a report"),
        );
        Ok(CallToolResult::success(vec![ContentBlock::json(report)?]))
    }

    #[tool(
        description = "share of input read from the cache, per model",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn cache_hit_by_model(&self, params: Parameters<Since>) -> Result<CallToolResult, McpError> {
        let report = reports::run(
            "cache_hit_by_model",
            &self.events(params).expect("cache_hit_by_model is a report"),
        );
        Ok(CallToolResult::success(vec![ContentBlock::json(report)?]))
    }

    #[tool(
        description = "requests, mean and max seconds, per model",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn seconds_by_model(&self, params: Parameters<Since>) -> Result<CallToolResult, McpError> {
        let report = reports::run(
            "seconds_by_model",
            &self.events(params).expect("seconds_by_model is a report"),
        );
        Ok(CallToolResult::success(vec![ContentBlock::json(report)?]))
    }

    #[tool(
        description = "refusals per stop category",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn refusals_by_category(&self, params: Parameters<Since>) -> Result<CallToolResult, McpError> {
        let report = reports::run(
            "refusals_by_category",
            &self
                .events(params)
                .expect("refusals_by_category is a report"),
        );
        Ok(CallToolResult::success(vec![ContentBlock::json(report)?]))
    }

    #[tool(
        description = "calls and tokens per advisor model",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn advisor_spend(&self, params: Parameters<Since>) -> Result<CallToolResult, McpError> {
        let report = reports::run(
            "advisor_spend",
            &self.events(params).expect("advisor_spend is a report"),
        );
        Ok(CallToolResult::success(vec![ContentBlock::json(report)?]))
    }
}

/// A tool error: `isError: true` with the reason as text, so the model
/// sees why (a JSON-RPC error would reach it as an opaque failure).
fn _tool_error(why: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(why)])
}

#[tool_handler]
impl ServerHandler for AskClaude {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::new("ask-claude", "0.3.0"))
        .with_instructions("...")
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = AskClaude.serve((stdin(), stdout())).await?;
    server.waiting().await?;
    Ok(())
}

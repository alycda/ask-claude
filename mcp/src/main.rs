//! ask-claude-mcp, the binary: an MCP server on stdio, newline-delimited
//! JSON-RPC in on stdin and out on stdout, anything else on stderr. It
//! exits 0 when stdin closes. The tools and resources it serves are in
//! docs/plans/2026-11-08-002-feat-mcp-server-plan.md; tests/protocol.rs
//! and the files after it hold it to them.

use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use tokio::io::{stdin, stdout};

use rmcp::handler::server::wrapper::Parameters;

#[derive(serde::Deserialize, rmcp::schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct Since {
    /// only count activity on or after this date
    _since: Option<String>,
}

struct AskClaude;

#[tool_router]
impl AskClaude {
    #[tool(
        description = "tokens per chat; sessions take their last total",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn tokens_by_chat(
        &self,
        Parameters(Since { _since }): Parameters<Since>,
    ) -> Result<CallToolResult, McpError> {
        todo!();

        // Ok(CallToolResult::default())
    }

    #[tool(
        description = "share of input read from the cache, per model",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn cache_hit_by_model(
        &self,
        Parameters(Since { _since }): Parameters<Since>,
    ) -> Result<CallToolResult, McpError> {
        todo!()
    }

    #[tool(
        description = "requests, mean and max seconds, per model",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn seconds_by_model(
        &self,
        Parameters(Since { _since }): Parameters<Since>,
    ) -> Result<CallToolResult, McpError> {
        todo!()
    }

    #[tool(
        description = "refusals per stop category",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn refusals_by_category(
        &self,
        Parameters(Since { _since }): Parameters<Since>,
    ) -> Result<CallToolResult, McpError> {
        todo!()
    }

    #[tool(
        description = "calls and tokens per advisor model",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn advisor_spend(
        &self,
        Parameters(Since { _since }): Parameters<Since>,
    ) -> Result<CallToolResult, McpError> {
        todo!()
    }
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

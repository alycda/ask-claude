//! ask-claude-mcp, the binary: an MCP server on stdio, newline-delimited
//! JSON-RPC in on stdin and out on stdout, anything else on stderr. It
//! exits 0 when stdin closes. The tools and resources it serves are in
//! docs/plans/2026-11-08-002-feat-mcp-server-plan.md; tests/protocol.rs
//! and the files after it hold it to them.

fn main() {
    todo!(
        "U1: an rmcp ServerHandler on rmcp::transport::stdio(), see the TODO above the first test"
    )
}

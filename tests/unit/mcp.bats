#!/usr/bin/env bats
# shellcheck disable=SC2016  # jq programs in single quotes mention $ on purpose
# bats file_tags=mcp
load helpers

# The script's own --local-mcp client against the MCP server in mcp/: the
# two ends of this repository speaking to each other. Tagged mcp, so
# `just unit-tests` skips this file; `just mcp-bats` builds the server and
# runs it. ASK_CLAUDE_MCP_BIN names the server binary.

MCP_BIN="${ASK_CLAUDE_MCP_BIN:-$ROOT/mcp/target/debug/ask-claude-mcp}"

@test "mcp: the server binary is built" {
    [ -x "$MCP_BIN" ]
}

@test "mcp: agent tools lists the five reports as custom tools for an agent" {
    run --separate-stderr "$BIN" agent tools --local-mcp "$MCP_BIN"
    [ "$status" -eq 0 ]
    jq -e 'map(.name) | sort == ["advisor_spend", "cache_hit_by_model", "refusals_by_category", "seconds_by_model", "tokens_by_chat"]' <<<"$output"
    jq -e 'all(.[]; .type == "custom" and .input_schema.type == "object" and (.input_schema.properties | has("_since")))' <<<"$output"
}

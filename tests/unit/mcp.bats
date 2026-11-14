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
    jq -e 'all(.[]; .type == "custom" and .input_schema.type == "object" and (.input_schema.properties | has("since")))' <<<"$output"
}

# The session in tests/fixtures/mcp/session: turn 1 calls the custom tool
# tokens_by_chat with no input, turn 2 ends. The script asks the server and
# sends the report back in request 2. The event log is off, so the only
# events.jsonl the report reads is the fixture.
@test "mcp: an agent's custom tool call runs a report on the server, and the report goes back" {
    mkdir -p "$XDG_STATE_HOME/ask-claude"
    chmod 700 "$XDG_STATE_HOME/ask-claude"
    cp "$FIX/mcp/events.jsonl" "$XDG_STATE_HOME/ask-claude/events.jsonl"
    ASK_CLAUDE_EVENTS=off ASK_CLAUDE_CONFIRM=allow ASK_CLAUDE_AGENT=agent_FIXTURE ASK_CLAUDE_ENVIRONMENT=env_FIXTURE \
        ASK_CLAUDE_REPLAY="$FIX/mcp/session" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" \
        run --separate-stderr "$BIN" agent --local-mcp "$MCP_BIN" "Which chat used the most tokens?" </dev/null
    [ "$status" -eq 0 ]
    [ "$(wc -l <"$TMP/requests.jsonl")" -eq 2 ]
    sed -n 2p "$TMP/requests.jsonl" | jq -e '.events[0] | .type == "user.custom_tool_result" and .custom_tool_use_id == "sevt_FIXTURE04" and .is_error == false'
    diff -u <(jq -S . "$FIX/mcp/reports/tokens_by_chat.json") \
        <(sed -n 2p "$TMP/requests.jsonl" | jq '.events[0].content[0].text | fromjson' | jq -S .)
}

#!/usr/bin/env bats
load helpers

# lib/reassemble.jq: `jq -s` over the event payloads -> one message object.
# Golden = claude.sh's inline program run over the same fixture.

reassemble() { events "$1" | jq -s -f "$ROOT/lib/reassemble.jq"; }

@test "reassemble: text" {
    reassemble text > "$TMP/out.json"
    assert_golden "$FIX/sse/text.message.json" "$TMP/out.json"
}

@test "reassemble: thinking block keeps thinking text and signature" {
    reassemble thinking > "$TMP/out.json"
    assert_golden "$FIX/sse/thinking.message.json" "$TMP/out.json"
    jq -e '.content[0].type=="thinking" and (.content[0].signature|length>0)' "$TMP/out.json"
}

@test "reassemble: code execution parses tool input and keeps the result block" {
    reassemble code-execution > "$TMP/out.json"
    assert_golden "$FIX/sse/code-execution.message.json" "$TMP/out.json"
    jq -e '[.content[].type]==["server_tool_use","bash_code_execution_tool_result","text"]' "$TMP/out.json"
    jq -e '.content[0].input|type=="object"' "$TMP/out.json"
    jq -e '.content[0]|has("partial")|not' "$TMP/out.json"
    jq -e '.container.id=="container_FIXTURE"' "$TMP/out.json"
}

@test "reassemble: pause_turn and refusal stop reasons survive" {
    reassemble pause-turn | jq -e '.stop_reason=="pause_turn"'
    reassemble refusal   | jq -e '.stop_reason=="refusal" and .stop_details.category=="cyber"'
}

@test "reassemble: usage merges input and output token counts" {
    reassemble text | jq -e '.usage.input_tokens>0 and .usage.output_tokens>0'
}

@test "reassemble: web search, web fetch and MCP blocks fold like the oracle's, MCP input included" {
    for f in web-search web-fetch mcp; do
        events "$f" | jq -s -f "$ROOT/lib/reassemble.jq" > "$TMP/out.json"
        assert_golden "$FIX/sse/$f.message.json" "$TMP/out.json"
    done
    jq -e '.content[0].type=="mcp_tool_use" and (.content[0].input|type)=="object" and (.content[0]|has("partial")|not)' "$FIX/sse/mcp.message.json"
}

@test "reassemble: the advisor call and its result fold verbatim, iterations kept" {
    for f in advisor advisor-redacted; do
        events "$f" | jq -s -f "$ROOT/lib/reassemble.jq" > "$TMP/out.json"
        assert_golden "$FIX/sse/$f.message.json" "$TMP/out.json"
    done
    jq -e '.content[0] == {type: "server_tool_use", id: "srvtoolu_FIXTURE", name: "advisor", input: {}}' "$FIX/sse/advisor.message.json"
    jq -e '.content[1].content.type == "advisor_result" and (.content[1].content.text | length) > 0' "$FIX/sse/advisor.message.json"
    jq -e '.content[1].content.type == "advisor_redacted_result" and .content[1].content.encrypted_content == "REDACTED"' "$FIX/sse/advisor-redacted.message.json"
    jq -e '[.usage.iterations[].type] == ["message", "advisor_message", "message"]' "$FIX/sse/advisor.message.json"
}

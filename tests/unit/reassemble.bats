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

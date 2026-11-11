#!/usr/bin/env bats
load helpers

# lib/event.jq: one event-log line, from a reassembled message (Messages), a
# fold (sessions) or null (a request that failed before either existed).
# Goldens: tests/fixtures/events/*.jsonl, one compact line each.

# event <kind> <input-file|-> <round> <error> [chat]
# "-" is a request that failed before there was a message: the input is null.
event() {
    if [ "$2" = - ]; then echo null; else cat "$2"; fi |
        jq -c --arg ts 2026-11-07T00:00:00Z --arg version 0.2.6 --arg kind "$1" \
            --arg chat "${5:-20260102T150405-1234}" --arg host api.anthropic.com \
            --argjson round "$3" --argjson seconds 2 --arg error "$4" \
            -f "$ROOT/lib/event.jq"
}

@test "event: text reply" {
    event messages "$FIX/sse/text.message.json" 0 "" >"$TMP/out.jsonl"
    assert_golden "$FIX/events/text.jsonl" "$TMP/out.jsonl"
}

@test "event: advisor sums from usage.iterations" {
    event messages "$FIX/sse/advisor.message.json" 0 "" >"$TMP/out.jsonl"
    assert_golden "$FIX/events/advisor.jsonl" "$TMP/out.jsonl"
}

@test "event: redacted advice still carries the advisor's usage" {
    event messages "$FIX/sse/advisor-redacted.message.json" 0 "" >"$TMP/out.jsonl"
    assert_golden "$FIX/events/advisor-redacted.jsonl" "$TMP/out.jsonl"
}

@test "event: refusal carries its stop_details category" {
    event messages "$FIX/sse/refusal.message.json" 0 "" >"$TMP/out.jsonl"
    assert_golden "$FIX/events/refusal.jsonl" "$TMP/out.jsonl"
}

@test "event: a pause_turn round keeps its number" {
    event messages "$FIX/sse/pause-turn.message.json" 3 "" >"$TMP/out.jsonl"
    assert_golden "$FIX/events/pause-turn.jsonl" "$TMP/out.jsonl"
}

@test "event: a failed request has an error category and no usage" {
    event messages - 0 overloaded_error >"$TMP/out.jsonl"
    assert_golden "$FIX/events/error.jsonl" "$TMP/out.jsonl"
}

@test "event: a session turn's usage is the session's" {
    event session "$FIX/sessions/text.fold.json" 0 "" sesn_FIXTURE01 >"$TMP/out.jsonl"
    assert_golden "$FIX/events/session-text.jsonl" "$TMP/out.jsonl"
}

@test "event: a session turn keeps its custom-tool rounds and stop reason" {
    event session "$FIX/sessions/budget.fold.json" 2 "" sesn_FIXTURE01 >"$TMP/out.jsonl"
    assert_golden "$FIX/events/session-budget.jsonl" "$TMP/out.jsonl"
}

@test "event: a session error keeps the fold's usage and the category" {
    event session "$FIX/sessions/error.fold.json" 0 api_error sesn_FIXTURE01 >"$TMP/out.jsonl"
    assert_golden "$FIX/events/session-error.jsonl" "$TMP/out.jsonl"
}

@test "event: strings from a hostile endpoint become unknown" {
    event messages "$FIX/events/hostile.message.json" 0 "Error: your prompt was secret" >"$TMP/out.jsonl"
    assert_golden "$FIX/events/hostile.jsonl" "$TMP/out.jsonl"
    refute grep -q 'secret\|prompt\|asked' "$TMP/out.jsonl"
    # BSD grep has no -P: the control characters go in as bytes
    refute env LC_ALL=C grep -q $'[\x01-\x1f\x7f]' "$TMP/out.jsonl"
}

@test "event: a 64-character identifier is kept, 65 is unknown" {
    local m64 m65
    m64=$(printf 'm%.0s' {1..64})
    m65=$(printf 'm%.0s' {1..65})
    jq -n --arg m "$m64" '{model: $m}' >"$TMP/64.json"
    jq -n --arg m "$m65" '{model: $m}' >"$TMP/65.json"
    [ "$(event messages "$TMP/64.json" 0 "" | jq -r .model)" = "$m64" ]
    [ "$(event messages "$TMP/65.json" 0 "" | jq -r .model)" = unknown ]
}

@test "event: no line carries a reply's text" {
    local f text
    for f in text advisor advisor-redacted refusal pause-turn; do
        event messages "$FIX/sse/$f.message.json" 0 "" >"$TMP/out.jsonl"
        while IFS= read -r text; do
            [ -n "$text" ] || continue
            refute grep -qF -- "$text" "$TMP/out.jsonl"
        done < <(jq -r '.content[]? | (.text // empty), (.input // empty | tojson)' "$FIX/sse/$f.message.json")
    done
}

@test "event: one compact line, keys in a fixed order" {
    event messages "$FIX/sse/text.message.json" 0 "" >"$TMP/out.jsonl"
    [ "$(wc -l <"$TMP/out.jsonl")" -eq 1 ]
    [ "$(jq -c 'keys_unsorted' "$TMP/out.jsonl")" = '["v","ask_claude","ts","kind","chat","host","model","round","seconds","stop_reason","stop_category","usage_scope","input_tokens","output_tokens","cache_read","cache_write","list_cost","advisor","error","tools","tool_calls"]' ]
}

@test "event: usage that is not a number is null, and never written as given" {
    jq -n '{model: "claude-opus-5-5", stop_reason: "end_turn",
        usage: {input_tokens: "the user secret prompt", output_tokens: {t: "reply text"},
                cache_read_input_tokens: [1], cache_creation_input_tokens: "x",
                iterations: [{type: "advisor_message", model: "claude-opus-5-5", input_tokens: "advisor secret", output_tokens: 2}]}}' >"$TMP/in.json"
    event messages "$TMP/in.json" 0 "" >"$TMP/out.jsonl"
    jq -e '.input_tokens==null and .output_tokens==null and .cache_read==null and .cache_write==null' "$TMP/out.jsonl"
    jq -e '.advisor.input_tokens==0 and .advisor.output_tokens==2' "$TMP/out.jsonl"
    refute grep -q 'secret\|reply' "$TMP/out.jsonl"
}

@test "event: an identifier with a trailing newline is unknown" {
    jq -n '{model: "claude-x\n"}' >"$TMP/in.json"
    [ "$(event messages "$TMP/in.json" 0 "" | jq -r .model)" = unknown ]
}

@test "event: list_cost keeps the API's cents as a string, from a string or a number" {
    jq '.usage.list_cost = 42' "$FIX/sessions/text.fold.json" >"$TMP/n.json"
    jq '.usage.list_cost = "12.5"' "$FIX/sessions/text.fold.json" >"$TMP/s.json"
    jq '.usage.list_cost = "twelve dollars"' "$FIX/sessions/text.fold.json" >"$TMP/x.json"
    [ "$(event session "$TMP/n.json" 0 "" sesn_FIXTURE01 | jq -r .list_cost)" = 42 ]
    [ "$(event session "$TMP/s.json" 0 "" sesn_FIXTURE01 | jq -r .list_cost)" = 12.5 ]
    [ "$(event session "$TMP/x.json" 0 "" sesn_FIXTURE01 | jq -r .list_cost)" = unknown ]
}

@test "event: an empty chat is null, not unknown" {
    echo null | jq -c --arg ts 2026-11-07T00:00:00Z --arg version 0.2.7 --arg kind session --arg chat "" \
        --arg host api.anthropic.com --argjson round 0 --argjson seconds 0 --arg error unreadable \
        -f "$ROOT/lib/event.jq" | jq -e '.chat==null'
}

# tools and tool_calls (0.3.1): what the request let the model reach, and how
# often it did. tools comes from our own request body, tool_calls from the reply.

# event_tools <input-file|-> <tools-json> [error]
event_tools() {
    if [ "$1" = - ]; then echo null; else cat "$1"; fi |
        jq -c --arg ts 2026-11-07T00:00:00Z --arg version 0.3.1 --arg kind messages \
            --arg chat 20260102T150405-1234 --arg host api.anthropic.com \
            --argjson round 0 --argjson seconds 2 --arg error "${3:-}" --argjson tools "$2" \
            -f "$ROOT/lib/event.jq"
}

@test "event: tools names the server tools, a whole MCP server, and each tool of an allowlist" {
    event_tools "$FIX/sse/mcp.message.json" "$(jq -c .tools "$FIX/request/mcp-tool.json")" >"$TMP/out.jsonl"
    assert_golden "$FIX/events/mcp-tool.jsonl" "$TMP/out.jsonl"
    jq -e '.tools == ["mcp:mcp-deepwiki-com:read_wiki_structure", "mcp:mcp-deepwiki-com:ask_question", "mcp:mcp-example-com"]' "$TMP/out.jsonl"
    jq -e '.tool_calls == {web_search: 0, web_fetch: 0, mcp: 1}' "$TMP/out.jsonl"
}

@test "event: the web tools and the advisor are recorded by versioned type, in request order" {
    event_tools "$FIX/sse/text.message.json" "$(jq -c .tools "$FIX/request/advisor.json")" |
        jq -e '.tools == ["web_search_20260318", "web_fetch_20260318", "advisor_20260301"]'
}

@test "event: tool_calls counts web fetches from usage.server_tool_use" {
    event_tools "$FIX/sse/web-fetch.message.json" "$(jq -c .tools "$FIX/request/web.json")" |
        jq -e '.tool_calls == {web_search: 0, web_fetch: 1, mcp: 0}'
}

@test "event: a failed request keeps its tools, and has no tool_calls" {
    event_tools - "$(jq -c .tools "$FIX/request/web.json")" overloaded_error |
        jq -e '.tools == ["web_search_20260318", "web_fetch_20260318"] and .tool_calls == null and .error == "overloaded_error"'
}

@test "event: a tool name outside the identifier set is unknown, never written as given" {
    event_tools "$FIX/sse/text.message.json" \
        '[{"type":"mcp_toolset","mcp_server_name":"a","default_config":{"enabled":false},"configs":{"my secret\u001b[31m":{"enabled":true},"ok":{"enabled":false}}}]' >"$TMP/out.jsonl"
    jq -e '.tools == ["unknown"]' "$TMP/out.jsonl"
    refute grep -q 'secret' "$TMP/out.jsonl"
}

@test "event: without tools (a session, or a caller of 0.3.0) tools is null" {
    event messages "$FIX/sse/text.message.json" 0 "" | jq -e '.tools == null'
    event session "$FIX/sessions/text.fold.json" 0 "" sesn_FIXTURE01 | jq -e '.tools == null and .tool_calls == null'
}

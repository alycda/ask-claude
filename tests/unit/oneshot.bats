#!/usr/bin/env bats
load helpers

# One shot: `ask-claude [flags] <prompt>` with the transport replaced by a fixture.

replay() { ASK_CLAUDE_REPLAY="$FIX/sse/$1.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" "${@:2}"; }
history_file() { ls "$XDG_STATE_HOME"/ask-claude/*.json; }

@test "one-shot: prints the reply and records both turns" {
    run --separate-stderr replay text "ping"
    [ "$status" -eq 0 ]
    [ "$output" = "pong" ]
    [ "$(history_file | wc -l)" -eq 1 ]
    jq -e 'length==2 and .[0]=={"role":"user","content":"ping"}' "$(history_file)"
    jq -e '.[1].role=="assistant" and .[1].content[0].text=="pong"' "$(history_file)"
}

@test "one-shot: usage line on stderr shows model, tokens and cache counters" {
    run --separate-stderr replay text "ping"
    grep -qE '^\[claude-opus-5-5 in=[0-9]+ cache_read=[0-9]+ cache_write=[0-9]+ out=[0-9]+ [0-9]+s\]$' <<<"$stderr"
}

@test "one-shot: the request body is what the builder produces" {
    replay text "ping" >/dev/null 2>&1
    jq -e '.model=="claude-opus-5-5" and .stream==true and .messages==[{"role":"user","content":"ping"}]' "$TMP/requests.jsonl"
    jq -e 'has("tools")|not' "$TMP/requests.jsonl"
}

@test "one-shot: flags reach the request" {
    replay text -m claude-sonnet-5-5 -e low -S "be brief" --max-tokens 99 "ping" >/dev/null 2>&1
    jq -e '.model=="claude-sonnet-5-5" and .output_config.effort=="low" and .system=="be brief" and .max_tokens==99' "$TMP/requests.jsonl"
}

@test "one-shot: -f prefixes the file content, blank line between" {
    printf 'line one\nline two\n' > "$TMP/notes.md"
    replay text -f "$TMP/notes.md" "summarize" >/dev/null 2>&1
    jq -e '.messages[0].content=="line one\nline two\n\nsummarize"' "$TMP/requests.jsonl"
}

@test "one-shot: -f on an unreadable file exits 2 before any request" {
    run --separate-stderr replay text -f "$TMP/missing.md" "summarize"
    [ "$status" -eq 2 ]
    grep -q 'cannot read' <<<"$stderr"
    [ ! -e "$TMP/requests.jsonl" ]
}

@test "one-shot: API error exits 1 and drops the user turn" {
    run --separate-stderr replay error "ping"
    [ "$status" -eq 1 ]
    grep -q 'API error: model: claude-does-not-exist' <<<"$stderr"
    for f in $(history_file 2>/dev/null); do jq -e 'length==0' "$f"; done
}

@test "one-shot: refusal is reported with its category" {
    run --separate-stderr replay refusal "ping"
    [ "$status" -eq 0 ]
    grep -q '\[stopped: refusal cyber\]' <<<"$stderr"
}

@test "one-shot: pause_turn is continued, at most 8 rounds" {
    run --separate-stderr replay pause-turn "ping"
    [ "$status" -eq 0 ]
    grep -q '\[pause_turn: continuing, round 1\]' <<<"$stderr"
    [ "$(grep -c 'pause_turn: continuing' <<<"$stderr")" -le 8 ]
    grep -q '\[stopped: pause_turn\]' <<<"$stderr"
}

@test "one-shot: no key exits 2 with a pointer to ANTHROPIC_API_KEY" {
    unset ANTHROPIC_API_KEY
    run --separate-stderr replay text "ping"
    [ "$status" -eq 2 ]
    grep -q 'ANTHROPIC_API_KEY' <<<"$stderr"
}

@test "spec: -f --file binds" {
    run usage explain -f "$(spec_file)" -- ask-claude -f notes.md "summarize"
    grep -qE '^\s+flag\s+--file\s+notes.md' <<<"$output"
    refute grep -q refused <<<"$output"
}

@test "one-shot: --web and --mcp reach the request; a non-https MCP URL exits 2 first" {
    replay web-search --web "ping" >/dev/null 2>&1
    jq -e '.tools[0].type=="web_search_20250305" and .tools[1].type=="web_fetch_20250910"' "$TMP/requests.jsonl"
    rm -f "$TMP/requests.jsonl"
    replay mcp --mcp https://mcp.deepwiki.com/mcp "ping" >/dev/null 2>&1
    jq -e '.mcp_servers[0].url=="https://mcp.deepwiki.com/mcp" and .tools[0].type=="mcp_toolset"' "$TMP/requests.jsonl"
    rm -f "$TMP/requests.jsonl"
    run --separate-stderr replay mcp --mcp http://mcp.deepwiki.com/mcp "ping"
    [ "$status" -eq 2 ]
    grep -q 'https' <<<"$stderr"
    [ ! -e "$TMP/requests.jsonl" ]
}

@test "one-shot: a request log that cannot be written fails the request before anything is sent" {
    run --separate-stderr env ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/nodir/requests.jsonl" "$BIN" "ping"
    [ "$status" -eq 1 ]
    [ "$output" = "" ]
    grep -q 'request log' <<<"$stderr"
    [ ! -e "$TMP/nodir" ]
}

@test "one-shot: --advisor reaches the request, and the footer names the advisor's tokens" {
    run --separate-stderr replay advisor-redacted -m claude-haiku-5-5 --advisor claude-haiku-5-5 "ping"
    [ "$status" -eq 0 ]
    jq -e '.tools[-1] == {type: "advisor_20260301", name: "advisor", model: "claude-haiku-5-5", max_uses: 3}' "$TMP/requests.jsonl"
    grep -q '^(advisor) ' <<<"$output"
    grep -qE 'out=120 advisor=claude-haiku-5-5 in=2623 out=754 ' <<<"$stderr"
}

@test "one-shot: without an advisor the footer has no advisor field" {
    run --separate-stderr replay text "ping"
    ! grep -q 'advisor=' <<<"$stderr"
}

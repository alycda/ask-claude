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
    jq -e '.tools[0].type=="web_search_20260318" and .tools[1].type=="web_fetch_20260318"' "$TMP/requests.jsonl"
    rm -f "$TMP/requests.jsonl"
    replay mcp --mcp https://mcp.deepwiki.com/mcp "ping" >/dev/null 2>&1
    jq -e '.mcp_servers[0].url=="https://mcp.deepwiki.com/mcp" and .tools[0].type=="mcp_toolset"' "$TMP/requests.jsonl"
    rm -f "$TMP/requests.jsonl"
    run --separate-stderr replay mcp --mcp http://mcp.deepwiki.com/mcp "ping"
    [ "$status" -eq 2 ]
    grep -q 'https' <<<"$stderr"
    [ ! -e "$TMP/requests.jsonl" ]
}

@test "one-shot: ASK_CLAUDE_MCP_TOKEN with several --mcp exits 2 first, naming each server's own variable" {
    run --separate-stderr env ASK_CLAUDE_MCP_TOKEN=tok_SHARED ASK_CLAUDE_REPLAY="$FIX/sse/mcp.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" \
        "$BIN" --mcp https://mcp.deepwiki.com/mcp --mcp https://mcp.example.com/sse "ping"
    [ "$status" -eq 2 ]
    grep -q 'ASK_CLAUDE_MCP_TOKEN_MCP_DEEPWIKI_COM' <<<"$stderr"
    grep -q 'ASK_CLAUDE_MCP_TOKEN_MCP_EXAMPLE_COM' <<<"$stderr"
    refute grep -q 'tok_SHARED' <<<"$stderr"
    [ ! -e "$TMP/requests.jsonl" ]
}

@test "one-shot: several --mcp with their own tokens each send their own" {
    run --separate-stderr env ASK_CLAUDE_MCP_TOKEN_MCP_DEEPWIKI_COM=tok_A ASK_CLAUDE_MCP_TOKEN_MCP_EXAMPLE_COM=tok_B \
        ASK_CLAUDE_REPLAY="$FIX/sse/mcp.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" \
        "$BIN" --mcp https://mcp.deepwiki.com/mcp --mcp https://mcp.example.com/sse "ping"
    [ "$status" -eq 0 ]
    jq -e '.mcp_servers | map(.authorization_token) == ["tok_A", "tok_B"]' "$TMP/requests.jsonl"
}

@test "one-shot: the variable the refusal names is the one the request reads, for an odd host too" {
    url='https://user@Mcp.Example.com:8443/sse'
    run --separate-stderr env ASK_CLAUDE_MCP_TOKEN=tok_SHARED ASK_CLAUDE_REPLAY="$FIX/sse/mcp.sse" "$BIN" --mcp "$url" --mcp https://mcp.deepwiki.com/mcp "ping"
    [ "$status" -eq 2 ]
    var=$(grep -oE 'ASK_CLAUDE_MCP_TOKEN_[A-Z0-9_]*EXAMPLE[A-Z0-9_]*' <<<"$stderr")
    [ "$var" = ASK_CLAUDE_MCP_TOKEN_USER_MCP_EXAMPLE_COM ]
    run --separate-stderr env "$var=tok_ODD" ASK_CLAUDE_REPLAY="$FIX/sse/mcp.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" \
        "$BIN" --mcp "$url" --mcp https://mcp.deepwiki.com/mcp "ping"
    [ "$status" -eq 0 ]
    jq -e '.mcp_servers[0].authorization_token == "tok_ODD" and (.mcp_servers[1] | has("authorization_token") | not)' "$TMP/requests.jsonl"
}

@test "one-shot: --mcp-tool sends that server's toolset as an allowlist" {
    replay mcp --mcp https://mcp.deepwiki.com/mcp --mcp-tool mcp.deepwiki.com:ask_question "ping" >/dev/null 2>&1
    jq -e '.tools[0] == {type: "mcp_toolset", mcp_server_name: "mcp-deepwiki-com",
        default_config: {enabled: false}, configs: {ask_question: {enabled: true}}}' "$TMP/requests.jsonl"
}

@test "one-shot: an --mcp-tool that names no --mcp server exits 2 before any request" {
    run --separate-stderr replay mcp --mcp https://mcp.deepwiki.com/mcp --mcp-tool mcp.example.com:search "ping"
    [ "$status" -eq 2 ]
    grep -q 'names no --mcp server' <<<"$stderr"
    grep -q 'mcp.deepwiki.com' <<<"$stderr"
    [ ! -e "$TMP/requests.jsonl" ]
    run --separate-stderr replay mcp --mcp-tool mcp.deepwiki.com:ask_question "ping"
    [ "$status" -eq 2 ]
    [ ! -e "$TMP/requests.jsonl" ]
}

# The check in config.bash and the match in request.jq must fold case the
# same way. Bash ${h,,} follows the locale, and in a UTF-8 locale it folds
# the Kelvin sign (U+212A) to k, which jq's ascii_downcase does not: the
# entry passed the check, matched nothing in the request, and the server
# went out with every tool on. Both now fold ASCII only (0.3.1).
@test "one-shot: an --mcp-tool for a host that only matches under locale case folding exits 2" {
    local k=$'\xe2\x84\xaa' # U+212A as UTF-8 bytes, whatever the test's own locale
    run --separate-stderr env LC_ALL=C.UTF-8 ASK_CLAUDE_REPLAY="$FIX/sse/mcp.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" \
        "$BIN" --mcp "https://mcp.example.${k}om/mcp" --mcp-tool mcp.example.kom:search "ping"
    [ "$status" -eq 2 ]
    grep -q 'names no --mcp server' <<<"$stderr"
    [ ! -e "$TMP/requests.jsonl" ]
}

@test "one-shot: --mcp-tool still matches an ASCII host whatever its case" {
    replay mcp --mcp https://MCP.DeepWiki.com/mcp --mcp-tool mcp.deepwiki.COM:ask_question "ping" >/dev/null 2>&1
    jq -e '.tools[0].configs == {ask_question: {enabled: true}}' "$TMP/requests.jsonl"
}

@test "one-shot: an --mcp-tool that is not <host>:<tool> exits 2 before any request" {
    local bad
    for bad in ask_question mcp.deepwiki.com: :ask_question 'mcp.deepwiki.com:ask question' mcp.deepwiki.com:a:b; do
        run --separate-stderr replay mcp --mcp https://mcp.deepwiki.com/mcp --mcp-tool "$bad" "ping"
        [ "$status" -eq 2 ]
        grep -q '<host>:<tool>' <<<"$stderr"
    done
    [ ! -e "$TMP/requests.jsonl" ]
}

# A pause_turn continuation without skills sends back the container the
# previous round ran in (dynamic web filtering runs code there); the first
# request of a turn sends none.

# with_container <fixture> <out>: the fixture's stream, its reply carrying container_FIXTURE
with_container() {
    sed 's/"container":null/"container":{"id":"container_FIXTURE","expires_at":"2026-11-10T01:00:00Z"}/g' "$FIX/sse/$1.sse" >"$2"
}

@test "one-shot: a pause_turn continuation without skills sends back its round's container" {
    with_container pause-turn "$TMP/paused.sse"
    run --separate-stderr env ASK_CLAUDE_REPLAY="$TMP/paused.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" --web "ping"
    grep -q 'pause_turn: continuing, round 1' <<<"$stderr"
    jq -s -e '.[0] | has("container") | not' "$TMP/requests.jsonl"
    jq -s -e 'length > 1 and (.[1:] | all(.container == "container_FIXTURE"))' "$TMP/requests.jsonl"
}

@test "chat: without skills a new turn does not reuse the last turn's container" {
    with_container text "$TMP/text-container.sse"
    printf 'hi\nagain\n/quit\n' | ASK_CLAUDE_REPLAY="$TMP/text-container.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" >/dev/null 2>&1
    [ "$(jq -s length "$TMP/requests.jsonl")" -eq 2 ]
    jq -s -e 'all(has("container") | not)' "$TMP/requests.jsonl"
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
    refute grep -q 'advisor=' <<<"$stderr"
}

@test "one-shot: --advisor sends the advisor beta, and without it there is none" {
    ASK_CLAUDE_BETA_LOG="$TMP/betas" replay advisor-redacted -m claude-haiku-5-5 --advisor claude-haiku-5-5 "ping" >/dev/null 2>&1
    grep -q 'advisor-tool-2026-03-01' "$TMP/betas"
    rm -f "$TMP/betas"
    ASK_CLAUDE_BETA_LOG="$TMP/betas" replay text "ping" >/dev/null 2>&1
    [ -s "$TMP/betas" ]
    refute grep -q 'advisor' "$TMP/betas"
}

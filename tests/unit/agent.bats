#!/usr/bin/env bats
load helpers

# `ask-claude agent [flags] <prompt>`: one shot through a Managed Agents
# session, the transport replaced by recorded streams. A session turn is one
# stream; ASK_CLAUDE_REPLAY names one .sse file, or a directory whose
# 1.sse, 2.sse, ... are the turns' streams in order.

export ASK_CLAUDE_AGENT=agent_FIXTURE ASK_CLAUDE_ENVIRONMENT=env_FIXTURE
replay() { ASK_CLAUDE_REPLAY="$FIX/sessions/$1" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" agent "${@:2}"; }
request() { sed -n "$1p" "$TMP/requests.jsonl"; }   # the Nth body
STATE_DIR() { echo "$XDG_STATE_HOME/ask-claude"; }

@test "agent: prints the reply, logs the create body, exits 0" {
    run --separate-stderr replay text.sse "ping"
    [ "$status" -eq 0 ]
    [ "$output" = "pong" ]
    [ "$(wc -l < "$TMP/requests.jsonl")" -eq 1 ]
    request 1 | jq -S . > "$TMP/got.json"
    jq -S . "$FIX/agents/create-plain.json" > "$TMP/want.json"
    assert_golden "$TMP/want.json" "$TMP/got.json"
}

@test "agent: the usage line names the session, tokens and cost" {
    run --separate-stderr replay text.sse "ping"
    grep -qE '^\[sesn_FIXTURE01 in=4 out=4 cache_read=0 cache_write=4311 cost=\$0\.00 [0-9]+s\]$' <<<"$stderr"
}

@test "agent: the session is remembered in the state dir, with its title" {
    replay text.sse "ping" >/dev/null 2>&1
    ls "$(STATE_DIR)"/sessions/sesn_FIXTURE01 >/dev/null
    grep -q '^agent=agent_FIXTURE$' "$(STATE_DIR)/sessions/sesn_FIXTURE01"
}

@test "agent: --budget 0 sends no budget; a bad budget exits 2 before any request" {
    replay text.sse --budget 0 "ping" >/dev/null 2>&1
    request 1 | jq -e 'has("budget")|not'
    rm -f "$TMP/requests.jsonl"
    for b in abc -5 1.5 ""; do
        run --separate-stderr replay text.sse --budget "$b" "ping"
        [ "$status" -eq 2 ]
        grep -q 'budget' <<<"$stderr"
    done
    [ ! -e "$TMP/requests.jsonl" ]
}

@test "agent: no agent or environment id exits 2 and points at the Console" {
    for v in ASK_CLAUDE_AGENT ASK_CLAUDE_ENVIRONMENT; do
        run --separate-stderr env -u $v ASK_CLAUDE_REPLAY="$FIX/sessions/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" agent "ping"
        [ "$status" -eq 2 ]
        grep -qi 'console' <<<"$stderr"
        grep -q "$v" <<<"$stderr"
    done
    [ ! -e "$TMP/requests.jsonl" ]
}

@test "agent: no key exits 2 before any request" {
    run --separate-stderr env -u ANTHROPIC_API_KEY ASK_CLAUDE_REPLAY="$FIX/sessions/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" agent "ping"
    [ "$status" -eq 2 ]
    grep -q 'ANTHROPIC_API_KEY' <<<"$stderr"
    [ ! -e "$TMP/requests.jsonl" ]
}

@test "agent: a session error exits 1 with the message, and is not remembered" {
    run --separate-stderr replay error.sse "ping"
    [ "$status" -eq 1 ]
    grep -q 'the environment could not be started' <<<"$stderr"
    [ -z "$(ls "$(STATE_DIR)"/sessions 2>/dev/null)" ]
}

@test "agent: budget_reached exits 0 and says how to raise it" {
    run --separate-stderr replay budget.sse "ping"
    [ "$status" -eq 0 ]
    grep -q '\[stopped: budget_reached' <<<"$stderr"
    grep -q 'ask-claude agent budget sesn_FIXTURE01' <<<"$stderr"
}

@test "agent: a tool confirmation is denied when stdin is not a terminal" {
    run --separate-stderr replay confirm "ping" </dev/null
    [ "$status" -eq 0 ]
    [ "$(wc -l < "$TMP/requests.jsonl")" -eq 2 ]
    request 2 | jq -e '.events==[{"type":"user.tool_confirmation","tool_use_id":"sevt_FIXTURE04","result":"deny"}]'
    grep -q 'denied' <<<"$stderr"
    grep -q 'bash' <<<"$stderr"
}

@test "agent: a custom tool call gets an error result, the turn goes on" {
    run --separate-stderr replay custom-tool "ping" </dev/null
    [ "$status" -eq 0 ]
    [ "$(wc -l < "$TMP/requests.jsonl")" -eq 3 ]
    request 2 | jq -e '.events[0].type=="user.custom_tool_result" and .events[0].custom_tool_use_id=="sevt_FIXTURE03" and .events[0].is_error==true'
    request 2 | jq -e '.events[0].content[0].text | test("no local tools")'
    request 3 | jq -e '.events[0].custom_tool_use_id=="sevt_FIXTURE09"'
    grep -q 'weather tool is unavailable' <<<"$output"
}

@test "agent: -f is refused until the Files API lands" {
    printf 'x\n' > "$TMP/n.md"
    run --separate-stderr replay text.sse -f "$TMP/n.md" "ping"
    [ "$status" -eq 2 ]
    grep -q 'not implemented' <<<"$stderr"
    [ ! -e "$TMP/requests.jsonl" ]
}
